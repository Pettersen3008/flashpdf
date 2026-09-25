mod output;
mod row;
mod text;

pub(crate) use output::{LayoutBuffer, Segment};

use crate::document::{BoxNode, Element, Row, Stack};
use crate::font::FontBook;
use crate::geometry::LayoutArea;
use crate::layout::output::PositionedBox;
use crate::{FontId, Pt, RenderError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayoutError {
    InvalidWidth,
    UnknownFont(FontId),
    InvalidFontSize,
    UnsupportedCharacter(char),
    MissingGlyph,
    TextTooWide,
    InvalidBox,
    InvalidColumnWidth,
    ColumnsOverflow,
    InvalidRuns,
}

impl From<LayoutError> for RenderError {
    fn from(error: LayoutError) -> Self {
        match error {
            LayoutError::InvalidFontSize => Self::InvalidFontSize,
            LayoutError::UnsupportedCharacter(character) => Self::UnsupportedCharacter(character),
            LayoutError::MissingGlyph => Self::MissingGlyph,
            LayoutError::TextTooWide => Self::TextTooWide,
            LayoutError::InvalidWidth
            | LayoutError::UnknownFont(_)
            | LayoutError::InvalidBox
            | LayoutError::InvalidColumnWidth
            | LayoutError::ColumnsOverflow
            | LayoutError::InvalidRuns => Self::InvalidLayout,
        }
    }
}

pub(crate) struct LayoutEngine<'a> {
    fonts: &'a FontBook,
}

impl<'a> LayoutEngine<'a> {
    pub(crate) fn new(fonts: &'a FontBook) -> Self {
        Self { fonts }
    }

    pub(crate) fn layout(
        &self,
        element: &Element<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        if !area.width.get().is_finite() || area.width <= Pt::ZERO {
            return Err(LayoutError::InvalidWidth);
        }
        match element {
            Element::Paragraph(paragraph) => {
                text::ParagraphLayouter::layout(self.fonts, paragraph, area, output)
            }
            Element::Box(node) => self.layout_box(node, area, output),
            Element::Spacer(spacer) => Ok(spacer.height),
            Element::Stack(stack) => self.layout_stack(stack, area, output),
            Element::Row(row) => self.layout_row(row, area, output),
        }
    }

    fn layout_box(
        &self,
        node: &BoxNode<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let style = node.style;
        let horizontal = style.margin.right
            + style.margin.left
            + style.padding.right
            + style.padding.left
            + style.border.right
            + style.border.left;
        if horizontal >= area.width
            || style
                .margin
                .iter()
                .chain(style.padding.iter())
                .any(|value| *value < Pt::ZERO)
        {
            return Err(LayoutError::InvalidBox);
        }
        let inset_x = style.margin.left + style.padding.left + style.border.left;
        let inset_y = style.margin.top + style.padding.top + style.border.top;
        let inner_width = area.width - horizontal;
        let paint = (style.background.is_some()
            || style.border.iter().any(|edge| *edge > Pt::ZERO))
        .then(|| {
            let index = output.boxes.len();
            output.boxes.push(PositionedBox {
                origin: area.origin,
                width: Pt::ZERO,
                height: Pt::ZERO,
                style,
            });
            index
        });
        let mut inner_height = Pt::ZERO;
        for child in &node.children {
            inner_height += self.layout(
                child,
                area.translated(inset_x, inset_y + inner_height)
                    .with_width(inner_width),
                output,
            )?;
        }
        let box_height = inner_height
            + style.padding.top
            + style.padding.bottom
            + style.border.top
            + style.border.bottom;
        if let Some(index) = paint {
            output.boxes[index] = PositionedBox {
                origin: area.origin.translated(style.margin.left, style.margin.top),
                width: area.width - style.margin.right - style.margin.left,
                height: box_height,
                style,
            };
        }
        Ok(style.margin.top + box_height + style.margin.bottom)
    }

    fn layout_stack(
        &self,
        stack: &Stack<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let mut height = Pt::ZERO;
        for (index, child) in stack.children.iter().enumerate() {
            if index != 0 {
                height += stack.gap;
            }
            height += self.layout(child, area.translated(Pt::ZERO, height), output)?;
        }
        Ok(height)
    }

    fn layout_row(
        &self,
        row: &Row<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let widths = row::resolve_column_widths(&row.cells, area.width)?;
        let mut offset = Pt::ZERO;
        let mut height = Pt::ZERO;
        for (cell, width) in row.cells.iter().zip(widths) {
            let cell_height = self.layout(
                &cell.content,
                area.translated(offset, Pt::ZERO).with_width(width),
                output,
            )?;
            if cell_height > height {
                height = cell_height;
            }
            offset += width;
        }
        Ok(height)
    }
}
