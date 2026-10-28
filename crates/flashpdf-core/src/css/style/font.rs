use crate::css::Message;
use std::collections::HashMap;
use std::rc::Rc;

use super::super::{js_space, js_trim, Value};
use super::Style;

/// The numeric weight: `normal` is 400 and `bold` 700.
pub fn weight(value: &Value) -> Result<f32, Message> {
    if value.is("normal") {
        return Ok(400.0);
    }
    if value.is("bold") {
        return Ok(700.0);
    }
    let weight = match value {
        Value::Number(weight) => *weight,
        // `\d+(\.\d+)?` after trimming.
        Value::Text(text) => {
            let text = js_trim(text);
            let (whole, fraction) = text.split_once('.').unwrap_or((text, "0"));
            if digits(whole) && digits(fraction) {
                text.parse().unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        }
    };
    if !(1.0..=1000.0).contains(&weight) {
        return Err(format!("invalid fontWeight: {value} (use normal, bold, or 1 to 1000)").into());
    }
    Ok(weight as f32)
}

/// `font-style`, with an oblique angle in degrees, clockwise positive.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slant {
    Normal,
    Italic,
    Oblique(f32),
}

/// The angle CSS gives a bare `oblique`.
pub const OBLIQUE: f32 = 14.0;

pub fn slant(value: &Value) -> Result<Slant, Message> {
    let invalid = || -> Message {
        format!("invalid fontStyle: {value} (use normal, italic, oblique, or oblique with an angle from -90deg to 90deg)").into()
    };
    let text = value.text().map(js_trim).ok_or_else(invalid)?;
    match text {
        "normal" => return Ok(Slant::Normal),
        "italic" => return Ok(Slant::Italic),
        "oblique" => return Ok(Slant::Oblique(OBLIQUE)),
        _ => {}
    }
    let angle = text
        .strip_prefix("oblique")
        .filter(|rest| rest.starts_with(js_space))
        .and_then(|rest| js_trim(rest).strip_suffix("deg"))
        .and_then(|angle| angle.parse::<f32>().ok())
        .filter(|angle| (-90.0..=90.0).contains(angle))
        .ok_or_else(invalid)?;
    Ok(Slant::Oblique(angle))
}

/// `font-stretch` as a percent of the normal width.
pub fn stretch(value: &Value) -> Result<f32, Message> {
    const KEYWORDS: [(&str, f32); 9] = [
        ("ultra-condensed", 50.0),
        ("extra-condensed", 62.5),
        ("condensed", 75.0),
        ("semi-condensed", 87.5),
        ("normal", 100.0),
        ("semi-expanded", 112.5),
        ("expanded", 125.0),
        ("extra-expanded", 150.0),
        ("ultra-expanded", 200.0),
    ];
    let text = value.text().map(js_trim).unwrap_or_default();
    KEYWORDS
        .iter()
        .find(|(keyword, _)| *keyword == text)
        .map(|(_, percent)| *percent)
        .or_else(|| {
            text.strip_suffix('%')
                .and_then(|percent| percent.parse::<f32>().ok())
                .filter(|percent| percent.is_finite() && *percent >= 0.0)
        })
        .ok_or_else(|| -> Message {
            format!("invalid fontStretch: {value} (use a percent or a keyword from ultra-condensed to ultra-expanded)").into()
        })
}

/// Axis tags with their user values.
pub type Variations = Rc<[([u8; 4], f32)]>;

/// `font-variation-settings`: four-letter axis tags with their values, the last of a
/// repeated tag winning; none for `normal`.
pub fn variations(value: &Value) -> Result<Variations, Message> {
    let invalid = || -> Message {
        format!("invalid fontVariationSettings: {value} (use normal or a list such as '\"wght\" 650, \"wdth\" 80')").into()
    };
    let text = value.text().map(js_trim).ok_or_else(invalid)?;
    if text == "normal" {
        return Ok(Rc::new([]));
    }
    let mut settings: Vec<([u8; 4], f32)> = Vec::new();
    for setting in text.split(',') {
        let setting = js_trim(setting);
        let quote = setting
            .chars()
            .next()
            .filter(|quote| matches!(quote, '"' | '\''));
        let (tag, number) = quote
            .and_then(|quote| setting[1..].split_once(quote))
            .ok_or_else(invalid)?;
        let tag: [u8; 4] = tag
            .as_bytes()
            .try_into()
            .ok()
            .filter(|tag: &[u8; 4]| tag.iter().all(|byte| (0x20..=0x7e).contains(byte)))
            .ok_or_else(invalid)?;
        let number = js_trim(number)
            .parse::<f32>()
            .ok()
            .filter(|number| number.is_finite())
            .ok_or_else(invalid)?;
        settings.retain(|(own, _)| *own != tag);
        settings.push((tag, number));
    }
    Ok(settings.into())
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// The face slots of a family, as the engine numbers its fonts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSlots {
    pub regular: u32,
    pub bold: Option<u32>,
    pub italic: Option<u32>,
    pub bold_italic: Option<u32>,
}

