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

use pdf_writer::types::AssociationKind;

use crate::command::MAX_LAYOUT_DEPTH;
use crate::image::{ColorSpace, MAX_IMAGE_BYTES};
use crate::pdf::Attachment;
use crate::{Edges, FontId, OwnedCommand, RunDecoration, TextRun, TextStyle};

/// Bytes an adapter accepts per `push`. Sized so no adapter buffers a document.
pub const INPUT_CAPACITY: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
/// An open Stack, Box, Row or Table buffers its commands until it closes; this bounds that buffer.
pub const MAX_BLOCK_BYTES: usize = 16 * 1024 * 1024;
const HEADER_LEN: usize = 18;
const MAGIC: &[u8; 4] = b"FPDF";
const VERSION: u16 = 7;
const MAX_ATTACHMENT_BYTES: usize = 64 * 1024 * 1024;
/// Factur-X conformance level, embedded file name and relationship for profile bytes 1 to 6.
const FACTURX: [(&str, &str, AssociationKind); 6] = [
    ("MINIMUM", "factur-x.xml", AssociationKind::Data),
    ("BASIC WL", "factur-x.xml", AssociationKind::Data),
    ("BASIC", "factur-x.xml", AssociationKind::Alternative),
    ("EN 16931", "factur-x.xml", AssociationKind::Alternative),
    ("EXTENDED", "factur-x.xml", AssociationKind::Alternative),
    ("XRECHNUNG", "xrechnung.xml", AssociationKind::Alternative),
];

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
    /// Attachment bytes until an Attachment or PdfA record claims them.
    attachments: Vec<Option<Vec<u8>>>,
    attachment_bytes: usize,
    metadata: crate::pdf::Metadata,
    pdfa: bool,
    body_started: bool,
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
        if self.pdfa && matches!(image.color_space, ColorSpace::Cmyk) {
            return Err(ProtocolError::PdfACmyk);
        }
        Ok(match &mut self.renderer {
            Some(renderer) => renderer.add_image(image),
            None => {
                self.images.push(image);
                (self.images.len() - 1) as u16
            }
        })
    }

    /// Attachment bytes bypass the document window like images, and wait for
    /// the record that names them.
    pub fn add_attachment(&mut self, bytes: Vec<u8>) -> Result<u16, ProtocolError> {
        if self.ended {
            return Err(ProtocolError::DataAfterEnd);
        }
        self.attachment_bytes = self
            .attachment_bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_ATTACHMENT_BYTES)
            .ok_or(ProtocolError::AttachmentsTooLarge)?;
        if self.attachments.len() >= usize::from(u16::MAX) {
            return Err(ProtocolError::InvalidValue("attachment count"));
        }
        self.attachments.push(Some(bytes));
        Ok((self.attachments.len() - 1) as u16)
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
                let footer_rows = cursor.u16()?;
                let width = cursor.column()?;
                cursor.done()?;
                self.open(
                    Frame::Table,
                    OwnedCommand::TableStart(header_rows, footer_rows, width),
                )
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
                self.body_started = true;
                self.ensure_renderer()?.page_break();
                Ok(())
            }
            Opcode::Footer | Opcode::Header => {
                let size = pt(cursor.positive_f32("font size")?)?;
                let align = cursor.align()?;
                let color = cursor.rgb()?;
                let font = self.font(cursor.take(1)?[0])?;
                let text = cursor.text(true)?.to_owned();
                cursor.done()?;
                if self.body_started || !self.frames.is_empty() {
                    return Err(ProtocolError::InvalidNesting);
                }
                let style = TextStyle {
                    size,
                    align,
                    color,
                    font,
                };
                let renderer = self.ensure_renderer()?;
                if matches!(opcode, Opcode::Header) {
                    renderer.set_header(text, style)
                } else {
                    renderer.set_footer(text, style)
                }
                .map_err(ProtocolError::from)
            }
            Opcode::Metadata => {
                let tagged = match cursor.take(1)?[0] {
                    0 => false,
                    1 => true,
                    _ => return Err(ProtocolError::InvalidValue("tagged flag")),
                };
                let fields = (0..5)
                    .map(|_| cursor.alt().map(str::to_owned))
                    .collect::<Result<Vec<_>, _>>()?;
                cursor.done()?;
                self.before_body()?;
                let metadata = crate::pdf::Metadata {
                    title: fields[0].clone(),
                    author: fields[1].clone(),
                    subject: fields[2].clone(),
                    keywords: fields[3].clone(),
                    language: fields[4].clone(),
                    tagged,
                    ..std::mem::take(&mut self.metadata)
                };
                if tagged && metadata.language.is_empty() {
                    return Err(ProtocolError::InvalidValue("metadata language"));
                }
                if !metadata.language.is_empty() && !valid_language(&metadata.language) {
                    return Err(ProtocolError::InvalidValue("metadata language"));
                }
                self.metadata = metadata;
                Ok(())
            }
            Opcode::PdfA => {
                let profile = cursor.take(1)?[0];
                let facturx = match profile {
                    0 => None,
                    1..=6 => Some((FACTURX[usize::from(profile - 1)], cursor.u16()?)),
                    _ => return Err(ProtocolError::InvalidValue("Factur-X profile")),
                };
                cursor.done()?;
                self.before_body()?;
                if self
                    .images
                    .iter()
                    .any(|image| matches!(image.color_space, ColorSpace::Cmyk))
                {
                    return Err(ProtocolError::PdfACmyk);
                }
                self.pdfa = true;
                self.metadata.pdfa = true;
                if let Some(((level, name, relationship), slot)) = facturx {
                    let data = self.claim_attachment(slot)?;
                    // A cheap shape check; the XML's schema and content are the caller's.
                    let xml = std::str::from_utf8(&data)
                        .map(|xml| xml.trim_start_matches('\u{feff}').trim())
                        .unwrap_or_default();
                    if !xml.starts_with('<') || !xml.ends_with('>') {
                        return Err(ProtocolError::InvalidValue("Factur-X XML"));
                    }
                    self.metadata.facturx = Some((level, name));
                    self.attach(Attachment {
                        name: name.to_owned(),
                        mime: "text/xml".to_owned(),
                        description: "Factur-X/ZUGFeRD invoice".to_owned(),
                        relationship,
                        data,
                    })?;
                }
                Ok(())
            }
            Opcode::Attachment => {
                let slot = cursor.u16()?;
                let relationship = match cursor.take(1)?[0] {
                    0 => AssociationKind::Source,
                    1 => AssociationKind::Data,
                    2 => AssociationKind::Alternative,
                    3 => AssociationKind::Supplement,
                    4 => AssociationKind::Unspecified,
                    _ => return Err(ProtocolError::InvalidValue("attachment relationship")),
                };
                let name = cursor.alt()?.to_owned();
                let mime = cursor.alt()?.to_owned();
                let description = cursor.alt()?.to_owned();
                cursor.done()?;
                self.before_body()?;
                if name.is_empty() || name.chars().count() > 255 || name.contains(['/', '\\']) {
                    return Err(ProtocolError::InvalidValue("attachment name"));
                }
                if !valid_mime(&mime) {
                    return Err(ProtocolError::InvalidValue("attachment MIME type"));
                }
                let data = self.claim_attachment(slot)?;
                self.attach(Attachment {
                    name,
                    mime,
                    description,
                    relationship,
                    data,
                })
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
                    // Bit 0 hard break, 1 underline, 2 line-through, 3 a u16 link index follows,
                    // 4 an f32 line height follows.
                    let flags = cursor.take(1)?[0];
                    if flags > 0x1f {
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
                    let line_height = if flags & 16 != 0 {
                        Some(pt(cursor.positive_f32("line height")?)?)
                    } else {
                        None
                    };
                    let run = cursor.text(false)?;
                    text.push_str(run);
                    runs.push(TextRun {
                        len: run.len(),
                        font,
                        size,
                        line_height,
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
                let alt = cursor.alt()?.to_owned();
                cursor.done()?;
                self.layout_command(OwnedCommand::Image(slot, width, height, link, alt))
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
        if self.pdfa && font.index() < 2 {
            return Err(ProtocolError::PdfAFont);
        }
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

    /// Document-level records must precede the header, footer and body.
    fn before_body(&self) -> Result<(), ProtocolError> {
        if self.renderer.is_some() || !self.frames.is_empty() {
            return Err(ProtocolError::InvalidNesting);
        }
        Ok(())
    }

    fn claim_attachment(&mut self, slot: u16) -> Result<Vec<u8>, ProtocolError> {
        self.attachments
            .get_mut(usize::from(slot))
            .and_then(Option::take)
            .ok_or(ProtocolError::InvalidValue("attachment slot"))
    }

    fn attach(&mut self, attachment: Attachment) -> Result<(), ProtocolError> {
        let attachments = &mut self.metadata.attachments;
        match attachments.binary_search_by(|other| other.name.cmp(&attachment.name)) {
            Ok(_) => Err(ProtocolError::DuplicateAttachment),
            Err(index) => {
                attachments.insert(index, attachment);
                Ok(())
            }
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
            renderer.set_metadata(std::mem::take(&mut self.metadata));
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
            OwnedCommand::Image(_, _, _, link, alt) => {
                link.as_ref().map_or(0, String::len) + alt.len()
            }
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
        self.body_started = true;
        self.push_command(command)?;
        self.complete_child()?;
        if self.frames.is_empty() {
            self.flush_block()?;
        }
        Ok(())
    }

    fn open(&mut self, frame: Frame, command: OwnedCommand) -> Result<(), ProtocolError> {
        self.body_started = true;
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

fn valid_language(value: &str) -> bool {
    let mut parts = value.split('-');
    parts.next().is_some_and(|part| {
        (2..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphabetic())
    }) && parts.all(|part| {
        (1..=8).contains(&part.len()) && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

/// veraPDF's PDF/A-3 rule 6.8-1 pattern, which is stricter than RFC 6838.
fn valid_mime(value: &str) -> bool {
    let token = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_.+-".contains(&byte))
    };
    value.len() <= 255
        && value
            .split_once('/')
            .is_some_and(|(kind, subtype)| token(kind) && token(subtype))
}
