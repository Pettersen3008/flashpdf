mod flex;
mod output;
mod row;
mod text;

use std::collections::BTreeMap;

pub(crate) use output::{Decoration, LayoutBuffer, PositionedImage, Segment};
pub(crate) use row::table_width;

use crate::document::{BoxNode, Element, ImageNode, Paragraph, Row, Stack, Table};
use crate::font::FontBook;
use crate::geometry::LayoutArea;
use crate::image::Image;
use crate::layout::output::{PositionedBox, PositionedLine};
use crate::{
    BoxSize, BoxStyle, FontId, ImageSize, Pt, RenderError, Rgb, RunDecoration, TextAlign, TextRun,
};

/// CSS reference pixels are 96 per inch; PDF points are 72.
const PT_PER_PX: f32 = 0.75;
/// How far content may pass a fixed size before it overflows, absorbing float error.
const TOLERANCE: f32 = 1e-3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayoutError {
    InvalidWidth,
    UnknownFont(FontId),
    InvalidFontSize,
    UnsupportedCharacter(char),
    MissingGlyph(char),
    TextTooWide,
    InvalidBox,
    InvalidColumnWidth,
    ColumnsOverflow,
    InvalidRuns,
    UnknownImage,
    ImageTooWide,
    ContentOverflow,
}

impl From<LayoutError> for RenderError {
    fn from(error: LayoutError) -> Self {
        match error {
            LayoutError::InvalidFontSize => Self::InvalidFontSize,
            LayoutError::UnsupportedCharacter(character) => Self::UnsupportedCharacter(character),
            LayoutError::MissingGlyph(character) => Self::MissingGlyph(character),
            LayoutError::TextTooWide => Self::TextTooWide,
            LayoutError::ImageTooWide => Self::ImageTooWide,
            LayoutError::ContentOverflow => Self::ContentOverflow,
            LayoutError::InvalidWidth
            | LayoutError::UnknownFont(_)
            | LayoutError::InvalidBox
            | LayoutError::InvalidColumnWidth
            | LayoutError::ColumnsOverflow
            | LayoutError::InvalidRuns
            | LayoutError::UnknownImage => Self::InvalidLayout,
        }
    }
}

pub(crate) struct LayoutEngine<'a> {
    fonts: &'a FontBook,
    images: &'a [Image],
}

impl<'a> LayoutEngine<'a> {
    pub(crate) fn new(fonts: &'a FontBook, images: &'a [Image]) -> Self {
        Self { fonts, images }
    }

    pub(crate) fn layout(
        &self,
        element: &Element<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        if !area.width.get().is_finite() || area.width <= Pt::ZERO {
            return Err(LayoutError::InvalidWidth);
        }
        match element {
            Element::Paragraph(paragraph) => self.layout_paragraph(paragraph, area, output),
            Element::Box(node) => self.layout_box(node, area, None, output),
            Element::Spacer(spacer) => Ok(spacer.height),
            Element::Stack(stack) => self.layout_stack(stack, area, output),
            Element::Row(row) => self.layout_row(row, area, output),
            Element::Table(table) => self.layout_table(table, area, output),
            Element::Image(image) => self.layout_image(image, area, output),
            Element::Flex(node) => self.layout_flex(node, area, output),
        }
    }

    /// The text layouter merges adjacent runs whose font, size, and colour match
    /// and knows nothing of decoration. Each distinct (colour, decoration) pair
    /// is laid out under a stand-in colour so such runs never merge; the real
    /// colour and a `Decoration` are restored on every segment afterwards.
    fn layout_paragraph(
        &self,
        paragraph: &Paragraph<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let mut keys: Vec<(Rgb, RunDecoration)> = Vec::new();
        let mut indices = BTreeMap::new();
        let mut runs = Vec::with_capacity(paragraph.runs.len());
        for run in &paragraph.runs {
            let RunDecoration {
                underline,
                line_through,
                link,
            } = run.decoration;
            if link.is_some_and(|link| usize::from(link) >= paragraph.links.len()) {
                return Err(LayoutError::InvalidRuns);
            }
            let key = (
                [run.color.r, run.color.g, run.color.b],
                underline,
                line_through,
                link,
            );
            let index = *indices.entry(key).or_insert_with(|| {
                keys.push((run.color, run.decoration));
                keys.len() - 1
            });
            runs.push(TextRun {
                color: Rgb {
                    r: index as u8,
                    g: (index >> 8) as u8,
                    b: (index >> 16) as u8,
                },
                ..*run
            });
        }
        let keyed = Paragraph {
            align: paragraph.align,
            text: paragraph.text,
            runs,
            links: paragraph.links,
        };
        let first = output.segments.len();
        let link_base = output.links.len();
        let (height, _) = text::ParagraphLayouter::layout(self.fonts, &keyed, area, output)?;
        output.links.extend(paragraph.links.iter().cloned());
        for segment in &mut output.segments[first..] {
            let index = usize::from(segment.color.r)
                | usize::from(segment.color.g) << 8
                | usize::from(segment.color.b) << 16;
            let (color, decoration) = keys[index];
            segment.color = color;
            let (ascent, descent, _) = self
                .fonts
                .get(segment.font)
                .ok_or(LayoutError::UnknownFont(segment.font))?
                .metrics();
            let scale = segment.size.get() / 1000.0;
            output.decorations.push(Decoration {
                underline: decoration.underline,
                line_through: decoration.line_through,
                link: decoration.link.map(|link| link_base + usize::from(link)),
                ascent: Pt(ascent * scale),
                descent: Pt((-descent * scale).max(0.0)),
            });
        }
        Ok(height)
    }

