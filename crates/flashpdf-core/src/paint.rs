use std::ops::Range;

use pdf_writer::{Content, Name, Str};

use crate::font::font_name;
use crate::geometry::Point;
use crate::layout::{LayoutBuffer, Segment};
use crate::{Pt, Rgb, TextAlign};

/// A `/Link` annotation rectangle in page coordinates: `[x1, y1, x2, y2]`.
pub(crate) struct PageLink {
    pub(crate) rect: [f32; 4],
    pub(crate) uri: String,
}

pub(crate) struct PdfPainter<'a> {
    content: &'a mut Content,
    links: &'a mut Vec<PageLink>,
}

impl<'a> PdfPainter<'a> {
    pub(crate) fn new(content: &'a mut Content, links: &'a mut Vec<PageLink>) -> Self {
        Self { content, links }
    }

    pub(crate) fn paint(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        self.paint_boxes(layout, origin);
        self.paint_images(layout, lines.clone(), origin);
        self.paint_text(layout, lines, origin);
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
    fn paint_images(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        for image in layout
            .images
            .iter()
            .filter(|image| lines.contains(&image.line))
        {
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
        }
    }

    /// One text object per line; `Tf` and `rg` change only where the style does.
    /// Underlines, strike-throughs, and link rectangles follow per segment.
    fn paint_text(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        for line in &layout.lines[lines] {
            let segments = &layout.segments[line.segments.clone()];
            if segments.is_empty() {
                continue;
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
