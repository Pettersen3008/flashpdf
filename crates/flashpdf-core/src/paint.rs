use std::ops::Range;

use std::f32::consts::FRAC_PI_4;

use pdf_writer::types::LineCapStyle;
use pdf_writer::{Content, Name, Str};

use crate::font::font_name;
use crate::geometry::Point;
use crate::layout::{LayoutBuffer, Segment};
use crate::vector::{alpha_name, dash, op};
use crate::{BoxStyle, LineStyle, Pt, Radii, Rgb, TextAlign, OPAQUE};

type Corners = [[f32; 2]; 4];

/// A `/Link` annotation rectangle in page coordinates: `[x1, y1, x2, y2]`.
pub(crate) struct PageLink {
    pub(crate) rect: [f32; 4],
    pub(crate) uri: String,
}

pub(crate) enum PageTag {
    Paragraph,
    Figure(String),
}

pub(crate) struct PdfPainter<'a> {
    content: &'a mut Content,
    links: &'a mut Vec<PageLink>,
    tags: Option<&'a mut Vec<PageTag>>,
    /// Alphas this page's `/ExtGState` must define.
    alphas: &'a mut Vec<[u8; 2]>,
    /// The fill and stroke alpha in force; every `paint` ends opaque.
    alpha: u8,
}

impl<'a> PdfPainter<'a> {
    pub(crate) fn new(
        content: &'a mut Content,
        links: &'a mut Vec<PageLink>,
        tags: Option<&'a mut Vec<PageTag>>,
        alphas: &'a mut Vec<[u8; 2]>,
    ) -> Self {
        Self {
            content,
            links,
            tags,
            alphas,
            alpha: OPAQUE,
        }
    }

    #[inline(never)]
    fn set_alpha(&mut self, alpha: u8) {
        if alpha != self.alpha {
            self.alpha = alpha;
            if !self.alphas.contains(&[alpha; 2]) {
                self.alphas.push([alpha; 2]);
            }
            self.content
                .set_parameters(Name(&alpha_name([alpha, alpha])));
        }
    }

    pub(crate) fn paint(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        let mut ids = if self.tags.is_some() {
            vec![None; lines.len()]
        } else {
            Vec::new()
        };
        if let Some(tags) = &mut self.tags {
            let mut images = layout.images.iter().peekable();
            for line in lines.clone() {
                while images.peek().is_some_and(|image| image.line < line) {
                    images.next();
                }
                let image = images.peek().filter(|image| image.line == line);
                let tag = if let Some(image) = image {
                    (!image.alt.is_empty()).then(|| PageTag::Figure(image.alt.clone()))
                } else if !layout.lines[line].segments.is_empty() {
                    Some(PageTag::Paragraph)
                } else {
                    None
                };
                if let Some(tag) = tag {
                    ids[line - lines.start] = Some(tags.len() as i32);
                    tags.push(tag);
                }
            }
        }
        if self.tags.is_some() {
            self.content.begin_marked_content(Name(b"Artifact"));
        }
        self.paint_boxes(layout, origin);
        if self.tags.is_some() {
            self.content.end_marked_content();
        }
        self.paint_images(layout, lines.clone(), origin, &ids);
        self.paint_text(layout, lines, origin, &ids);
        self.set_alpha(OPAQUE);
    }

    fn paint_boxes(&mut self, layout: &LayoutBuffer, origin: Point) {
        for paint in &layout.boxes {
            let style = &paint.style;
            let x = origin.x.get() + paint.origin.x.get();
            let y = origin.y.get() - paint.origin.y.get() - paint.height.get();
            let (width, height) = (paint.width.get(), paint.height.get());
            self.set_alpha(style.alpha);
            // Four overlapping rectangles would composite their corners twice under an alpha.
            if style.radius.is_some()
                || (style.alpha != OPAQUE
                    && style.uniform_border()
                    && style.border_style.top == LineStyle::Solid)
            {
                self.paint_outline(style, [x, y, width, height]);
                continue;
            }
            if let Some(color) = style.background {
                self.set_fill(color);
                self.content.rect(x, y, width, height).fill_nonzero();
            }
            let [top, right, bottom, left] = style.border.into_array().map(|edge| edge.get());
            for (index, ((color, line), (x, y, width, height))) in style
                .border_color
                .into_array()
                .into_iter()
                .zip(style.border_style.into_array())
                .zip([
                    (x, y + height - top, width, top),
                    (x + width - right, y, right, height),
                    (x, y, width, bottom),
                    (x, y, left, height),
                ])
                .enumerate()
            {
                if width <= 0.0 || height <= 0.0 {
                    continue;
                }
                if line == LineStyle::Solid {
                    self.set_fill(color);
                    self.content.rect(x, y, width, height).fill_nonzero();
                } else if index % 2 == 0 {
                    // Top and bottom run the full width, so the corners belong to them.
                    let cross = y + height / 2.0;
                    self.dashed_side(color, line, height, true, cross, x..x + width);
                } else {
                    // Left and right run between the top and bottom borders.
                    let cross = x + width / 2.0;
                    self.dashed_side(
                        color,
                        line,
                        width,
                        false,
                        cross,
                        y + bottom..y + height - top,
                    );
                }
            }
        }
    }

