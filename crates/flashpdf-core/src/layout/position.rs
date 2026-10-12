use super::output::Overlay;
use super::{block_width, LayoutBuffer, LayoutEngine, LayoutError};
use crate::document::{Element, PositionNode};
use crate::geometry::{LayoutArea, Point};
use crate::{ImageSize, Offset, PositionKind, Pt};

#[derive(Clone, Copy)]
pub(super) struct Containing {
    origin: Point,
    width: Pt,
    height: Pt,
    viewport: bool,
}

fn offset(value: Option<Offset>, length: Pt) -> Option<Pt> {
    value.map(|value| {
        Pt(if value.percent {
            value.value * length.get() / 100.0
        } else {
            value.value
        })
    })
}

impl LayoutEngine<'_> {
    pub(super) fn layout_position(
        &self,
        node: &PositionNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        if self.measuring && node.style.kind != PositionKind::Relative {
            return Ok(Pt::ZERO);
        }
        let (page_width, page_height) = self.page.unwrap_or((area.width, Pt(842.0)));
        let viewport = Containing {
            origin: Point::ZERO,
            width: page_width,
            height: page_height,
            viewport: true,
        };
        let parent = if node.style.kind == PositionKind::Fixed {
            viewport
        } else {
            self.container
                .unwrap_or(if node.style.kind == PositionKind::Relative {
                    Containing {
                        width: area.width,
                        height: Pt::ZERO,
                        viewport: false,
                        ..viewport
                    }
                } else {
                    viewport
                })
        };
        let left = offset(node.style.inset.left, parent.width);
        let right = offset(node.style.inset.right, parent.width);
        let top = offset(node.style.inset.top, parent.height);
        let bottom = offset(node.style.inset.bottom, parent.height);
        if [left, right, top, bottom]
            .into_iter()
            .flatten()
            .any(|value| !value.get().is_finite())
        {
            return Err(LayoutError::InvalidBox);
        }
        let measuring = Self {
            measuring: true,
            ..*self
        };
        let mut scratch = LayoutBuffer::default();
        if node.style.kind == PositionKind::Relative {
            let height = measuring.layout(&node.content, area, &mut scratch)?;
            let dx = left
                .or_else(|| right.map(|right| Pt::ZERO - right))
                .unwrap_or(Pt::ZERO);
            let dy = top
                .or_else(|| bottom.map(|bottom| Pt::ZERO - bottom))
                .unwrap_or(Pt::ZERO);
            let shifted = area.translated(dx, dy);
            let container = bounds(&node.content, shifted, height, false)?;
            Self {
                container: Some(container),
                ..*self
            }
            .layout(&node.content, shifted, output)?;
            return Ok(height);
        }
        let mut content = node.content.clone();
        if let Element::Box(node) = &mut *content {
            for width in [
                &mut node.size.width,
                &mut node.size.min_width,
                &mut node.size.max_width,
            ] {
                if let ImageSize::Percent(value) = *width {
                    *width = ImageSize::Fixed(Pt(parent.width.get() * value.get() / 100.0));
                }
            }
            if let (None, Some(top), Some(bottom)) = (node.size.height, top, bottom) {
                let mut height =
                    parent.height - top - bottom - node.style.margin.top - node.style.margin.bottom;
                if !node.size.border_box {
                    height = height
                        - node.style.padding.top
                        - node.style.padding.bottom
                        - node.style.border.top
                        - node.style.border.bottom;
                }
                if height < Pt::ZERO {
                    return Err(LayoutError::InvalidBox);
                }
                node.size.height = Some(height);
            }
        }
        let mut available = parent.width - left.unwrap_or(Pt::ZERO) - right.unwrap_or(Pt::ZERO);
        if let Element::Box(node) = &*content {
            if let ImageSize::Fixed(width) = node.size.width {
                let frame = if node.size.border_box {
                    Pt::ZERO
                } else {
                    node.style.padding.left
                        + node.style.padding.right
                        + node.style.border.left
                        + node.style.border.right
                };
                available = Pt(available
                    .get()
                    .max((width + frame + node.style.margin.left + node.style.margin.right).get()));
            }
        }
        if !available.get().is_finite() || available <= Pt::ZERO {
            return Err(LayoutError::InvalidBox);
        }
        let explicit =
            matches!(&*content, Element::Box(node) if node.size.width != ImageSize::Auto);
        let width = if explicit || (left.is_some() && right.is_some()) {
            available
        } else {
            let (min, max) = measuring.intrinsic(&content)?;
            Pt((max + 0.001)
                .min(available.get())
                .max(min)
                .min(available.get())
                .max(0.001))
        };
        let chosen = LayoutArea::root(width);
        let height = measuring.layout(&content, chosen, &mut scratch)?;
        let child = bounds(&content, chosen, height, false)?;
        let outer_width = match &*content {
            Element::Box(node) => {
                child.width
                    + node.style.border.left
                    + node.style.border.right
                    + node.style.margin.left
                    + node.style.margin.right
            }
            _ => child.width,
        };
        let x = left.unwrap_or_else(|| {
            right.map_or(
                if parent.viewport {
                    self.static_origin.x + area.origin.x
                } else {
                    area.origin.x - parent.origin.x
                },
                |right| parent.width - right - outer_width,
            )
        });
        let y = top.unwrap_or_else(|| {
            bottom.map_or(
                if parent.viewport {
                    self.static_origin.y + area.origin.y
                } else {
                    area.origin.y - parent.origin.y
                },
                |bottom| parent.height - bottom - height,
            )
        });
        if ![x.get(), y.get(), height.get(), child.width.get()]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err(LayoutError::InvalidBox);
        }
        let mut layout = LayoutBuffer::default();
        Self {
            container: Some(child),
            ..*self
        }
        .layout(&content, chosen, &mut layout)?;
        output.overlays.push(Overlay {
            layout: Box::new(layout),
            origin: parent.origin.translated(x, y),
            viewport: parent.viewport,
            fixed: node.style.kind == PositionKind::Fixed,
            line: output.lines.len().saturating_sub(1),
        });
        Ok(Pt::ZERO)
    }
}

fn bounds(
    element: &Element<'_>,
    area: LayoutArea,
    height: Pt,
    viewport: bool,
) -> Result<Containing, LayoutError> {
    let (origin, width, height) = if let Element::Box(node) = element {
        let (left, width) = block_width(&node.style, &node.size, area.width)?.unwrap_or((
            node.style.margin.left,
            area.width - node.style.margin.left - node.style.margin.right,
        ));
        (
            area.origin.translated(
                left + node.style.border.left,
                node.style.margin.top + node.style.border.top,
            ),
            width - node.style.border.left - node.style.border.right,
            height
                - node.style.margin.top
                - node.style.margin.bottom
                - node.style.border.top
                - node.style.border.bottom,
        )
    } else {
        (area.origin, area.width, height)
    };
    Ok(Containing {
        origin,
        width,
        height,
        viewport,
    })
}
