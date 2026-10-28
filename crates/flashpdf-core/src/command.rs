use std::mem::discriminant;

use crate::document::{
    Block, BoxNode, Cell, Document, Element, FlexNode, ImageNode, Paragraph, Row, Spacer, Stack,
    Table,
};
use crate::{
    BoxSize, BoxStyle, Direction, Flex, FlexItem, Fraction, ImageSize, Percent, Pt, TextAlign,
    TextRun, TextStyle, VerticalAlign,
};

pub(crate) const MAX_LAYOUT_DEPTH: usize = 64;
pub(crate) const MAX_COLUMNS: usize = 256;

#[derive(Clone, Debug, PartialEq)]
pub enum Command<'a> {
    /// One run in one style; the parser lowers it to a one-run `Paragraph`.
    Text {
        text: &'a str,
        style: TextStyle,
    },
    /// `runs` slice `text` consecutively and must cover it exactly on char boundaries.
    Paragraph {
        align: TextAlign,
        direction: Direction,
        orphans: u16,
        widows: u16,
        text: &'a str,
        runs: &'a [TextRun],
        /// URIs the runs' `decoration.link` index into.
        links: &'a [String],
    },
    BoxStart {
        style: BoxStyle,
        size: BoxSize,
    },
    BoxEnd,
    Spacer(Pt),
    StackStart {
        gap: Pt,
    },
    StackEnd,
    /// `slots` has one entry per column; empty gives every column its own top-aligned cell.
    RowStart {
        columns: &'a [ColumnWidth],
        slots: &'a [Slot],
    },
    RowEnd,
    /// A CSS flex container with exactly one child per item.
    FlexStart {
        style: Flex,
        items: &'a [FlexItem],
    },
    FlexEnd,
    /// The first `header_rows` children repeat at the top of every page the table continues on,
    /// the next `footer_rows` after the last body row of every page. Body rows follow them.
    TableStart {
        header_rows: u16,
        footer_rows: u16,
        width: ColumnWidth,
    },
    TableEnd,
    PageBreak,
    /// Avoids a page break at this point, between the blocks around it.
    Avoid,
    /// A registered image as an atomic block box; `height` follows the aspect ratio when absent.
    Image {
        slot: u16,
        width: ImageSize,
        height: Option<Pt>,
        /// CSS 2.1 §10.4: with `width` and `height` both auto, the image scales uniformly into these.
        max_width: ImageSize,
        max_height: Option<Pt>,
        link: Option<&'a str>,
        alt: &'a str,
        alpha: u8,
    },
    /// The destination slot of a registered anchor, set at the top of the next line laid out,
    /// which a page break may carry onto a later page.
    Anchor(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pt),
    Fraction(Fraction),
    Percent(Percent),
}

/// What a row's column holds. Rows that a `rowspan` joins must be the
/// children of one Stack, which lays them out together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Starts a cell that spans `rows` rows.
    Cell { rows: u16, align: VerticalAlign },
    /// Widens the cell to its left (`colspan`).
    Join,
    /// A cell from an earlier row covers this column.
    Covered,
}

/// The protocol's decoded form owns data across input chunks.
#[derive(Clone)]
pub(crate) enum OwnedCommand {
    Paragraph(
        (TextAlign, Direction),
        [u16; 2],
        String,
        Vec<TextRun>,
        Vec<String>,
    ),
    BoxStart(BoxStyle, BoxSize),
    BoxEnd,
    Spacer(Pt),
    StackStart(Pt),
    StackEnd,
    RowStart(Vec<ColumnWidth>, Vec<Slot>),
    RowEnd,
    FlexStart(Flex, Vec<FlexItem>),
    FlexEnd,
    TableStart(u16, u16, ColumnWidth),
    TableEnd,
    Image(
        u16,
        ImageSize,
        Option<Pt>,
        ImageSize,
        Option<Pt>,
        Option<String>,
        String,
        u8,
    ),
    Anchor(u16),
}

