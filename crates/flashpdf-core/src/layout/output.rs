use std::ops::Range;

use crate::font::Stroke;
use crate::geometry::Point;
use crate::{BoxStyle, FontId, Pt, Rgb, TextAlign};

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
pub(crate) struct Segment {
    pub(crate) text: Range<usize>,
    pub(crate) x: Pt,
    pub(crate) font: FontId,
    pub(crate) size: Pt,
    pub(crate) color: Rgb,
    /// `Tc`, in points.
    pub(crate) letter_spacing: f32,
    pub(crate) oblique: bool,
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

pub(crate) struct PositionedBox {
    pub(crate) origin: Point,
    pub(crate) width: Pt,
    pub(crate) height: Pt,
    pub(crate) style: BoxStyle,
}

/// Paint attributes of the segment at the same index. The text layouter owns
/// `Segment`, so these ride beside it instead of inside it. Strokes are in 1/1000 em.
pub(crate) struct Decoration {
    pub(crate) underline: Option<Stroke>,
    pub(crate) line_through: Option<Stroke>,
    pub(crate) color: Rgb,
    /// Index into `LayoutBuffer::links`.
    pub(crate) link: Option<usize>,
    /// Font ascent and (positive) descent at the segment size, for the link rectangle.
    pub(crate) ascent: Pt,
    pub(crate) descent: Pt,
}

pub(crate) struct PositionedImage {
    pub(crate) origin: Point,
    pub(crate) width: Pt,
    pub(crate) height: Pt,
    pub(crate) slot: u16,
    pub(crate) link: Option<usize>,
    pub(crate) alt: String,
    /// The empty pseudo-line whose box carries the image through page splitting.
    pub(crate) line: usize,
}

#[derive(Default)]
pub(crate) struct LayoutBuffer {
    pub(crate) lines: Vec<PositionedLine>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) decorations: Vec<Decoration>,
    pub(crate) text: Vec<u8>,
    /// Sorted by `at`.
    pub(crate) gaps: Vec<Gap>,
    pub(crate) boxes: Vec<PositionedBox>,
    pub(crate) images: Vec<PositionedImage>,
    pub(crate) links: Vec<String>,
}

impl LayoutBuffer {
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.segments.clear();
        self.decorations.clear();
        self.text.clear();
        self.gaps.clear();
        self.boxes.clear();
        self.images.clear();
        self.links.clear();
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.lines.capacity() * std::mem::size_of::<PositionedLine>()
            + self.segments.capacity() * std::mem::size_of::<Segment>()
            + self.text.capacity()
    }
}
