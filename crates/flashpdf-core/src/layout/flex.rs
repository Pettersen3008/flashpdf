//! A CSS flex container: §9.7 sizes the items along the main axis, §8.1 and
//! §9.5 place them, and §8.3 and §9.4 align them across.

use std::ops::Range;

use crate::document::{Element, FlexNode};
use crate::geometry::LayoutArea;
use crate::layout::output::Marks;
use crate::layout::{
    border_size, border_width, overflows, painted, text, LayoutBuffer, LayoutEngine, LayoutError,
    TOLERANCE,
};
use crate::{Align, BoxSize, ColumnWidth, ImageSize, Justify, Pt};

/// An item's box: margins, padding plus border, and sizing; all zero and auto for an item
/// that is not a box.
struct Frame {
    /// Top, right, bottom, left.
    margin: [f32; 4],
    inset_x: f32,
    inset_y: f32,
    size: BoxSize,
}

impl Frame {
    // Out of line: a copy at each call costs WASM bytes and saves no measurable time.
    #[inline(never)]
    fn of(element: &Element<'_>) -> Self {
        let Element::Box(node) = element else {
            return Self {
                margin: [0.0; 4],
                inset_x: 0.0,
                inset_y: 0.0,
                size: BoxSize::AUTO,
            };
        };
        let style = &node.style;
        Self {
            margin: style.margin.into_array().map(Pt::get),
            inset_x: (style.padding.left
                + style.padding.right
                + style.border.left
                + style.border.right)
                .get(),
            inset_y: (style.padding.top
                + style.padding.bottom
                + style.border.top
                + style.border.bottom)
                .get(),
            size: node.size,
        }
    }

    /// The left and right margins, or the top and bottom ones.
    fn margins(&self, vertical: bool) -> f32 {
        let start = usize::from(!vertical);
        self.margin[start] + self.margin[start + 2]
    }

    /// A width as the border-box width it sets against `available`.
    fn width(&self, value: ImageSize, available: f32) -> Option<f32> {
        border_width(value, available, self.inset_x, self.size.border_box)
    }

    fn height(&self, value: Pt) -> f32 {
        border_size(value.get(), self.inset_y, self.size.border_box)
    }

    /// One `auto` margin bit: top 1, right 2, bottom 4, left 8.
    fn auto(&self, bit: u8) -> f32 {
        f32::from(u8::from(self.size.auto_margin & bit != 0))
    }
}

/// One item along the main axis, in margin-box sizes, with its box and laid-out output.
struct Item {
    frame: Frame,
    base: f32,
    min: f32,
    max: f32,
    grow: f32,
    shrink: f32,
    /// The inner flex base size, which scales the shrink factor.
    inner: f32,
    size: f32,
    frozen: bool,
    violation: f32,
    span: (Marks, Marks),
    height: f32,
}

impl Item {
    fn clamp(&self, value: f32) -> f32 {
        value.min(self.max).max(self.min)
    }
}

/// CSS Flexbox §9.7: grows or shrinks the items toward filling `space` and returns the
/// space they take.
fn resolve(items: &mut [Item], space: f32, grow: bool) -> f32 {
    let mut initial = space;
    for item in items.iter_mut() {
        item.size = item.clamp(item.base);
        let factor = if grow { item.grow } else { item.shrink };
        item.frozen = factor == 0.0
            || if grow {
                item.base > item.size
            } else {
                item.base < item.size
            };
        initial -= if item.frozen { item.size } else { item.base };
    }
    loop {
        let (mut factors, mut scaled, mut remaining, mut open) = (0.0, 0.0, space, false);
        for item in items.iter() {
            if item.frozen {
                remaining -= item.size;
            } else {
                open = true;
                factors += if grow { item.grow } else { item.shrink };
                scaled += item.shrink * item.inner;
                remaining -= item.base;
            }
        }
        if !open {
            return space - remaining;
        }
        if factors < 1.0 && (initial * factors).abs() < remaining.abs() {
            remaining = initial * factors;
        }
        let mut violation = 0.0;
        for item in items.iter_mut() {
            if !item.frozen {
                let share = if grow {
                    item.grow / factors
                } else if scaled > 0.0 {
                    item.shrink * item.inner / scaled
                } else {
                    0.0
                };
                let target = item.base + remaining * share;
                item.size = item.clamp(target);
                item.violation = item.size - target;
                violation += item.violation;
            }
        }
        for item in items.iter_mut() {
            item.frozen |= violation == 0.0 || item.violation * violation > 0.0;
        }
    }
}

