//! Binary command protocol: incremental decoding and boundary validation.
//!
//! Adapters own transport only. Every value is validated here before it becomes
//! a domain type, so the WASM and N-API bindings cannot diverge.

use crate::OwnedCommand;

/// Bytes an adapter accepts per `push`. Sized so no adapter buffers a document.
pub const INPUT_CAPACITY: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
const HEADER_LEN: usize = 18;
const MAGIC: &[u8; 4] = b"FPDF";
const VERSION: u16 = 2;

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
    pub fn add_font(&mut self, bytes: Vec<u8>) -> Result<u8, String> {
        if self.page.is_some() {
            return Err("fonts must be registered before document".into());
        }
        if self.fonts.len() >= 254 {
            return Err("too many fonts".into());
        }
        self.fonts
            .push(crate::EmbeddedFont::parse(bytes).map_err(|error| error.to_string())?);
        Ok((self.fonts.len() + 1) as u8)
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<(), String> {
        if self.ended && !bytes.is_empty() {
            return Err("data after end".into());
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
                    return Err("record too large".into());
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

    fn read_header(&mut self, bytes: &[u8]) -> Result<(), String> {
        if &bytes[..4] != MAGIC {
            return Err("invalid magic".into());
        }
        if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION {
            return Err("unknown version".into());
        }
        let width = positive_f32(&bytes[6..10], "page width")?;
        let height = positive_f32(&bytes[10..14], "page height")?;
        let margin = nonnegative_f32(&bytes[14..18], "margin")?;
        self.page =
            Some(crate::Page::new(pt(width)?, pt(height)?, pt(margin)?).map_err(render_error)?);
        Ok(())
    }

    fn read_record(&mut self, opcode: u8, payload: &[u8]) -> Result<(), String> {
        if self.ended {
            return Err("data after end".into());
        }
        let mut cursor = Cursor::new(payload);
        match opcode {
            1 => {
                let size = pt(cursor.positive_f32("font size")?)?;
                let text = cursor.text()?.to_owned();
                cursor.done()?;
                self.layout_command(OwnedCommand::Text(text, size))
            }
            2 => {
                let space = pt(cursor.nonnegative_f32("spacer")?)?;
                cursor.done()?;
                self.layout_command(OwnedCommand::Spacer(space))
            }
            3 => {
                let gap = pt(cursor.nonnegative_f32("stack gap")?)?;
                cursor.done()?;
                self.open(Frame::Stack, OwnedCommand::StackStart(gap))
            }
            4 => {
                cursor.done()?;
                self.close(false, OwnedCommand::StackEnd)
            }
            5 => {
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
            6 => {
                cursor.done()?;
                self.close(true, OwnedCommand::RowEnd)
            }
            7 => {
                cursor.done()?;
                if !self.frames.is_empty() {
                    return Err("invalid nesting".into());
                }
                self.ensure_renderer()?
                    .push(&[crate::Command::PageBreak])
                    .map_err(render_error)
            }
            16 => {
                let size = pt(cursor.positive_f32("font size")?)?;
                let align = match cursor.take(1)?[0] {
                    0 => crate::TextAlign::Left,
                    1 => crate::TextAlign::Center,
                    2 => crate::TextAlign::Right,
                    _ => return Err("invalid text alignment".into()),
                };
                let color: [u8; 3] = cursor.take(3)?.try_into().unwrap();
                let font = cursor.take(1)?[0];
                let font_count = self
                    .renderer
                    .as_ref()
                    .map_or(self.fonts.len() + 2, crate::Renderer::font_count);
                if usize::from(font) >= font_count {
                    return Err("unknown font".into());
                }
                let text = cursor.text()?.to_owned();
                cursor.done()?;
                self.layout_command(OwnedCommand::StyledText(text, size, align, color, font))
            }
            17 => {
                let margin = [
                    pt(cursor.nonnegative_f32("margin top")?)?,
                    pt(cursor.nonnegative_f32("margin right")?)?,
                    pt(cursor.nonnegative_f32("margin bottom")?)?,
                    pt(cursor.nonnegative_f32("margin left")?)?,
                ];
                let padding = [
                    pt(cursor.nonnegative_f32("padding top")?)?,
                    pt(cursor.nonnegative_f32("padding right")?)?,
                    pt(cursor.nonnegative_f32("padding bottom")?)?,
                    pt(cursor.nonnegative_f32("padding left")?)?,
                ];
                let border = [
                    pt(cursor.nonnegative_f32("border top width")?)?,
                    pt(cursor.nonnegative_f32("border right width")?)?,
                    pt(cursor.nonnegative_f32("border bottom width")?)?,
                    pt(cursor.nonnegative_f32("border left width")?)?,
                ];
                let background = match cursor.take(1)?[0] {
                    0 => None,
                    1 => Some(cursor.take(3)?.try_into().unwrap()),
                    _ => return Err("invalid background".into()),
                };
                let border_color = [
                    cursor.take(3)?.try_into().unwrap(),
                    cursor.take(3)?.try_into().unwrap(),
                    cursor.take(3)?.try_into().unwrap(),
                    cursor.take(3)?.try_into().unwrap(),
                ];
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
            18 => {
                cursor.done()?;
                self.close_box()
            }
            255 => {
                cursor.done()?;
                if !self.frames.is_empty() {
                    return Err("invalid nesting".into());
                }
                if self.renderer.is_none() {
                    return Err("empty document".into());
                }
                self.ended = true;
                Ok(())
            }
            _ => Err("unknown opcode".into()),
        }
    }

    fn ensure_renderer(&mut self) -> Result<&mut crate::Renderer, String> {
        let page = self.page.ok_or("missing header")?;
        if self.renderer.is_none() {
            self.renderer = Some(crate::Renderer::with_fonts(
                page,
                std::mem::take(&mut self.fonts),
            ));
        }
        Ok(self.renderer.as_mut().unwrap())
    }

    fn layout_command(&mut self, command: OwnedCommand) -> Result<(), String> {
        self.block.push(command);
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn open(&mut self, frame: Frame, command: OwnedCommand) -> Result<(), String> {
        if self.frames.len() >= 64 {
            return Err("nesting exceeds 64".into());
        }
        self.block.push(command);
        self.frames.push(frame);
        Ok(())
    }

    fn close(&mut self, row: bool, command: OwnedCommand) -> Result<(), String> {
        let frame = self.frames.pop().ok_or("invalid nesting")?;
        match (row, frame) {
            (false, Frame::Stack) => {}
            (true, Frame::Row { columns, cells }) if columns == cells => {}
            (true, Frame::Row { .. }) => return Err("row cell count mismatch".into()),
            _ => return Err("invalid nesting".into()),
        }
        self.block.push(command);
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn close_box(&mut self) -> Result<(), String> {
        if !matches!(self.frames.pop(), Some(Frame::Box)) {
            return Err("invalid nesting".into());
        }
        self.block.push(OwnedCommand::BoxEnd);
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn complete_child(&mut self) -> Result<(), String> {
        if let Some(Frame::Row { columns, cells }) = self.frames.last_mut() {
            *cells += 1;
            if *cells > *columns {
                return Err("row cell count mismatch".into());
            }
        }
        Ok(())
    }

    fn flush_block(&mut self) -> Result<(), String> {
        let page = self.page.ok_or("missing header")?;
        let Self {
            renderer, block, ..
        } = self;
        renderer
            .get_or_insert_with(|| {
                crate::Renderer::with_fonts(page, std::mem::take(&mut self.fonts))
            })
            .push_owned(block)
            .map_err(render_error)?;
        block.clear();
        Ok(())
    }

    pub fn finish(self) -> Result<Vec<u8>, String> {
        if self.page.is_none() || !self.ended || !self.pending.is_empty() {
            return Err("truncated protocol".into());
        }
        self.renderer
            .map(crate::Renderer::finish)
            .ok_or_else(|| "empty document".into())
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

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self.offset.checked_add(length).ok_or("invalid length")?;
        let value = self.bytes.get(self.offset..end).ok_or("truncated record")?;
        self.offset = end;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn positive_f32(&mut self, name: &str) -> Result<f32, String> {
        positive_f32(self.take(4)?, name)
    }

    fn nonnegative_f32(&mut self, name: &str) -> Result<f32, String> {
        nonnegative_f32(self.take(4)?, name)
    }

    fn text(&mut self) -> Result<&'a str, String> {
        let length = usize::try_from(self.u32()?).map_err(|_| "invalid string length")?;
        let text = std::str::from_utf8(self.take(length)?).map_err(|_| "invalid UTF-8")?;
        validate_win_ansi(text)?;
        Ok(text)
    }

    fn columns(&mut self) -> Result<Vec<crate::ColumnWidth>, String> {
        let count = usize::from(self.u16()?);
        if count == 0 || count > 256 {
            return Err("invalid column count".into());
        }
        let mut columns = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = self.take(1)?[0];
            let value = pt(self.positive_f32("column width")?)?;
            columns.push(match kind {
                0 => crate::ColumnWidth::Fixed(value),
                1 => crate::ColumnWidth::Fraction(value),
                2 => crate::ColumnWidth::Percent(value),
                _ => return Err("invalid column kind".into()),
            });
        }
        Ok(columns)
    }

    fn done(&self) -> Result<(), String> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err("invalid record length".into())
        }
    }
}

fn pt(value: f32) -> Result<crate::Pt, String> {
    crate::Pt::new(value).map_err(render_error)
}

fn positive_f32(bytes: &[u8], name: &str) -> Result<f32, String> {
    let value = f32::from_le_bytes(bytes.try_into().map_err(|_| "truncated record")?);
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(format!("invalid {name}"))
    }
}

fn nonnegative_f32(bytes: &[u8], name: &str) -> Result<f32, String> {
    let value = f32::from_le_bytes(bytes.try_into().map_err(|_| "truncated record")?);
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(format!("invalid {name}"))
    }
}

fn validate_win_ansi(text: &str) -> Result<(), String> {
    for character in text.chars() {
        if !matches!(
            character,
            '\t'..='\r'
                | ' '..='~'
                | '\u{00a0}'..='\u{00ff}'
                | '\u{20ac}'
                | '\u{201a}'
                | '\u{0192}'
                | '\u{201e}'
                | '\u{2026}'
                | '\u{2020}'
                | '\u{2021}'
                | '\u{02c6}'
                | '\u{2030}'
                | '\u{0160}'
                | '\u{2039}'
                | '\u{0152}'
                | '\u{017d}'
                | '\u{2018}'
                | '\u{2019}'
                | '\u{201c}'
                | '\u{201d}'
                | '\u{2022}'
                | '\u{2013}'
                | '\u{2014}'
                | '\u{02dc}'
                | '\u{2122}'
                | '\u{0161}'
                | '\u{203a}'
                | '\u{0153}'
                | '\u{017e}'
                | '\u{0178}'
        ) {
            return Err("unsupported WinAnsi character".into());
        }
    }
    Ok(())
}

fn render_error(error: crate::RenderError) -> String {
    format!("{error:?}")
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
        decoder.push(bytes)?;
        decoder.finish()
    }

    fn push_error(bytes: &[u8]) -> String {
        let mut decoder = Decoder::default();
        decoder.push(bytes).unwrap_err()
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
