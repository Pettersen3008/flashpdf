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

#[derive(Default)]
pub(crate) struct LayoutBuffer {
    pub(crate) lines: Vec<PositionedLine>,
    pub(crate) segments: Vec<Segment>,
    pub(crate) text: Vec<u8>,
    pub(crate) boxes: Vec<PositionedBox>,
}

impl LayoutBuffer {
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.segments.clear();
        self.text.clear();
        self.boxes.clear();
    }

    pub(crate) fn used_fonts(&self) -> impl Iterator<Item = FontId> + '_ {
        self.segments.iter().map(|segment| segment.font)
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.lines.capacity() * std::mem::size_of::<PositionedLine>()
            + self.segments.capacity() * std::mem::size_of::<Segment>()
            + self.text.capacity()
    }
}