    /// A box with rounded corners, or a uniform border under an alpha: the background fills the
    /// outline, and the border strokes the outline's centre line, which is its own width inside.
    #[inline(never)]
    fn paint_outline(&mut self, style: &BoxStyle, rect: [f32; 4]) {
        let [x, y, width, height] = rect;
        let corners = corners(style.radius, width, height);
        if let Some(color) = style.background {
            self.set_fill(color);
            outline(self.content, rect, corners);
            op(self.content, "f", &[]);
        }
        let border = style.border.top.get();
        if border > 0.0 {
            let half = border / 2.0;
            let middle = [x + half, y + half, width - border, height - border];
            let corners = shrink(corners, half);
            let length = perimeter(middle, corners);
            let (color, line) = (style.border_color.top, style.border_style.top);
            self.begin_stroke(color, line, border, length, true);
            outline(self.content, middle, corners);
            self.content.stroke().restore_state();
        }
    }

    /// One dashed or dotted side as a line along `span` at `cross`, which is y when `horizontal`.
    #[inline(never)]
    fn dashed_side(
        &mut self,
        color: Rgb,
        line: LineStyle,
        width: f32,
        horizontal: bool,
        cross: f32,
        span: Range<f32>,
    ) {
        // A dot's edge, not its centre, meets the end of the side.
        let inset = if line == LineStyle::Dotted {
            width / 2.0
        } else {
            0.0
        };
        let start = span.start + inset;
        let end = (span.end - inset).max(start);
        let point = |along: f32| {
            if horizontal {
                [along, cross]
            } else {
                [cross, along]
            }
        };
        self.begin_stroke(color, line, width, end - start, false);
        op(self.content, "m", &point(start));
        op(self.content, "l", &point(end));
        self.content.stroke().restore_state();
    }

    /// Opens a `q` that the caller closes after drawing a path `length` long. Browsers draw
    /// dashes two or three widths long and dots as round-capped zero-length dashes; the gap
    /// stretches so whole dashes fill the path.
    #[inline(never)]
    fn begin_stroke(&mut self, color: Rgb, line: LineStyle, width: f32, length: f32, closed: bool) {
        let content = &mut *self.content;
        content.save_state();
        op(content, "RG", &color.normalized());
        op(content, "w", &[width]);
        if line == LineStyle::Solid {
            return;
        }
        let (dash_length, gap) = match line {
            LineStyle::Dotted => (0.0, 2.0 * width),
            _ if width >= 3.0 => (2.0 * width, width),
            _ => (3.0 * width, 2.0 * width),
        };
        if line == LineStyle::Dotted {
            content.set_line_cap(LineCapStyle::RoundCap);
        }
        dash(
            content,
            &[dash_length, fit(length, dash_length, gap, closed)],
            0.0,
        );
    }

    /// `q w 0 0 h x y cm /ImN Do Q`; only images whose pseudo-line is in range.
    fn paint_images(
        &mut self,
        layout: &LayoutBuffer,
        lines: Range<usize>,
        origin: Point,
        ids: &[Option<i32>],
    ) {
        for image in layout
            .images
            .iter()
            .filter(|image| lines.contains(&image.line))
        {
            self.set_alpha(image.alpha);
            if let Some(id) = ids.get(image.line - lines.start).copied().flatten() {
                self.content
                    .begin_marked_content_with_properties(Name(b"Figure"))
                    .properties()
                    .identify(id);
            } else if self.tags.is_some() {
                self.content.begin_marked_content(Name(b"Artifact"));
            }
            let (width, height) = (image.width.get(), image.height.get());
            let x = origin.x.get() + image.origin.x.get();
            let y = origin.y.get() - image.origin.y.get() - height;
            let mut buffer = [0_u8; 8];
            self.content
                .save_state()
                .transform([width, 0.0, 0.0, height, x, y])
                .x_object(Name(image_name(image.slot, &mut buffer)))
                .restore_state();
            if let Some(link) = image.link {
                self.links.push(PageLink {
                    rect: [x, y, x + width, y + height],
                    uri: layout.links[link].clone(),
                });
            }
            if self.tags.is_some() {
                self.content.end_marked_content();
            }
        }
    }