    /// An image is an atomic block box, left-aligned in its container. It also
    /// pushes an empty pseudo-line so the page splitter can carry it between lines.
    fn layout_image(
        &self,
        node: &ImageNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let image = self
            .images
            .get(usize::from(node.slot))
            .ok_or(LayoutError::UnknownImage)?;
        let (pixel_width, pixel_height) = (image.width as f32, image.height as f32);
        let available = area.width.get();
        let requested = match node.width {
            ImageSize::Auto => None,
            ImageSize::Fixed(width) => Some(width.get()),
            ImageSize::Percent(percent) => Some(available * percent.get() / 100.0),
        };
        let (width, height) = match (requested, node.height) {
            (None, None) => {
                let width = (pixel_width * PT_PER_PX).min(available);
                (width, width * pixel_height / pixel_width)
            }
            (None, Some(height)) => (height.get() * pixel_width / pixel_height, height.get()),
            (Some(width), None) => (width, width * pixel_height / pixel_width),
            (Some(width), Some(height)) => (width, height.get()),
        };
        if width > available * (1.0 + f32::EPSILON * 4.0) {
            return Err(LayoutError::ImageTooWide);
        }
        if !(width > 0.0 && height > 0.0 && width.is_finite() && height.is_finite()) {
            return Err(LayoutError::InvalidBox);
        }
        let line = output.lines.len();
        output.lines.push(PositionedLine {
            segments: output.segments.len()..output.segments.len(),
            origin: area.origin,
            top: area.origin.y,
            bottom: area.origin.y + Pt(height),
            available_width: area.width,
            text_width: Pt::ZERO,
            align: TextAlign::Left,
        });
        let link = node.link.map(|uri| {
            output.links.push(uri.to_owned());
            output.links.len() - 1
        });
        output.images.push(PositionedImage {
            origin: area.origin,
            width: Pt(width),
            height: Pt(height),
            slot: node.slot,
            link,
            alt: node.alt.to_owned(),
            line,
        });
        Ok(Pt(height))
    }

    /// `width` imposes the border-box width a flex container chose.
    fn layout_box(
        &self,
        node: &BoxNode<'_>,
        area: LayoutArea,
        width: Option<Pt>,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let style = node.style;
        let size = node.size;
        let horizontal = style.margin.right
            + style.margin.left
            + style.padding.right
            + style.padding.left
            + style.border.right
            + style.border.left;
        if style
            .margin
            .iter()
            .chain(style.padding.iter())
            .any(|value| *value < Pt::ZERO)
        {
            return Err(LayoutError::InvalidBox);
        }
        let sized = match width {
            Some(width) => Some((style.margin.left, width)),
            None => block_width(&style, &size, area.width)?,
        };
        // An auto width keeps the arithmetic of earlier releases, so their output is unchanged.
        let (left, width, inner_width) = match sized {
            Some((left, width)) => (
                left,
                width,
                width
                    - style.padding.right
                    - style.padding.left
                    - style.border.right
                    - style.border.left,
            ),
            None => (
                style.margin.left,
                area.width - style.margin.right - style.margin.left,
                area.width - horizontal,
            ),
        };
        if inner_width < Pt::ZERO || (inner_width == Pt::ZERO && !node.children.is_empty()) {
            return Err(LayoutError::InvalidBox);
        }
        let inset_x = left + style.padding.left + style.border.left;
        let inset_y = style.margin.top + style.padding.top + style.border.top;
        let paint = painted(&style).then(|| {
            let index = output.boxes.len();
            output.boxes.push(PositionedBox {
                origin: area.origin,
                width: Pt::ZERO,
                height: Pt::ZERO,
                style,
            });
            index
        });
        let mut inner_height = Pt::ZERO;
        for child in &node.children {
            inner_height += self.layout(
                child,
                area.translated(inset_x, inset_y + inner_height)
                    .with_width(inner_width),
                output,
            )?;
        }
        let box_height = inner_height
            + style.padding.top
            + style.padding.bottom
            + style.border.top
            + style.border.bottom;
        let inset =
            style.padding.top + style.padding.bottom + style.border.top + style.border.bottom;
        let border = |value: Pt| border_size(value.get(), inset.get(), size.border_box);
        let content = box_height.get();
        let min = border(size.min_height);
        let mut height = size.height.map_or(content, border);
        if let Some(max) = size.max_height {
            height = height.min(border(max));
        }
        let height = height.max(min);
        if overflows(height - content) {
            return Err(LayoutError::ContentOverflow);
        }
        if let Some(index) = paint {
            output.boxes[index] = PositionedBox {
                origin: area.origin.translated(left, style.margin.top),
                width,
                height: Pt(height),
                style,
            };
        }
        Ok(style.margin.top + Pt(height) + style.margin.bottom)
    }

