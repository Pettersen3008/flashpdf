use crate::css::Message;
use std::collections::HashMap;

use super::super::{js_space, js_trim, Value};
use super::Style;

pub fn bold(value: &Value) -> Result<bool, Message> {
    if value.is("normal") || value.is("bold") {
        return Ok(value.is("bold"));
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
    Ok(weight >= 600.0)
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

pub fn italic(value: &Value) -> Result<bool, Message> {
    if !(value.is("normal") || value.is("italic") || value.is("oblique")) {
        return Err(format!("invalid fontStyle: {value} (use normal, italic, or oblique)").into());
    }
    Ok(!value.is("normal"))
}

/// The face slot for a weight and style, and whether the engine must slant an upright face for
/// `oblique`. None is a bold face the family lacks.
pub fn face(family: &FontSlots, bold: bool, slanted: bool) -> (Option<u32>, bool) {
    let own = if slanted {
        if bold {
            family.bold_italic
        } else {
            family.italic
        }
    } else {
        None
    };
    match own {
        Some(own) => (Some(own), false),
        None => (
            if bold {
                family.bold
            } else {
                Some(family.regular)
            },
            slanted,
        ),
    }
}

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

/// The family, boldness, and slant an element's text uses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Font {
    pub family: FontSlots,
    pub weight: bool,
    pub italic: bool,
}

pub fn font(
    s: &Style,
    inherited: FontSlots,
    inherited_bold: bool,
    inherited_italic: bool,
    fonts: &HashMap<String, FontSlots>,
) -> Result<Font, Message> {
    let mut family = inherited;
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
    let weight = s
        .get(prop!("fontWeight"))
        .map_or(Ok(inherited_bold), bold)?;
    let slanted = s
        .get(prop!("fontStyle"))
        .map_or(Ok(inherited_italic), italic)?;
    if weight && face(&family, true, slanted).0.is_none() {
        let name = declared.map_or_else(|| "inherited".into(), Value::to_string);
        return Err(format!("font family has no bold face: {name}").into());
    }
    Ok(Font {
        family,
        weight,
        italic: slanted,
    })
}