    /// One text object per line; `Tf` and `rg` change only where the style does.
    /// Underlines, strike-throughs, and link rectangles follow per segment.
    fn paint_text(
        &mut self,
        layout: &LayoutBuffer,
        lines: Range<usize>,
        origin: Point,
        ids: &[Option<i32>],
    ) {
        for (offset, line) in layout.lines[lines].iter().enumerate() {
            let segments = &layout.segments[line.segments.clone()];
            if segments.is_empty() {
                continue;
            }
            if let Some(id) = ids.get(offset).copied().flatten() {
                self.content
                    .begin_marked_content_with_properties(Name(b"P"))
                    .properties()
                    .identify(id);
            }
            self.content.begin_text();
            let x = origin.x
                + aligned(
                    line.origin.x,
                    line.align,
                    line.available_width,
                    line.text_width,
                );
            let baseline = (origin.y - line.origin.y).get();
            let mut previous: Option<&Segment> = None;
            for (index, segment) in segments.iter().enumerate() {
                self.set_alpha(layout.decorations[line.segments.start + index].alpha);
                if previous.is_none_or(|last| last.color != segment.color) {
                    self.set_fill(segment.color);
                }
                if previous
                    .is_none_or(|last| last.font != segment.font || last.size != segment.size)
                {
                    let mut buffer = [0_u8; 4];
                    self.content.set_font(
                        Name(font_name(segment.font, &mut buffer)),
                        segment.size.get(),
                    );
                }
                match previous {
                    None => self.content.next_line(x.get(), baseline),
                    Some(last) => self.content.next_line((segment.x - last.x).get(), 0.0),
                };
                self.content.show(Str(&layout.text[segment.text.clone()]));
                previous = Some(segment);
            }
            self.content.end_text();
            for (index, segment) in segments.iter().enumerate() {
                let decoration = &layout.decorations[line.segments.start + index];
                let start = (x + segment.x).get();
                let end = segments
                    .get(index + 1)
                    .map_or(line.text_width, |next| next.x);
                let width = (end - segment.x).get();
                let size = segment.size.get();
                let thickness = size / 14.0;
                if decoration.underline || decoration.line_through {
                    self.set_alpha(decoration.alpha);
                    self.set_fill(segment.color);
                }
                if decoration.underline {
                    self.content
                        .rect(start, baseline - size / 8.0 - thickness, width, thickness)
                        .fill_nonzero();
                }
                if decoration.line_through {
                    self.content
                        .rect(
                            start,
                            baseline + size * 0.3 - thickness / 2.0,
                            width,
                            thickness,
                        )
                        .fill_nonzero();
                }
                if let Some(link) = decoration.link {
                    self.links.push(PageLink {
                        rect: [
                            start,
                            baseline - decoration.descent.get(),
                            start + width,
                            baseline + decoration.ascent.get(),
                        ],
                        uri: layout.links[link].clone(),
                    });
                }
            }
            if self.tags.is_some() {
                self.content.end_marked_content();
            }
        }
    }

    pub(crate) fn paint_line(
        &mut self,
        text: &[u8],
        style: crate::TextStyle,
        origin: Point,
        available_width: Pt,
        text_width: Pt,
    ) {
        self.content.begin_text();
        self.set_fill(style.color);
        let mut buffer = [0_u8; 4];
        self.content
            .set_font(Name(font_name(style.font, &mut buffer)), style.size.get());
        let x = aligned(origin.x, style.align, available_width, text_width);
        self.content.next_line(x.get(), origin.y.get());
        self.content.show(Str(text));
        self.content.end_text();
    }

    fn set_fill(&mut self, color: Rgb) {
        let [r, g, b] = color.normalized();
        self.content.set_fill_rgb(r, g, b);
    }
}

fn aligned(x: Pt, align: TextAlign, available_width: Pt, text_width: Pt) -> Pt {
    match align {
        TextAlign::Left => x,
        TextAlign::Center => x + Pt((available_width - text_width).get() / 2.0),
        TextAlign::Right => x + available_width - text_width,
    }
}

