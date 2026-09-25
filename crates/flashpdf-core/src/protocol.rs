//! Binary command protocol: incremental decoding and boundary validation.
//!
//! Adapters own transport only. Every value is validated here before it becomes
//! a domain type, so the WASM and N-API bindings cannot diverge.

mod cursor;
mod error;
mod opcode;

#[cfg(test)]
mod tests;

pub use error::ProtocolError;

use cursor::{nonnegative_f32, positive_f32, pt, Cursor};
use opcode::Opcode;

use crate::command::MAX_LAYOUT_DEPTH;
use crate::{Edges, FontId, OwnedCommand, Rgb, TextAlign, TextStyle};

/// Bytes an adapter accepts per `push`. Sized so no adapter buffers a document.
pub const INPUT_CAPACITY: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
/// An open Stack, Box or Row buffers its commands until it closes; this bounds that buffer.
pub const MAX_BLOCK_BYTES: usize = 16 * 1024 * 1024;
const HEADER_LEN: usize = 18;
const MAGIC: &[u8; 4] = b"FPDF";
const VERSION: u16 = 2;

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    page: Option<crate::Page>,
    renderer: Option<crate::Renderer>,
    block: Vec<OwnedCommand>,
    block_bytes: usize,
    frames: Vec<Frame>,
    ended: bool,
    fonts: Vec<crate::EmbeddedFont>,
}

#[derive(Clone, Copy)]
enum Frame {
    Stack,
    Box,
    Row { columns: usize, cells: usize },
}

impl Decoder {
    /// Font files bypass the bounded document window because a whole TTF must
    /// remain available until the PDF's font stream is written.
    pub fn add_font(&mut self, bytes: Vec<u8>) -> Result<u8, ProtocolError> {
        if self.page.is_some() {
            return Err(ProtocolError::FontsAfterDocument);
        }
        if self.fonts.len() >= 254 {
            return Err(ProtocolError::TooManyFonts);
        }
        self.fonts
            .push(crate::EmbeddedFont::parse(bytes).map_err(ProtocolError::InvalidFont)?);
        Ok((self.fonts.len() + 1) as u8)
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
        if self.ended && !bytes.is_empty() {
            return Err(ProtocolError::DataAfterEnd);
        }
        self.pending.extend_from_slice(bytes);
        let mut pending = std::mem::take(&mut self.pending);
        let mut consumed = 0;
        let result = (|| {
            if self.page.is_none() {
                if pending.len() < HEADER_LEN {
                    return Ok(());
                }
                self.read_header(&pending[..HEADER_LEN])?;
                consumed = HEADER_LEN;
            }
            loop {
                let remaining = &pending[consumed..];
                if remaining.len() < 5 {
                    return Ok(());
                }
                let length = u32::from_le_bytes(remaining[1..5].try_into().unwrap()) as usize;
                if length > MAX_RECORD {
                    return Err(ProtocolError::RecordTooLarge);
                }
                let total = 5 + length;
                if remaining.len() < total {
                    return Ok(());
                }
                let start = consumed;
                consumed += total;
                self.read_record(pending[start], &pending[start + 5..consumed])?;
            }
        })();
        if consumed > 0 {
            let remaining = pending.len() - consumed;
            pending.copy_within(consumed.., 0);
            pending.truncate(remaining);
        }
        self.pending = pending;
        result
    }

    fn read_header(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
        if &bytes[..4] != MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION {
            return Err(ProtocolError::UnknownVersion);
        }
        let width = positive_f32(&bytes[6..10], "page width")?;
        let height = positive_f32(&bytes[10..14], "page height")?;
        let margin = nonnegative_f32(&bytes[14..18], "margin")?;
        self.page = Some(crate::Page::new(pt(width)?, pt(height)?, pt(margin)?)?);
        Ok(())
    }

