use std::ops::Range;

use crate::font::Stroke;
use crate::geometry::Point;
use crate::{BoxStyle, FontId, Pt, Rgb, TextAlign};

/// A drawn list marker's slot, with its gap to the text, and its size, in em.
pub(crate) const MARKER_SLOT: f32 = 0.75;
pub(crate) const MARKER_SHAPE: f32 = 0.3;

#[derive(Clone)]
pub(crate) struct PositionedLine {
    pub(crate) segments: Range<usize>,
    /// Baseline; `top..bottom` is the line box the page splitter measures.
    pub(crate) origin: Point,
    pub(crate) top: Pt,
    pub(crate) bottom: Pt,
    pub(crate) available_width: Pt,
    pub(crate) text_width: Pt,
    pub(crate) align: TextAlign,
}

/// One style's stretch of a line; `x` is its offset from the line start.
#[derive(Clone)]
pub(crate) struct Segment {
    pub(crate) text: Range<usize>,
    pub(crate) x: Pt,
    pub(crate) font: FontId,
    pub(crate) size: Pt,
    pub(crate) color: Rgb,
    /// `Tc`, in points.
    pub(crate) letter_spacing: f32,
    pub(crate) oblique: bool,
    /// Baseline shift in points, up positive.
    pub(crate) rise: f32,
    /// 1, 2, or 3 draws a disc, circle, or square of this size instead of `text`, for a list marker.
    pub(crate) shape: u8,
}

/// Extra advance before the code starting at byte `at` of `LayoutBuffer::text`,
/// in 1/1000 em of its segment, as a `TJ` adjustment with the sign flipped.
/// Inside a segment it is painted; at a segment start it is already in `x`.
#[derive(Clone, Copy)]
pub(crate) struct Gap {
    pub(crate) at: u32,
    pub(crate) amount: f32,
    /// After a space or no-break space, so `text-align: justify` widens it.
    pub(crate) stretch: bool,
}

/// The baseline shift of the one code at byte `at`, as a mark sits above its
/// base, in 1/1000 em of its segment, up positive.
#[derive(Clone, Copy)]
pub(crate) struct Lift {
    pub(crate) at: u32,
    pub(crate) amount: f32,
}

/// A range of codes and the source text they show, for `/ActualText`.
pub(crate) type Actual = (Range<u32>, Box<str>);

#[derive(Clone)]
pub(crate) struct PositionedBox {
    pub(crate) origin: Point,
    pub(crate) width: Pt,
    pub(crate) height: Pt,
    pub(crate) style: BoxStyle,
}

/// A positioned box, laid out in a buffer of its own, which paints after the normal flow
/// around it, ordered by `z` and then as laid out.
#[derive(Clone)]
pub(crate) struct Layer {
    pub(crate) z: i32,
    /// A flow layer paints at these buffer indices, rather than at page end.
    pub(crate) flow: Option<Marks>,
    pub(crate) effects: Option<std::boxed::Box<LayerEffects>>,
    /// How far right and down the buffer paints from where it was laid out.
    pub(crate) offset: Point,
    /// The buffer is laid out from the page area's top left corner, not the block's.
    pub(crate) page: bool,
    pub(crate) buffer: LayoutBuffer,
}

#[derive(Clone)]
pub(crate) struct LayerEffects {
    pub(crate) source: usize,
    pub(crate) rect: PositionedBox,
    pub(crate) transform: Option<[f32; 6]>,
    pub(crate) clip: Option<std::rc::Rc<crate::effects::ResolvedClip>>,
    pub(crate) clip_border: bool,
    pub(crate) overflow: bool,
    pub(crate) paint: bool,
}

/// Paint attributes of the segment at the same index. The text layouter owns
/// `Segment`, so these ride beside it instead of inside it. Strokes are in 1/1000 em.
#[derive(Clone)]
pub(crate) struct Decoration {
    pub(crate) structure: u32,
    pub(crate) underline: Option<Stroke>,
    pub(crate) line_through: Option<Stroke>,
    pub(crate) color: Rgb,
    pub(crate) background: Option<Rgb>,
    /// Index into `LayoutBuffer::links`.
    pub(crate) link: Option<usize>,
    pub(crate) alpha: u8,
    /// Font ascent and (positive) descent at the segment size, for the link rectangle.
    pub(crate) ascent: Pt,
    pub(crate) descent: Pt,
}

