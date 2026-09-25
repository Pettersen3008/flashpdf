use std::ops::Range;

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
}

pub(crate) struct PositionedBox {
    pub(crate) origin: Point,
    pub(crate) width: Pt,
    pub(crate) height: Pt,
    pub(crate) style: BoxStyle,
}

/// Paint attributes of the segment at the same index. The text layouter owns
/// `Segment`, so these ride beside it instead of inside it.
pub(crate) struct Decoration {
    pub(crate) underline: bool,
    pub(crate) line_through: bool,
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
