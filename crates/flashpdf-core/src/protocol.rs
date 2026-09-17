//! Binary command protocol: incremental decoding and boundary validation.
//!
//! Adapters own transport only. Every value is validated here before it becomes
//! a domain type, so the WASM and N-API bindings cannot diverge.

use crate::font::valid_win_ansi;
use crate::{Edges, FontId, Fraction, OwnedCommand, Percent, Rgb, TextAlign, TextStyle};

/// Bytes an adapter accepts per `push`. Sized so no adapter buffers a document.
pub const INPUT_CAPACITY: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
const HEADER_LEN: usize = 18;
const MAGIC: &[u8; 4] = b"FPDF";
const VERSION: u16 = 2;

#[derive(Clone, Copy)]
#[repr(u8)]
enum Opcode {
    Text = 1,
    Spacer = 2,
    StackStart = 3,
    StackEnd = 4,
    RowStart = 5,
    RowEnd = 6,
    PageBreak = 7,
    StyledText = 16,
    BoxStart = 17,
    BoxEnd = 18,
    End = 255,
}

impl TryFrom<u8> for Opcode {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Text,
            2 => Self::Spacer,
            3 => Self::StackStart,
            4 => Self::StackEnd,
            5 => Self::RowStart,
            6 => Self::RowEnd,
            7 => Self::PageBreak,
            16 => Self::StyledText,
            17 => Self::BoxStart,
            18 => Self::BoxEnd,
            255 => Self::End,
            _ => return Err(ProtocolError::UnknownOpcode),
        })
    }
}

#[derive(Debug)]
pub enum ProtocolError {
    FontsAfterDocument,
    TooManyFonts,
    InvalidFont(String),
    DataAfterEnd,
    RecordTooLarge,
    InvalidMagic,
    UnknownVersion,
    InvalidNesting,
    InvalidTextAlignment,
    UnknownFont,
    InvalidBackground,
    EmptyDocument,
    UnknownOpcode,
    MissingHeader,
    NestingTooDeep,
    RowCellCountMismatch,
    TruncatedProtocol,
    InvalidLength,
    TruncatedRecord,
    InvalidStringLength,
    InvalidUtf8,
    UnsupportedWinAnsi,
    InvalidColumnCount,
    InvalidColumnKind,
    InvalidRecordLength,
    InvalidValue(&'static str),
    Render(crate::RenderError),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::FontsAfterDocument => "fonts must be registered before document",
            Self::TooManyFonts => "too many fonts",
            Self::InvalidFont(message) => return formatter.write_str(message),
            Self::DataAfterEnd => "data after end",
            Self::RecordTooLarge => "record too large",
            Self::InvalidMagic => "invalid magic",
            Self::UnknownVersion => "unknown version",
            Self::InvalidNesting => "invalid nesting",
            Self::InvalidTextAlignment => "invalid text alignment",
            Self::UnknownFont => "unknown font",
            Self::InvalidBackground => "invalid background",
            Self::EmptyDocument => "empty document",
            Self::UnknownOpcode => "unknown opcode",
            Self::MissingHeader => "missing header",
            Self::NestingTooDeep => "nesting exceeds 64",
            Self::RowCellCountMismatch => "row cell count mismatch",
            Self::TruncatedProtocol => "truncated protocol",
            Self::InvalidLength => "invalid length",
            Self::TruncatedRecord => "truncated record",
            Self::InvalidStringLength => "invalid string length",
            Self::InvalidUtf8 => "invalid UTF-8",
            Self::UnsupportedWinAnsi => "unsupported WinAnsi character",
            Self::InvalidColumnCount => "invalid column count",
            Self::InvalidColumnKind => "invalid column kind",
            Self::InvalidRecordLength => "invalid record length",
            Self::InvalidValue(name) => return write!(formatter, "invalid {name}"),
            Self::Render(error) => return write!(formatter, "{error:?}"),
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProtocolError {}

impl From<crate::RenderError> for ProtocolError {
    fn from(error: crate::RenderError) -> Self {
        Self::Render(error)
    }
}

#[derive(Default)]
pub struct Decoder {
    pending: Vec<u8>,
    page: Option<crate::Page>,
    renderer: Option<crate::Renderer>,
    block: Vec<OwnedCommand>,
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
        match Opcode::try_from(opcode)? {
            Opcode::Text => {
                let size = pt(cursor.positive_f32("font size")?)?;
                let text = cursor.text()?.to_owned();
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
                self.ensure_renderer()?
                    .push(&[crate::Command::PageBreak])
                    .map_err(ProtocolError::from)
            }
            Opcode::StyledText => {
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
                let text = cursor.text()?.to_owned();
                cursor.done()?;
                self.layout_command(OwnedCommand::Text(
                    text,
                    TextStyle {
                        size,
                        align,
                        color,
                        font,
                    },
                ))
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

    fn layout_command(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        self.block.push(command);
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn open(&mut self, frame: Frame, command: OwnedCommand) -> Result<(), ProtocolError> {
        if self.frames.len() >= 64 {
            return Err(ProtocolError::NestingTooDeep);
        }
        self.block.push(command);
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
        self.block.push(command);
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
        self.block.push(OwnedCommand::BoxEnd);
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

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ProtocolError::InvalidLength)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ProtocolError::TruncatedRecord)?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn positive_f32(&mut self, name: &'static str) -> Result<f32, ProtocolError> {
        positive_f32(self.take(4)?, name)
    }

    fn nonnegative_f32(&mut self, name: &'static str) -> Result<f32, ProtocolError> {
        nonnegative_f32(self.take(4)?, name)
    }

    fn text(&mut self) -> Result<&'a str, ProtocolError> {
        let length =
            usize::try_from(self.u32()?).map_err(|_| ProtocolError::InvalidStringLength)?;
        let text =
            std::str::from_utf8(self.take(length)?).map_err(|_| ProtocolError::InvalidUtf8)?;
        if !valid_win_ansi(text) {
            return Err(ProtocolError::UnsupportedWinAnsi);
        }
        Ok(text)
    }

    fn columns(&mut self) -> Result<Vec<crate::ColumnWidth>, ProtocolError> {
        let count = usize::from(self.u16()?);
        if count == 0 || count > 256 {
            return Err(ProtocolError::InvalidColumnCount);
        }
        let mut columns = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = self.take(1)?[0];
            let value = self.positive_f32("column width")?;
            columns.push(match kind {
                0 => crate::ColumnWidth::Fixed(pt(value)?),
                1 => crate::ColumnWidth::Fraction(Fraction::new(value)?),
                2 => crate::ColumnWidth::Percent(Percent::new(value)?),
                _ => return Err(ProtocolError::InvalidColumnKind),
            });
        }
        Ok(columns)
    }

    fn done(&self) -> Result<(), ProtocolError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidRecordLength)
        }
    }
}

