//! The CSS subset in CSS.md: stylesheet parsing, selectors, the cascade, var() and calc().
//! Messages match the TypeScript layer this replaces, so a test can assert them unchanged.

mod cascade;
mod math;
mod parser;
mod properties;
mod selector;

pub use math::evaluate_math;
pub use parser::{parse, Stylesheet};
pub use properties::{property, Prop};
pub use selector::Tree;

use std::rc::Rc;

/// A property a declaration sets: a known one, a custom `--name`, or an unknown style-object key
/// that normalisation rejects.
#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Prop(Prop),
    Custom(Rc<str>),
    Other(Rc<str>),
}

impl Key {
    pub fn name(&self) -> &str {
        match self {
            Self::Prop(prop) => prop.name(),
            Self::Custom(name) | Self::Other(name) => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Text(Rc<str>),
    Number(f64),
}

impl Value {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::Number(_) => None,
        }
    }
}

/// Declarations in cascade order. Setting a key moves it last, because normalisation applies
/// shorthands and longhands in that order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cascaded(pub Vec<(Key, Value)>);

impl Cascaded {
    pub fn get(&self, key: &Key) -> Option<&Value> {
        self.0
            .iter()
            .find(|(own, _)| own == key)
            .map(|(_, value)| value)
    }
    pub fn set(&mut self, key: Key, value: Value) {
        self.remove(&key);
        self.0.push((key, value));
    }
    pub fn remove(&mut self, key: &Key) {
        self.0.retain(|(own, _)| own != key);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Declarations {
    pub normal: Cascaded,
    pub important: Cascaded,
}

pub(crate) fn whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')
}

/// `line:column` of a byte offset, the column in UTF-16 units as JavaScript counts it.
pub(crate) fn position(source: &str, index: usize) -> String {
    let before = &source[..index];
    let line = before.bytes().filter(|&byte| byte == b'\n').count() + 1;
    let last = before.rfind('\n').map_or(0, |at| at + 1);
    format!("{line}:{}", before[last..].encode_utf16().count() + 1)
}

/// `JSON.stringify` of a string.
pub(crate) fn json_string(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + 2);
    result.push('"');
    for character in text.chars() {
        match character {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\x08' => result.push_str("\\b"),
            '\x0c' => result.push_str("\\f"),
            '\0'..='\x1f' => result.push_str(&format!("\\u{:04x}", character as u32)),
            _ => result.push(character),
        }
    }
    result.push('"');
    result
}

/// `String(number)` in JavaScript.
pub(crate) fn js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if value == 0.0 {
        return "0".into();
    }
    let magnitude = value.abs();
    if (1e-6..1e21).contains(&magnitude) {
        return format!("{value}");
    }
    let text = format!("{value:e}");
    match text.split_once('e') {
        Some((digits, exponent)) if !exponent.starts_with('-') => format!("{digits}e+{exponent}"),
        _ => text,
    }
}

/// A finite f32 value as the engine stores it; `positive` also rejects zero.
pub(crate) fn number(value: f64, positive: bool) -> Result<f64, String> {
    let single = value as f32;
    if !single.is_finite() || (if positive { single <= 0.0 } else { value < 0.0 }) {
        return Err("invalid number".into());
    }
    Ok(f64::from(single))
}

/// Points per unit. CSS fixes 1in = 2.54cm = 25.4mm = 101.6Q = 96px = 72pt = 6pc; rem is the 12pt root size.
fn unit_points(unit: &str) -> Option<f64> {
    Some(match unit {
        "pt" => 1.0,
        "px" => 0.75,
        "pc" => 12.0,
        "in" => 72.0,
        "cm" => 72.0 / 2.54,
        "mm" => 72.0 / 25.4,
        "Q" => 72.0 / 101.6,
        "rem" => 12.0,
        _ => return None,
    })
}

/// Length of the CSS number `\d+(\.\d*)?|\.\d+` with an optional exponent at the start of `text`.
pub(crate) fn number_end(text: &[u8], exponent: bool) -> usize {
    let digits = |from: usize| {
        from + text[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let mut end = digits(0);
    if end > 0 {
        if text.get(end) == Some(&b'.') {
            end = digits(end + 1);
        }
    } else if text.first() == Some(&b'.') && text.get(1).is_some_and(u8::is_ascii_digit) {
        end = digits(1);
    } else {
        return 0;
    }
    if exponent && matches!(text.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(text.get(end + 1), Some(b'+' | b'-')));
        let after = digits(end + 1 + sign);
        if after > end + 1 + sign {
            end = after;
        }
    }
    end
}

/// `Number(text)` for text `number_end` accepted.
pub(crate) fn parse_number(text: &str) -> f64 {
    text.parse().unwrap_or(f64::NAN)
}

/// A length in points: `em` scales `parent`, every other unit is absolute.
pub fn point(value: &str, parent: f64, positive: bool) -> Result<f64, String> {
    if value == "0" {
        return Ok(0.0);
    }
    let trimmed = value.trim();
    let end = number_end(trimmed.as_bytes(), true);
    let unit = &trimmed[end..];
    let scale = if unit == "em" {
        Some(parent)
    } else {
        unit_points(unit)
    };
    match scale {
        Some(scale) if end > 0 => number(parse_number(&trimmed[..end]) * scale, positive),
        _ => Err(format!("invalid length: {value}")),
    }
}

#[cfg(test)]
mod tests;
