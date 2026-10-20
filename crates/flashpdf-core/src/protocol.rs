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

use cursor::{pt, Cursor};
use opcode::Opcode;

use pdf_writer::types::AssociationKind;

use crate::command::MAX_LAYOUT_DEPTH;
use crate::image::{ColorSpace, MAX_IMAGE_BYTES};
use crate::pdf::Attachment;
use crate::{
    Align, BoxSize, Edges, Flex, FlexItem, FontId, Justify, LineStyle, Marker, OwnedCommand, Pt,
    Radii, RunDecoration, TextRun, BUILT_IN, OPAQUE,
};

/// Bytes an adapter accepts per `push`. Sized so no adapter buffers a document.
pub const INPUT_CAPACITY: usize = 4096;
const MAX_RECORD: usize = 64 * 1024;
/// An open Stack, Row or nested Table, and a Box inside one, buffers its commands
/// until it closes; a Table outside them buffers its header and footer rows, then
/// one body row at a time.
pub const MAX_BLOCK_BYTES: usize = 16 * 1024 * 1024;
/// Total bytes of registered fonts per render.
const MAX_FONT_BYTES: usize = 64 * 1024 * 1024;
/// PDF readers cap each page side at 14,400 units (ISO 32000-1, Annex C).
const MAX_PAGE_SIDE: f32 = 14_400.0;
const HEADER_LEN: usize = 30;
const MAGIC: &[u8; 4] = b"FPDF";
const VERSION: u16 = 20;
const MAX_ATTACHMENT_BYTES: usize = 64 * 1024 * 1024;
/// Longest bookmark title, in bytes: 255 characters at four bytes each.
const MAX_TITLE_BYTES: usize = 1020;
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
    records: usize,
    page: Option<crate::Page>,
    renderer: Option<crate::Renderer>,
    block: Vec<OwnedCommand>,
    block_bytes: usize,
    frames: Vec<Frame>,
    ended: bool,
    fonts: Vec<crate::EmbeddedFont>,
    font_bytes: usize,
    /// Images registered before the renderer exists; later ones go straight to it.
    images: Vec<crate::Image>,
    image_bytes: usize,
    /// Attachment bytes until an Attachment or PdfA record claims them.
    attachments: Vec<Option<Vec<u8>>>,
    attachment_bytes: usize,
    metadata: crate::pdf::Metadata,
    pdfa: bool,
    body_started: bool,
    /// `(footer, first page)` of a Header or Footer record waiting for its block.
    furniture: Option<(bool, bool)>,
}

/// A streaming `Table`'s `head` counts its `TableStart` and the header and footer
/// rows it still buffers; at zero its body rows stream. A held one has `None`.
/// A `Box` is `true` once the renderer has it open and its children stream.
#[derive(Clone, Copy)]
enum Frame {
    Stack,
    Box(bool),
    Row { cells: usize, done: usize },
    Flex { cells: usize, done: usize },
    Table { head: Option<usize> },
}