    fn layout_stack(
        &self,
        stack: &Stack<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let mut height = Pt::ZERO;
        for (index, child) in stack.children.iter().enumerate() {
            if index != 0 {
                height += stack.gap;
            }
            height += self.layout(child, area.translated(Pt::ZERO, height), output)?;
        }
        Ok(height)
    }

    fn layout_row(
        &self,
        row: &Row<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let widths =
            row::resolve_column_widths(row.cells.iter().map(|cell| cell.width), area.width)?;
        let mut offset = Pt::ZERO;
        let mut height = Pt::ZERO;
        let mut placed = Vec::with_capacity(row.cells.len());
        for (cell, width) in row.cells.iter().zip(widths) {
            let index = output.boxes.len();
            let cell_height = self.layout(
                &cell.content,
                area.translated(offset, Pt::ZERO).with_width(width),
                output,
            )?;
            placed.push((index, cell_height));
            if cell_height > height {
                height = cell_height;
            }
            offset += width;
        }
        // Cells stretch to the row (`align-items: stretch`): a painted cell box
        // pushed its rectangle first, so growing it tiles the row's backgrounds and borders.
        for (cell, (index, cell_height)) in row.cells.iter().zip(placed) {
            if let Element::Box(node) = &cell.content {
                if painted(&node.style) {
                    output.boxes[index].height += height - cell_height;
                }
            }
        }
        Ok(height)
    }

    /// Nested tables stack their rows; only a top-level table paginates with a repeating header.
    fn layout_table(
        &self,
        table: &Table<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let area = area.with_width(table_width(table.width, area.width)?);
        let mut height = Pt::ZERO;
        for row in &table.rows {
            height += self.layout(row, area.translated(Pt::ZERO, height), output)?;
        }
        Ok(height)
    }
}

/// CSS 2.1 §10.3.3 and §10.4: a block's left margin and border-box width in `available`,
/// or `None` when an auto width fills it unclamped.
fn block_width(
    style: &BoxStyle,
    size: &BoxSize,
    available: Pt,
) -> Result<Option<(Pt, Pt)>, LayoutError> {
    let inset =
        (style.padding.left + style.padding.right + style.border.left + style.border.right).get();
    let resolve = |value| border_width(value, available.get(), inset, size.border_box);
    let fill = (available - style.margin.left - style.margin.right).get();
    let (max, min) = (resolve(size.max_width), resolve(size.min_width));
    let Some(width) = resolve(size.width).or_else(|| {
        (max.is_some_and(|max| max < fill) || min.is_some_and(|min| min > fill)).then_some(fill)
    }) else {
        return Ok(None);
    };
    let width = max.map_or(width, |max| width.min(max));
    let width = min.map_or(width, |min| width.max(min));
    let free = fill - width;
    if overflows(free) {
        return Err(LayoutError::InvalidBox);
    }
    let free = free.max(0.0);
    let left = match (size.auto_margin & 8 != 0, size.auto_margin & 2 != 0) {
        (true, true) => free / 2.0,
        (true, false) => free,
        _ => 0.0,
    };
    Ok(Some((style.margin.left + Pt(left), Pt(width))))
}

/// A `width`, `min-width`, or `max-width` as a border-box width; `Auto` is `None`.
// Out of line: a copy at each call costs WASM bytes and saves no measurable time.
#[inline(never)]
fn border_width(value: ImageSize, available: f32, inset: f32, border_box: bool) -> Option<f32> {
    let value = match value {
        ImageSize::Auto => return None,
        ImageSize::Fixed(value) => value.get(),
        ImageSize::Percent(percent) => available * percent.get() / 100.0,
    };
    Some(border_size(value, inset, border_box))
}

/// `box-sizing`: padding and border add to a content-box size and fit inside a border-box one.
fn border_size(value: f32, inset: f32, border_box: bool) -> f32 {
    if border_box {
        value.max(inset)
    } else {
        value + inset
    }
}

/// Space below `-TOLERANCE` overflows, and so does NaN from sizes too large for a float.
fn overflows(free: f32) -> bool {
    free.is_nan() || free < -TOLERANCE
}

fn painted(style: &BoxStyle) -> bool {
    style.background.is_some() || style.border.iter().any(|edge| *edge > Pt::ZERO)
}
