mod flex;
mod grid;
mod output;
mod position;
mod row;
mod text;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use output::Marks;
pub(crate) use output::{
    Actual, Decoration, Gap, Layer, LayoutBuffer, Lift, PositionedBox, PositionedImage, Segment,
    MARKER_SHAPE,
};
pub(crate) use row::table_width;

use crate::document::{BoxNode, Element, ImageNode, Paragraph, Row, Table};
use crate::font::FontBook;
use crate::geometry::{LayoutArea, Point};
use crate::image::Image;
use crate::layout::output::PositionedLine;
use crate::{
    BoxSize, BoxStyle, FontId, ImageSize, Page, Pt, RenderError, Rgb, RunDecoration, TextAlign,
    TextRun, VerticalAlign,
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
    LineTooWide,
    InvalidBox,
    InvalidColumnWidth,
    ColumnsOverflow,
    InvalidRuns,
    UnknownImage,
    ImageTooWide,
    UnclosedRowSpan,
    ContentOverflow,
}

impl From<LayoutError> for RenderError {
    fn from(error: LayoutError) -> Self {
        match error {
            LayoutError::InvalidFontSize => Self::InvalidFontSize,
            LayoutError::UnsupportedCharacter(character) => Self::UnsupportedCharacter(character),
            LayoutError::MissingGlyph(character) => Self::MissingGlyph(character),
            LayoutError::TextTooWide => Self::TextTooWide,
            LayoutError::LineTooWide => Self::LineTooWide,
            LayoutError::ImageTooWide => Self::ImageTooWide,
            LayoutError::ContentOverflow => Self::ContentOverflow,
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
    /// The page area's width and height, the containing block of a box with no positioned ancestor.
    page: (Pt, Pt),
    /// Absolute and fixed boxes met in flow, so a containing block looks for its own only when
    /// the count moved while its content laid out.
    absolutes: Cell<usize>,
    // Intrinsic probes revisit nested grids at the same constraints within this block.
    grids: RefCell<BTreeMap<grid::GridKey, grid::GridRows>>,
}

impl<'a> LayoutEngine<'a> {
    pub(crate) fn new(fonts: &'a FontBook, images: &'a [Image], page: Page) -> Self {
        Self {
            fonts,
            images,
            page: (
                page.width - page.margin.left - page.margin.right,
                page.height - page.margin.top - page.margin.bottom,
            ),
            absolutes: Cell::new(0),
            grids: RefCell::new(BTreeMap::new()),
        }
    }

    /// Lays out a block the page places, then the absolute and fixed boxes in it that have
    /// no positioned ancestor, from the page area.
    pub(crate) fn layout_root(
        &self,
        element: &Element<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let before = self.absolutes.get();
        let height = self.layout(element, area, output)?;
        if self.absolutes.get() != before {
            self.place_absolutes(std::slice::from_ref(element), None, output)?;
        }
        Ok(height)
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
            Element::Stack(stack) => self.layout_rows(&stack.children, stack.gap, area, output),
            Element::Row(_) => {
                self.layout_rows(std::slice::from_ref(element), Pt::ZERO, area, output)
            }
            Element::Table(table) => self.layout_table(table, area, output),
            Element::Image(image) => self.layout_image(image, area, output),
            Element::Anchor(slot) => {
                output.anchors.push((*slot, output.lines.len()));
                Ok(Pt::ZERO)
            }
            Element::Flex(node) => self.layout_flex(node, area, output),
            Element::Grid(node) => self.layout_grid(node, area, output),
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
                color,
                link,
                alpha,
                background,
            } = run.decoration;
            if link.is_some_and(|link| usize::from(link) >= paragraph.links.len()) {
                return Err(LayoutError::InvalidRuns);
            }
            let key = (
                [run.color.r, run.color.g, run.color.b],
                underline,
                line_through,
                color.map(|color| [color.r, color.g, color.b]),
                link,
                alpha,
                background.map(|color| [color.r, color.g, color.b]),
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
        let keyed = Paragraph { runs, ..*paragraph };
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
            let font = self
                .fonts
                .get(segment.font)
                .ok_or(LayoutError::UnknownFont(segment.font))?;
            let (ascent, descent, _) = font.metrics();
            let (underline, line_through) = font.strokes();
            let scale = segment.size.get() / 1000.0;
            output.decorations.push(Decoration {
                underline: decoration.underline.then_some(underline),
                line_through: decoration.line_through.then_some(line_through),
                color: decoration.color.unwrap_or(color),
                background: decoration.background,
                link: decoration.link.map(|link| link_base + usize::from(link)),
                alpha: decoration.alpha,
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
        let (natural_width, natural_height) = image.size();
        let available = area.width.get();
        let requested = match node.width {
            ImageSize::Auto => None,
            ImageSize::Fixed(width) => Some(width.get()),
            ImageSize::Percent(percent) => Some(available * percent.get() / 100.0),
        };
        let max_width = match node.max_width {
            ImageSize::Auto => f32::INFINITY,
            ImageSize::Fixed(width) => width.get(),
            ImageSize::Percent(percent) => available * percent.get() / 100.0,
        };
        let max_height = node.max_height.map_or(f32::INFINITY, Pt::get);
        // CSS 2.1 §10.4 and §10.7: a max clamps its own axis; only an auto axis follows the ratio.
        let (width, height) = match (requested, node.height) {
            (None, None) => {
                let max_width = available.min(max_width);
                let scale = (max_width / natural_width)
                    .min(max_height / natural_height)
                    .min(1.0);
                // `natural * (max / natural)` can round a few ulps past `max`, and a max never yields.
                (
                    (natural_width * scale).min(max_width),
                    (natural_height * scale).min(max_height),
                )
            }
            (None, Some(height)) => {
                let height = height.get().min(max_height);
                (
                    (height * natural_width / natural_height).min(max_width),
                    height,
                )
            }
            (Some(width), None) => {
                let width = width.min(max_width);
                (
                    width,
                    (width * natural_height / natural_width).min(max_height),
                )
            }
            (Some(width), Some(height)) => (width.min(max_width), height.get().min(max_height)),
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
            alpha: node.alpha,
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
        match node.size.position {
            None => Ok(self
                .block(node, (node.size, node.style), area, width, output)?
                .0),
            Some(position) => {
                self.positioned(node, position, (node.size, node.style), area, width, output)
            }
        }
    }

    /// Lays out a box with `size` for its own and returns its margin-box height and its border box.
    fn block(
        &self,
        node: &BoxNode<'_>,
        (size, style): (BoxSize, BoxStyle),
        area: LayoutArea,
        width: Option<Pt>,
        output: &mut LayoutBuffer,
    ) -> Result<(Pt, Rect), LayoutError> {
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
        let vertical_extent =
            style.padding.top + style.padding.bottom + style.border.top + style.border.bottom;
        let assigned_height = size.height.map(|height| {
            (border_size(height.get(), vertical_extent.get(), size.border_box)
                - vertical_extent.get())
            .max(0.0)
        });
        for child in &node.children {
            let child_area = area
                .translated(inset_x, inset_y + inner_height)
                .with_width(inner_width);
            inner_height += match child {
                Element::Grid(grid) if grid.style.boxed => {
                    self.layout_grid_height(grid, child_area, assigned_height, output)?
                }
                _ => self.layout(child, child_area, output)?,
            };
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
        let origin = area.origin.translated(left, style.margin.top);
        if let Some(index) = paint {
            output.boxes[index] = PositionedBox {
                origin,
                width,
                height: Pt(height),
                style,
            };
        }
        Ok((
            style.margin.top + Pt(height) + style.margin.bottom,
            Rect {
                origin,
                width,
                height: Pt(height),
            },
        ))
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
        let mut pending_gap = Pt::ZERO;
        for child in children {
            if !matches!(child, Element::Anchor(_)) {
                height += pending_gap;
                pending_gap = gap;
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
            let line = output.lines.len();
            output
                .anchors
                .extend(cell.anchors.iter().map(|slot| (*slot, line)));
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
                // A positioned cell box paints in its own layer, which does not stretch.
                painted: matches!(&cell.content, Element::Box(node)
                    if painted(&node.style) && node.size.position.is_none()),
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

/// A box's border box in layout coordinates, y down.
#[derive(Clone, Copy)]
struct Rect {
    origin: Point,
    width: Pt,
    height: Pt,
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

pub(crate) fn painted(style: &BoxStyle) -> bool {
    style.background.is_some() || style.border.iter().any(|edge| *edge > Pt::ZERO)
}
