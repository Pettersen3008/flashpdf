mod cmap;
mod shape;
mod standard;
mod subset;
mod truetype;

pub(crate) use shape::{Shaped, NO_KERNING, NO_LIGATURES};
pub(crate) use standard::name as base_font;
pub(crate) use truetype::EmbeddedFont;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Ref, Str};

use crate::layout::LayoutBuffer;
use crate::pdf::deflate;
use crate::{FontId, RenderError, BUILT_IN};

pub(crate) enum Font {
    /// A built-in face, by its `FontId` slot.
    Standard(u8),
    Embedded(Box<EmbeddedFont>),
}

/// Centre offset above the baseline and thickness of a decoration line, in 1/1000 em.
#[derive(Clone, Copy)]
pub(crate) struct Stroke {
    pub(crate) offset: f32,
    pub(crate) thickness: f32,
}

pub(crate) struct FontBook {
    fonts: Vec<Font>,
    used: Vec<bool>,
    /// Per embedded font, after the built-in slots: text of the shaped glyphs no code
    /// point spells, first use first, for `/ToUnicode`.
    names: RefCell<Vec<BTreeMap<u16, Box<str>>>>,
}

impl FontBook {
    pub(crate) fn new(embedded: Vec<EmbeddedFont>) -> Self {
        let mut fonts: Vec<Font> = (0..BUILT_IN as u8).map(Font::Standard).collect();
        fonts.extend(
            embedded
                .into_iter()
                .map(|font| Font::Embedded(Box::new(font))),
        );
        let used = vec![false; fonts.len()];
        let names = RefCell::new(vec![BTreeMap::new(); fonts.len() - BUILT_IN]);
        Self { fonts, used, names }
    }

    /// Shapes `text` in an embedded font, or names a character it has no glyph for.
    pub(crate) fn shape(&self, id: FontId, text: &str, flags: u8) -> Result<Arc<Shaped>, char> {
        let Some(Font::Embedded(font)) = self.get(id) else {
            unreachable!("only embedded fonts shape");
        };
        let shaped = font.shaping.shape(font, text, flags)?;
        if !shaped.names.is_empty() {
            let names = &mut self.names.borrow_mut()[id.index() - BUILT_IN];
            for (gid, name) in &shaped.names {
                names.entry(*gid).or_insert_with(|| name.clone());
            }
        }
        Ok(shaped)
    }

    pub(crate) fn get(&self, id: FontId) -> Option<&Font> {
        self.fonts.get(id.index())
    }

    pub(crate) fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Records that `text`, as encoded by `encode_into` for this font, is painted.
    pub(crate) fn mark_used(&mut self, id: FontId, text: &[u8]) {
        self.used[id.index()] = true;
        if let Font::Embedded(font) = &mut self.fonts[id.index()] {
            for pair in text.chunks_exact(2) {
                font.mark(u16::from_be_bytes([pair[0], pair[1]]));
            }
        }
    }

    pub(crate) fn mark_layout(&mut self, layout: &LayoutBuffer) {
        for segment in &layout.segments {
            self.mark_used(segment.font, &layout.text[segment.text.clone()]);
        }
    }

    pub(crate) fn into_parts(mut self) -> (Vec<Font>, Vec<bool>) {
        for (font, names) in self.fonts[BUILT_IN..]
            .iter_mut()
            .zip(self.names.into_inner())
        {
            if let Font::Embedded(font) = font {
                font.names = names;
            }
        }
        (self.fonts, self.used)
    }
}

impl Font {
    /// Appends the show-string code for `character` and returns its advance in
    /// 1/1000 em: one WinAnsi byte for a built-in font, a big-endian glyph id for an
    /// embedded font. This is the only place that decides how text is encoded.
    pub(crate) fn encode_into(
        &self,
        character: char,
        out: &mut Vec<u8>,
    ) -> Result<u16, RenderError> {
        match self {
            Self::Standard(face) => {
                let byte = crate::win_ansi::encode(character)?;
                out.push(byte);
                Ok(standard::width(*face, byte))
            }
            Self::Embedded(font) => {
                // A font without U+00A0 shows its space, as browsers do; preformatted text relies on it.
                let gid = font
                    .glyph(character)
                    .or_else(|| font.glyph(' ').filter(|_| character == '\u{a0}'))
                    .ok_or(RenderError::MissingGlyph(character))?;
                out.extend(gid.to_be_bytes());
                Ok(font.advance(gid))
            }
        }
    }

    /// AFM pair kerning in 1/1000 em between two codes from `encode_into`; harfrust kerns embedded fonts.
    pub(crate) fn kern(&self, left: &[u8], right: &[u8]) -> i16 {
        match self {
            Self::Standard(face) => standard::kern(*face, left[0], right[0]),
            Self::Embedded(_) => 0,
        }
    }

    /// Bytes per code in a show string.
    pub(crate) fn stride(&self) -> usize {
        match self {
            Self::Standard(_) => 1,
            Self::Embedded(_) => 2,
        }
    }

