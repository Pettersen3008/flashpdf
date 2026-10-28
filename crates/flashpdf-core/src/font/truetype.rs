use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::sync::Arc;

use pdf_writer::Rect;

use super::cff::Cff;
use super::shape::{Program, Shaping};
use super::variable::{self, Axis, Instance};
use super::{cmap, subset, Stroke};
use crate::pdf::{deflate, Stream};

pub(super) const INVALID: &str = "invalid TrueType font";

#[derive(Clone)]
pub(crate) struct EmbeddedFont {
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) shaping: Arc<Shaping>,
    cmap: Vec<cmap::Range>,
    /// Advance per glyph id in 1/1000 em.
    advances: Vec<u16>,
    /// Glyph ids shown so far; the subset and `/W` array cover exactly these.
    pub(crate) used: BTreeSet<u16>,
    /// A bit per glyph id in `used`, so a glyph shown again skips the tree.
    seen: Vec<u64>,
    pub(crate) bbox: Rect,
    pub(crate) name: String,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) line_gap: f32,
    pub(crate) cap_height: f32,
    pub(crate) stem_v: f32,
    pub(crate) underline: Stroke,
    pub(crate) strikeout: Stroke,
    /// The face's style from OS/2, which orders fallback candidates.
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// Text of used glyphs no code point maps to, such as ligatures, for `/ToUnicode`.
    pub(crate) names: BTreeMap<u16, Box<str>>,
    /// 1000 over units per em.
    pub(crate) scale: f32,
    /// The `fvar` axes; none for a static font.
    pub(crate) axes: Vec<Axis>,
    /// Normalized F2Dot14 coordinates of an instance; empty for the default one.
    pub(crate) coords: Box<[i16]>,
    /// Where an OpenType font's `CFF ` table sits; `None` for TrueType outlines.
    pub(crate) cff: Option<Range<usize>>,
}