fn pt(value: f32) -> Result<crate::Pt, ProtocolError> {
    crate::Pt::new(value).map_err(ProtocolError::from)
}

fn positive_f32(bytes: &[u8], name: &'static str) -> Result<f32, ProtocolError> {
    let value = f32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ProtocolError::TruncatedRecord)?,
    );
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(ProtocolError::InvalidValue(name))
    }
}

fn nonnegative_f32(bytes: &[u8], name: &'static str) -> Result<f32, ProtocolError> {
    let value = f32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ProtocolError::TruncatedRecord)?,
    );
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(ProtocolError::InvalidValue(name))
    }
}

#[cfg(test)]
mod tests {
    use super::{Decoder, HEADER_LEN, MAGIC, VERSION};

    fn header(version: u16, width: f32, height: f32, margin: f32) -> Vec<u8> {
        let mut bytes = MAGIC.to_vec();
        bytes.extend(version.to_le_bytes());
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        bytes.extend(margin.to_le_bytes());
        assert_eq!(bytes.len(), HEADER_LEN);
        bytes
    }

    fn record(opcode: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![opcode];
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        bytes
    }

    fn text(value: &[u8]) -> Vec<u8> {
        let mut bytes = (value.len() as u32).to_le_bytes().to_vec();
        bytes.extend(value);
        bytes
    }

    fn columns(values: &[(u8, f32)]) -> Vec<u8> {
        let mut bytes = (values.len() as u16).to_le_bytes().to_vec();
        for (kind, value) in values {
            bytes.push(*kind);
            bytes.extend(value.to_le_bytes());
        }
        bytes
    }

    fn plain(value: &[u8]) -> Vec<u8> {
        let mut payload = 10.0_f32.to_le_bytes().to_vec();
        payload.extend(text(value));
        record(1, &payload)
    }

    fn box_start() -> Vec<u8> {
        let mut payload = Vec::new();
        for _ in 0..12 {
            payload.extend(0.0_f32.to_le_bytes());
        }
        payload.extend([1, 0xee, 0xee, 0xee]);
        payload.extend([0, 0, 0].repeat(4));
        record(17, &payload)
    }

