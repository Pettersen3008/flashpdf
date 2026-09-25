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
use crate::image::MAX_IMAGE_BYTES;
use crate::{Edges, FontId, OwnedCommand, RunDecoration, TextRun, TextStyle};

/// Bytes an adapter accepts per `push`. Sized so no adapter buffers a document.
pub const INPUT_CAPACITY: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
/// An open Stack, Box, Row or Table buffers its commands until it closes; this bounds that buffer.
pub const MAX_BLOCK_BYTES: usize = 16 * 1024 * 1024;
const HEADER_LEN: usize = 18;
const MAGIC: &[u8; 4] = b"FPDF";
const VERSION: u16 = 5;

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
    /// Images registered before the renderer exists; later ones go straight to it.
    images: Vec<crate::Image>,
    image_bytes: usize,
}

#[derive(Clone, Copy)]
enum Frame {
    Stack,
    Box,
    Row { columns: usize, cells: usize },
    Table,
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

    /// Images register at any point before the record that paints them, and are
    /// held whole until the PDF's XObject streams are written.
    pub fn add_image(&mut self, bytes: Vec<u8>) -> Result<u16, ProtocolError> {
        if self.ended {
            return Err(ProtocolError::DataAfterEnd);
        }
        self.image_bytes = self
            .image_bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_IMAGE_BYTES)
            .ok_or(ProtocolError::ImagesTooLarge)?;
        if self.image_count() >= usize::from(u16::MAX) {
            return Err(ProtocolError::TooManyImages);
        }
        let image = crate::Image::parse(bytes).map_err(ProtocolError::InvalidImage)?;
        Ok(match &mut self.renderer {
            Some(renderer) => renderer.add_image(image),
            None => {
                self.images.push(image);
                (self.images.len() - 1) as u16
            }
        })
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
                self.close(OwnedCommand::StackEnd)
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
                self.close(OwnedCommand::RowEnd)
            }
            Opcode::TableStart => {
                let header_rows = cursor.u16()?;
                let width = cursor.column()?;
                cursor.done()?;
                self.open(Frame::Table, OwnedCommand::TableStart(header_rows, width))
            }
            Opcode::TableEnd => {
                cursor.done()?;
                self.close(OwnedCommand::TableEnd)
            }
            Opcode::PageBreak => {
                cursor.done()?;
                if !self.frames.is_empty() {
                    return Err(ProtocolError::InvalidNesting);
                }
                self.ensure_renderer()?.page_break();
                Ok(())
            }
            Opcode::Footer => {
                let size = pt(cursor.positive_f32("font size")?)?;
                let align = cursor.align()?;
                let color = cursor.rgb()?;
                let font = self.font(cursor.take(1)?[0])?;
                let text = cursor.text(true)?.to_owned();
                cursor.done()?;
                if self.renderer.is_some() || !self.frames.is_empty() {
                    return Err(ProtocolError::InvalidNesting);
                }
                let style = TextStyle {
                    size,
                    align,
                    color,
                    font,
                };
                self.ensure_renderer()?
                    .set_footer(text, style)
                    .map_err(ProtocolError::from)
            }
            Opcode::Paragraph => {
                let align = cursor.align()?;
                let links: Vec<String> = (0..cursor.u16()?)
                    .map(|_| cursor.uri().map(str::to_owned))
                    .collect::<Result<_, _>>()?;
                let count = usize::from(cursor.u16()?);
                let mut text = String::new();
                let mut runs = Vec::with_capacity(count);
                for _ in 0..count {
                    let font = self.font(cursor.take(1)?[0])?;
                    let size = pt(cursor.positive_f32("font size")?)?;
                    let color = cursor.rgb()?;
                    // Bit 0 hard break, 1 underline, 2 line-through, 3 a u16 link index follows.
                    let flags = cursor.take(1)?[0];
                    if flags > 0x0f {
                        return Err(ProtocolError::InvalidValue("run flags"));
                    }
                    let link = if flags & 8 != 0 {
                        Some(cursor.u16()?)
                            .filter(|index| usize::from(*index) < links.len())
                            .ok_or(ProtocolError::InvalidValue("link index"))?
                            .into()
                    } else {
                        None
                    };
                    let run = cursor.text(false)?;
                    text.push_str(run);
                    runs.push(TextRun {
                        len: run.len(),
                        font,
                        size,
                        color,
                        hard_break: flags & 1 != 0,
                        decoration: RunDecoration {
                            underline: flags & 2 != 0,
                            line_through: flags & 4 != 0,
                            link,
                        },
                    });
                }
                cursor.done()?;
                self.layout_command(OwnedCommand::Paragraph(align, text, runs, links))
            }
            Opcode::Image => {
                let slot = cursor.u16()?;
                if usize::from(slot) >= self.image_count() {
                    return Err(ProtocolError::UnknownImage);
                }
                let width = cursor.image_size()?;
                let height = match cursor.take(1)?[0] {
                    0 => None,
                    1 => Some(pt(cursor.positive_f32("image height")?)?),
                    _ => return Err(ProtocolError::InvalidValue("image height kind")),
                };
                let link = match cursor.take(1)?[0] {
                    0 => None,
                    1 => Some(cursor.uri()?.to_owned()),
                    _ => return Err(ProtocolError::InvalidValue("image link flag")),
                };
                // Alt text rides in the record for a future tagged-PDF pass; nothing reads it yet.
                cursor.alt()?;
                cursor.done()?;
                self.layout_command(OwnedCommand::Image(slot, width, height, link))
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
                    1 => Some(cursor.rgb()?),
                    _ => return Err(ProtocolError::InvalidBackground),
                };
                let border_color = Edges {
                    top: cursor.rgb()?,
                    right: cursor.rgb()?,
                    bottom: cursor.rgb()?,
                    left: cursor.rgb()?,
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
                self.close(OwnedCommand::BoxEnd)
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

    /// Slots 0 and 1 are Helvetica; the rest are registered fonts.
    fn font(&self, slot: u8) -> Result<FontId, ProtocolError> {
        let font = FontId::new(slot);
        let count = self
            .renderer
            .as_ref()
            .map_or(self.fonts.len() + 2, crate::Renderer::font_count);
        if font.index() < count {
            Ok(font)
        } else {
            Err(ProtocolError::UnknownFont)
        }
    }

    fn image_count(&self) -> usize {
        self.renderer
            .as_ref()
            .map_or(self.images.len(), crate::Renderer::image_count)
    }

    fn ensure_renderer(&mut self) -> Result<&mut crate::Renderer, ProtocolError> {
        let page = self.page.ok_or(ProtocolError::MissingHeader)?;
        if self.renderer.is_none() {
            let mut renderer = crate::Renderer::with_fonts(page, std::mem::take(&mut self.fonts));
            for image in std::mem::take(&mut self.images) {
                renderer.add_image(image);
            }
            self.renderer = Some(renderer);
        }
        Ok(self.renderer.as_mut().unwrap())
    }

    fn push_command(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        let payload = match &command {
            OwnedCommand::Paragraph(_, text, runs, links) => {
                text.len()
                    + runs.len() * size_of::<TextRun>()
                    + links
                        .iter()
                        .map(|link| link.len() + size_of::<String>())
                        .sum::<usize>()
            }
            OwnedCommand::RowStart(columns) => columns.len() * size_of::<crate::ColumnWidth>(),
            OwnedCommand::Image(_, _, _, link) => link.as_ref().map_or(0, String::len),
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

    fn close(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        let frame = self.frames.pop().ok_or(ProtocolError::InvalidNesting)?;
        match (&command, frame) {
            (OwnedCommand::StackEnd, Frame::Stack)
            | (OwnedCommand::BoxEnd, Frame::Box)
            | (OwnedCommand::TableEnd, Frame::Table) => {}
            (OwnedCommand::RowEnd, Frame::Row { columns, cells }) if columns == cells => {}
            (OwnedCommand::RowEnd, Frame::Row { .. }) => {
                return Err(ProtocolError::RowCellCountMismatch)
            }
            _ => return Err(ProtocolError::InvalidNesting),
        }
        self.layout_command(command)
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
        let block = std::mem::take(&mut self.block);
        let result = self
            .ensure_renderer()?
            .push_owned(&block)
            .map_err(ProtocolError::from);
        self.block = block;
        self.block.clear();
        self.block_bytes = 0;
        result
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
