use crate::win_ansi::decode;
use pdf_writer::Rect;

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
            let Some(character) = decode((index + 32) as u8) else {
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

#[cfg(test)]
mod tests {
    use super::{cmap_glyph, EmbeddedFont};

    #[test]
    fn given_hhea_claiming_no_ascent_when_parsing_a_font_then_rejects_it() {
        let mut bytes =
            include_bytes!("../../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf").to_vec();
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
