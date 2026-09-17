use crate::{FontId, RenderError};
use pdf_writer::Rect;

const HELVETICA_WIDTHS: [u16; 224] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    0, 556, 0, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 222, 222,
    333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 0, 500, 667, 278, 333, 556, 556, 556, 556,
    260, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 556, 537, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 667, 667, 667, 667, 667, 667, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278, 278, 556, 556,
    556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

const HELVETICA_BOLD_WIDTHS: [u16; 224] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
    0, 556, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 278, 278,
    500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 667, 278, 333, 556, 556, 556, 556,
    280, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 611, 556, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 556, 556, 556, 556, 556, 278, 278, 278, 278, 611, 611,
    611, 611, 611, 611, 611, 584, 611, 611, 611, 611, 611, 556, 611, 556,
];

pub(crate) struct EmbeddedFont {
    pub(crate) bytes: Vec<u8>,
    pub(crate) widths: [Option<u16>; 224],
    pub(crate) bbox: Rect,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    pub(crate) cap_height: f32,
}

impl EmbeddedFont {
    pub(crate) fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        if !bytes.starts_with(&[0, 1, 0, 0]) {
            return Err("invalid TrueType font".into());
        }
        let head = ttf_table(&bytes, b"head")?;
        let hhea = ttf_table(&bytes, b"hhea")?;
        let hmtx = ttf_table(&bytes, b"hmtx")?;
        let maxp = ttf_table(&bytes, b"maxp")?;
        let cmap = ttf_table(&bytes, b"cmap")?;
        let units = be_u16(head, 18)?;
        if units == 0 {
            return Err("invalid TrueType font".into());
        }
        let metrics = usize::from(be_u16(hhea, 34)?);
        let glyphs = usize::from(be_u16(maxp, 4)?);
        if metrics == 0 || metrics > glyphs || hmtx.len() < metrics * 4 {
            return Err("invalid TrueType font".into());
        }
        let scale = 1000.0 / f32::from(units);
        let mut widths = [None; 224];
        for (index, width) in widths.iter_mut().enumerate() {
            let Some(character) = decode_win_ansi((index + 32) as u8) else {
                continue;
            };
            let Some(glyph) = cmap_glyph(cmap, character, glyphs)? else {
                continue;
            };
            let metric = usize::from(glyph).min(metrics - 1);
            *width = Some((f32::from(be_u16(hmtx, metric * 4)?) * scale).round() as u16);
        }
        let bbox = Rect::new(
            f32::from(be_i16(head, 36)?) * scale,
            f32::from(be_i16(head, 38)?) * scale,
            f32::from(be_i16(head, 40)?) * scale,
            f32::from(be_i16(head, 42)?) * scale,
        );
        let ascent = f32::from(be_i16(hhea, 4)?) * scale;
        let descent = f32::from(be_i16(hhea, 6)?) * scale;
        // Line height is ascent - descent, so a font claiming ascent <= descent
        // would stack every line on one baseline and give blocks zero height.
        if ascent <= 0.0 || ascent <= descent {
            return Err("invalid TrueType font".into());
        }
        Ok(Self {
            bytes,
            widths,
            bbox,
            ascent,
            descent,
            cap_height: ascent,
        })
    }
}

fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    Ok(u16::from_be_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or("invalid TrueType font")?
            .try_into()
            .unwrap(),
    ))
}

fn be_i16(bytes: &[u8], offset: usize) -> Result<i16, String> {
    Ok(i16::from_be_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or("invalid TrueType font")?
            .try_into()
            .unwrap(),
    ))
}

fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    Ok(u32::from_be_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or("invalid TrueType font")?
            .try_into()
            .unwrap(),
    ))
}

fn ttf_table<'a>(bytes: &'a [u8], wanted: &[u8; 4]) -> Result<&'a [u8], String> {
    let count = usize::from(be_u16(bytes, 4)?);
    for index in 0..count {
        let record = 12 + index * 16;
        if bytes.get(record..record + 4) != Some(wanted) {
            continue;
        }
        let start =
            usize::try_from(be_u32(bytes, record + 8)?).map_err(|_| "invalid TrueType font")?;
        let length =
            usize::try_from(be_u32(bytes, record + 12)?).map_err(|_| "invalid TrueType font")?;
        return bytes
            .get(start..start.checked_add(length).ok_or("invalid TrueType font")?)
            .ok_or_else(|| "invalid TrueType font".into());
    }
    Err("invalid TrueType font".into())
}

fn cmap_glyph(cmap: &[u8], character: char, glyphs: usize) -> Result<Option<u16>, String> {
    let count = usize::from(be_u16(cmap, 2)?);
    let mut selected = None;
    for index in 0..count {
        let record = 4 + index * 8;
        let platform = be_u16(cmap, record)?;
        let encoding = be_u16(cmap, record + 2)?;
        if platform != 0 && !(platform == 3 && matches!(encoding, 1 | 10)) {
            continue;
        }
        let offset =
            usize::try_from(be_u32(cmap, record + 4)?).map_err(|_| "invalid TrueType font")?;
        let subtable = cmap.get(offset..).ok_or("invalid TrueType font")?;
        if be_u16(subtable, 0)? == 4 {
            selected = Some(subtable);
            break;
        }
    }
    let cmap = selected.ok_or("unsupported TrueType cmap")?;
    let length = usize::from(be_u16(cmap, 2)?);
    let cmap = cmap.get(..length).ok_or("invalid TrueType font")?;
    let segments = usize::from(be_u16(cmap, 6)? / 2);
    if segments == 0 {
        return Err("invalid TrueType font".into());
    }
    let end_codes = 14;
    let start_codes = end_codes + segments * 2 + 2;
    let deltas = start_codes + segments * 2;
    let offsets = deltas + segments * 2;
    let code = character as u16;
    for index in 0..segments {
        let end = be_u16(cmap, end_codes + index * 2)?;
        if code > end {
            continue;
        }
        let start = be_u16(cmap, start_codes + index * 2)?;
        if code < start {
            return Ok(None);
        }
        let delta = be_i16(cmap, deltas + index * 2)? as u16;
        let range = usize::from(be_u16(cmap, offsets + index * 2)?);
        let glyph = if range == 0 {
            code.wrapping_add(delta)
        } else {
            let glyph = be_u16(
                cmap,
                offsets + index * 2 + range + usize::from(code - start) * 2,
            )?;
            if glyph == 0 {
                return Ok(None);
            }
            glyph.wrapping_add(delta)
        };
        if glyph == 0 {
            return Ok(None);
        }
        if usize::from(glyph) >= glyphs {
            return Err("invalid TrueType font".into());
        }
        return Ok(Some(glyph));
    }
    Ok(None)
}

