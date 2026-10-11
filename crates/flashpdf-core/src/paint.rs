use std::ops::Range;

use pdf_writer::{Content, Name, Str};

use crate::font::font_name;
use crate::geometry::Point;
use crate::layout::{Gap, LayoutBuffer, Segment, MARKER_SHAPE};
use crate::{Pt, Rgb, TextAlign};

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
}

impl<'a> PdfPainter<'a> {
    pub(crate) fn new(
        content: &'a mut Content,
        links: &'a mut Vec<PageLink>,
        tags: Option<&'a mut Vec<PageTag>>,
    ) -> Self {
        Self {
            content,
            links,
            tags,
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
    }

    fn paint_boxes(&mut self, layout: &LayoutBuffer, origin: Point) {
        for paint in &layout.boxes {
            let x = origin.x.get() + paint.origin.x.get();
            let y = origin.y.get() - paint.origin.y.get() - paint.height.get();
            if let Some(color) = paint.style.background {
                self.set_fill(color);
                self.content
                    .rect(x, y, paint.width.get(), paint.height.get())
                    .fill_nonzero();
            }
            let [top, right, bottom, left] = paint.style.border.into_array().map(|edge| edge.get());
            for (color, (x, y, width, height)) in
                paint.style.border_color.into_array().iter().zip([
                    (x, y + paint.height.get() - top, paint.width.get(), top),
                    (x + paint.width.get() - right, y, right, paint.height.get()),
                    (x, y, paint.width.get(), bottom),
                    (x, y, left, paint.height.get()),
                ])
            {
                if width > 0.0 && height > 0.0 {
                    self.set_fill(*color);
                    self.content.rect(x, y, width, height).fill_nonzero();
                }
            }
        }
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
            let x = origin.x
                + aligned(
                    line.origin.x,
                    line.align,
                    line.available_width,
                    line.text_width,
                );
            let baseline = (origin.y - line.origin.y).get();
            // A segment's start and width on the line; it ends where the next one starts.
            let extent = |index: usize| {
                let segment = &segments[index];
                let end = segments
                    .get(index + 1)
                    .map_or(line.text_width, |next| next.x);
                ((x + segment.x).get(), (end - segment.x).get())
            };
            for (index, segment) in segments.iter().enumerate() {
                let decoration = &layout.decorations[line.segments.start + index];
                if let Some(color) = decoration.background {
                    let (start, width) = extent(index);
                    self.set_fill(color);
                    let (bottom, top) = (
                        baseline + segment.rise - decoration.descent.get(),
                        baseline + segment.rise + decoration.ascent.get(),
                    );
                    self.polygon(&[
                        (start, bottom),
                        (start + width, bottom),
                        (start + width, top),
                        (start, top),
                    ]);
                    self.content.fill_nonzero();
                }
            }
            self.content.begin_text();
            let mut previous: Option<&Segment> = None;
            for segment in segments {
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
                let spacing = previous.map_or(0.0, |last| last.letter_spacing);
                if segment.letter_spacing != spacing {
                    self.content.set_char_spacing(segment.letter_spacing);
                }
                match previous {
                    Some(last) if last.oblique == segment.oblique => self
                        .content
                        .next_line((segment.x - last.x).get(), segment.rise - last.rise),
                    None if !segment.oblique => self
                        .content
                        .next_line((x + segment.x).get(), baseline + segment.rise),
                    _ => self.content.set_text_matrix(slant(
                        segment.oblique,
                        (x + segment.x).get(),
                        baseline + segment.rise,
                    )),
                };
                let first = layout
                    .gaps
                    .partition_point(|gap| gap.at as usize <= segment.text.start);
                let last = layout
                    .gaps
                    .partition_point(|gap| (gap.at as usize) < segment.text.end);
                if segment.shape == 0 {
                    show(
                        self.content,
                        &layout.text[segment.text.clone()],
                        segment.text.start,
                        &layout.gaps[first..last],
                    );
                }
                previous = Some(segment);
            }
            // Text state outlives the text object, and every other line starts from Tc 0.
            if previous.is_some_and(|last| last.letter_spacing != 0.0) {
                self.content.set_char_spacing(0.0);
            }
            self.content.end_text();
            for (index, segment) in segments.iter().enumerate() {
                let decoration = &layout.decorations[line.segments.start + index];
                let (start, width) = extent(index);
                let baseline = baseline + segment.rise;
                let scale = segment.size.get() / 1000.0;
                if segment.shape != 0 {
                    self.set_fill(decoration.color);
                    let middle = baseline + decoration.ascent.get() / 2.0;
                    self.marker_shape(
                        segment.shape,
                        start,
                        middle,
                        segment.size.get() * MARKER_SHAPE,
                    );
                }
                let strokes = [decoration.underline, decoration.line_through];
                if strokes.iter().any(Option::is_some) {
                    self.set_fill(decoration.color);
                }
                for stroke in strokes.into_iter().flatten() {
                    let thickness = stroke.thickness * scale;
                    self.content
                        .rect(
                            start,
                            baseline + stroke.offset * scale - thickness / 2.0,
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

    /// A list marker's disc, ring, or square: `size` wide with its left edge at `x`, centred at `y`.
    /// Discs and rings are 12-sided polygons, which read as round at marker size.
    #[inline(never)]
    fn marker_shape(&mut self, shape: u8, x: f32, y: f32, size: f32) {
        let half = size / 2.0;
        // A disc is one outline, a ring two, and a square four corners.
        let rings = if shape == 3 { 0 } else { usize::from(shape) };
        if shape == 3 {
            self.polygon(&[
                (x, y - half),
                (x + size, y - half),
                (x + size, y + half),
                (x, y + half),
            ]);
        }
        for radius in [half, half * 0.65].into_iter().take(rings) {
            let (mut dx, mut dy) = (radius, 0.0);
            let mut points = [(0.0, 0.0); 12];
            for point in &mut points {
                *point = (x + half + dx, y + dy);
                (dx, dy) = (dx * 0.866 - dy * 0.5, dx * 0.5 + dy * 0.866);
            }
            self.polygon(&points);
        }
        if shape == 2 {
            self.content.fill_even_odd();
        } else {
            self.content.fill_nonzero();
        }
    }

    /// Every marker and background outline goes through here, so the float formatting is compiled once.
    #[inline(never)]
    fn polygon(&mut self, points: &[(f32, f32)]) {
        for (index, point) in points.iter().enumerate() {
            self.content
                .op(if index == 0 { "m" } else { "l" })
                .operands([point.0, point.1]);
        }
        self.content.close_path();
    }

    pub(crate) fn paint_line(
        &mut self,
        text: &[u8],
        gaps: &[Gap],
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
        let spacing = style.typography.letter_spacing;
        if spacing != 0.0 {
            self.content.set_char_spacing(spacing);
        }
        let x = aligned(origin.x, style.align, available_width, text_width);
        if style.typography.oblique {
            self.content
                .set_text_matrix(slant(true, x.get(), origin.y.get()));
        } else {
            self.content.next_line(x.get(), origin.y.get());
        }
        show(self.content, text, 0, gaps);
        if spacing != 0.0 {
            self.content.set_char_spacing(0.0);
        }
        self.content.end_text();
    }

    fn set_fill(&mut self, color: Rgb) {
        let [r, g, b] = color.normalized();
        self.content.set_fill_rgb(r, g, b);
    }
}

/// Shows `text`, which starts at byte `start`, with each nonzero gap as a `TJ` adjustment.
fn show(content: &mut Content, text: &[u8], start: usize, gaps: &[Gap]) {
    if gaps.iter().all(|gap| gap.amount == 0.0) {
        content.show(Str(text));
        return;
    }
    let mut operation = content.show_positioned();
    let mut items = operation.items();
    let mut from = 0;
    for gap in gaps.iter().filter(|gap| gap.amount != 0.0) {
        let at = gap.at as usize - start;
        items.show(Str(&text[from..at])).adjust(-gap.amount);
        from = at;
    }
    items.show(Str(&text[from..]));
}

/// Chrome's synthetic italic skews by a quarter, about 14 degrees.
const OBLIQUE_SKEW: f32 = 0.25;

fn slant(oblique: bool, x: f32, y: f32) -> [f32; 6] {
    [
        1.0,
        0.0,
        if oblique { OBLIQUE_SKEW } else { 0.0 },
        1.0,
        x,
        y,
    ]
}

fn aligned(x: Pt, align: TextAlign, available_width: Pt, text_width: Pt) -> Pt {
    match align {
        // A wrapped justified line fills its width, and its last line starts left.
        TextAlign::Left | TextAlign::Justify => x,
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