impl OwnedCommand {
    fn borrow(&self) -> Command<'_> {
        match self {
            Self::Paragraph((align, direction), [orphans, widows], text, runs, links) => {
                Command::Paragraph {
                    align: *align,
                    direction: *direction,
                    orphans: *orphans,
                    widows: *widows,
                    text,
                    runs,
                    links,
                }
            }
            Self::BoxStart(style, size) => Command::BoxStart {
                style: style.clone(),
                size: *size,
            },
            Self::BoxEnd => Command::BoxEnd,
            Self::Spacer(value) => Command::Spacer(*value),
            Self::StackStart(gap) => Command::StackStart { gap: *gap },
            Self::StackEnd => Command::StackEnd,
            Self::RowStart(columns, slots) => Command::RowStart { columns, slots },
            Self::RowEnd => Command::RowEnd,
            Self::FlexStart(style, items) => Command::FlexStart {
                style: *style,
                items,
            },
            Self::FlexEnd => Command::FlexEnd,
            Self::TableStart(header_rows, footer_rows, width) => Command::TableStart {
                header_rows: *header_rows,
                footer_rows: *footer_rows,
                width: *width,
            },
            Self::TableEnd => Command::TableEnd,
            Self::Image(slot, width, height, max_width, max_height, link, alt, alpha) => {
                Command::Image {
                    slot: *slot,
                    width: *width,
                    height: *height,
                    max_width: *max_width,
                    max_height: *max_height,
                    link: link.as_deref(),
                    alt,
                    alpha: *alpha,
                }
            }
            Self::Anchor(slot) => Command::Anchor(*slot),
        }
    }
}

pub(crate) trait CommandSource {
    fn command(&self, index: usize) -> Option<Command<'_>>;
    fn len(&self) -> usize;
}

impl<'command> CommandSource for [Command<'command>] {
    fn command(&self, index: usize) -> Option<Command<'_>> {
        self.get(index).cloned()
    }

    fn len(&self) -> usize {
        <[Command<'command>]>::len(self)
    }
}