impl Decoder {
    /// Font files bypass the bounded document window because a whole TTF must
    /// remain available until the PDF's font stream is written.
    pub fn add_font(&mut self, bytes: Vec<u8>) -> Result<u8, ProtocolError> {
        if self.page.is_some() {
            return Err(ProtocolError::FontsAfterDocument);
        }
        if self.fonts.len() + BUILT_IN >= 256 {
            return Err(ProtocolError::TooManyFonts);
        }
        let total = self
            .font_bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_FONT_BYTES)
            .ok_or(ProtocolError::FontsTooLarge)?;
        self.fonts
            .push(crate::EmbeddedFont::parse(bytes).map_err(ProtocolError::InvalidFont)?);
        self.font_bytes = total;
        Ok((self.fonts.len() + BUILT_IN - 1) as u8)
    }

    /// Images register at any point before the record that paints them, and are
    /// held whole until the PDF's XObject streams are written.
    pub fn add_image(&mut self, bytes: Vec<u8>) -> Result<u16, ProtocolError> {
        if self.ended {
            return Err(ProtocolError::DataAfterEnd);
        }
        let total = self
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
        self.image_bytes = total;
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
                self.records += 1;
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

    /// Records decoded so far. After a failed `push` it is the index of the failing
    /// record, so an adapter that batches records can tell which call wrote it.
    pub fn records(&self) -> usize {
        self.records
    }

    fn read_header(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
        if &bytes[..4] != MAGIC {
            return Err(ProtocolError::InvalidMagic);
        }
        if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != VERSION {
            return Err(ProtocolError::UnknownVersion);
        }
        let mut cursor = Cursor::new(&bytes[6..]);
        let width = cursor.positive_f32("page width")?;
        let height = cursor.positive_f32("page height")?;
        if width > MAX_PAGE_SIDE || height > MAX_PAGE_SIDE {
            return Err(ProtocolError::InvalidValue("page size"));
        }
        let margin = Edges {
            top: pt(cursor.nonnegative_f32("page margin top")?)?,
            right: pt(cursor.nonnegative_f32("page margin right")?)?,
            bottom: pt(cursor.nonnegative_f32("page margin bottom")?)?,
            left: pt(cursor.nonnegative_f32("page margin left")?)?,
        };
        self.page = Some(crate::Page::new(pt(width)?, pt(height)?, margin)?);
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
                self.feed(OwnedCommand::Spacer(space))
            }
            Opcode::StackStart => {
                let gap = pt(cursor.nonnegative_f32("stack gap")?)?;
                cursor.done()?;
                self.feed(OwnedCommand::StackStart(gap))
            }
            Opcode::StackEnd => {
                cursor.done()?;
                self.feed(OwnedCommand::StackEnd)
            }
            Opcode::RowStart => {
                let (columns, slots): (Vec<_>, Vec<_>) = cursor.columns()?.into_iter().unzip();
                cursor.done()?;
                let cells = slots
                    .iter()
                    .filter(|slot| matches!(slot, crate::Slot::Cell { .. }))
                    .count();
                let _ = cells;
                self.feed(OwnedCommand::RowStart(columns, slots))
            }
            Opcode::FlexStart => {
                let flags = cursor.take(1)?[0];
                let justify = match cursor.take(1)?[0] {
                    0 => Justify::Start,
                    1 => Justify::End,
                    2 => Justify::Center,
                    3 => Justify::SpaceBetween,
                    4 => Justify::SpaceAround,
                    5 => Justify::SpaceEvenly,
                    _ => return Err(ProtocolError::InvalidValue("justify-content")),
                };
                let style = Flex {
                    column: match flags {
                        0 | 1 => flags == 1,
                        _ => return Err(ProtocolError::InvalidValue("flex flags")),
                    },
                    justify,
                    align: cursor.alignment()?.unwrap_or(Align::Stretch),
                    gap: pt(cursor.nonnegative_f32("flex gap")?)?,
                    min_height: pt(cursor.nonnegative_f32("flex min height")?)?,
                    max_height: cursor.height()?,
                };
                let count = usize::from(cursor.u16()?);
                if count == 0 || count > crate::command::MAX_COLUMNS {
                    return Err(ProtocolError::InvalidColumnCount);
                }
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(FlexItem {
                        grow: cursor.nonnegative_f32("flex grow")?,
                        shrink: cursor.nonnegative_f32("flex shrink")?,
                        basis: cursor.size("flex basis")?,
                        align: cursor.alignment()?,
                    });
                }
                cursor.done()?;
                self.open(
                    Frame::Flex {
                        cells: count,
                        done: 0,
                    },
                    OwnedCommand::FlexStart(style, items),
                )
            }
            Opcode::FlexEnd => {
                cursor.done()?;
                self.close(OwnedCommand::FlexEnd)
            }
            Opcode::RowEnd => {
                cursor.done()?;
                self.feed(OwnedCommand::RowEnd)
            }
            Opcode::TableStart => {
                let header_rows = cursor.u16()?;
                let footer_rows = cursor.u16()?;
                let width = cursor.column()?;
                cursor.done()?;
                self.feed(OwnedCommand::TableStart(header_rows, footer_rows, width))
            }
            Opcode::TableEnd => {
                cursor.done()?;
                self.feed(OwnedCommand::TableEnd)
            }
            Opcode::PageBreak => {
                cursor.done()?;
                if !self.placing() || self.furniture.is_some() {
                    return Err(ProtocolError::InvalidNesting);
                }
                self.body_started = true;
                Ok(self.ensure_renderer()?.page_break()?)
            }
            Opcode::Avoid => {
                cursor.done()?;
                // Inside a held block every break is already avoided.
                if self.placing()
                    || matches!(self.frames.last(), Some(Frame::Table { head: Some(0) }))
                {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.avoid();
                    }
                }
                Ok(())
            }
            Opcode::Footer | Opcode::Header => {
                let first = match cursor.take(1)?[0] {
                    0 => false,
                    1 => true,
                    _ => return Err(ProtocolError::InvalidValue("furniture page")),
                };
                cursor.done()?;
                if self.body_started || !self.frames.is_empty() || self.furniture.is_some() {
                    return Err(ProtocolError::InvalidNesting);
                }
                self.ensure_renderer()?;
                self.furniture = Some((matches!(opcode, Opcode::Footer), first));
                Ok(())
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
            Opcode::Anchor => {
                let name = cursor.alt()?;
                let level = cursor.take(1)?[0];
                let title = cursor.alt()?;
                cursor.done()?;
                if name.len() > 255 || !name.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
                    return Err(ProtocolError::InvalidValue("anchor name"));
                }
                // A bookmark has a level 1 to 6 and a title; nothing else has either.
                if level > 6 || (level == 0) != title.is_empty() || title.len() > MAX_TITLE_BYTES {
                    return Err(ProtocolError::InvalidValue("bookmark"));
                }
                let slot = self
                    .ensure_renderer()?
                    .add_anchor(name, level, title)
                    .map_err(ProtocolError::InvalidValue)?;
                self.feed(OwnedCommand::Anchor(slot))
            }
            Opcode::Paragraph => {
                let align = cursor.align()?;
                let keep = [cursor.u16()?, cursor.u16()?];
                if keep.contains(&0) {
                    return Err(ProtocolError::InvalidValue("orphans or widows"));
                }
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
                    // 4 an f32 line height follows, 5 to 7, 9, and 11 to 14 typography, 8 a decoration
                    // colour follows, and 10 a background colour.
                    let flags = cursor.u16()?;
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
                    let typography = cursor.typography(flags)?;
                    if typography.marker != Marker::None && !runs.is_empty() {
                        return Err(ProtocolError::InvalidValue("marker run"));
                    }
                    let decoration_color = if flags & 256 != 0 {
                        Some(cursor.rgb()?)
                    } else {
                        None
                    };
                    let background = if flags & 1024 != 0 {
                        Some(cursor.rgb()?)
                    } else {
                        None
                    };
                    let alpha = if flags & 32768 != 0 {
                        cursor.take(1)?[0]
                    } else {
                        OPAQUE
                    };
                    let run = cursor.text(self.furniture.is_some())?;
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
                            color: decoration_color,
                            link,
                            alpha,
                            background,
                        },
                        typography,
                    });
                }
                cursor.done()?;
                self.feed(OwnedCommand::Paragraph(align, keep, text, runs, links))
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
                let alpha = cursor.take(1)?[0];
                cursor.done()?;
                self.feed(OwnedCommand::Image(slot, width, height, link, alt, alpha))
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
                let alpha = cursor.take(1)?[0];
                // Two bits per side, top first: 0 solid, 1 dashed, 2 dotted.
                let styles = cursor.take(1)?[0];
                let side = |index: u8| match styles >> (index * 2) & 3 {
                    0 => Ok(LineStyle::Solid),
                    1 => Ok(LineStyle::Dashed),
                    2 => Ok(LineStyle::Dotted),
                    _ => Err(ProtocolError::InvalidValue("border style")),
                };
                let border_style = Edges {
                    top: side(0)?,
                    right: side(1)?,
                    bottom: side(2)?,
                    left: side(3)?,
                };
                let radius = match cursor.take(1)?[0] {
                    0 => None,
                    1 => {
                        let percent = cursor.take(1)?[0];
                        let mut values = [0.0; 8];
                        for value in &mut values {
                            *value = cursor.nonnegative_f32("border radius")?;
                        }
                        Some(Radii { values, percent })
                    }
                    _ => return Err(ProtocolError::InvalidValue("border radius flag")),
                };
                // Bit 0 is border-box sizing; bits 1 to 4 are auto margins in `Edges` order.
                let flags = cursor.take(1)?[0];
                if flags > 0x1f {
                    return Err(ProtocolError::InvalidValue("box flags"));
                }
                let size = BoxSize {
                    width: cursor.size("width")?,
                    min_width: cursor.size("min width")?,
                    max_width: cursor.size("max width")?,
                    height: cursor.height()?,
                    min_height: cursor.height()?.unwrap_or(Pt::ZERO),
                    max_height: cursor.height()?,
                    border_box: flags & 1 != 0,
                    auto_margin: flags >> 1,
                };
                cursor.done()?;
                let style = crate::BoxStyle {
                    margin,
                    padding,
                    border,
                    background,
                    border_color,
                    border_style,
                    radius,
                    alpha,
                };
                if radius.is_some() && !style.uniform_border() {
                    return Err(ProtocolError::InvalidValue(
                        "border radius with unequal borders",
                    ));
                }
                self.feed(OwnedCommand::BoxStart(style, size))
            }
            Opcode::BoxEnd => {
                cursor.done()?;
                self.feed(OwnedCommand::BoxEnd)
            }
            Opcode::End => {
                cursor.done()?;
                if !self.frames.is_empty() || self.furniture.is_some() {
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

    /// The built-in fonts take the first slots; the rest are registered fonts.
    fn font(&self, slot: u8) -> Result<FontId, ProtocolError> {
        let font = FontId::new(slot);
        if self.pdfa && font.index() < BUILT_IN {
            return Err(ProtocolError::PdfAFont);
        }
        let count = self
            .renderer
            .as_ref()
            .map_or(self.fonts.len() + BUILT_IN, crate::Renderer::font_count);
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
            OwnedCommand::Paragraph(_, _, text, runs, links) => {
                text.len()
                    + runs.len() * size_of::<TextRun>()
                    + links
                        .iter()
                        .map(|link| link.len() + size_of::<String>())
                        .sum::<usize>()
            }
            OwnedCommand::RowStart(columns, slots) => {
                columns.len() * size_of::<crate::ColumnWidth>()
                    + slots.len() * size_of::<crate::Slot>()
            }
            OwnedCommand::FlexStart(_, items) => items.len() * size_of::<FlexItem>(),
            OwnedCommand::Image(_, _, _, link, alt, _) => {
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

    /// Routes a decoded layout command.
    fn feed(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        let frame = match &command {
            OwnedCommand::StackStart(_) => Frame::Stack,
            OwnedCommand::BoxStart(style, size) => Frame::Box(
                self.placing()
                    && self.furniture.is_none()
                    && *size == BoxSize::AUTO
                    && style.radius.is_none(),
            ),
            OwnedCommand::RowStart(_, slots) => Frame::Row {
                cells: slots
                    .iter()
                    .filter(|slot| matches!(slot, crate::Slot::Cell { .. }))
                    .count(),
                done: 0,
            },
            OwnedCommand::FlexStart(_, items) => Frame::Flex {
                cells: items.len(),
                done: 0,
            },
            OwnedCommand::TableStart(header_rows, footer_rows, _) => Frame::Table {
                head: self
                    .placing()
                    .then(|| 1 + usize::from(*header_rows) + usize::from(*footer_rows)),
            },
            OwnedCommand::StackEnd
            | OwnedCommand::BoxEnd
            | OwnedCommand::RowEnd
            | OwnedCommand::FlexEnd
            | OwnedCommand::TableEnd => return self.close(command),
            OwnedCommand::Paragraph(..)
            | OwnedCommand::Spacer(_)
            | OwnedCommand::Image(..)
            | OwnedCommand::Anchor(_) => return self.layout_command(command),
        };
        self.open(frame, command)?;
        self.stream_table()
    }

    /// Whether a completed block places now: outside every frame, or directly in streaming boxes.
    fn placing(&self) -> bool {
        matches!(self.frames.last(), None | Some(Frame::Box(true)))
    }

    fn layout_command(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        self.body_started = self.furniture.is_none();
        let anchor = matches!(command, OwnedCommand::Anchor(_));
        self.push_command(command)?;
        if !anchor {
            self.complete_child()?;
        }
        if self.placing() {
            return self.flush_block(crate::Renderer::push_owned);
        }
        self.stream_table()
    }

    /// Runs after a streaming table's `TableStart` and after each of its children:
    /// hands the renderer the header and footer rows once, then each body row.
    fn stream_table(&mut self) -> Result<(), ProtocolError> {
        let Some(Frame::Table { head: Some(head) }) = self.frames.last_mut() else {
            return Ok(());
        };
        if *head == 0 {
            return self.flush_block(crate::Renderer::push_table_row);
        }
        *head -= 1;
        if *head == 0 {
            self.block.push(OwnedCommand::TableEnd);
            return self.flush_block(crate::Renderer::push_table_head);
        }
        Ok(())
    }

    fn open(&mut self, frame: Frame, command: OwnedCommand) -> Result<(), ProtocolError> {
        if self.frames.len() >= MAX_LAYOUT_DEPTH {
            return Err(ProtocolError::NestingTooDeep);
        }
        match (frame, command) {
            (Frame::Box(true), OwnedCommand::BoxStart(style, _)) => {
                self.ensure_renderer()?.open_box(style)?
            }
            (_, command) => self.push_command(command)?,
        }
        self.frames.push(frame);
        Ok(())
    }

    fn close(&mut self, command: OwnedCommand) -> Result<(), ProtocolError> {
        let frame = self.frames.pop().ok_or(ProtocolError::InvalidNesting)?;
        match (&command, frame) {
            (OwnedCommand::TableEnd, Frame::Table { head: Some(0) }) => {
                return Ok(self.ensure_renderer()?.end_table()?);
            }
            (OwnedCommand::BoxEnd, Frame::Box(true)) => {
                return Ok(self.ensure_renderer()?.close_box()?);
            }
            (OwnedCommand::StackEnd, Frame::Stack)
            | (OwnedCommand::BoxEnd, Frame::Box(false))
            | (OwnedCommand::TableEnd, Frame::Table { .. }) => {}
            (OwnedCommand::RowEnd, Frame::Row { cells, done })
            | (OwnedCommand::FlexEnd, Frame::Flex { cells, done })
                if cells == done => {}
            (OwnedCommand::RowEnd, Frame::Row { .. }) => {
                return Err(ProtocolError::RowCellCountMismatch)
            }
            _ => return Err(ProtocolError::InvalidNesting),
        }
        self.layout_command(command)
    }

    fn complete_child(&mut self) -> Result<(), ProtocolError> {
        if let Some(Frame::Row { cells, done } | Frame::Flex { cells, done }) =
            self.frames.last_mut()
        {
            *done += 1;
            if *done > *cells {
                return Err(ProtocolError::RowCellCountMismatch);
            }
        }
        Ok(())
    }

    fn flush_block(
        &mut self,
        push: fn(&mut crate::Renderer, &[OwnedCommand]) -> Result<(), crate::RenderError>,
    ) -> Result<(), ProtocolError> {
        let furniture = self.furniture.take();
        self.body_started |= furniture.is_none();
        let block = std::mem::take(&mut self.block);
        self.block_bytes = 0;
        let renderer = self.ensure_renderer()?;
        if let Some((footer, first)) = furniture {
            return Ok(renderer.set_furniture(footer, first, block)?);
        }
        let result = push(renderer, &block).map_err(ProtocolError::from);
        self.block = block;
        self.block.clear();
        self.block_bytes = 0;
        result
    }

    pub fn finish(self) -> Result<Vec<u8>, ProtocolError> {
        if self.page.is_none() || !self.ended || !self.pending.is_empty() {
            return Err(ProtocolError::TruncatedProtocol);
        }
        Ok(self
            .renderer
            .ok_or(ProtocolError::EmptyDocument)?
            .finish()?)
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
