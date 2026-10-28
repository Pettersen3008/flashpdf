mod effects;
pub(crate) use effects::{write_shadings, Shading, Shadings};
use std::ops::Range;

use std::f32::consts::FRAC_PI_4;

use pdf_writer::types::LineCapStyle;
use pdf_writer::{Content, Name, Str, TextStr};

use crate::font::font_name;
use crate::geometry::Point;
use crate::layout::{Actual, Gap, Layer, LayoutBuffer, Lift, PositionedBox, Segment, MARKER_SHAPE};
use crate::renderer::layer_origin;
use crate::vector::{alpha_name, dash, op};
use crate::{BoxStyle, FontId, LineStyle, Page, Pt, Radii, Rgb, TextAlign, OPAQUE};

type Corners = [[f32; 2]; 4];

/// A `/Link` annotation rectangle in page coordinates: `[x1, y1, x2, y2]`.
pub(crate) struct PageLink {
    pub(crate) rect: [f32; 4],
    pub(crate) uri: String,
    pub(crate) structure: u32,
}

pub(crate) enum PageTag {
    Semantic(u32),
    Paragraph,
    Figure(String),
}

/// The fill colour and font a page's content stream last set, so painting skips
/// operators that would not change them. Both persist across `BT`/`ET`.
#[derive(Default)]
pub(crate) struct PaintState {
    fill: Option<Rgb>,
    font: Option<(FontId, Pt)>,
}

pub(crate) struct PdfPainter<'a> {
    content: &'a mut Content,
    links: &'a mut Vec<PageLink>,
    tags: Option<&'a mut Vec<PageTag>>,
    /// Alphas this page's `/ExtGState` must define.
    alphas: &'a mut Vec<[u8; 2]>,
    shadings: &'a mut Shadings,
    /// The fill and stroke alpha in force; every `paint` ends opaque.
    alpha: u8,
    state: &'a mut PaintState,
    ua: bool,
    artifact: bool,
}

impl<'a> PdfPainter<'a> {
    pub(crate) fn new(
        content: &'a mut Content,
        links: &'a mut Vec<PageLink>,
        tags: Option<&'a mut Vec<PageTag>>,
        alphas: &'a mut Vec<[u8; 2]>,
        shadings: &'a mut Shadings,
        state: &'a mut PaintState,
        ua: bool,
    ) -> Self {
        Self {
            content,
            links,
            tags,
            alphas,
            shadings,
            state,
            alpha: OPAQUE,
            ua,
            artifact: false,
        }
    }

    pub(crate) fn artifact(&mut self, artifact: bool) -> &mut Self {
        self.artifact = artifact;
        self
    }