impl EmbeddedFont {
    pub(crate) fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        let cff = if bytes.starts_with(b"OTTO") {
            if table_range(&bytes, b"CFF2").is_ok() {
                return Err("CFF2 outlines are not supported; use a TrueType or CFF font".into());
            }
            let range = table_range(&bytes, b"CFF ")?;
            Cff::parse(&bytes[range.clone()]).ok_or(INVALID)?;
            Some(range)
        } else if bytes.starts_with(&[0, 1, 0, 0]) {
            None
        } else {
            return Err(INVALID.into());
        };
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
        let selection = os2.and_then(|os2| be_u16(os2, 62).ok()).unwrap_or(0);
        // `post` gives the underline centre, as FreeType reads it; OS/2 gives the strikeout top.
        // A missing or zero-thickness value falls back to Helvetica's.
        let stroke = |table: Option<&[u8]>, position: usize, size: usize, top: bool| {
            let (position, size) = table
                .and_then(|table| Some((be_i16(table, position).ok()?, be_i16(table, size).ok()?)))
                .filter(|(_, size)| *size > 0)?;
            let thickness = f32::from(size) * scale;
            let offset = f32::from(position) * scale;
            Some(Stroke {
                offset: if top {
                    offset - thickness / 2.0
                } else {
                    offset
                },
                thickness,
            })
        };
        let post = ttf_table(&bytes, b"post").ok();
        let underline = stroke(post, 8, 10, false).unwrap_or(Stroke {
            offset: -100.0,
            thickness: 50.0,
        });
        let strikeout = stroke(os2, 28, 26, true).unwrap_or(Stroke {
            offset: 262.0,
            thickness: 50.0,
        });
        let name = ttf_table(&bytes, b"name")
            .ok()
            .and_then(postscript_name)
            .unwrap_or_else(|| "FlashPDF".into());
        let axes = variable::axes(&bytes).map_err(|()| INVALID.to_owned())?;
        let shaping = Shaping::of(bytes).map_err(|()| INVALID.to_owned())?;
        Ok(Self {
            bytes: shaping.bytes.clone(),
            shaping,
            cmap,
            advances,
            used: BTreeSet::new(),
            seen: vec![0; 1024],
            bbox,
            name,
            ascent,
            descent,
            line_gap: line_gap.max(0.0),
            cap_height,
            // ponytail: PDF has no true stem width without outlines; this is the common weight-class heuristic.
            // A product, not `powi`, which may lower to a platform routine and differ from wasm.
            stem_v: (50.0 + (f32::from(weight) / 65.0) * (f32::from(weight) / 65.0)).round(),
            underline,
            strikeout,
            bold: weight >= 600 || selection & 0x20 != 0,
            italic: selection & 1 != 0,
            names: BTreeMap::new(),
            scale,
            axes,
            coords: Box::default(),
            cff,
        })
    }

    /// Normalized coordinates for user axis values, each clamped to its axis.
    pub(crate) fn coordinates(&self, values: &[([u8; 4], f32)]) -> Box<[i16]> {
        variable::normalize(&self.bytes, self.axes.len(), values)
    }

    /// This font at `coords`, from [`Self::coordinates`] for `values`, with its own glyph
    /// set: advances from `HVAR` or `gvar`, line metrics from `MVAR`, and a `/BaseFont`
    /// that names the axis values.
    pub(crate) fn instance(&self, coords: Box<[i16]>, values: &[([u8; 4], f32)]) -> Self {
        let mut font = self.clone();
        font.used.clear();
        font.seen = vec![0; 1024];
        font.names.clear();
        let Some(instance) = Instance::new(&self.bytes, &coords) else {
            return font;
        };
        let scale = self.scale;
        if let (Ok(hhea), Ok(hmtx)) = (
            ttf_table(&self.bytes, b"hhea"),
            ttf_table(&self.bytes, b"hmtx"),
        ) {
            let metrics = usize::from(be_u16(hhea, 34).unwrap_or(1)).max(1);
            for (gid, advance) in font.advances.iter_mut().enumerate() {
                let units = i32::from(be_u16(hmtx, gid.min(metrics - 1) * 4).unwrap_or(0))
                    + instance.advance_delta(gid as u16);
                *advance = (units.max(0) as f32 * scale).round() as u16;
            }
        }
        let delta = |tag: &[u8; 4]| instance.metric(tag) * scale;
        font.ascent += delta(b"hasc");
        font.descent += delta(b"hdsc");
        font.line_gap = (font.line_gap + delta(b"hlgp")).max(0.0);
        font.cap_height += delta(b"cpht");
        font.underline.offset += delta(b"undo");
        font.underline.thickness += delta(b"unds");
        font.strikeout.offset += delta(b"stro") - delta(b"strs") / 2.0;
        font.strikeout.thickness += delta(b"strs");
        // Each axis at its clamped user value, as the instance shows it.
        let user = |axis: &Axis| {
            let value = values
                .iter()
                .rev()
                .find(|(tag, _)| *tag == axis.tag)
                .map_or(axis.default, |(_, value)| *value);
            value.clamp(axis.min, axis.max)
        };
        let mut suffix = String::new();
        for (axis, coord) in self.axes.iter().zip(&coords) {
            let value = user(axis);
            match &axis.tag {
                b"wght" => {
                    font.bold = value >= 600.0;
                    font.stem_v = (50.0 + (value / 65.0) * (value / 65.0)).round();
                }
                b"ital" => font.italic |= value >= 0.5,
                b"slnt" => font.italic |= value < 0.0,
                _ => {}
            }
            if *coord != 0 {
                let tag = axis.tag.iter().filter(|byte| byte.is_ascii_alphanumeric());
                suffix.push('_');
                suffix.push_str(&value.to_string());
                suffix.extend(tag.map(|byte| char::from(*byte)));
            }
        }
        // A PostScript name holds 63 characters; past that a hash of the coordinates tells
        // instances apart.
        font.name = format!("{}{suffix}", self.name);
        if font.name.len() > 63 {
            let hash = coords.iter().fold(0x811c_9dc5_u32, |hash, coord| {
                coord.to_be_bytes().iter().fold(hash, |hash, byte| {
                    (hash ^ u32::from(*byte)).wrapping_mul(0x0100_0193)
                })
            });
            font.name = format!("{:.54}_{hash:08X}", self.name);
        }
        font.coords = coords;
        font
    }

    pub(crate) fn mark(&mut self, gid: u16) {
        let (word, bit) = (usize::from(gid / 64), 1 << (gid % 64));
        if self.seen[word] & bit == 0 {
            self.seen[word] |= bit;
            self.used.insert(gid);
        }
    }

    pub(crate) fn glyph(&self, character: char) -> Option<u16> {
        cmap::lookup(&self.cmap, character)
    }

    pub(crate) fn advance(&self, gid: u16) -> u16 {
        self.advances.get(usize::from(gid)).copied().unwrap_or(0)
    }

    /// Lowest code point that maps to `gid`, leaving out private use, which reads as nothing.
    pub(crate) fn character(&self, gid: u16) -> Option<char> {
        cmap::reverse(&self.cmap, &BTreeSet::from([gid]))
            .pop_first()
            .map(|(_, character)| character)
            .filter(|character| !matches!(character, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..))
    }

    /// Text per used glyph for `/ToUnicode`: what shaping made it from, else its lowest code point.
    pub(crate) fn unicode(&self) -> Vec<(u16, String)> {
        let characters = cmap::reverse(&self.cmap, &self.used);
        self.used
            .iter()
            .filter_map(|gid| {
                let text = match self.names.get(gid) {
                    Some(name) => name.to_string(),
                    None => characters.get(gid)?.to_string(),
                };
                Some((*gid, text))
            })
            .collect()
    }

    /// The font program to embed: the used-glyph subset, or the whole file
    /// when a glyph table is malformed in a way parsing did not catch.
    // ponytail: an instance with malformed variation data embeds the default outlines;
    // reject such fonts at registration if one turns up.
    pub(crate) fn program(&self) -> Vec<u8> {
        if let Some(range) = &self.cff {
            let table = &self.bytes[range.clone()];
            // `parse` checked the table, so this always subsets.
            return Cff::parse(table)
                .map_or_else(|| table.to_vec(), |cff| cff.subset(Some(&self.used)));
        }
        let instance = (!self.coords.is_empty())
            .then(|| Instance::new(&self.bytes, &self.coords))
            .flatten();
        subset::subset(&self.bytes, &self.used, instance.as_ref())
            .unwrap_or_else(|| self.bytes.to_vec())
    }

    /// [`Self::program`] deflated, shared with earlier renders that showed the same glyphs.
    pub(crate) fn deflated_program(&self) -> Program {
        self.shaping.program(&self.used, &self.coords, || {
            let program = self.program();
            (program.len(), deflate(&program, Stream::Table))
        })
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
    Ok(&bytes[table_range(bytes, wanted)?])
}

