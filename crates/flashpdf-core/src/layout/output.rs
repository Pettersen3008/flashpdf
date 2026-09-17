use std::ops::Range;

use crate::geometry::Point;
use crate::{BoxStyle, FontId, Pt, TextStyle};

pub(crate) struct PositionedLine {
    pub(crate) text: Range<usize>,
    pub(crate) origin: Point,
    pub(crate) available_width: Pt,
    pub(crate) text_width: Pt,
    pub(crate) style: TextStyle,
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
    pub(crate) text: Vec<u8>,
    pub(crate) boxes: Vec<PositionedBox>,
}

impl LayoutBuffer {
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
        self.text.clear();
        self.boxes.clear();
    }

    pub(crate) fn used_fonts(&self) -> impl Iterator<Item = FontId> + '_ {
        self.lines.iter().map(|line| line.style.font)
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.lines.capacity() * std::mem::size_of::<PositionedLine>() + self.text.capacity()
    }
}
