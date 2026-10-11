use std::ops::Range;

use pdf_writer::{Content, Name, Str};

use crate::font::font_name;
use crate::geometry::Point;
use crate::layout::{LayoutBuffer, Segment};
use crate::structure::{Structure, ARTIFACT};
use crate::{Pt, Rgb, TextAlign};

/// A `/Link` annotation rectangle in page coordinates: `[x1, y1, x2, y2]`.
pub(crate) struct PageLink {
    pub(crate) rect: [f32; 4],
    pub(crate) uri: String,
    /// Tagged output only: the link text or the image's alternate text, and the Link element.
    pub(crate) contents: String,
    pub(crate) element: Option<u32>,
}

pub(crate) struct PdfPainter<'a> {
    content: &'a mut Content,
    links: &'a mut Vec<PageLink>,
    tree: Option<&'a mut Structure>,
    /// A repeated table header or footer, which paints as an artifact.
    repeat: bool,
}

impl<'a> PdfPainter<'a> {
    pub(crate) fn new(
        content: &'a mut Content,
        links: &'a mut Vec<PageLink>,
        tree: Option<&'a mut Structure>,
        repeat: bool,
    ) -> Self {
        Self {
            content,
            links,
            tree,
            repeat,
        }
    }

    pub(crate) fn paint(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        let tagged = self.artifact();
        self.paint_boxes(layout, origin);
        if tagged {
            self.content.end_marked_content();
        }
        self.paint_images(layout, lines.clone(), origin);
        self.paint_text(layout, lines, origin);
    }

    /// Begins an artifact in tagged output; true when the caller must end it.
    fn artifact(&mut self) -> bool {
        let Some(tree) = &mut self.tree else {
            return false;
        };
        tree.mark(self.content, ARTIFACT, false);
        true
    }

    fn push_link(&mut self, rect: [f32; 4], uri: &str, contents: &str, element: Option<u32>) {
        let tagged = self.tree.as_mut().map(|tree| tree.annotate(element));
        self.links.push(PageLink {
            rect,
            uri: uri.to_owned(),
            contents: tagged.map_or_else(String::new, |_| contents.to_owned()),
            element,
        });
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
            // A linked image's content sits in a Link element inside its Figure.
            let mut element = None;
            let mut alt = "";
            if let Some(tree) = &mut self.tree {
                element = image.link.and_then(|_| tree.link(image.tag));
                tree.mark(self.content, element.unwrap_or(image.tag), self.repeat);
                if let Some(figure) = tree.elements.get_mut(image.tag as usize) {
                    if !self.repeat {
                        let (right, top) = ((x + width).ceil(), (y + height).ceil());
                        figure.bbox =
                            Some([x.floor(), y.floor(), right, top].map(|edge| edge as i32));
                    }
                    alt = &figure.alt;
                }
            }
            let mut buffer = [0_u8; 8];
            self.content
                .save_state()
                .transform([width, 0.0, 0.0, height, x, y])
                .x_object(Name(image_name(image.slot, &mut buffer)))
                .restore_state();
            if let Some(link) = image.link {
                let alt = alt.to_owned();
                self.push_link(
                    [x, y, x + width, y + height],
                    &layout.links[link].uri,
                    &alt,
                    element,
                );
            }
            if self.tree.is_some() {
                self.content.end_marked_content();
            }
        }
    }

    /// One text object per line; `Tf` and `rg` change only where the style does.
    /// Underlines, strike-throughs, and link rectangles follow per segment.
    /// In tagged output each run of segments with the same link is one marked-content
    /// sequence, and a link keeps its Link element across lines until other text interrupts it.
    fn paint_text(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        let mut open: Option<(usize, u32)> = None;
        for line in &layout.lines[lines] {
            let segments = &layout.segments[line.segments.clone()];
            if segments.is_empty() {
                continue;
            }
            let decorations = &layout.decorations[line.segments.clone()];
            self.content.begin_text();
            let x = origin.x
                + aligned(
                    line.origin.x,
                    line.align,
                    line.available_width,
                    line.text_width,
                );
            let baseline = (origin.y - line.origin.y).get();
            // Each segment's start and width on the page.
            let extent = |index: usize| {
                let segment: &Segment = &segments[index];
                let end = segments
                    .get(index + 1)
                    .map_or(line.text_width, |next| next.x);
                ((x + segment.x).get(), (end - segment.x).get())
            };
            let mut previous: Option<&Segment> = None;
            let mut marked = None;
            for (index, (segment, decoration)) in segments.iter().zip(decorations).enumerate() {
                let mut element = None;
                if let Some(tree) = &mut self.tree {
                    let link = decoration.link;
                    element = link.and_then(|link| match open {
                        Some((last, element)) if last == link => Some(element),
                        _ => tree.link(line.tag),
                    });
                    open = link.zip(element);
                    if marked != Some(link) {
                        if marked.is_some() {
                            self.content.end_marked_content();
                        }
                        tree.mark(self.content, element.unwrap_or(line.tag), self.repeat);
                        marked = Some(link);
                    }
                }
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
                if let Some(link) = decoration.link {
                    let (start, width) = extent(index);
                    let link = &layout.links[link];
                    self.push_link(
                        [
                            start,
                            baseline - decoration.descent.get(),
                            start + width,
                            baseline + decoration.ascent.get(),
                        ],
                        &link.uri,
                        &link.text,
                        element,
                    );
                }
            }
            if marked.is_some() {
                self.content.end_marked_content();
            }
            self.content.end_text();
            let decorated = (decorations.iter())
                .any(|decoration| decoration.underline || decoration.line_through)
                && self.artifact();
            for (index, (segment, decoration)) in segments.iter().zip(decorations).enumerate() {
                let (start, width) = extent(index);
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
            }
            if decorated {
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
