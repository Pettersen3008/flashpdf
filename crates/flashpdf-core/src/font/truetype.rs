use std::collections::{BTreeMap, BTreeSet};

use pdf_writer::Rect;

use super::{cmap, subset};

pub(super) const INVALID: &str = "invalid TrueType font";

pub(crate) struct EmbeddedFont {
    pub(crate) bytes: Vec<u8>,
    cmap: Vec<cmap::Range>,
    /// Advance per glyph id in 1/1000 em.
    advances: Vec<u16>,
    /// Glyph ids shown so far; the subset and `/W` array cover exactly these.
    pub(crate) used: BTreeSet<u16>,
    pub(crate) bbox: Rect,
    pub(crate) name: String,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) line_gap: f32,
    pub(crate) cap_height: f32,
    pub(crate) stem_v: f32,
}

impl EmbeddedFont {
    pub(crate) fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        if !bytes.starts_with(&[0, 1, 0, 0]) {
            return Err(INVALID.into());
        }
        let head = ttf_table(&bytes, b"head")?;
        let hhea = ttf_table(&bytes, b"hhea")?;
        let hmtx = ttf_table(&bytes, b"hmtx")?;
        let maxp = ttf_table(&bytes, b"maxp")?;
        let cmap = ttf_table(&bytes, b"cmap")?;
        let units = be_u16(head, 18)?;
        if units == 0 {
            return Err(INVALID.into());
        }
        let metrics = usize::from(be_u16(hhea, 34)?);
        let glyphs = be_u16(maxp, 4)?;
        if metrics == 0 || metrics > usize::from(glyphs) || hmtx.len() < metrics * 4 {
            return Err(INVALID.into());
        }
        let scale = 1000.0 / f32::from(units);
        let cmap = cmap::parse(cmap, glyphs)?;
        let advances = (0..usize::from(glyphs))
            .map(|gid| {
                let advance = be_u16(hmtx, gid.min(metrics - 1) * 4)?;
                Ok((f32::from(advance) * scale).round() as u16)
            })
            .collect::<Result<_, String>>()?;
        let bbox = Rect::new(
            f32::from(be_i16(head, 36)?) * scale,
            f32::from(be_i16(head, 38)?) * scale,
            f32::from(be_i16(head, 40)?) * scale,
            f32::from(be_i16(head, 42)?) * scale,
        );
        let os2 = ttf_table(&bytes, b"OS/2").ok();
        let mut ascent = f32::from(be_i16(hhea, 4)?) * scale;
        let mut descent = f32::from(be_i16(hhea, 6)?) * scale;
        let mut line_gap = f32::from(be_i16(hhea, 8)?) * scale;
        // A positive descender or no ascender would stack lines on one baseline;
        // when hhea is degenerate, USE_TYPO_METRICS (fsSelection bit 7) says the
        // OS/2 typo metrics are the ones the designer trusts.
        if let Some(os2) = os2.filter(|os2| {
            (ascent <= 0.0 || descent > 0.0) && be_u16(os2, 62).is_ok_and(|bits| bits & 0x80 != 0)
        }) {
            ascent = f32::from(be_i16(os2, 68)?) * scale;
            descent = f32::from(be_i16(os2, 70)?) * scale;
            line_gap = f32::from(be_i16(os2, 72)?) * scale;
        }
        if ascent <= 0.0 || descent > 0.0 {
            return Err(INVALID.into());
        }
        let cap_height = os2
            .filter(|os2| be_u16(os2, 0).is_ok_and(|version| version >= 2))
            .and_then(|os2| be_i16(os2, 88).ok())
            .map(|value| f32::from(value) * scale)
            .filter(|value| *value > 0.0)
            .unwrap_or(ascent);
        let weight = os2.and_then(|os2| be_u16(os2, 4).ok()).unwrap_or(400);
        let name = ttf_table(&bytes, b"name")
            .ok()
            .and_then(postscript_name)
            .unwrap_or_else(|| "FlashPDF".into());
        Ok(Self {
            bytes,
            cmap,
            advances,
            used: BTreeSet::new(),
            bbox,
            name,
            ascent,
            descent,
            line_gap: line_gap.max(0.0),
            cap_height,
            // ponytail: PDF has no true stem width without outlines; this is the common weight-class heuristic.
            stem_v: (50.0 + (f32::from(weight) / 65.0).powi(2)).round(),
        })
    }

    pub(crate) fn glyph(&self, character: char) -> Option<u16> {
        cmap::lookup(&self.cmap, character)
    }

    pub(crate) fn advance(&self, gid: u16) -> u16 {
        self.advances.get(usize::from(gid)).copied().unwrap_or(0)
    }

    /// Lowest code point per used glyph, for `/ToUnicode`.
    pub(crate) fn unicode(&self) -> BTreeMap<u16, char> {
        cmap::reverse(&self.cmap, &self.used)
    }

    /// The font program to embed: the used-glyph subset, or the whole file
    /// when a glyph table is malformed in a way parsing did not catch.
    pub(crate) fn program(&self) -> Vec<u8> {
        subset::subset(&self.bytes, &self.used).unwrap_or_else(|| self.bytes.clone())
    }

    /// `/BaseFont` with the PDF subset prefix.
    pub(crate) fn subset_name(&self) -> String {
        format!("{}+{}", subset::tag(&self.used), self.name)
    }
}

