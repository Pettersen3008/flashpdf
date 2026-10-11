mod output;
mod row;
mod text;

use std::collections::BTreeMap;

use output::Marks;
pub(crate) use output::{Decoration, LayoutBuffer, PositionedBox, PositionedImage, Segment};
pub(crate) use row::table_width;

use crate::document::{BoxNode, Element, ImageNode, Paragraph, Row, Table};
use crate::font::FontBook;
use crate::geometry::LayoutArea;
use crate::image::Image;
use crate::layout::output::PositionedLine;
use crate::{
    BoxStyle, FontId, ImageSize, Pt, RenderError, Rgb, RunDecoration, TextAlign, TextRun,
    VerticalAlign,
};

/// CSS reference pixels are 96 per inch; PDF points are 72.
const PT_PER_PX: f32 = 0.75;

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
    UnclosedRowSpan,
}

impl From<LayoutError> for RenderError {
    fn from(error: LayoutError) -> Self {
        match error {
            LayoutError::InvalidFontSize => Self::InvalidFontSize,
            LayoutError::UnsupportedCharacter(character) => Self::UnsupportedCharacter(character),
            LayoutError::MissingGlyph(character) => Self::MissingGlyph(character),
            LayoutError::TextTooWide => Self::TextTooWide,
            LayoutError::ImageTooWide => Self::ImageTooWide,
            LayoutError::InvalidWidth
            | LayoutError::UnknownFont(_)
            | LayoutError::InvalidBox
            | LayoutError::InvalidColumnWidth
            | LayoutError::ColumnsOverflow
            | LayoutError::InvalidRuns
            | LayoutError::UnknownImage
            | LayoutError::UnclosedRowSpan => Self::InvalidLayout,
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
            Element::Box(node) => self.layout_box(node, area, output),
            Element::Spacer(spacer) => Ok(spacer.height),
            Element::Stack(stack) => self.layout_rows(&stack.children, stack.gap, area, output),
            Element::Row(_) => {
                self.layout_rows(std::slice::from_ref(element), Pt::ZERO, area, output)
            }
            Element::Table(table) => self.layout_table(table, area, output),
            Element::Image(image) => self.layout_image(image, area, output),
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
        let height = text::ParagraphLayouter::layout(self.fonts, &keyed, area, output)?;
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

    fn layout_box(
        &self,
        node: &BoxNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let style = node.style;
        let horizontal = style.margin.right
            + style.margin.left
            + style.padding.right
            + style.padding.left
            + style.border.right
            + style.border.left;
        if horizontal >= area.width
            || style
                .margin
                .iter()
                .chain(style.padding.iter())
                .any(|value| *value < Pt::ZERO)
        {
            return Err(LayoutError::InvalidBox);
        }
        let inset_x = style.margin.left + style.padding.left + style.border.left;
        let inset_y = style.margin.top + style.padding.top + style.border.top;
        let inner_width = area.width - horizontal;
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
        if let Some(index) = paint {
            output.boxes[index] = PositionedBox {
                origin: area.origin.translated(style.margin.left, style.margin.top),
                width: area.width - style.margin.right - style.margin.left,
                height: box_height,
                style,
            };
        }
        Ok(style.margin.top + box_height + style.margin.bottom)
    }

    /// Lays out block children top to bottom. Rows among them share row spans,
    /// and each span must close by the last child.
    fn layout_rows(
        &self,
        children: &[Element<'_>],
        gap: Pt,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let mut spans = Vec::new();
        let mut height = Pt::ZERO;
        for (index, child) in children.iter().enumerate() {
            if index != 0 {
                height += gap;
            }
            let area = area.translated(Pt::ZERO, height);
            height += match child {
                Element::Row(row) => self.layout_row(row, area, output, &mut spans)?,
                _ => self.layout(child, area, output)?,
            };
        }
        if spans.is_empty() {
            Ok(height)
        } else {
            Err(LayoutError::UnclosedRowSpan)
        }
    }

    fn layout_row(
        &self,
        row: &Row<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
        spans: &mut Vec<PlacedCell>,
    ) -> Result<Pt, LayoutError> {
        let widths = row::resolve_column_widths(row.columns.iter().copied(), area.width)?;
        let sum = |widths: &[Pt]| widths.iter().fold(Pt::ZERO, |sum, width| sum + *width);
        for span in spans.iter_mut() {
            span.rows -= 1;
        }
        let mut height = Pt::ZERO;
        let mut placed = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            let start = output.marks();
            let cell_height = self.layout(
                &cell.content,
                area.translated(sum(&widths[..cell.column]), Pt::ZERO)
                    .with_width(sum(&widths[cell.column..cell.column + cell.span])),
                output,
            )?;
            let laid = PlacedCell {
                top: area.origin.y,
                height: cell_height,
                rows: cell.rows.saturating_sub(1),
                align: cell.align,
                painted: matches!(&cell.content, Element::Box(node) if painted(&node.style)),
                start,
                end: output.marks(),
            };
            if laid.rows > 0 {
                spans.push(laid);
            } else {
                if cell_height > height {
                    height = cell_height;
                }
                placed.push(laid);
            }
        }
        // A span's last row grows to fit what its content needs below the rows above.
        for span in spans.iter().filter(|span| span.rows == 0) {
            let rest = span.top + span.height - area.origin.y;
            if rest > height {
                height = rest;
            }
        }
        // Cells stretch to the row (`align-items: stretch`): a painted cell box
        // pushed its rectangle first, so growing it tiles the row's backgrounds and
        // borders. Content then moves down by its share of the free space.
        for cell in placed
            .iter()
            .chain(spans.iter().filter(|span| span.rows == 0))
        {
            let space = (area.origin.y - cell.top) + height - cell.height;
            if cell.painted {
                output.boxes[cell.start.boxes].height += space;
            }
            let shift = match cell.align {
                VerticalAlign::Top => continue,
                VerticalAlign::Middle => Pt(space.get() / 2.0),
                VerticalAlign::Bottom => space,
            };
            let mut from = cell.start;
            from.boxes += usize::from(cell.painted);
            output.shift(from, cell.end, shift);
        }
        spans.retain(|span| span.rows > 0);
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
        let (head, body) = table.rows.split_at(table.header_rows + table.footer_rows);
        let (header, footer) = head.split_at(table.header_rows);
        let mut height = Pt::ZERO;
        for row in header.iter().chain(body).chain(footer) {
            height += self.layout(row, area.translated(Pt::ZERO, height), output)?;
        }
        Ok(height)
    }
}

/// A cell laid out before its row height is known; `rows` counts the rows a span still covers.
struct PlacedCell {
    top: Pt,
    height: Pt,
    rows: u16,
    align: VerticalAlign,
    painted: bool,
    start: Marks,
    end: Marks,
}

pub(crate) fn painted(style: &BoxStyle) -> bool {
    style.background.is_some() || style.border.iter().any(|edge| *edge > Pt::ZERO)
}