    /// Advance of one code produced by `encode_into`.
    pub(crate) fn advance(&self, code: &[u8]) -> u16 {
        match self {
            Self::Standard(face) => standard::width(*face, code[0]),
            Self::Embedded(font) => font.advance(u16::from_be_bytes([code[0], code[1]])),
        }
    }

    /// Ascent, descent and line gap per 1000 units; a line is ascent - descent + gap.
    pub(crate) fn metrics(&self) -> (f32, f32, f32) {
        match self {
            Self::Standard(face) => {
                let (ascent, descent) = standard::metrics(*face);
                (ascent, descent, 0.0)
            }
            Self::Embedded(font) => (font.ascent, font.descent, font.line_gap),
        }
    }

    /// Underline and line-through strokes: the AFM underline, shared by every built-in
    /// face, and half its x-height, or the embedded font's `post` and `OS/2` values.
    pub(crate) fn strokes(&self) -> (Stroke, Stroke) {
        match self {
            Self::Standard(face) => (
                Stroke {
                    offset: -100.0,
                    thickness: 50.0,
                },
                Stroke {
                    offset: standard::strike(*face),
                    thickness: 50.0,
                },
            ),
            Self::Embedded(font) => (font.underline, font.strikeout),
        }
    }
}

/// Helvetica is `/F1` to `/F4` and registered fonts follow from `/F5`, as before Times and
/// Courier existed, so a document that uses neither keeps its bytes. Times is `/T1` to `/T4`
/// and Courier `/C1` to `/C4`.
pub(crate) fn font_name(font: FontId, buffer: &mut [u8; 4]) -> &[u8] {
    let slot = u32::from(font.slot());
    let (prefix, number) = match slot {
        0..=3 => (b'F', slot + 1),
        4..=7 => (b'T', slot - 3),
        8..=11 => (b'C', slot - 7),
        _ => (b'F', slot - 7),
    };
    buffer[0] = prefix;
    let mut length = 1;
    for divisor in [100, 10] {
        if number >= divisor {
            buffer[length] = b'0' + (number / divisor % 10) as u8;
            length += 1;
        }
    }
    buffer[length] = b'0' + (number % 10) as u8;
    &buffer[..length + 1]
}

pub(crate) struct EmbeddedRefs {
    pub(crate) font: Ref,
    pub(crate) descendant: Ref,
    pub(crate) descriptor: Ref,
    pub(crate) file: Ref,
    pub(crate) to_unicode: Ref,
}

/// Type0 over a CIDFontType2 with Identity-H, so show strings are glyph ids;
/// `/W` lists only used glyphs and `/ToUnicode` maps them back for extraction.
/// `program` is the subset's length and its deflated bytes.
pub(crate) fn write_embedded(
    pdf: &mut Pdf,
    refs: EmbeddedRefs,
    font: &EmbeddedFont,
    (length, program): (usize, Vec<u8>),
) {
    let name = font.subset_name();
    let name = Name(name.as_bytes());
    pdf.type0_font(refs.font)
        .base_font(name)
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(refs.descendant)
        .to_unicode(refs.to_unicode);
    let mut cid = pdf.cid_font(refs.descendant);
    cid.subtype(CidFontType::Type2)
        .base_font(name)
        .system_info(SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"Identity"),
            supplement: 0,
        })
        .font_descriptor(refs.descriptor)
        .default_width(0.0);
    let used: Vec<u16> = font.used.iter().copied().collect();
    let mut widths = cid.widths();
    let mut start = 0;
    while start < used.len() {
        let mut end = start;
        while used.get(end + 1) == Some(&(used[end] + 1)) {
            end += 1;
        }
        widths.consecutive(
            used[start],
            used[start..=end]
                .iter()
                .map(|gid| f32::from(font.advance(*gid))),
        );
        start = end + 1;
    }
    widths.finish();
    cid.cid_to_gid_map_predefined(Name(b"Identity"));
    cid.finish();

    pdf.font_descriptor(refs.descriptor)
        .name(name)
        .flags(FontFlags::SYMBOLIC)
        .bbox(font.bbox)
        .italic_angle(0.0)
        .ascent(font.ascent)
        .descent(font.descent)
        .cap_height(font.cap_height)
        .stem_v(font.stem_v)
        .font_file2(refs.file);
    pdf.stream(refs.file, &program)
        .filter(Filter::FlateDecode)
        .pair(Name(b"Length1"), length as i32)
        .finish();

    let mut cmap = UnicodeCmap::new(
        Name(b"Adobe-Identity-UCS"),
        SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"UCS"),
            supplement: 0,
        },
    );
    // A ligature maps to all the characters it shows.
    for (gid, text) in font.unicode() {
        cmap.pair_with_multiple(gid, text.chars());
    }
    pdf.stream(refs.to_unicode, &deflate(&cmap.finish()))
        .filter(Filter::FlateDecode);
}