/// Moves one item's output down by `dy`.
fn shift(output: &mut LayoutBuffer, (from, to): (Marks, Marks), dy: f32) {
    output.shift(from, to, Pt(dy));
}

/// Where an item sits in `free` space across the container.
fn offset(align: Align, free: f32) -> f32 {
    match align {
        Align::End => free,
        Align::Center => free / 2.0,
        _ => 0.0,
    }
}

/// Aligns a row's items across a line `cross` tall whose top is `top` below the container's.
fn align_row(
    node: &FlexNode<'_>,
    items: &[Item],
    row: Range<usize>,
    cross: f32,
    top: f32,
    output: &mut LayoutBuffer,
) -> Result<(), LayoutError> {
    let specs = node.items[row.clone()]
        .iter()
        .zip(&node.children[row.clone()]);
    for (item, (spec, child)) in items[row].iter().zip(specs) {
        let frame = &item.frame;
        let free = cross - item.height;
        if overflows(free) {
            return Err(LayoutError::ContentOverflow);
        }
        let autos = frame.auto(1) + frame.auto(4);
        let align = spec.align.unwrap_or(node.style.align);
        let dy = if autos > 0.0 {
            free * frame.auto(1) / autos
        } else {
            offset(align, free)
        };
        // §9.4 step 11: an item with an auto height stretches to the line, up to its max-height.
        if let Element::Box(node) = child {
            // A positioned item paints in its own layer, which does not stretch.
            let auto = node.size.height.is_none() && autos == 0.0 && node.size.position.is_none();
            if align == Align::Stretch && auto && painted(&node.style) {
                let room = node.size.max_height.map_or(free, |max| {
                    frame.height(max) + frame.margins(true) - item.height
                });
                output.boxes[item.span.0.boxes].height += Pt(free.min(room).max(0.0));
            }
        }
        shift(output, item.span, top + dy);
    }
    Ok(())
}

/// `justify-content` or `align-content` over `count` items or rows: the space before the
/// first and the space between each two.
fn distribute(justify: Justify, free: f32, count: f32) -> (f32, f32) {
    match justify {
        Justify::Start => (0.0, 0.0),
        Justify::End => (free, 0.0),
        Justify::Center => (free / 2.0, 0.0),
        Justify::SpaceBetween if count > 1.0 => (0.0, free / (count - 1.0)),
        Justify::SpaceBetween => (0.0, 0.0),
        Justify::SpaceAround => (free / count / 2.0, free / count),
        Justify::SpaceEvenly => (free / (count + 1.0), free / (count + 1.0)),
    }
}

/// §9.3: items fill a row until the next one's hypothetical outer width would pass `width`.
fn breaks(items: &[Item], gap: f32, width: f32) -> Vec<Range<usize>> {
    let (mut rows, mut start, mut used) = (Vec::new(), 0, 0.0);
    for (index, item) in items.iter().enumerate() {
        let size = item.clamp(item.base);
        if index == start {
            used = size;
        } else if overflows(width - (used + gap + size)) {
            rows.push(start..index);
            (start, used) = (index, size);
        } else {
            used += gap + size;
        }
    }
    rows.push(start..items.len());
    rows
}

/// A row container's items, measured once, and the range of each row they wrap into.
pub(crate) struct FlexRows {
    items: Vec<Item>,
    pub(crate) rows: Vec<Range<usize>>,
}