    /// Every opcode the JSX lowering emits, on a 120x60 page whose 40pt of body
    /// forces the stream across three pages.
    fn representative() -> Vec<u8> {
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        bytes.extend(plain(b"Before"));
        bytes.extend(box_start());
        bytes.extend(plain(b"Boxed"));
        bytes.extend(record(18, &[]));
        bytes.extend(record(5, &columns(&[(0, 50.0), (1, 1.0)])));
        bytes.extend(plain(b"row"));
        bytes.extend(plain(b"1"));
        bytes.extend(record(6, &[]));
        bytes.extend(record(2, &5.0_f32.to_le_bytes()));
        bytes.extend(record(3, &0.0_f32.to_le_bytes()));
        bytes.extend(plain(b"Stacked"));
        bytes.extend(record(4, &[]));
        let mut styled = 10.0_f32.to_le_bytes().to_vec();
        styled.extend([1, 0xcc, 0x00, 0x00, 1]);
        styled.extend(text(b"Styled"));
        bytes.extend(record(16, &styled));
        bytes.extend(record(7, &[]));
        bytes.extend(plain(b"After"));
        bytes.extend(record(255, &[]));
        bytes
    }

    fn decode(bytes: &[u8]) -> Result<Vec<u8>, String> {
        let mut decoder = Decoder::default();
        decoder.push(bytes).map_err(|error| error.to_string())?;
        decoder.finish().map_err(|error| error.to_string())
    }

    fn push_error(bytes: &[u8]) -> String {
        let mut decoder = Decoder::default();
        decoder.push(bytes).unwrap_err().to_string()
    }

    #[test]
    fn given_embedded_regular_and_bold_fonts_when_decoding_then_pdf_embeds_both() {
        let font = include_bytes!("../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");
        let mut decoder = Decoder::default();
        assert_eq!(decoder.add_font(font.to_vec()).unwrap(), 2);
        assert_eq!(decoder.add_font(font.to_vec()).unwrap(), 3);
        let mut bytes = header(VERSION, 120.0, 60.0, 10.0);
        for slot in [2, 3] {
            let mut styled = 10.0_f32.to_le_bytes().to_vec();
            styled.extend([0, 0, 0, 0, slot]);
            styled.extend(text(b"Font"));
            bytes.extend(record(16, &styled));
        }
        bytes.extend(record(255, &[]));
        decoder.push(&bytes).unwrap();
        let pdf = decoder.finish().unwrap();
        let document = lopdf::Document::load_mem(&pdf).unwrap();
        let page = *document.get_pages().values().next().unwrap();
        let fonts = document.get_page_fonts(page).unwrap();
        assert_eq!(fonts.len(), 2);
        for font in fonts.values() {
            assert_eq!(
                font.get(b"Subtype").unwrap().as_name().unwrap(),
                b"TrueType"
            );
            let descriptor = font.get(b"FontDescriptor").unwrap().as_reference().unwrap();
            assert!(document
                .get_dictionary(descriptor)
                .unwrap()
                .has(b"FontFile2"));
        }
    }

    #[test]
    fn given_every_split_point_when_decoding_then_output_is_valid_and_deterministic() {
        let bytes = representative();
        let expected = decode(&bytes).unwrap();
        assert!(expected.starts_with(b"%PDF-"));
        let pdf = lopdf::Document::load_mem(&expected).unwrap();
        assert_eq!(pdf.get_pages().len(), 3);
        assert_eq!(
            pdf.extract_text(&[1]).unwrap().trim(),
            "Before\nBoxed\nrow\n1"
        );
        assert_eq!(pdf.extract_text(&[2]).unwrap().trim(), "Stacked\nStyled");
        assert_eq!(pdf.extract_text(&[3]).unwrap().trim(), "After");
        for split in 0..=bytes.len() {
            let mut decoder = Decoder::default();
            decoder.push(&bytes[..split]).unwrap();
            decoder.push(&bytes[split..]).unwrap();
            assert_eq!(decoder.finish().unwrap(), expected, "split {split}");
        }
    }

