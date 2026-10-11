//! A single-line CSS flex container: §9.7 sizes the items along the main axis, §8.1 and
//! §9.5 place them, and §8.3 and §9.4 align them across.

use crate::document::{Element, FlexNode};
use crate::geometry::LayoutArea;
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
        let style = node.style;
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
struct Line {
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
    span: (Mark, Mark),
    height: f32,
}

impl Line {
    fn clamp(&self, value: f32) -> f32 {
        value.min(self.max).max(self.min)
    }
}

/// CSS Flexbox §9.7: grows or shrinks the items toward filling `space` and returns the
/// space they take.
fn resolve(lines: &mut [Line], space: f32, grow: bool) -> f32 {
    let mut initial = space;
    for line in lines.iter_mut() {
        line.size = line.clamp(line.base);
        let factor = if grow { line.grow } else { line.shrink };
        line.frozen = factor == 0.0
            || if grow {
                line.base > line.size
            } else {
                line.base < line.size
            };
        initial -= if line.frozen { line.size } else { line.base };
    }
    loop {
        let (mut factors, mut scaled, mut remaining, mut open) = (0.0, 0.0, space, false);
        for line in lines.iter() {
            if line.frozen {
                remaining -= line.size;
            } else {
                open = true;
                factors += if grow { line.grow } else { line.shrink };
                scaled += line.shrink * line.inner;
                remaining -= line.base;
            }
        }
        if !open {
            return space - remaining;
        }
        if factors < 1.0 && (initial * factors).abs() < remaining.abs() {
            remaining = initial * factors;
        }
        let mut violation = 0.0;
        for line in lines.iter_mut() {
            if !line.frozen {
                let share = if grow {
                    line.grow / factors
                } else if scaled > 0.0 {
                    line.shrink * line.inner / scaled
                } else {
                    0.0
                };
                let target = line.base + remaining * share;
                line.size = line.clamp(target);
                line.violation = line.size - target;
                violation += line.violation;
            }
        }
        for line in lines.iter_mut() {
            line.frozen |= violation == 0.0 || line.violation * violation > 0.0;
        }
    }
}

/// Where an item's output starts or ends: lines, boxes, and images.
type Mark = (usize, usize, usize);

fn mark(output: &LayoutBuffer) -> Mark {
    (output.lines.len(), output.boxes.len(), output.images.len())
}

/// Moves one item's output down by `dy`.
fn shift(output: &mut LayoutBuffer, (from, to): (Mark, Mark), dy: f32) {
    let dy = Pt(dy);
    for line in &mut output.lines[from.0..to.0] {
        line.origin.y += dy;
        line.top += dy;
        line.bottom += dy;
    }
    for paint in &mut output.boxes[from.1..to.1] {
        paint.origin.y += dy;
    }
    for image in &mut output.images[from.2..to.2] {
        image.origin.y += dy;
    }
}

/// Where an item sits in `free` space across the container.
fn offset(align: Align, free: f32) -> f32 {
    match align {
        Align::End => free,
        Align::Center => free / 2.0,
        _ => 0.0,
    }
}