impl CommandSource for [OwnedCommand] {
    fn command(&self, index: usize) -> Option<Command<'_>> {
        self.get(index).map(OwnedCommand::borrow)
    }

    fn len(&self) -> usize {
        <[OwnedCommand]>::len(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandParseError {
    UnexpectedCommand,
    UnexpectedEnd,
    InvalidRow,
    InvalidTable,
    MaxDepthExceeded,
}

pub(crate) struct CommandParser<'a, S: ?Sized> {
    source: &'a S,
    index: usize,
}

impl<'a, S: CommandSource + ?Sized> CommandParser<'a, S> {
    pub(crate) fn new(source: &'a S) -> Self {
        Self { source, index: 0 }
    }

    pub(crate) fn parse_document(mut self) -> Result<Document<'a>, CommandParseError> {
        let mut blocks = Vec::new();
        while self.index < self.source.len() {
            blocks.push(self.parse_block()?);
        }
        Ok(Document { blocks })
    }

    pub(crate) fn parse_single_block(mut self) -> Result<Block<'a>, CommandParseError> {
        let block = self.parse_block()?;
        if self.index == self.source.len() {
            Ok(block)
        } else {
            Err(CommandParseError::UnexpectedCommand)
        }
    }

    fn parse_block(&mut self) -> Result<Block<'a>, CommandParseError> {
        let block = match self.peek() {
            Some(Command::PageBreak) => Block::PageBreak,
            Some(Command::Avoid) => Block::Avoid,
            _ => return Ok(Block::Element(self.parse_element(0)?)),
        };
        self.index += 1;
        Ok(block)
    }

    fn parse_element(&mut self, depth: usize) -> Result<Element<'a>, CommandParseError> {
        match self.next()? {
            Command::Text { text, style } => Ok(Element::Paragraph(Paragraph {
                align: style.align,
                direction: Direction::Ltr,
                orphans: 2,
                widows: 2,
                text,
                runs: vec![TextRun {
                    len: text.len(),
                    font: style.font,
                    size: style.size,
                    line_height: None,
                    color: style.color,
                    hard_break: false,
                    decoration: Default::default(),
                    typography: style.typography,
                }],
                links: &[],
            })),
            Command::Paragraph {
                align,
                direction,
                orphans,
                widows,
                text,
                runs,
                links,
            } => Ok(Element::Paragraph(Paragraph {
                align,
                direction,
                orphans,
                widows,
                text,
                runs: runs.to_vec(),
                links,
            })),
            Command::Image {
                slot,
                width,
                height,
                max_width,
                max_height,
                link,
                alt,
                alpha,
            } => Ok(Element::Image(ImageNode {
                slot,
                width,
                height,
                max_width,
                max_height,
                link,
                alt,
                alpha,
            })),
            Command::Spacer(height) => Ok(Element::Spacer(Spacer { height })),
            Command::Anchor(slot) => Ok(Element::Anchor(slot)),
            Command::BoxStart { style, size } => {
                self.check_depth(depth)?;
                let children = self.parse_children(Command::BoxEnd, depth + 1)?;
                Ok(Element::Box(BoxNode {
                    style,
                    size,
                    children,
                }))
            }
            Command::StackStart { gap } => {
                self.check_depth(depth)?;
                let children = self.parse_children(Command::StackEnd, depth + 1)?;
                Ok(Element::Stack(Stack { gap, children }))
            }
            Command::RowStart { columns, slots } => {
                self.check_depth(depth)?;
                if columns.is_empty()
                    || columns.len() > MAX_COLUMNS
                    || !(slots.is_empty() || slots.len() == columns.len())
                {
                    return Err(CommandParseError::InvalidRow);
                }
                let mut cells: Vec<Cell<'a>> = Vec::with_capacity(columns.len());
                for column in 0..columns.len() {
                    match slots.get(column).copied().unwrap_or(Slot::Cell {
                        rows: 1,
                        align: VerticalAlign::Top,
                    }) {
                        Slot::Cell { rows, align } => {
                            let mut anchors = Vec::new();
                            while let Some(Command::Anchor(slot)) = self.peek() {
                                self.index += 1;
                                anchors.push(slot);
                            }
                            cells.push(Cell {
                                anchors,
                                column,
                                span: 1,
                                rows,
                                align,
                                content: self.parse_element(depth + 1)?,
                            });
                        }
                        Slot::Join => match cells.last_mut() {
                            Some(cell) if cell.column + cell.span == column => cell.span += 1,
                            _ => return Err(CommandParseError::InvalidRow),
                        },
                        Slot::Covered => {}
                    }
                }
                if !matches!(self.next()?, Command::RowEnd) {
                    return Err(CommandParseError::InvalidRow);
                }
                Ok(Element::Row(Row { columns, cells }))
            }
            Command::FlexStart { style, items } => {
                self.check_depth(depth)?;
                if items.is_empty() || items.len() > MAX_COLUMNS {
                    return Err(CommandParseError::InvalidRow);
                }
                let mut children = Vec::with_capacity(items.len());
                for _ in items {
                    children.push(self.parse_element(depth + 1)?);
                }
                if !matches!(self.next()?, Command::FlexEnd) {
                    return Err(CommandParseError::InvalidRow);
                }
                Ok(Element::Flex(FlexNode {
                    style,
                    items,
                    children,
                }))
            }
            Command::TableStart {
                header_rows,
                footer_rows,
                width,
            } => {
                self.check_depth(depth)?;
                let rows = self.parse_children(Command::TableEnd, depth + 1)?;
                let header_rows = usize::from(header_rows);
                let footer_rows = usize::from(footer_rows);
                let mut columns = None;
                if header_rows + footer_rows > rows.len()
                    || rows.iter().any(|row| !uniform_rows(row, &mut columns))
                {
                    return Err(CommandParseError::InvalidTable);
                }
                Ok(Element::Table(Table {
                    header_rows,
                    footer_rows,
                    width,
                    rows,
                }))
            }
            Command::BoxEnd
            | Command::StackEnd
            | Command::RowEnd
            | Command::FlexEnd
            | Command::TableEnd
            | Command::PageBreak
            | Command::Avoid => Err(CommandParseError::UnexpectedCommand),
        }
    }

    /// Comparing variants rather than whole commands keeps `Command`'s `PartialEq` out of the WASM.
    fn parse_children(
        &mut self,
        end: Command<'_>,
        depth: usize,
    ) -> Result<Vec<Element<'a>>, CommandParseError> {
        let mut children = Vec::new();
        loop {
            if self
                .peek()
                .is_some_and(|command| discriminant(&command) == discriminant(&end))
            {
                self.index += 1;
                return Ok(children);
            }
            if self.index == self.source.len() {
                return Err(CommandParseError::UnexpectedEnd);
            }
            children.push(self.parse_element(depth)?);
        }
    }

    fn check_depth(&self, depth: usize) -> Result<(), CommandParseError> {
        if depth < MAX_LAYOUT_DEPTH {
            Ok(())
        } else {
            Err(CommandParseError::MaxDepthExceeded)
        }
    }

    fn peek(&self) -> Option<Command<'a>> {
        self.source.command(self.index)
    }

    fn next(&mut self) -> Result<Command<'a>, CommandParseError> {
        let command = self
            .source
            .command(self.index)
            .ok_or(CommandParseError::UnexpectedEnd)?;
        self.index += 1;
        Ok(command)
    }
}

/// A table child is a Row, or a Box/Stack group of them, and every Row has the
/// same column count; `columns` learns it from the first Row.
pub(crate) fn uniform_rows(element: &Element<'_>, columns: &mut Option<usize>) -> bool {
    match element {
        Element::Row(row) => *columns.get_or_insert(row.columns.len()) == row.columns.len(),
        Element::Box(BoxNode { children, .. }) | Element::Stack(Stack { children, .. }) => {
            children.iter().all(|child| uniform_rows(child, columns))
        }
        Element::Paragraph(_)
        | Element::Spacer(_)
        | Element::Table(_)
        | Element::Image(_)
        | Element::Anchor(_)
        | Element::Flex(_) => false,
    }
}