/// `/Im1` is slot 0, mirroring `font_name`.
pub(crate) fn image_name(slot: u16, buffer: &mut [u8; 8]) -> &[u8] {
    let mut digits = itoa::Buffer::new();
    let number = digits.format(u32::from(slot) + 1);
    buffer[..2].copy_from_slice(b"Im");
    buffer[2..2 + number.len()].copy_from_slice(number.as_bytes());
    &buffer[..2 + number.len()]
}

/// Radii as `[horizontal, vertical]` per corner, clockwise from the top left. Percentages
/// resolve against the box, and radii that overrun a side all shrink by the same factor
/// (CSS Backgrounds 3, section 5.5). The sums use f64 because Tailwind's `rounded-full`
/// is 3.4e38 and two of them overflow f32.
#[inline(never)]
fn corners(radii: Option<Radii>, width: f32, height: f32) -> Corners {
    let Some(Radii { values, percent }) = radii else {
        return [[0.0; 2]; 4];
    };
    let side = [f64::from(width), f64::from(height)];
    let mut corner = [[0.0_f64; 2]; 4];
    for (index, value) in values.iter().enumerate() {
        let value = f64::from(*value);
        corner[index / 2][index % 2] = if percent >> index & 1 != 0 {
            value / 100.0 * side[index % 2]
        } else {
            value
        };
    }
    let [tl, tr, br, bl] = corner;
    let scale = [
        (side[0], tl[0] + tr[0]),
        (side[1], tr[1] + br[1]),
        (side[0], br[0] + bl[0]),
        (side[1], bl[1] + tl[1]),
    ]
    .into_iter()
    .fold(1.0, |scale, (length, sum)| {
        if sum > length && length / sum < scale {
            length / sum
        } else {
            scale
        }
    });
    corner.map(|pair| pair.map(|value| (value * scale) as f32))
}

fn shrink(corners: Corners, by: f32) -> Corners {
    corners.map(|pair| pair.map(|value| (value - by).max(0.0)))
}

/// A closed rectangle path whose corners are quarter ellipses; plain `re` when all are square.
#[inline(never)]
fn outline(content: &mut Content, rect: [f32; 4], corners: Corners) {
    if corners == [[0.0; 2]; 4] {
        return op(content, "re", &rect);
    }
    // A cubic follows a quarter ellipse to 0.03% when its handles reach this far along each edge.
    const KAPPA: f32 = 0.552_284_8;
    let [x, y, width, height] = rect;
    // Each corner's point, and the direction into the box along its horizontal and vertical edge.
    let anchors = [
        (x, y + height, 1.0, -1.0),
        (x + width, y + height, -1.0, -1.0),
        (x + width, y, -1.0, 1.0),
        (x, y, 1.0, 1.0),
    ];
    for (index, ((ax, ay, dx, dy), [rx, ry])) in anchors.into_iter().zip(corners).enumerate() {
        let horizontal = (ax + dx * rx, ay);
        let vertical = (ax, ay + dy * ry);
        // Clockwise, a top-left or bottom-right corner arrives along its vertical edge.
        let (from, to) = if index % 2 == 0 {
            (vertical, horizontal)
        } else {
            (horizontal, vertical)
        };
        op(
            content,
            if index == 0 { "m" } else { "l" },
            &[from.0, from.1],
        );
        let handle = |(px, py): (f32, f32)| [px + KAPPA * (ax - px), py + KAPPA * (ay - py)];
        let [a, b] = handle(from);
        let [c, d] = handle(to);
        op(content, "c", &[a, b, c, d, to.0, to.1]);
    }
    op(content, "h", &[]);
}

/// Length of the `outline` path. A quarter ellipse is a quarter circle of the mean radius, which
/// is exact for the circular corners nearly every border uses, so each corner cuts
/// `(rx + ry) * (1 - pi / 4)` from the rectangle's perimeter.
fn perimeter(rect: [f32; 4], corners: Corners) -> f32 {
    let cut: f32 = corners.iter().flatten().sum();
    2.0 * (rect[2] + rect[3]) - cut * (1.0 - FRAC_PI_4)
}

/// The gap that spreads whole dashes evenly over `length`; a closed path has as many gaps as dashes.
fn fit(length: f32, dash: f32, gap: f32, closed: bool) -> f32 {
    let open = if closed { 0.0 } else { gap };
    let count = ((length + open) / (dash + gap)).floor().max(1.0);
    let gaps = if closed { count } else { count - 1.0 };
    if gaps > 0.0 {
        ((length - count * dash) / gaps).max(0.0)
    } else {
        gap
    }
}