    #[test]
    fn given_bad_headers_records_and_text_when_decoding_then_rejects() {
        let mut truncated = representative();
        truncated.pop();
        assert_eq!(decode(&truncated).unwrap_err(), "truncated protocol");

        let mut bad_magic = representative();
        bad_magic[0] = b'X';
        assert_eq!(push_error(&bad_magic), "invalid magic");

        let mut bad_version = representative();
        bad_version[4..6].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(push_error(&bad_version), "unknown version");

        let mut unknown = header(VERSION, 120.0, 60.0, 10.0);
        unknown.extend(record(42, &[]));
        assert_eq!(push_error(&unknown), "unknown opcode");

        let mut legacy_font = header(VERSION, 120.0, 60.0, 10.0);
        legacy_font.extend(record(19, &[]));
        assert_eq!(push_error(&legacy_font), "unknown opcode");

        let mut oversized_record = header(VERSION, 120.0, 60.0, 10.0);
        oversized_record.push(1);
        oversized_record.extend(65_537_u32.to_le_bytes());
        assert_eq!(push_error(&oversized_record), "record too large");

        let mut invalid_utf8 = header(VERSION, 120.0, 60.0, 10.0);
        let mut payload = 10.0_f32.to_le_bytes().to_vec();
        payload.extend(text(&[0xff]));
        invalid_utf8.extend(record(1, &payload));
        assert_eq!(push_error(&invalid_utf8), "invalid UTF-8");

        let mut unsupported = header(VERSION, 120.0, 60.0, 10.0);
        let mut payload = 10.0_f32.to_le_bytes().to_vec();
        payload.extend(text("🙂".as_bytes()));
        unsupported.extend(record(1, &payload));
        assert_eq!(push_error(&unsupported), "unsupported WinAnsi character");

        let mut bad_length = header(VERSION, 120.0, 60.0, 10.0);
        let mut payload = 10.0_f32.to_le_bytes().to_vec();
        payload.extend(4_u32.to_le_bytes());
        payload.push(b'x');
        bad_length.extend(record(1, &payload));
        assert_eq!(push_error(&bad_length), "truncated record");
    }

    #[test]
    fn given_invalid_numbers_layout_and_rows_when_decoding_then_rejects() {
        assert_eq!(
            push_error(&header(VERSION, f32::NAN, 60.0, 10.0)),
            "invalid page width"
        );
        assert_eq!(
            push_error(&header(VERSION, 120.0, 60.0, 30.0)),
            "InvalidPage"
        );

        let mut zero_size = header(VERSION, 120.0, 60.0, 10.0);
        let mut payload = 0.0_f32.to_le_bytes().to_vec();
        payload.extend(text(b"x"));
        zero_size.extend(record(1, &payload));
        assert_eq!(push_error(&zero_size), "invalid font size");

        let mut infinite_column = header(VERSION, 120.0, 60.0, 10.0);
        infinite_column.extend(record(5, &columns(&[(0, f32::INFINITY)])));
        assert_eq!(push_error(&infinite_column), "invalid column width");

        let mut zero_columns = header(VERSION, 120.0, 60.0, 10.0);
        zero_columns.extend(record(5, &0_u16.to_le_bytes()));
        assert_eq!(push_error(&zero_columns), "invalid column count");

        let mut too_many_columns = header(VERSION, 120.0, 60.0, 10.0);
        too_many_columns.extend(record(5, &257_u16.to_le_bytes()));
        assert_eq!(push_error(&too_many_columns), "invalid column count");

        let mut bad_close = header(VERSION, 120.0, 60.0, 10.0);
        bad_close.extend(record(4, &[]));
        assert_eq!(push_error(&bad_close), "invalid nesting");

        let mut deep = header(VERSION, 120.0, 60.0, 10.0);
        for _ in 0..65 {
            deep.extend(record(3, &0.0_f32.to_le_bytes()));
        }
        assert_eq!(push_error(&deep), "nesting exceeds 64");

        let mut bad_layout_row = header(VERSION, 120.0, 60.0, 10.0);
        bad_layout_row.extend(record(5, &columns(&[(0, 50.0), (1, 1.0)])));
        let mut payload = 10.0_f32.to_le_bytes().to_vec();
        payload.extend(text(b"one"));
        bad_layout_row.extend(record(1, &payload));
        bad_layout_row.extend(record(6, &[]));
        assert_eq!(push_error(&bad_layout_row), "row cell count mismatch");

        let mut oversized = header(VERSION, 120.0, 60.0, 10.0);
        oversized.extend(plain("word ".repeat(100).as_bytes()));
        assert_eq!(push_error(&oversized), "PageOverflow");
    }
}
