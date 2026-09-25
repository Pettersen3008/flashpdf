use crate::document::{
    Block, BoxNode, Cell, Document, Element, ImageNode, Paragraph, Row, Spacer, Stack, Table,
};
use crate::{BoxStyle, Fraction, ImageSize, Percent, Pt, TextAlign, TextRun, TextStyle};

pub(crate) const MAX_LAYOUT_DEPTH: usize = 64;
pub(crate) const MAX_COLUMNS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    /// One run in one style; the parser lowers it to a one-run `Paragraph`.
    Text {
        text: &'a str,
        style: TextStyle,
    },
    /// `runs` slice `text` consecutively and must cover it exactly on char boundaries.
    Paragraph {
        align: TextAlign,
        text: &'a str,
        runs: &'a [TextRun],
        /// URIs the runs' `decoration.link` index into.
        links: &'a [String],
    },
    BoxStart {
        style: BoxStyle,
    },
    BoxEnd,
    Spacer(Pt),
    StackStart {
        gap: Pt,
    },
    StackEnd,
    RowStart {
        columns: &'a [ColumnWidth],
    },
    RowEnd,
    /// The first `header_rows` children repeat at the top of every page the table continues on.
    TableStart {
        header_rows: u16,
        width: ColumnWidth,
    },
    TableEnd,
    PageBreak,
    /// A registered image as an atomic block box; `height` follows the aspect ratio when absent.
    Image {
        slot: u16,
        width: ImageSize,
        height: Option<Pt>,
        link: Option<&'a str>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pt),
    Fraction(Fraction),
    Percent(Percent),
}

/// The protocol's decoded form owns data across input chunks.
pub(crate) enum OwnedCommand {
    Paragraph(TextAlign, String, Vec<TextRun>, Vec<String>),
    BoxStart(BoxStyle),
    BoxEnd,
    Spacer(Pt),
    StackStart(Pt),
    StackEnd,
    RowStart(Vec<ColumnWidth>),
    RowEnd,
    TableStart(u16, ColumnWidth),
    TableEnd,
    Image(u16, ImageSize, Option<Pt>, Option<String>),
}

impl OwnedCommand {
    fn borrow(&self) -> Command<'_> {
        match self {
            Self::Paragraph(align, text, runs, links) => Command::Paragraph {
                align: *align,
                text,
                runs,
                links,
            },
            Self::BoxStart(style) => Command::BoxStart { style: *style },
            Self::BoxEnd => Command::BoxEnd,
            Self::Spacer(value) => Command::Spacer(*value),
            Self::StackStart(gap) => Command::StackStart { gap: *gap },
            Self::StackEnd => Command::StackEnd,
            Self::RowStart(columns) => Command::RowStart { columns },
            Self::RowEnd => Command::RowEnd,
            Self::TableStart(header_rows, width) => Command::TableStart {
                header_rows: *header_rows,
                width: *width,
            },
            Self::TableEnd => Command::TableEnd,
            Self::Image(slot, width, height, link) => Command::Image {
                slot: *slot,
                width: *width,
                height: *height,
                link: link.as_deref(),
            },
        }
    }
}

pub(crate) trait CommandSource {
    fn command(&self, index: usize) -> Option<Command<'_>>;
    fn len(&self) -> usize;
}

impl<'command> CommandSource for [Command<'command>] {
    fn command(&self, index: usize) -> Option<Command<'_>> {
        self.get(index).copied()
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
        if self.peek() == Some(Command::PageBreak) {
            self.index += 1;
            Ok(Block::PageBreak)
        } else {
            Ok(Block::Element(self.parse_element(0)?))
        }
    }

    fn parse_element(&mut self, depth: usize) -> Result<Element<'a>, CommandParseError> {
        match self.next()? {
            Command::Text { text, style } => Ok(Element::Paragraph(Paragraph {
                align: style.align,
                text,
                runs: vec![TextRun {
                    len: text.len(),
                    font: style.font,
                    size: style.size,
                    color: style.color,
                    hard_break: false,
                    decoration: Default::default(),
                }],
                links: &[],
            })),
            Command::Paragraph {
                align,
                text,
                runs,
                links,
            } => Ok(Element::Paragraph(Paragraph {
                align,
                text,
                runs: runs.to_vec(),
                links,
            })),
            Command::Image {
                slot,
                width,
                height,
                link,
            } => Ok(Element::Image(ImageNode {
                slot,
                width,
                height,
                link,
            })),
            Command::Spacer(height) => Ok(Element::Spacer(Spacer { height })),
            Command::BoxStart { style } => {
                self.check_depth(depth)?;
                let children = self.parse_children(Command::BoxEnd, depth + 1)?;
                Ok(Element::Box(BoxNode { style, children }))
            }
            Command::StackStart { gap } => {
                self.check_depth(depth)?;
                let children = self.parse_children(Command::StackEnd, depth + 1)?;
                Ok(Element::Stack(Stack { gap, children }))
            }
            Command::RowStart { columns } => {
                self.check_depth(depth)?;
                if columns.is_empty() || columns.len() > MAX_COLUMNS {
                    return Err(CommandParseError::InvalidRow);
                }
                let mut cells = Vec::with_capacity(columns.len());
                for width in columns {
                    cells.push(Cell {
                        width: *width,
                        content: self.parse_element(depth + 1)?,
                    });
                }
                if self.next()? != Command::RowEnd {
                    return Err(CommandParseError::InvalidRow);
                }
                Ok(Element::Row(Row { cells }))
            }
            Command::TableStart { header_rows, width } => {
                self.check_depth(depth)?;
                let rows = self.parse_children(Command::TableEnd, depth + 1)?;
                let header_rows = usize::from(header_rows);
                let mut columns = None;
                if header_rows > rows.len()
                    || rows.iter().any(|row| !uniform_rows(row, &mut columns))
                {
                    return Err(CommandParseError::InvalidTable);
                }
                Ok(Element::Table(Table {
                    header_rows,
                    width,
                    rows,
                }))
            }
            Command::BoxEnd
            | Command::StackEnd
            | Command::RowEnd
            | Command::TableEnd
            | Command::PageBreak => Err(CommandParseError::UnexpectedCommand),
        }
    }

    fn parse_children(
        &mut self,
        end: Command<'_>,
        depth: usize,
    ) -> Result<Vec<Element<'a>>, CommandParseError> {
        let mut children = Vec::new();
        loop {
            if self.peek() == Some(end) {
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
/// same cell count; `columns` learns it from the first Row.
fn uniform_rows(element: &Element<'_>, columns: &mut Option<usize>) -> bool {
    match element {
        Element::Row(row) => *columns.get_or_insert(row.cells.len()) == row.cells.len(),
        Element::Box(BoxNode { children, .. }) | Element::Stack(Stack { children, .. }) => {
            children.iter().all(|child| uniform_rows(child, columns))
        }
        Element::Paragraph(_) | Element::Spacer(_) | Element::Table(_) | Element::Image(_) => false,
    }
}