    fn read_record(&mut self, opcode: u8, payload: &[u8]) -> Result<(), ProtocolError> {
        if self.ended {
            return Err(ProtocolError::DataAfterEnd);
        }
        let mut cursor = Cursor::new(payload);
        let opcode = Opcode::try_from(opcode)?;
        match opcode {
            Opcode::Text => {
                let size = pt(cursor.positive_f32("font size")?)?;
                let text = cursor.text(false)?.to_owned();
                cursor.done()?;
                self.layout_command(OwnedCommand::Text(text, TextStyle::plain(size)))
            }
            Opcode::Spacer => {
                let space = pt(cursor.nonnegative_f32("spacer")?)?;
                cursor.done()?;
                self.layout_command(OwnedCommand::Spacer(space))
            }
            Opcode::StackStart => {
                let gap = pt(cursor.nonnegative_f32("stack gap")?)?;
                cursor.done()?;
                self.open(Frame::Stack, OwnedCommand::StackStart(gap))
            }
            Opcode::StackEnd => {
                cursor.done()?;
                self.close(false, OwnedCommand::StackEnd)
            }
            Opcode::RowStart => {
                let columns = cursor.columns()?;
                cursor.done()?;
                self.open(
                    Frame::Row {
                        columns: columns.len(),
                        cells: 0,
                    },
                    OwnedCommand::RowStart(columns),
                )
            }
            Opcode::RowEnd => {
                cursor.done()?;
                self.close(true, OwnedCommand::RowEnd)
            }
            Opcode::PageBreak => {
                cursor.done()?;
                if !self.frames.is_empty() {
                    return Err(ProtocolError::InvalidNesting);
                }
                self.ensure_renderer()?.page_break();
                Ok(())
            }
            Opcode::StyledText | Opcode::Footer => {
                let footer = matches!(opcode, Opcode::Footer);
                let size = pt(cursor.positive_f32("font size")?)?;
                let align = match cursor.take(1)?[0] {
                    0 => TextAlign::Left,
                    1 => TextAlign::Center,
                    2 => TextAlign::Right,
                    _ => return Err(ProtocolError::InvalidTextAlignment),
                };
                let color = Rgb::from(<[u8; 3]>::try_from(cursor.take(3)?).unwrap());
                let font = FontId::new(cursor.take(1)?[0]);
                let font_count = self
                    .renderer
                    .as_ref()
                    .map_or(self.fonts.len() + 2, crate::Renderer::font_count);
                if font.index() >= font_count {
                    return Err(ProtocolError::UnknownFont);
                }
                let text = cursor.text(footer)?.to_owned();
                cursor.done()?;
                let style = TextStyle {
                    size,
                    align,
                    color,
                    font,
                };
                if footer {
                    if self.renderer.is_some() || !self.frames.is_empty() {
                        return Err(ProtocolError::InvalidNesting);
                    }
                    self.ensure_renderer()?
                        .set_footer(text, style)
                        .map_err(ProtocolError::from)
                } else {
                    self.layout_command(OwnedCommand::Text(text, style))
                }
            }
            Opcode::BoxStart => {
                let margin = Edges {
                    top: pt(cursor.nonnegative_f32("margin top")?)?,
                    right: pt(cursor.nonnegative_f32("margin right")?)?,
                    bottom: pt(cursor.nonnegative_f32("margin bottom")?)?,
                    left: pt(cursor.nonnegative_f32("margin left")?)?,
                };
                let padding = Edges {
                    top: pt(cursor.nonnegative_f32("padding top")?)?,
                    right: pt(cursor.nonnegative_f32("padding right")?)?,
                    bottom: pt(cursor.nonnegative_f32("padding bottom")?)?,
                    left: pt(cursor.nonnegative_f32("padding left")?)?,
                };
                let border = Edges {
                    top: pt(cursor.nonnegative_f32("border top width")?)?,
                    right: pt(cursor.nonnegative_f32("border right width")?)?,
                    bottom: pt(cursor.nonnegative_f32("border bottom width")?)?,
                    left: pt(cursor.nonnegative_f32("border left width")?)?,
                };
                let background = match cursor.take(1)?[0] {
                    0 => None,
                    1 => Some(Rgb::from(<[u8; 3]>::try_from(cursor.take(3)?).unwrap())),
                    _ => return Err(ProtocolError::InvalidBackground),
                };
                let border_color = Edges {
                    top: Rgb::from(<[u8; 3]>::try_from(cursor.take(3)?).unwrap()),
                    right: Rgb::from(<[u8; 3]>::try_from(cursor.take(3)?).unwrap()),
                    bottom: Rgb::from(<[u8; 3]>::try_from(cursor.take(3)?).unwrap()),
                    left: Rgb::from(<[u8; 3]>::try_from(cursor.take(3)?).unwrap()),
                };
                cursor.done()?;
                self.open(
                    Frame::Box,
                    OwnedCommand::BoxStart(crate::BoxStyle {
                        margin,
                        padding,
                        border,
                        background,
                        border_color,
                    }),
                )
            }
            Opcode::BoxEnd => {
                cursor.done()?;
                self.close_box()
            }
            Opcode::End => {
                cursor.done()?;
                if !self.frames.is_empty() {
                    return Err(ProtocolError::InvalidNesting);
                }
                if self.renderer.is_none() {
                    return Err(ProtocolError::EmptyDocument);
                }
                self.ended = true;
                Ok(())
            }
        }
    }