/// Where a table sits in the file, checked to lie inside it.
pub(super) fn table_range(bytes: &[u8], wanted: &[u8; 4]) -> Result<Range<usize>, String> {
    let count = usize::from(be_u16(bytes, 4)?);
    for index in 0..count {
        let record = 12 + index * 16;
        if bytes.get(record..record + 4) != Some(wanted) {
            continue;
        }
        let start = usize::try_from(be_u32(bytes, record + 8)?).map_err(|_| INVALID)?;
        let length = usize::try_from(be_u32(bytes, record + 12)?).map_err(|_| INVALID)?;
        let range = start..start.checked_add(length).ok_or(INVALID)?;
        return match bytes.get(range.clone()) {
            Some(_) => Ok(range),
            None => Err(INVALID.into()),
        };
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

    #[test]
    fn given_abel_when_parsing_then_centres_the_underline_on_post_and_the_strikeout_below_the_os2_top(
    ) {
        let font = EmbeddedFont::parse(abel()).unwrap();
        // post: -369 and 82 units; OS/2: a 102-unit stroke whose top is at 512, of 2048.
        let round = |value: f32| (value * 100.0).round() / 100.0;
        assert_eq!(round(font.underline.offset), -180.18);
        assert_eq!(round(font.underline.thickness), 40.04);
        assert_eq!(round(font.strikeout.offset), 225.1);
        assert_eq!(round(font.strikeout.thickness), 49.8);
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

    fn roboto_flex() -> EmbeddedFont {
        EmbeddedFont::parse(
            include_bytes!("../../../../packages/flashpdf/test/fixtures/RobotoFlex-subset.ttf")
                .to_vec(),
        )
        .unwrap()
    }

    /// The points of a simple glyph and the offsets of a composite's components in `program`.
    fn outline(program: &[u8], gid: u16) -> Vec<(i32, i32)> {
        use read_fonts::tables::glyf::{Anchor, Glyph};
        use read_fonts::{types::GlyphId, FontRef, TableProvider};
        let font = FontRef::new(program).unwrap();
        let (glyf, loca) = (font.glyf().unwrap(), font.loca(None).unwrap());
        match loca
            .get_glyf(GlyphId::new(u32::from(gid)), &glyf)
            .unwrap()
            .unwrap()
        {
            Glyph::Simple(glyph) => glyph
                .points()
                .map(|point| (i32::from(point.x), i32::from(point.y)))
                .collect(),
            Glyph::Composite(glyph) => glyph
                .components()
                .map(|component| match component.anchor {
                    Anchor::Offset { x, y } => (i32::from(x), i32::from(y)),
                    Anchor::Point { .. } => unreachable!("Roboto Flex offsets its components"),
                })
                .collect(),
        }
    }

    #[test]
    fn given_roboto_flex_at_bold_condensed_when_instancing_then_outlines_offsets_and_advances_match_fonttools(
    ) {
        let font = roboto_flex();
        let tags: Vec<[u8; 4]> = font.axes.iter().map(|axis| axis.tag).collect();
        assert_eq!(tags, [*b"wght", *b"wdth", *b"slnt"]);
        let values = [(*b"wght", 700.0), (*b"wdth", 75.0)];
        let mut bold = font.instance(font.coordinates(&values), &values);
        let (a, aring, iacute) = (
            font.glyph('A').unwrap(),
            font.glyph('\u{c5}').unwrap(),
            font.glyph('\u{ed}').unwrap(),
        );
        // fontTools' instancer: A advances 1298 of 2048 units, iacute 562.
        assert_eq!(bold.advance(a), 634);
        assert_eq!(bold.advance(iacute), 274);
        assert_eq!(bold.name, "RobotoFlex-Regular_700wght_75wdth");
        assert!(bold.bold && !font.bold);
        bold.used.extend([a, aring, iacute]);
        let program = bold.program();
        assert!(ttf_table(&program, b"fpgm").is_err());
        let expected = [
            (-21, 0),
            (261, 0),
            (593, 1016),
            (603, 1049),
            (618, 1125),
            (623, 1156),
        ];
        let points = outline(&program, a);
        assert_eq!(points.len(), 18);
        for (point, expected) in points.iter().zip(expected) {
            assert!(
                (point.0 - expected.0).abs() <= 1 && (point.1 - expected.1).abs() <= 1,
                "{point:?} {expected:?}"
            );
        }
        assert_eq!(outline(&program, aring), [(0, 0), (128, 0)]);
        assert_eq!(outline(&program, iacute), [(0, 0), (-230, 0)]);
        assert_eq!(bold.program(), program);
    }

    #[test]
    fn given_an_axis_value_outside_its_range_when_instancing_then_clamps_it_and_the_default_has_no_coordinates(
    ) {
        let font = roboto_flex();
        let slanted = font.coordinates(&[(*b"slnt", -40.0), (*b"wght", 300.0)]);
        assert_eq!(
            slanted,
            font.coordinates(&[(*b"slnt", -10.0), (*b"wght", 300.0)])
        );
        let instance = font.instance(slanted, &[(*b"slnt", -40.0), (*b"wght", 300.0)]);
        assert_eq!(instance.name, "RobotoFlex-Regular_300wght_-10slnt");
        assert!(instance.italic);
        assert!(font
            .coordinates(&[(*b"wght", 400.0)])
            .iter()
            .all(|coord| *coord == 0));
    }

    #[test]
    fn given_a_variable_font_without_hvar_when_instancing_then_advances_come_from_the_gvar_phantom_points(
    ) {
        let mut bytes =
            include_bytes!("../../../../packages/flashpdf/test/fixtures/RobotoFlex-subset.ttf")
                .to_vec();
        let record = (0..usize::from(be_u16(&bytes, 4).unwrap()))
            .map(|index| 12 + index * 16)
            .find(|record| &bytes[*record..record + 4] == b"HVAR")
            .unwrap();
        bytes[record] = b'X';
        let font = EmbeddedFont::parse(bytes).unwrap();
        let values = [(*b"wght", 700.0), (*b"wdth", 75.0)];
        let bold = font.instance(font.coordinates(&values), &values);
        assert_eq!(bold.advance(font.glyph('A').unwrap()), 634);
    }

    #[test]
    fn given_cff2_outlines_when_parsing_then_rejects_naming_them() {
        let mut bytes =
            include_bytes!("../../../../packages/flashpdf/test/fixtures/TestSansCFF-subset.otf")
                .to_vec();
        let record = (0..usize::from(be_u16(&bytes, 4).unwrap()))
            .map(|index| 12 + index * 16)
            .find(|record| &bytes[*record..record + 4] == b"CFF ")
            .unwrap();
        bytes[record + 3] = b'2';
        assert_eq!(
            EmbeddedFont::parse(bytes).err().unwrap(),
            "CFF2 outlines are not supported; use a TrueType or CFF font"
        );
    }
}
