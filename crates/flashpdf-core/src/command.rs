use crate::document::{Block, BoxNode, Cell, Document, Element, Row, Spacer, Stack, TextNode};
use crate::{BoxStyle, Fraction, Percent, Pt, TextStyle};

pub(crate) const MAX_LAYOUT_DEPTH: usize = 64;
pub(crate) const MAX_COLUMNS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    Text { text: &'a str, style: TextStyle },
    BoxStart { style: BoxStyle },
    BoxEnd,
    Spacer(Pt),
    StackStart { gap: Pt },
    StackEnd,
    RowStart { columns: &'a [ColumnWidth] },
    RowEnd,
    PageBreak,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pt),
    Fraction(Fraction),
    Percent(Percent),
}

/// The protocol's decoded form owns data across input chunks.
pub(crate) enum OwnedCommand {
    Text(String, TextStyle),
    BoxStart(BoxStyle),
    BoxEnd,
    Spacer(Pt),
    StackStart(Pt),
    StackEnd,
    RowStart(Vec<ColumnWidth>),
    RowEnd,
}

impl OwnedCommand {
    fn borrow(&self) -> Command<'_> {
        match self {
            Self::Text(text, style) => Command::Text {
                text,
                style: *style,
            },
            Self::BoxStart(style) => Command::BoxStart { style: *style },
            Self::BoxEnd => Command::BoxEnd,
            Self::Spacer(value) => Command::Spacer(*value),
            Self::StackStart(gap) => Command::StackStart { gap: *gap },
            Self::StackEnd => Command::StackEnd,
            Self::RowStart(columns) => Command::RowStart { columns },
            Self::RowEnd => Command::RowEnd,
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
            Command::Text { text, style } => Ok(Element::Text(TextNode { text, style })),
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
            Command::BoxEnd | Command::StackEnd | Command::RowEnd | Command::PageBreak => {
                Err(CommandParseError::UnexpectedCommand)
            }
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