/// nameID 6 (PostScript), else 4 (full) or 1 (family), reduced to the characters a PostScript name allows.
fn postscript_name(name: &[u8]) -> Option<String> {
    let count = be_u16(name, 2).ok()?;
    let strings = usize::from(be_u16(name, 4).ok()?);
    let mut best: Option<(u8, String)> = None;
    for index in 0..usize::from(count) {
        let record = 6 + index * 12;
        let rank = match be_u16(name, record + 6).ok()? {
            6 => 0,
            4 => 1,
            1 => 2,
            _ => continue,
        };
        if best.as_ref().is_some_and(|(previous, _)| *previous <= rank) {
            continue;
        }
        let start = strings + usize::from(be_u16(name, record + 10).ok()?);
        let data = name.get(start..start + usize::from(be_u16(name, record + 8).ok()?))?;
        let text: String = match (be_u16(name, record).ok()?, be_u16(name, record + 2).ok()?) {
            (1, 0) => data.iter().map(|byte| char::from(*byte)).collect(),
            (0, _) | (3, 1) | (3, 10) => char::decode_utf16(
                data.chunks_exact(2)
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]])),
            )
            .filter_map(Result::ok)
            .collect(),
            _ => continue,
        };
        let text: String = text
            .chars()
            .filter(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.' | '+')
            })
            .take(63)
            .collect();
        if !text.is_empty() {
            best = Some((rank, text));
        }
    }
    best.map(|(_, name)| name)
}

pub(super) fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(INVALID)?
            .try_into()
            .unwrap(),
    ))
}

pub(super) fn be_i16(bytes: &[u8], offset: usize) -> Result<i16, String> {
    Ok(i16::from_be_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or(INVALID)?
            .try_into()
            .unwrap(),
    ))
}

pub(super) fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or(INVALID)?
            .try_into()
            .unwrap(),
    ))
}

pub(super) fn ttf_table<'a>(bytes: &'a [u8], wanted: &[u8; 4]) -> Result<&'a [u8], String> {
    let count = usize::from(be_u16(bytes, 4)?);
    for index in 0..count {
        let record = 12 + index * 16;
        if bytes.get(record..record + 4) != Some(wanted) {
            continue;
        }
        let start = usize::try_from(be_u32(bytes, record + 8)?).map_err(|_| INVALID)?;
        let length = usize::try_from(be_u32(bytes, record + 12)?).map_err(|_| INVALID)?;
        return bytes
            .get(start..start.checked_add(length).ok_or(INVALID)?)
            .ok_or_else(|| INVALID.into());
    }
    Err(INVALID.into())
}

#[cfg(test)]
mod tests {
    use super::{be_u16, be_u32, ttf_table, EmbeddedFont};

    fn abel() -> Vec<u8> {
        include_bytes!("../../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf").to_vec()
    }

    #[test]
    fn given_hhea_claiming_no_ascent_when_parsing_a_font_then_rejects_it() {
        let mut bytes = abel();
        assert!(EmbeddedFont::parse(bytes.clone()).is_ok());
        let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
        let record = (0..count)
            .map(|index| 12 + index * 16)
            .find(|record| &bytes[*record..record + 4] == b"hhea")
            .unwrap();
        let hhea = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize;
        bytes[hhea + 4..hhea + 8].fill(0);
        assert!(EmbeddedFont::parse(bytes).is_err());
    }

    #[test]
    fn given_abel_when_parsing_then_reads_name_cap_height_and_weight_stem() {
        let font = EmbeddedFont::parse(abel()).unwrap();
        assert_eq!(font.name, "Abel-Regular");
        assert_eq!(font.cap_height.round(), 700.0);
        assert_eq!(font.stem_v, 88.0);
        assert!(font.descent < 0.0 && font.line_gap >= 0.0);
    }

    /// Glyph lengths from a font program's `loca`, checking the directory is well formed.
    fn glyph_lengths(program: &[u8]) -> Vec<usize> {
        let loca = ttf_table(program, b"loca").unwrap();
        let glyphs = usize::from(be_u16(ttf_table(program, b"maxp").unwrap(), 4).unwrap());
        assert_eq!(loca.len(), (glyphs + 1) * 4);
        let offsets: Vec<usize> = (0..=glyphs)
            .map(|index| be_u32(loca, index * 4).unwrap() as usize)
            .collect();
        assert_eq!(
            *offsets.last().unwrap(),
            ttf_table(program, b"glyf").unwrap().len()
        );
        offsets.windows(2).map(|pair| pair[1] - pair[0]).collect()
    }

    #[test]
    fn given_two_used_glyphs_when_subsetting_then_only_they_keep_outlines_and_the_program_shrinks()
    {
        let mut font = EmbeddedFont::parse(abel()).unwrap();
        let (a, ae) = (font.glyph('A').unwrap(), font.glyph('\u{c6}').unwrap());
        font.used.extend([a, ae]);
        let program = font.program();
        assert!(program.len() < font.bytes.len() / 4, "{}", program.len());
        let lengths = glyph_lengths(&program);
        assert_eq!(lengths.len(), usize::from(a.max(ae)) + 1);
        // Glyph 0 stays too, but Abel's .notdef has no outline of its own.
        for (gid, length) in lengths.iter().enumerate().skip(1) {
            let kept = gid == usize::from(a) || gid == usize::from(ae);
            assert_eq!(*length > 0, kept, "glyph {gid}");
        }
        assert_eq!(font.subset_name().len(), 7 + font.name.len());
        assert!(font.subset_name().ends_with("+Abel-Regular"));
        assert_eq!(font.program(), program);
    }
}
