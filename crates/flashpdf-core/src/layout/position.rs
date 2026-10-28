//! CSS 2.1 §9.3 and §10.3.7: relative, absolute, and fixed boxes. Each lays out into a layer
//! of its own that paints after the normal flow; a relative one keeps its place in flow.

use crate::document::{BoxNode, Element};
use crate::geometry::{LayoutArea, Point};
use crate::layout::{
    border_width, overflows, Layer, LayoutBuffer, LayoutEngine, LayoutError, Rect,
};
use crate::{Inset, Position, PositionKind, Pt};

/// An inset in points against `base`, the containing block's side along it; None is `auto`.
fn resolve(inset: Inset, base: Pt) -> Option<f32> {
    match inset {
        Inset::Auto => None,
        Inset::Points(points) => Some(points),
        Inset::Percent(percent) => Some(base.get() * percent / 100.0),
    }
}

/// The absolute and fixed boxes whose nearest positioned ancestor is out of `elements`, in
/// document order. A positioned box holds its own.
fn collect<'e, 'a>(
    elements: &'e [Element<'a>],
    clips: &mut Vec<usize>,
    found: &mut Vec<(&'e BoxNode<'a>, Vec<usize>)>,
) {
    for element in elements {
        match element {
            Element::Box(node) => match node.size.position.map(|position| position.kind) {
                Some(PositionKind::Absolute | PositionKind::Fixed) => {
                    found.push((node, clips.clone()))
                }
                Some(PositionKind::Relative) => {}
                None => {
                    if node.effects.is_some_and(crate::BoxEffects::transformed) {
                        continue;
                    }
                    if node.effects.is_some() {
                        clips.push(std::ptr::from_ref(node) as usize);
                    }
                    collect(&node.children, clips, found);
                    if node.effects.is_some() {
                        clips.pop();
                    }
                }
            },
            Element::Stack(stack) => collect(&stack.children, clips, found),
            Element::Flex(node) => collect(&node.children, clips, found),
            Element::Table(table) => collect(&table.rows, clips, found),
            Element::Row(row) => {
                for cell in &row.cells {
                    collect(std::slice::from_ref(&cell.content), clips, found);
                }
            }
            _ => {}
        }
    }
}

fn clip_effect(
    buffer: &LayoutBuffer,
    source: usize,
    offset: Point,
) -> Option<super::output::LayerEffects> {
    for layer in &buffer.layers {
        let offset = offset.translated(layer.offset.x, layer.offset.y);
        if let Some(effect) = &layer.effects {
            if effect.source == source {
                let mut effect = effect.as_ref().clone();
                effect.rect.origin = effect.rect.origin.translated(offset.x, offset.y);
                effect.paint = false;
                return Some(effect);
            }
        }
        if let Some(effect) = clip_effect(&layer.buffer, source, offset) {
            return Some(effect);
        }
    }
    None
}