impl LayoutEngine<'_> {
    pub(super) fn layout_flex(
        &self,
        node: &FlexNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let flex = node.style;
        if flex.column {
            return self.layout_column(node, area, output);
        }
        let FlexRows { mut items, rows } = self.flex_rows(node, area.width)?;
        let mut heights = Vec::with_capacity(rows.len());
        for row in &rows {
            heights.push(self.place_row(node, &mut items, row.clone(), area, output)?);
        }
        let count = rows.len() as f32;
        let total = heights.iter().fold(0.0, |sum, height| sum + height)
            + flex.row_gap.get() * (count - 1.0);
        // The rows are as tall as they need, within the container's height bounds.
        let max_height = flex.max_height.map_or(f32::INFINITY, Pt::get);
        let cross = total.min(max_height).max(flex.min_height.get());
        if !flex.wrap {
            align_row(node, &items, rows[0].clone(), cross, 0.0, output)?;
            return Ok(Pt(cross));
        }
        let free = cross - total;
        if overflows(free) {
            return Err(LayoutError::ContentOverflow);
        }
        let free = free.max(0.0);
        // §9.4 step 15: `stretch`, also `normal`, shares the free space among the rows.
        let (mut top, between, extra) = match flex.align_content {
            Some(justify) => {
                let (start, between) = distribute(justify, free, count);
                (start, between, 0.0)
            }
            None => (0.0, 0.0, free / count),
        };
        for (row, height) in rows.into_iter().zip(heights) {
            align_row(node, &items, row, height + extra, top, output)?;
            top += height + extra + between + flex.row_gap.get();
        }
        Ok(Pt(cross))
    }

    /// Measures a row container's items and wraps them into rows when it wraps.
    pub(crate) fn flex_rows(
        &self,
        node: &FlexNode<'_>,
        width: Pt,
    ) -> Result<FlexRows, LayoutError> {
        let width = width.get();
        let mut items = Vec::with_capacity(node.children.len());
        for (spec, child) in node.items.iter().zip(&node.children) {
            let frame = Frame::of(child);
            let margins = frame.margins(false);
            let (min_content, max_content) = self.content(child, &frame)?;
            let outer = |value| frame.width(value, width).map(|value| value + margins);
            let max = outer(frame.size.max_width);
            let specified = outer(frame.size.width);
            let base = match spec.basis {
                ImageSize::Auto => specified.unwrap_or(max_content),
                basis => outer(basis).unwrap_or(0.0),
            };
            // §4.5: an automatic minimum is the min-content width, capped by `width` and `max-width`.
            let min = outer(frame.size.min_width).unwrap_or_else(|| {
                let min = specified.map_or(min_content, |value| value.min(min_content));
                max.map_or(min, |max| min.min(max))
            });
            items.push(Item {
                inner: (base - margins - frame.inset_x).max(0.0),
                frame,
                base,
                min,
                max: max.unwrap_or(f32::INFINITY),
                grow: spec.grow,
                shrink: spec.shrink,
                size: base,
                frozen: false,
                violation: 0.0,
                span: Default::default(),
                height: 0.0,
            });
        }
        let rows = if node.style.wrap {
            breaks(&items, node.style.gap.get(), width)
        } else {
            std::iter::once(0..items.len()).collect()
        };
        Ok(FlexRows { items, rows })
    }

    /// Lays out row `index` of a wrapped container on its own, as tall as its tallest item, so
    /// the page splitter can break between rows.
    pub(crate) fn layout_flex_row(
        &self,
        node: &FlexNode<'_>,
        rows: &mut FlexRows,
        index: usize,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let row = rows.rows[index].clone();
        let before = self.absolutes.get();
        let height = self.place_row(node, &mut rows.items, row.clone(), area, output)?;
        align_row(node, &rows.items, row.clone(), height, 0.0, output)?;
        if self.absolutes.get() != before {
            self.place_absolutes(&node.children[row], None, output)?;
        }
        Ok(Pt(height))
    }

    /// Sizes and places one row's items along it at the top of `area`; returns the tallest
    /// item's margin-box height.
    fn place_row(
        &self,
        node: &FlexNode<'_>,
        items: &mut [Item],
        row: Range<usize>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<f32, LayoutError> {
        let flex = node.style;
        let width = area.width.get();
        let gap = flex.gap.get();
        let items = &mut items[row.clone()];
        let gaps = gap * (items.len() - 1) as f32;
        let (mut hypothetical, mut autos) = (gaps, 0.0);
        for item in items.iter() {
            autos += item.frame.auto(8) + item.frame.auto(2);
            hypothetical += item.clamp(item.base);
        }
        let used = resolve(items, width - gaps, hypothetical < width);
        let free = width - gaps - used;
        if overflows(free) {
            return Err(LayoutError::ColumnsOverflow);
        }
        let free = free.max(0.0);
        // §8.1: auto margins take the free space before `justify-content` sees it.
        let (mut position, between, each) = if autos > 0.0 {
            (0.0, 0.0, free / autos)
        } else {
            let (start, between) = distribute(flex.justify, free, items.len() as f32);
            (start, between, 0.0)
        };
        let mut tallest = 0.0;
        for (item, child) in items.iter_mut().zip(&node.children[row]) {
            position += each * item.frame.auto(8);
            let from = output.marks();
            let border = item.size - item.frame.margins(false);
            item.height = self.item(child, area, position, border, &item.frame, output)?;
            item.span = (from, output.marks());
            tallest = f32::max(tallest, item.height);
            position += item.size + each * item.frame.auto(2) + between + gap;
        }
        Ok(tallest)
    }

    /// A column places each item across, then stacks the heights it laid out; its items do
    /// not flex.
    fn layout_column(
        &self,
        node: &FlexNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let flex = node.style;
        let width = area.width.get();
        let gap = flex.gap.get();
        let gaps = gap * (node.children.len() - 1) as f32;
        let (mut hypothetical, mut autos) = (gaps, 0.0);
        let mut placed = Vec::with_capacity(node.children.len());
        for (spec, child) in node.items.iter().zip(&node.children) {
            let frame = Frame::of(child);
            autos += frame.auto(1) + frame.auto(4);
            let align = spec.align.unwrap_or(flex.align);
            let across = frame.auto(8) + frame.auto(2);
            let fill = width - frame.margins(false);
            let border = match frame.width(frame.size.width, width) {
                Some(value) => value,
                None if align == Align::Stretch && across == 0.0 => fill,
                None => {
                    let (min, max) = self.content(child, &frame)?;
                    max.min(width).max(min) - frame.margins(false)
                }
            };
            let border = frame
                .width(frame.size.max_width, width)
                .map_or(border, |max| border.min(max));
            let border = frame
                .width(frame.size.min_width, width)
                .map_or(border, |min| border.max(min));
            let free = fill - border;
            if overflows(free) {
                return Err(LayoutError::ColumnsOverflow);
            }
            let x = if across > 0.0 {
                free * frame.auto(8) / across
            } else {
                offset(align, free)
            };
            let from = output.marks();
            let height = self.item(child, area, x, border, &frame, output)?;
            hypothetical += height;
            placed.push((frame, (from, output.marks()), height));
        }
        let max_height = flex.max_height.map_or(f32::INFINITY, Pt::get);
        let main = hypothetical.min(max_height).max(flex.min_height.get());
        let free = main - gaps - (hypothetical - gaps);
        if overflows(free) {
            return Err(LayoutError::ContentOverflow);
        }
        let free = free.max(0.0);
        let (mut position, between, each) = if autos > 0.0 {
            (0.0, 0.0, free / autos)
        } else {
            let (start, between) = distribute(flex.justify, free, placed.len() as f32);
            (start, between, 0.0)
        };
        for (frame, span, height) in placed {
            position += each * frame.auto(1);
            shift(output, span, position);
            position += height + each * frame.auto(4) + between + gap;
        }
        Ok(Pt(main))
    }

    /// Lays out one item at `x` with a border-box width; returns its margin-box height.
    fn item(
        &self,
        child: &Element<'_>,
        area: LayoutArea,
        x: f32,
        border: f32,
        frame: &Frame,
        output: &mut LayoutBuffer,
    ) -> Result<f32, LayoutError> {
        let place = area
            .translated(Pt(x), Pt::ZERO)
            .with_width(Pt(border + frame.margins(false)));
        Ok(match child {
            Element::Box(node) => self.layout_box(
                node,
                (node.size, node.style.clone()),
                place,
                Some(Pt(border)),
                output,
            )?,
            _ => self.layout(child, place, output)?,
        }
        .get())
    }

    /// Min-content and max-content widths of an item's margin box, ignoring its own `width`.
    fn content(&self, element: &Element<'_>, frame: &Frame) -> Result<(f32, f32), LayoutError> {
        let Element::Box(node) = element else {
            return self.intrinsic(element);
        };
        let (min, max) = self.contents(&node.children)?;
        let extra = frame.inset_x + frame.margins(false);
        Ok((min + extra, max + extra))
    }

    /// Min-content and max-content widths of the element's margin box.
    pub(super) fn intrinsic(&self, element: &Element<'_>) -> Result<(f32, f32), LayoutError> {
        Ok(match element {
            Element::Paragraph(paragraph) => {
                let mut scratch = LayoutBuffer::default();
                let unbounded = LayoutArea::root(Pt(f32::MAX));
                let (_, widest) = text::ParagraphLayouter::layout(
                    self.fonts,
                    paragraph,
                    unbounded,
                    &mut scratch,
                )?;
                let longest = scratch
                    .lines
                    .iter()
                    .map(|line| line.text_width.get())
                    .fold(0.0, f32::max);
                // Rounding up keeps a content-sized box from wrapping on float error.
                (widest.get() + TOLERANCE, longest + TOLERANCE)
            }
            Element::Image(node) => {
                let image = self
                    .images
                    .get(usize::from(node.slot))
                    .ok_or(LayoutError::UnknownImage)?;
                let ratio = image.width as f32 / image.height as f32;
                let natural = match node.height {
                    Some(height) => height.get() * ratio,
                    None => image.width as f32 * super::PT_PER_PX,
                };
                let natural = node
                    .max_height
                    .map_or(natural, |max| natural.min(max.get() * ratio));
                let natural = match node.max_width {
                    ImageSize::Fixed(max) => natural.min(max.get()),
                    _ => natural,
                };
                match node.width {
                    ImageSize::Fixed(width) => (width.get(), width.get()),
                    // CSS Sizing §5.2.2: a percent width can shrink to nothing.
                    ImageSize::Percent(_) => (0.0, natural),
                    ImageSize::Auto => (natural, natural),
                }
            }
            Element::Spacer(_) | Element::Anchor(_) => (0.0, 0.0),
            // Out of flow, an absolute or fixed box takes no width from its container.
            Element::Box(node)
                if node
                    .size
                    .position
                    .is_some_and(|position| position.kind != crate::PositionKind::Relative) =>
            {
                (0.0, 0.0)
            }
            Element::Box(node) => {
                let frame = Frame::of(element);
                let fixed = |value| match value {
                    ImageSize::Fixed(_) => frame.width(value, 0.0),
                    _ => None,
                };
                let (mut min, mut max) = match fixed(frame.size.width) {
                    Some(width) => (width, width),
                    None => {
                        let (min, max) = self.contents(&node.children)?;
                        (min + frame.inset_x, max + frame.inset_x)
                    }
                };
                if let Some(cap) = fixed(frame.size.max_width) {
                    (min, max) = (min.min(cap), max.min(cap));
                }
                if let Some(floor) = fixed(frame.size.min_width) {
                    (min, max) = (min.max(floor), max.max(floor));
                }
                let margins = frame.margins(false);
                (min + margins, max + margins)
            }
            Element::Stack(stack) => self.contents(&stack.children)?,
            Element::Grid(node) => self.grid_intrinsic(node)?,
            Element::Table(table) => match table.width {
                ColumnWidth::Fixed(width) => (width.get(), width.get()),
                _ => self.contents(&table.rows)?,
            },
            Element::Flex(node) if node.style.column => self.contents(&node.children)?,
            // Row items sit side by side, so their widths add up.
            Element::Flex(node) => {
                let gaps = node.style.gap.get() * (node.children.len() - 1) as f32;
                // A wrapping row can put each item on its own row, so its widest item is its minimum.
                let wrap = node.style.wrap;
                let mut sum = (if wrap { 0.0 } else { gaps }, gaps);
                for child in &node.children {
                    let (min, max) = self.intrinsic(child)?;
                    let min = if wrap {
                        f32::max(sum.0, min)
                    } else {
                        sum.0 + min
                    };
                    sum = (min, sum.1 + max);
                }
                sum
            }
            Element::Row(row) => {
                let mut sum = (0.0, 0.0);
                for cell in &row.cells {
                    let (min, max) = match row.columns[cell.column] {
                        ColumnWidth::Fixed(width) => (width.get(), width.get()),
                        _ => self.intrinsic(&cell.content)?,
                    };
                    sum = (sum.0 + min, sum.1 + max);
                }
                sum
            }
        })
    }

    /// The widest min-content and max-content widths among stacked children.
    pub(super) fn contents(&self, children: &[Element<'_>]) -> Result<(f32, f32), LayoutError> {
        let mut widest = (0.0, 0.0);
        for child in children {
            let (min, max) = self.intrinsic(child)?;
            widest = (f32::max(widest.0, min), f32::max(widest.1, max));
        }
        Ok(widest)
    }
}