    fn begin_tag(&mut self, structure: u32, artifact: bool) {
        if artifact || self.artifact || structure == 0 {
            self.content.begin_marked_content(Name(b"Artifact"));
        } else if let Some(tags) = &mut self.tags {
            let id = tags.len() as i32;
            tags.push(PageTag::Semantic(structure));
            self.content
                .begin_marked_content_with_properties(Name(b"Span"))
                .properties()
                .identify(id);
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

    pub(crate) fn paint(
        &mut self,
        layout: &LayoutBuffer,
        lines: Range<usize>,
        origin: Point,
        page: Page,
    ) {
        let mut line = lines.start;
        let mut box_start = 0;
        for layer in layout.layers.iter().filter(|layer| layer.flow.is_some()) {
            let at = layer.flow.unwrap();
            if !lines.contains(&at.lines) {
                continue;
            }
            self.paint_part(layout, line..at.lines, box_start..at.boxes, origin);
            self.paint_layer(layer, origin, page);
            line = at.lines + 1;
            box_start = at.boxes;
        }
        self.paint_part(
            layout,
            line..lines.end,
            box_start..layout.boxes.len(),
            origin,
        );
    }

    fn paint_part(
        &mut self,
        layout: &LayoutBuffer,
        lines: Range<usize>,
        boxes: Range<usize>,
        origin: Point,
    ) {
        let mut ids = if self.tags.is_some() {
            vec![None; lines.len()]
        } else {
            Vec::new()
        };
        if let Some(tags) = self.tags.as_mut().filter(|_| !self.ua) {
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
        self.paint_boxes(&layout.boxes[boxes], origin);
        self.paint_images(layout, lines.clone(), origin, &ids);
        self.paint_text(layout, lines, origin, &ids);
        self.set_alpha(OPAQUE);
    }

    /// The whole buffer at `origin`, then its positioned layers over it.
    pub(crate) fn paint_flow(&mut self, layout: &LayoutBuffer, origin: Point, page: Page) {
        self.paint(layout, 0..layout.lines.len(), origin, page);
        let mut layers: Vec<&Layer> = layout
            .layers
            .iter()
            .filter(|layer| layer.flow.is_none())
            .collect();
        layers.sort_by_key(|layer| layer.z);
        for layer in layers {
            self.paint_layer(layer, origin, page);
        }
    }

    /// One layer of the block at `origin`, with the layers inside it.
    pub(crate) fn paint_layer(&mut self, layer: &Layer, origin: Point, page: Page) {
        let origin = layer_origin(layer, origin, page);
        let Some(effect) = &layer.effects else {
            self.paint_flow(&layer.buffer, origin, page);
            return;
        };
        let x = origin.x.get() + effect.rect.origin.x.get();
        let y = origin.y.get() - effect.rect.origin.y.get();
        let matrix = effect
            .transform
            .map(|[a, b, c, d, e, f]| [a, -b, -c, d, x - a * x + c * y + e, y + b * x - d * y - f]);
        if matrix.is_some_and(|m| {
            f64::from(m[0]) * f64::from(m[3]) - f64::from(m[1]) * f64::from(m[2]) == 0.0
        }) {
            return;
        }
        let link_start = self.links.len();
        let shading_start = self.shadings.items.len();
        self.content.save_state();
        if let Some(matrix) = matrix {
            op(self.content, "cm", &matrix);
        }
        if let Some(clip) = &effect.clip {
            use crate::vector::PathOp;
            if effect.clip_border {
                let (width, height) = (effect.rect.width.get(), effect.rect.height.get());
                outline(
                    self.content,
                    [x, y - height, width, height],
                    corners(effect.rect.style.radius, width, height),
                );
            } else if let Some(([cx, cy, width, height], radius)) = clip.outline {
                outline(
                    self.content,
                    [x + cx, y - cy - height, width, height],
                    corners(Some(radius), width, height),
                );
            } else {
                for operation in &clip.operations {
                    match operation {
                        PathOp::Move([px, py]) => op(self.content, "m", &[x + px, y - py]),
                        PathOp::Line([px, py]) => op(self.content, "l", &[x + px, y - py]),
                        PathOp::Cubic(points) => op(
                            self.content,
                            "c",
                            &std::array::from_fn::<_, 6, _>(|i| {
                                if i % 2 == 0 {
                                    x + points[i]
                                } else {
                                    y - points[i]
                                }
                            }),
                        ),
                        PathOp::Close => op(self.content, "h", &[]),
                    }
                }
            }
            op(self.content, if clip.even_odd { "W*" } else { "W" }, &[]);
            op(self.content, "n", &[]);
        }
        if effect.paint {
            self.paint_boxes(std::slice::from_ref(&effect.rect), origin);
        }
        if effect.overflow {
            let border = effect.rect.style.border;
            let width = effect.rect.width.get();
            let height = effect.rect.height.get();
            let radii = corners(effect.rect.style.radius, width, height);
            let [top, right, bottom, left] = border.into_array().map(Pt::get);
            let inset = [[left, top], [right, top], [right, bottom], [left, bottom]];
            let radii: Corners = std::array::from_fn(|i| {
                std::array::from_fn(|j| (radii[i][j] - inset[i][j]).max(0.0))
            });
            let (width, height) = (
                (width - left - right).max(0.0),
                (height - top - bottom).max(0.0),
            );
            let radii = corners(
                Some(Radii {
                    values: std::array::from_fn(|i| radii[i / 2][i % 2]),
                    percent: 0,
                }),
                width,
                height,
            );
            outline(
                self.content,
                [
                    x + left,
                    y - effect.rect.height.get() + bottom,
                    width,
                    height,
                ],
                radii,
            );
            op(self.content, "W", &[]);
            op(self.content, "n", &[]);
        }
        *self.state = PaintState::default();
        self.paint_flow(&layer.buffer, origin, page);
        // PDF pattern matrices map to page space, so retain every enclosing layer transform.
        if let Some(matrix) = matrix {
            for shading in &mut self.shadings.items[shading_start..] {
                match crate::effects::multiply(matrix, shading.pattern) {
                    Ok(pattern) => shading.pattern = pattern,
                    Err(_) => self.shadings.invalid = true,
                }
            }
        }
        self.content.restore_state();
        let clip_bounds = effect.clip.as_ref().map(|clip| {
            clip.outline
                .map(|([cx, cy, w, h], _)| [x + cx, y - cy - h, x + cx + w, y - cy])
                .or_else(|| path_bounds(&clip.operations, x, y))
        });
        let padding_bounds = effect.overflow.then(|| {
            let border = effect.rect.style.border;
            [
                x + border.left.get(),
                y - effect.rect.height.get() + border.bottom.get(),
                x + effect.rect.width.get() - border.right.get(),
                y - border.top.get(),
            ]
        });
        let mut index = link_start;
        while index < self.links.len() {
            let mut visible = true;
            for bounds in clip_bounds.into_iter().chain(padding_bounds.map(Some)) {
                let Some(bounds) = bounds else {
                    visible = false;
                    break;
                };
                let rect = &mut self.links[index].rect;
                rect[0] = rect[0].max(bounds[0]);
                rect[1] = rect[1].max(bounds[1]);
                rect[2] = rect[2].min(bounds[2]);
                rect[3] = rect[3].min(bounds[3]);
                visible &= rect[0] < rect[2] && rect[1] < rect[3];
            }
            if visible {
                index += 1;
            } else {
                self.links.remove(index);
            }
        }
        if let Some([a, b, c, d, e, f]) = matrix {
            for link in &mut self.links[link_start..] {
                let [left, bottom, right, top] = link.rect;
                let points = [[left, bottom], [left, top], [right, bottom], [right, top]]
                    .map(|[x, y]| [a * x + c * y + e, b * x + d * y + f]);
                link.rect = [
                    points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
                    points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
                    points
                        .iter()
                        .map(|p| p[0])
                        .fold(f32::NEG_INFINITY, f32::max),
                    points
                        .iter()
                        .map(|p| p[1])
                        .fold(f32::NEG_INFINITY, f32::max),
                ];
            }
        }
        *self.state = PaintState::default();
    }

    /// Backgrounds and borders, as one artifact when tagged.
    pub(crate) fn paint_boxes(&mut self, boxes: &[PositionedBox], origin: Point) {
        if self.tags.is_some() {
            self.content.begin_marked_content(Name(b"Artifact"));
        }
        for paint in boxes {
            let style = &paint.style;
            let x = origin.x.get() + paint.origin.x.get();
            let y = origin.y.get() - paint.origin.y.get() - paint.height.get();
            let (width, height) = (paint.width.get(), paint.height.get());
            self.set_alpha(style.alpha);
            self.paint_shadows(style, [x, y, width, height], false);
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
            self.paint_gradient(style, [x, y, width, height]);
            self.paint_shadows(style, [x, y, width, height], true);
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
        if self.tags.is_some() {
            self.content.end_marked_content();
        }
        self.set_alpha(OPAQUE);
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
        self.paint_gradient(style, rect);
        self.paint_shadows(style, rect, true);
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
            if self.ua {
                self.begin_tag(image.structure, image.alt.is_empty());
            } else if let Some(id) = ids.get(image.line - lines.start).copied().flatten() {
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
                    structure: if self.artifact { 0 } else { image.structure },
                });
            }
            if self.tags.is_some() {
                self.content.end_marked_content();
            }
        }
    }

    /// One text object per line; `Tf` and `rg` appear only where the page's state changes.
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
            let background_tag = self.ua
                && layout.decorations[line.segments.clone()]
                    .iter()
                    .any(|decoration| decoration.background.is_some());
            if background_tag {
                self.content.begin_marked_content(Name(b"Artifact"));
            }
            for (index, segment) in segments.iter().enumerate() {
                let decoration = &layout.decorations[line.segments.start + index];
                if let Some(color) = decoration.background {
                    self.set_alpha(decoration.alpha);
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
            if background_tag {
                self.content.end_marked_content();
            }
            self.content.begin_text();
            let mut previous: Option<&Segment> = None;
            for (index, segment) in segments.iter().enumerate() {
                self.set_alpha(layout.decorations[line.segments.start + index].alpha);
                self.set_fill(segment.color);
                self.set_font(segment.font, segment.size);
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
                let range = segment.text.clone();
                let within = |at: u32| range.contains(&(at as usize));
                let lifts = &layout.lifts[layout.lifts.partition_point(|lift| !within(lift.at))..];
                let lifts = &lifts[..lifts.partition_point(|lift| within(lift.at))];
                let actual: &[_] = match self.tags {
                    Some(_) => {
                        let start = layout
                            .actual
                            .partition_point(|(codes, _)| codes.end as usize <= range.start);
                        let actual = &layout.actual[start..];
                        &actual[..actual
                            .partition_point(|(codes, _)| (codes.start as usize) < range.end)]
                    }
                    None => &[],
                };
                if self.ua {
                    self.begin_tag(
                        layout.decorations[line.segments.start + index].structure,
                        segment.shape != 0,
                    );
                }
                if segment.shape == 0 && lifts.is_empty() && actual.is_empty() {
                    show(
                        self.content,
                        &layout.text[range.clone()],
                        range.start,
                        &layout.gaps[first..last],
                    );
                } else if segment.shape == 0 {
                    show_marked(
                        self.content,
                        &layout.text,
                        range,
                        &layout.gaps[first..last],
                        (lifts, actual),
                        segment.size.get(),
                    );
                }
                if self.ua {
                    self.content.end_marked_content();
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
                self.set_alpha(decoration.alpha);
                let (start, width) = extent(index);
                let baseline = baseline + segment.rise;
                let scale = segment.size.get() / 1000.0;
                let strokes = [decoration.underline, decoration.line_through];
                let decoration_tag =
                    self.ua && (segment.shape != 0 || strokes.iter().any(Option::is_some));
                if decoration_tag {
                    self.begin_tag(decoration.structure, segment.shape == 0);
                }
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
                if decoration_tag {
                    self.content.end_marked_content();
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
                        structure: if self.artifact {
                            0
                        } else {
                            decoration.structure
                        },
                    });
                }
            }
            if self.tags.is_some() && !self.ua {
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

    fn set_fill(&mut self, color: Rgb) {
        if self.state.fill != Some(color) {
            self.state.fill = Some(color);
            let [r, g, b] = color.normalized();
            self.content.set_fill_rgb(r, g, b);
        }
    }

    fn set_font(&mut self, font: FontId, size: Pt) {
        if self.state.font != Some((font, size)) {
            self.state.font = Some((font, size));
            let mut buffer = [0_u8; 4];
            self.content
                .set_font(Name(font_name(font, &mut buffer)), size.get());
        }
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
        if at > from {
            items.show(Str(&text[from..at]));
        }
        items.adjust(-gap.amount);
        from = at;
    }
    if from < text.len() {
        items.show(Str(&text[from..]));
    }
}

/// `show` for a segment with marks: a lifted glyph sits on its own `Ts`, and a cluster
/// whose glyphs do not spell its text is a `/Span` with `/ActualText`. A cluster split
/// over two lines gives its text to the first part and none to the rest.
fn show_marked(
    content: &mut Content,
    text: &[u8],
    range: Range<usize>,
    gaps: &[Gap],
    (lifts, actual): (&[Lift], &[Actual]),
    size: f32,
) {
    let mut cuts: Vec<usize> = lifts
        .iter()
        .flat_map(|lift| [lift.at, lift.at + 2])
        .chain(
            actual
                .iter()
                .flat_map(|(codes, _)| [codes.start, codes.end]),
        )
        .map(|at| at as usize)
        .filter(|at| *at > range.start && *at < range.end)
        .chain([range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let (mut from, mut rise, mut open, mut gap) = (range.start, 0.0, None, 0);
    for to in cuts {
        let span = actual
            .iter()
            .find(|(codes, _)| (codes.start as usize..codes.end as usize).contains(&from));
        if let (Some((codes, source)), None) = (span, open) {
            let source = if codes.start as usize >= range.start {
                &**source
            } else {
                ""
            };
            content
                .begin_marked_content_with_properties(Name(b"Span"))
                .properties()
                .actual_text(TextStr(source));
            open = Some(codes.end as usize);
        }
        let lift = lifts
            .iter()
            .find(|lift| lift.at as usize == from)
            .map_or(0.0, |lift| lift.amount * size / 1000.0);
        if lift != rise {
            content.set_rise(lift);
            rise = lift;
        }
        let end = gap + gaps[gap..].partition_point(|next| (next.at as usize) < to);
        show(content, &text[from..to], from, &gaps[gap..end]);
        gap = end;
        if open.is_some_and(|end| end <= to) || to == range.end && open.is_some() {
            content.end_marked_content();
            open = None;
        }
        from = to;
    }
    if rise != 0.0 {
        content.set_rise(0.0);
    }
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
        TextAlign::Left | TextAlign::Justify | TextAlign::Start => x,
        TextAlign::Center => x + Pt((available_width - text_width).get() / 2.0),
        TextAlign::Right | TextAlign::End => x + available_width - text_width,
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

fn path_bounds(operations: &[crate::vector::PathOp], x: f32, y: f32) -> Option<[f32; 4]> {
    use crate::vector::PathOp;
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    for operation in operations {
        let points: &[f32] = match operation {
            PathOp::Move(p) | PathOp::Line(p) => p,
            PathOp::Cubic(p) => p,
            PathOp::Close => continue,
        };
        for pair in points.chunks_exact(2) {
            let (px, py) = (x + pair[0], y - pair[1]);
            bounds[0] = bounds[0].min(px);
            bounds[1] = bounds[1].min(py);
            bounds[2] = bounds[2].max(px);
            bounds[3] = bounds[3].max(py);
        }
    }
    (bounds[0] < bounds[2] && bounds[1] < bounds[3]).then_some(bounds)
}