/// Adds `buffer` as a layer. Under `z-index: auto` the box makes no stacking context, so the
/// layers inside it join the parent's, after it.
fn push_layer(
    output: &mut LayoutBuffer,
    z: Option<i32>,
    offset: Point,
    page: bool,
    mut buffer: LayoutBuffer,
) {
    let inner = if z.is_none() {
        buffer
            .layers
            .extract_if(.., |layer| layer.flow.is_none())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    output.layers.push(Layer {
        z: z.unwrap_or(0),
        flow: None,
        effects: None,
        offset,
        page,
        buffer,
    });
    for mut layer in inner {
        if !layer.page {
            layer.offset = layer.offset.translated(offset.x, offset.y);
        }
        output.layers.push(layer);
    }
}

impl LayoutEngine<'_> {
    /// A relative box lays out in flow and paints offset; an absolute or fixed one takes no
    /// space here and waits for its containing block, which `place_absolutes` lays it out in.
    pub(super) fn positioned(
        &self,
        node: &BoxNode<'_>,
        position: Position,
        area: LayoutArea,
        width: Option<Pt>,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        if position.kind != PositionKind::Relative {
            self.absolutes.set(self.absolutes.get() + 1);
            return Ok(Pt::ZERO);
        }
        let before = self.absolutes.get();
        let mut buffer = LayoutBuffer::default();
        let (height, rect) = self.block(node, node.size, area, width, &mut buffer)?;
        if self.absolutes.get() != before {
            self.place_absolutes(&node.children, Some(padding_box(node, rect)), &mut buffer)?;
        }
        let inset = position.inset;
        // §9.4.3: `left` wins over `right` and `top` over `bottom`; the compiler rejects a
        // percent top or bottom, which would need the containing block's height.
        let dx = resolve(inset.left, area.width)
            .or_else(|| resolve(inset.right, area.width).map(|right| -right))
            .unwrap_or(0.0);
        let dy = resolve(inset.top, Pt::ZERO)
            .or_else(|| resolve(inset.bottom, Pt::ZERO).map(|bottom| -bottom))
            .unwrap_or(0.0);
        self.effect_layer(node, rect, &mut buffer)?;
        push_layer(
            output,
            position.z.or_else(|| {
                node.effects
                    .is_some_and(crate::BoxEffects::transformed)
                    .then_some(0)
            }),
            Point {
                x: Pt(dx),
                y: Pt(dy),
            },
            false,
            buffer,
        );
        Ok(height)
    }

    /// Lays out the absolute and fixed boxes of `elements` that `block`, a padding box, contains,
    /// or the page area when `None`.
    pub(super) fn place_absolutes(
        &self,
        elements: &[Element<'_>],
        block: Option<Rect>,
        output: &mut LayoutBuffer,
    ) -> Result<(), LayoutError> {
        let mut found = Vec::new();
        collect(elements, &mut Vec::new(), &mut found);
        for (node, clips) in found {
            let first = output.layers.len();
            self.absolute(node, block, output)?;
            for source in clips.into_iter().rev() {
                let effect =
                    clip_effect(output, source, Point::ZERO).ok_or(LayoutError::InvalidBox)?;
                for layer in output.layers.split_off(first) {
                    output.layers.push(Layer {
                        z: layer.z,
                        offset: Point::ZERO,
                        page: false,
                        flow: None,
                        effects: Some(Box::new(effect.clone())),
                        buffer: LayoutBuffer {
                            layers: vec![layer],
                            ..LayoutBuffer::default()
                        },
                    });
                }
            }
        }
        Ok(())
    }

    /// CSS 2.1 §10.3.7 and §10.6.4, with `left` and `right`, and `top` and `bottom`, never both
    /// `auto`, which the compiler ensures: an auto width shrinks to fit, or fills between two
    /// insets, and an auto height follows the content, or fills between two insets.
    fn absolute(
        &self,
        node: &BoxNode<'_>,
        block: Option<Rect>,
        output: &mut LayoutBuffer,
    ) -> Result<(), LayoutError> {
        let Some(position) = node.size.position else {
            return Ok(());
        };
        let page = Rect {
            origin: Point::ZERO,
            width: self.page.0,
            height: self.page.1,
        };
        let (block, on_page) = match block {
            Some(block) if position.kind == PositionKind::Absolute => (block, false),
            _ => (page, true),
        };
        let (style, mut size) = (node.style, node.size);
        let inset = position.inset;
        let (left, right) = (
            resolve(inset.left, block.width),
            resolve(inset.right, block.width),
        );
        let (top, bottom) = (
            resolve(inset.top, block.height),
            resolve(inset.bottom, block.height),
        );
        let margin_x = (style.margin.left + style.margin.right).get();
        let margin_y = (style.margin.top + style.margin.bottom).get();
        let inset_x =
            (style.padding.left + style.padding.right + style.border.left + style.border.right)
                .get();
        let inset_y =
            (style.padding.top + style.padding.bottom + style.border.top + style.border.bottom)
                .get();
        let cb = block.width.get();
        let room = cb - left.unwrap_or(0.0) - right.unwrap_or(0.0) - margin_x;
        let resolved = |value| border_width(value, cb, inset_x, size.border_box);
        let width = match (resolved(size.width), left.zip(right)) {
            (Some(width), _) => width,
            (None, Some(_)) => room,
            (None, None) => {
                let (min, max) = self.contents(&node.children)?;
                (max + inset_x).min((min + inset_x).max(room))
            }
        };
        let width = resolved(size.max_width).map_or(width, |max| width.min(max));
        let width = resolved(size.min_width).map_or(width, |min| width.max(min));
        if overflows(width - inset_x) {
            return Err(LayoutError::InvalidBox);
        }
        let width = width.max(inset_x);
        let x = left.unwrap_or_else(|| cb - right.unwrap_or(0.0) - margin_x - width);
        if size.height.is_none() {
            if let (Some(top), Some(bottom)) = (top, bottom) {
                let fill = block.height.get() - top - bottom - margin_y;
                let fill = if size.border_box {
                    fill.max(inset_y)
                } else {
                    fill - inset_y
                };
                size.height = Some(Pt(fill.max(0.0)));
            }
        }
        let area = LayoutArea {
            origin: block.origin.translated(Pt(x), Pt::ZERO),
            width: Pt(width + margin_x),
        };
        let before = self.absolutes.get();
        let mut buffer = LayoutBuffer::default();
        let (height, rect) = self.block(node, size, area, Some(Pt(width)), &mut buffer)?;
        if self.absolutes.get() != before {
            self.place_absolutes(&node.children, Some(padding_box(node, rect)), &mut buffer)?;
        }
        let y = top.unwrap_or_else(|| block.height.get() - bottom.unwrap_or(0.0) - height.get());
        self.effect_layer(node, rect, &mut buffer)?;
        push_layer(
            output,
            position.z.or_else(|| {
                node.effects
                    .is_some_and(crate::BoxEffects::transformed)
                    .then_some(0)
            }),
            Point {
                x: Pt::ZERO,
                y: Pt(y),
            },
            on_page,
            buffer,
        );
        Ok(())
    }
}

/// The padding box inside a laid-out border box: an absolute child's containing block.
pub(super) fn padding_box(node: &BoxNode<'_>, rect: Rect) -> Rect {
    let border = node.style.border;
    Rect {
        origin: rect.origin.translated(border.left, border.top),
        width: rect.width - border.left - border.right,
        height: rect.height - border.top - border.bottom,
    }
}