    fn ensure_renderer(&mut self) -> Result<&mut crate::Renderer, ProtocolError> {
        let page = self.page.ok_or(ProtocolError::MissingHeader)?;
        if self.renderer.is_none() {
            self.renderer = Some(crate::Renderer::with_fonts(
                page,
                std::mem::take(&mut self.fonts),
            ));
        }
        Ok(self.renderer.as_mut().unwrap())
    }

    fn push_command(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        let payload = match &command {
            OwnedCommand::Text(text, _) => text.len(),
            OwnedCommand::RowStart(columns) => columns.len() * size_of::<crate::ColumnWidth>(),
            _ => 0,
        };
        self.block_bytes += payload + size_of::<OwnedCommand>();
        if self.block_bytes > MAX_BLOCK_BYTES {
            return Err(ProtocolError::BlockTooLarge);
        }
        self.block.push(command);
        Ok(())
    }

    fn layout_command(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        self.push_command(command)?;
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn open(&mut self, frame: Frame, command: OwnedCommand) -> Result<(), ProtocolError> {
        if self.frames.len() >= MAX_LAYOUT_DEPTH {
            return Err(ProtocolError::NestingTooDeep);
        }
        self.push_command(command)?;
        self.frames.push(frame);
        Ok(())
    }

    fn close(&mut self, row: bool, command: OwnedCommand) -> Result<(), ProtocolError> {
        let frame = self.frames.pop().ok_or(ProtocolError::InvalidNesting)?;
        match (row, frame) {
            (false, Frame::Stack) => {}
            (true, Frame::Row { columns, cells }) if columns == cells => {}
            (true, Frame::Row { .. }) => return Err(ProtocolError::RowCellCountMismatch),
            _ => return Err(ProtocolError::InvalidNesting),
        }
        self.push_command(command)?;
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn close_box(&mut self) -> Result<(), ProtocolError> {
        if !matches!(self.frames.pop(), Some(Frame::Box)) {
            return Err(ProtocolError::InvalidNesting);
        }
        self.push_command(OwnedCommand::BoxEnd)?;
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn complete_child(&mut self) -> Result<(), ProtocolError> {
        if let Some(Frame::Row { columns, cells }) = self.frames.last_mut() {
            *cells += 1;
            if *cells > *columns {
                return Err(ProtocolError::RowCellCountMismatch);
            }
        }
        Ok(())
    }

    fn flush_block(&mut self) -> Result<(), ProtocolError> {
        let page = self.page.ok_or(ProtocolError::MissingHeader)?;
        let Self {
            renderer, block, ..
        } = self;
        renderer
            .get_or_insert_with(|| {
                crate::Renderer::with_fonts(page, std::mem::take(&mut self.fonts))
            })
            .push_owned(block)
            .map_err(ProtocolError::from)?;
        block.clear();
        self.block_bytes = 0;
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, ProtocolError> {
        if self.page.is_none() || !self.ended || !self.pending.is_empty() {
            return Err(ProtocolError::TruncatedProtocol);
        }
        self.renderer
            .map(crate::Renderer::finish)
            .ok_or(ProtocolError::EmptyDocument)
    }
}