impl LayoutEngine<'_> {
    pub(super) fn layout_flex(
        &self,
        node: &FlexNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let flex = node.style;
        let column = flex.column;
        // `auto` margin bits at the main axis's start and end.
        let (start, end) = if column { (1, 4) } else { (8, 2) };
        let width = area.width.get();
        let gap = flex.gap.get();
        let gaps = gap * (node.children.len() - 1) as f32;
        let (mut hypothetical, mut autos) = (gaps, 0.0);
        let mut lines = Vec::with_capacity(node.children.len());
        for (item, child) in node.items.iter().zip(&node.children) {
            let frame = Frame::of(child);
            autos += frame.auto(start) + frame.auto(end);
            let margins = frame.margins(column);
            let mut span = <(Mark, Mark)>::default();
            let mut height = 0.0;
            let (base, min, max) = if column {
                // A column places each item across, then stacks the heights it laid out; its
                // items do not flex.
                let align = item.align.unwrap_or(flex.align);
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
                let from = mark(output);
                height = self.item(child, area, x, border, &frame, output)?;
                span = (from, mark(output));
                (height, height, height)
            } else {
                let (min_content, max_content) = self.content(child, &frame)?;
                let outer = |value| frame.width(value, width).map(|value| value + margins);
                let max = outer(frame.size.max_width);
                let specified = outer(frame.size.width);
                let base = match item.basis {
                    ImageSize::Auto => specified.unwrap_or(max_content),
                    basis => outer(basis).unwrap_or(0.0),
                };
                // §4.5: an automatic minimum is the min-content width, capped by `width` and `max-width`.
                let min = outer(frame.size.min_width).unwrap_or_else(|| {
                    let min = specified.map_or(min_content, |value| value.min(min_content));
                    max.map_or(min, |max| min.min(max))
                });
                (base, min, max.unwrap_or(f32::INFINITY))
            };
            let line = Line {
                inner: (base - margins - frame.inset_x).max(0.0),
                frame,
                base,
                min,
                max,
                grow: item.grow,
                shrink: item.shrink,
                size: base,
                frozen: false,
                violation: 0.0,
                span,
                height,
            };
            hypothetical += line.clamp(base);
            lines.push(line);
        }
        let max_height = flex.max_height.map_or(f32::INFINITY, Pt::get);
        let main = if column {
            hypothetical.min(max_height).max(flex.min_height.get())
        } else {
            width
        };
        let used = if column {
            hypothetical - gaps
        } else {
            resolve(&mut lines, main - gaps, hypothetical < main)
        };
        let free = main - gaps - used;
        if overflows(free) {
            return Err(if column {
                LayoutError::ContentOverflow
            } else {
                LayoutError::ColumnsOverflow
            });
        }
        let free = free.max(0.0);
        let count = lines.len() as f32;
        // §8.1: auto margins take the free space before `justify-content` sees it.
        let (mut position, between, each) = match flex.justify {
            _ if autos > 0.0 => (0.0, 0.0, free / autos),
            Justify::Start => (0.0, 0.0, 0.0),
            Justify::End => (free, 0.0, 0.0),
            Justify::Center => (free / 2.0, 0.0, 0.0),
            Justify::SpaceBetween if count > 1.0 => (0.0, free / (count - 1.0), 0.0),
            Justify::SpaceBetween => (0.0, 0.0, 0.0),
            Justify::SpaceAround => (free / count / 2.0, free / count, 0.0),
            Justify::SpaceEvenly => (free / (count + 1.0), free / (count + 1.0), 0.0),
        };
        let mut tallest = 0.0;
        for (line, child) in lines.iter_mut().zip(&node.children) {
            position += each * line.frame.auto(start);
            if column {
                shift(output, line.span, position);
            } else {
                let from = mark(output);
                let border = line.size - line.frame.margins(false);
                line.height = self.item(child, area, position, border, &line.frame, output)?;
                line.span = (from, mark(output));
                tallest = f32::max(tallest, line.height);
            }
            position += line.size + each * line.frame.auto(end) + between + gap;
        }
        if column {
            return Ok(Pt(main));
        }
        // A row's line is as tall as its tallest item, within the container's height bounds.
        let cross = tallest.min(max_height).max(flex.min_height.get());
        for (line, (item, child)) in lines.iter().zip(node.items.iter().zip(&node.children)) {
            let frame = &line.frame;
            let free = cross - line.height;
            if overflows(free) {
                return Err(LayoutError::ContentOverflow);
            }
            let autos = frame.auto(1) + frame.auto(4);
            let align = item.align.unwrap_or(flex.align);
            let dy = if autos > 0.0 {
                free * frame.auto(1) / autos
            } else {
                offset(align, free)
            };
            // §9.4 step 11: an item with an auto height stretches to the line, up to its max-height.
            if let Element::Box(node) = child {
                let auto = node.size.height.is_none() && autos == 0.0;
                if align == Align::Stretch && auto && painted(&node.style) {
                    let room = node.size.max_height.map_or(free, |max| {
                        frame.height(max) + frame.margins(true) - line.height
                    });
                    output.boxes[line.span.0 .1].height += Pt(free.min(room).max(0.0));
                }
            }
            shift(output, line.span, dy);
        }
        Ok(Pt(cross))
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
            Element::Box(node) => self.layout_box(node, place, Some(Pt(border)), output)?,
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
    fn intrinsic(&self, element: &Element<'_>) -> Result<(f32, f32), LayoutError> {
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
                let natural = match node.height {
                    Some(height) => height.get() * image.width as f32 / image.height as f32,
                    None => image.width as f32 * super::PT_PER_PX,
                };
                match node.width {
                    ImageSize::Fixed(width) => (width.get(), width.get()),
                    // CSS Sizing §5.2.2: a percent width can shrink to nothing.
                    ImageSize::Percent(_) => (0.0, natural),
                    ImageSize::Auto => (natural, natural),
                }
            }
            Element::Spacer(_) => (0.0, 0.0),
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
            Element::Table(table) => match table.width {
                ColumnWidth::Fixed(width) => (width.get(), width.get()),
                _ => self.contents(&table.rows)?,
            },
            Element::Flex(node) if node.style.column => self.contents(&node.children)?,
            // Row items sit side by side, so their widths add up.
            Element::Flex(node) => {
                let gaps = node.style.gap.get() * (node.children.len() - 1) as f32;
                let mut sum = (gaps, gaps);
                for child in &node.children {
                    let (min, max) = self.intrinsic(child)?;
                    sum = (sum.0 + min, sum.1 + max);
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
    fn contents(&self, children: &[Element<'_>]) -> Result<(f32, f32), LayoutError> {
        let mut widest = (0.0, 0.0);
        for child in children {
            let (min, max) = self.intrinsic(child)?;
            widest = (f32::max(widest.0, min), f32::max(widest.1, max));
        }
        Ok(widest)
    }
}
