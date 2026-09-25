use crate::RenderError;

/// The standard-font encoding: Helvetica has no glyphs beyond it.
pub(crate) fn encode(character: char) -> Result<u8, RenderError> {
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

/// Control characters never render; body text may hold the ASCII whitespace
/// word splitting consumes, a footer only the page sentinels. Whether a
/// printable character has a glyph is the font's decision at layout.
pub(crate) fn valid(text: &str, footer: bool) -> bool {
    text.chars().all(|character| {
        !character.is_control()
            || if footer {
                matches!(character, crate::PAGE_NUMBER | crate::TOTAL_PAGES)
            } else {
                character.is_ascii_whitespace()
            }
    })
}