const fn slots(regular: u32) -> FontSlots {
    FontSlots {
        regular,
        bold: Some(regular + 1),
        italic: Some(regular + 2),
        bold_italic: Some(regular + 3),
    }
}

pub const HELVETICA: FontSlots = slots(0);
pub const TIMES: FontSlots = slots(4);
pub const COURIER: FontSlots = slots(8);

/// Built-in families by name, and by generic keyword, which CSS recognises only unquoted.
fn built_in(key: &str, quoted: bool) -> Option<FontSlots> {
    match key {
        "helvetica" | "arial" => Some(HELVETICA),
        "times" | "times new roman" => Some(TIMES),
        "courier" | "courier new" => Some(COURIER),
        _ if quoted => None,
        "sans-serif" | "system-ui" | "ui-sans-serif" => Some(HELVETICA),
        "serif" | "ui-serif" => Some(TIMES),
        "monospace" | "ui-monospace" => Some(COURIER),
        _ => None,
    }
}

/// The names of a `font-family` list, each with whether it was quoted.
pub fn families(value: &str) -> Vec<(&str, bool)> {
    value
        .split(',')
        .filter_map(|token| {
            let name = js_trim(token);
            let bytes = name.as_bytes();
            let quoted = bytes.len() >= 2
                && matches!(bytes[0], b'"' | b'\'')
                && bytes[bytes.len() - 1] == bytes[0]
                && !name.contains(['\n', '\r', '\u{2028}', '\u{2029}']);
            let name = js_trim(if quoted {
                &name[1..name.len() - 1]
            } else {
                name
            });
            (!name.is_empty()).then_some((name, quoted))
        })
        .collect()
}

/// The family, weight, slant, width, and axis settings an element's text uses.
#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: FontSlots,
    pub weight: f32,
    pub slant: Slant,
    pub stretch: f32,
    /// `font-variation-settings`; `None` is `normal`.
    pub variations: Option<Variations>,
}

impl Font {
    pub const fn new(family: FontSlots) -> Self {
        Self {
            family,
            weight: 400.0,
            slant: Slant::Normal,
            stretch: 100.0,
            variations: None,
        }
    }

    /// Whether this is bold for a font without a weight axis.
    pub fn bold(&self) -> bool {
        self.weight >= 600.0
    }
}

pub fn font(
    s: &Style,
    inherited: &Font,
    fonts: &HashMap<String, FontSlots>,
) -> Result<Font, Message> {
    let mut family = inherited.family;
    let declared = s.get(prop!("fontFamily"));
    if let Some(value) = declared {
        let names = value.text().map(families).unwrap_or_default();
        // An entry that matches nothing is skipped for a later one; a list with no match rejects.
        let found = names.into_iter().find_map(|(name, quoted)| {
            let key = name
                .to_lowercase()
                .split(js_space)
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            fonts.get(name).copied().or_else(|| built_in(&key, quoted))
        });
        family = found.ok_or_else(|| format!(
            "unregistered font family: {value} (register it in fonts, or end the list with serif, sans-serif, or monospace)"
        ))?;
    }
    let variations = match s.get(prop!("fontVariationSettings")) {
        None => inherited.variations.clone(),
        Some(value) => Some(variations(value)?).filter(|settings| !settings.is_empty()),
    };
    Ok(Font {
        family,
        weight: s
            .get(prop!("fontWeight"))
            .map_or(Ok(inherited.weight), weight)?,
        slant: s
            .get(prop!("fontStyle"))
            .map_or(Ok(inherited.slant), slant)?,
        stretch: s
            .get(prop!("fontStretch"))
            .map_or(Ok(inherited.stretch), stretch)?,
        variations,
    })
}