#[derive(Clone)]
pub(crate) struct PositionedImage {
    pub(crate) structure: u32,
    pub(crate) origin: Point,
    pub(crate) width: Pt,
    pub(crate) height: Pt,
    pub(crate) slot: u16,
    pub(crate) link: Option<usize>,
    pub(crate) alt: String,
    pub(crate) alpha: u8,
    /// The empty pseudo-line whose box carries the image through page splitting.
    pub(crate) line: usize,
}

#[derive(Clone, Default)]
pub(crate) struct LayoutBuffer {
    pub(crate) lines: Vec<PositionedLine>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) decorations: Vec<Decoration>,
    pub(crate) text: Vec<u8>,
    /// Sorted by `at`.
    pub(crate) gaps: Vec<Gap>,
    /// Sorted by `at`.
    pub(crate) lifts: Vec<Lift>,
    /// Codes whose `/ToUnicode` text differs from their source text, with that text, for
    /// a tagged PDF's `/ActualText`. Sorted and disjoint.
    pub(crate) actual: Vec<Actual>,
    pub(crate) boxes: Vec<PositionedBox>,
    pub(crate) images: Vec<PositionedImage>,
    pub(crate) links: Vec<String>,
    /// Anchor slot and the index of the line it waits for; `lines.len()` when none follows.
    pub(crate) anchors: Vec<(u16, usize)>,
    /// Positioned boxes, in the order they were laid out.
    pub(crate) layers: Vec<Layer>,
}

/// Buffer lengths before or after a cell, so its content can move once the row height is known.
#[derive(Clone, Copy, Default)]
pub(crate) struct Marks {
    pub(crate) lines: usize,
    pub(crate) boxes: usize,
    pub(crate) images: usize,
    pub(crate) layers: usize,
}

impl LayoutBuffer {
    pub(crate) fn marks(&self) -> Marks {
        Marks {
            lines: self.lines.len(),
            boxes: self.boxes.len(),
            images: self.images.len(),
            layers: self.layers.len(),
        }
    }

    /// Moves what was laid out between `from` and `to` down by `dy`.
    pub(crate) fn shift(&mut self, from: Marks, to: Marks, dy: Pt) {
        for line in &mut self.lines[from.lines..to.lines] {
            line.origin.y += dy;
            line.top += dy;
            line.bottom += dy;
        }
        for paint in &mut self.boxes[from.boxes..to.boxes] {
            paint.origin.y += dy;
        }
        for image in &mut self.images[from.images..to.images] {
            image.origin.y += dy;
        }
        for layer in &mut self.layers[from.layers..to.layers] {
            if !layer.page {
                layer.offset.y += dy;
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.segments.clear();
        self.decorations.clear();
        self.text.clear();
        self.gaps.clear();
        self.lifts.clear();
        self.actual.clear();
        self.boxes.clear();
        self.images.clear();
        self.links.clear();
        self.anchors.clear();
        self.layers.clear();
    }

    /// Heap bytes held, leaving out link and alt text.
    pub(crate) fn capacity(&self) -> usize {
        use std::mem::size_of;
        self.lines.capacity() * size_of::<PositionedLine>()
            + self.segments.capacity() * size_of::<Segment>()
            + self.decorations.capacity() * size_of::<Decoration>()
            + self.text.capacity()
            + self.lifts.capacity() * size_of::<Lift>()
            + self.actual.capacity() * size_of::<Actual>()
            + self.boxes.capacity() * size_of::<PositionedBox>()
            + self.images.capacity() * size_of::<PositionedImage>()
            + self.links.capacity() * size_of::<String>()
            + self.layers.capacity() * size_of::<Layer>()
            + self
                .layers
                .iter()
                .map(|layer| {
                    layer.buffer.capacity()
                        + layer.effects.as_ref().map_or(0, |effects| {
                            size_of::<LayerEffects>()
                                + effects.clip.as_ref().map_or(0, |clip| {
                                    size_of::<crate::effects::ResolvedClip>()
                                        + 2 * size_of::<usize>()
                                        + clip.operations.capacity()
                                            * size_of::<crate::vector::PathOp>()
                                })
                        })
                })
                .sum::<usize>()
    }
}