pub(crate) enum Font {
    Helvetica { bold: bool },
    Embedded(Box<EmbeddedFont>),
}

impl Font {
    pub(crate) fn width(&self, byte: u8) -> Result<u16, RenderError> {
        match self {
            Self::Helvetica { bold } => Ok((if *bold {
                &HELVETICA_BOLD_WIDTHS
            } else {
                &HELVETICA_WIDTHS
            })[usize::from(byte) - 32]),
            Self::Embedded(font) => {
                font.widths[usize::from(byte) - 32].ok_or(RenderError::MissingGlyph)
            }
        }
    }

    pub(crate) fn metrics(&self) -> (f32, f32) {
        match self {
            Self::Helvetica { .. } => (718.0, -207.0),
            Self::Embedded(font) => (font.ascent, font.descent),
        }
    }
}

/// `/F1` is slot 0, so a PDF resource name never collides with a slot number.
pub(crate) fn font_name(font: FontId, buffer: &mut [u8; 4]) -> &[u8] {
    let number = u32::from(font.slot()) + 1;
    buffer[0] = b'F';
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
pub(crate) fn encode_win_ansi(character: char) -> Result<u8, RenderError> {
    let byte = match character {
        ' '..='~' | '\u{00a0}'..='\u{00ff}' => character as u8,
        '\u{20ac}' => 128,
        '\u{201a}' => 130,
        '\u{0192}' => 131,
        '\u{201e}' => 132,
        '\u{2026}' => 133,
        '\u{2020}' => 134,
        '\u{2021}' => 135,
        '\u{02c6}' => 136,
        '\u{2030}' => 137,
        '\u{0160}' => 138,
        '\u{2039}' => 139,
        '\u{0152}' => 140,
        '\u{017d}' => 142,
        '\u{2018}' => 145,
        '\u{2019}' => 146,
        '\u{201c}' => 147,
        '\u{201d}' => 148,
        '\u{2022}' => 149,
        '\u{2013}' => 150,
        '\u{2014}' => 151,
        '\u{02dc}' => 152,
        '\u{2122}' => 153,
        '\u{0161}' => 154,
        '\u{203a}' => 155,
        '\u{0153}' => 156,
        '\u{017e}' => 158,
        '\u{0178}' => 159,
        _ => return Err(RenderError::UnsupportedCharacter(character)),
    };
    Ok(byte)
}

pub(crate) fn valid_win_ansi(text: &str) -> bool {
    text.chars()
        .all(|character| matches!(character, '\t'..='\r') || encode_win_ansi(character).is_ok())
}

pub(crate) fn decode_win_ansi(byte: u8) -> Option<char> {
    Some(match byte {
        32..=126 | 160..=255 => char::from(byte),
        128 => '\u{20ac}',
        130 => '\u{201a}',
        131 => '\u{0192}',
        132 => '\u{201e}',
        133 => '\u{2026}',
        134 => '\u{2020}',
        135 => '\u{2021}',
        136 => '\u{02c6}',
        137 => '\u{2030}',
        138 => '\u{0160}',
        139 => '\u{2039}',
        140 => '\u{0152}',
        142 => '\u{017d}',
        145 => '\u{2018}',
        146 => '\u{2019}',
        147 => '\u{201c}',
        148 => '\u{201d}',
        149 => '\u{2022}',
        150 => '\u{2013}',
        151 => '\u{2014}',
        152 => '\u{02dc}',
        153 => '\u{2122}',
        154 => '\u{0161}',
        155 => '\u{203a}',
        156 => '\u{0153}',
        158 => '\u{017e}',
        159 => '\u{0178}',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{cmap_glyph, EmbeddedFont};

    #[test]
    fn given_hhea_claiming_no_ascent_when_parsing_a_font_then_rejects_it() {
        let mut bytes =
            include_bytes!("../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf").to_vec();
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

    fn cmap(delta: u16) -> Vec<u8> {
        [
            &[0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12][..],
            &[
                0,
                4,
                0,
                32,
                0,
                0,
                0,
                4,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                65,
                255,
                255,
                0,
                0,
                0,
                65,
                255,
                255,
                (delta >> 8) as u8,
                delta as u8,
                0,
                1,
                0,
                0,
                0,
                0,
            ],
        ]
        .concat()
    }

    #[test]
    fn given_a_zero_or_out_of_range_cmap_glyph_when_parsing_then_rejects_it() {
        assert_eq!(cmap_glyph(&cmap(0xffbf), 'A', 100).unwrap(), None);
        assert_eq!(
            cmap_glyph(&cmap(35), 'A', 100),
            Err("invalid TrueType font".into())
        );
    }
}
