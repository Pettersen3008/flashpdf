use std::ops::Range;

use pdf_writer::{Content, Name, Str};

use crate::font::font_name;
use crate::geometry::Point;
use crate::layout::LayoutBuffer;
use crate::{Pt, Rgb, TextAlign};

pub(crate) struct PdfPainter<'a> {
    content: &'a mut Content,
}

impl<'a> PdfPainter<'a> {
    pub(crate) fn new(content: &'a mut Content) -> Self {
        Self { content }
    }

    pub(crate) fn paint(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        self.paint_boxes(layout, origin);
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

    fn paint_text(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        for line in &layout.lines[lines] {
            self.paint_line(
                &layout.text[line.text.clone()],
                line.style,
                Point {
                    x: origin.x + line.origin.x,
                    y: origin.y - line.origin.y,
                },
                line.available_width,
                line.text_width,
            );
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
        let x = match style.align {
            TextAlign::Left => origin.x,
            TextAlign::Center => origin.x + Pt((available_width - text_width).get() / 2.0),
            TextAlign::Right => origin.x + available_width - text_width,
        };
        self.content.next_line(x.get(), origin.y.get());
        self.content.show(Str(text));
        self.content.end_text();
    }

    fn set_fill(&mut self, color: Rgb) {
        let [r, g, b] = color.normalized();
        self.content.set_fill_rgb(r, g, b);
    }
}
