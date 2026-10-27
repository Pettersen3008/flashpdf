use super::super::{js_trim, words, Value};
use crate::css::Message;

const TYPES: [&str; 12] = [
    "disc",
    "circle",
    "square",
    "decimal",
    "decimal-leading-zero",
    "lower-alpha",
    "lower-latin",
    "upper-alpha",
    "upper-latin",
    "lower-roman",
    "upper-roman",
    "none",
];

/// The type's name.
pub fn list_style_type(value: &Value) -> Result<&'static str, Message> {
    let word = value.text().map(|text| js_trim(text).to_lowercase());
    TYPES
        .into_iter()
        .find(|name| word.as_deref() == Some(name))
        .ok_or_else(|| format!(
            "invalid listStyleType: {value} (use disc, circle, square, decimal, decimal-leading-zero, lower-alpha, lower-latin, upper-alpha, upper-latin, lower-roman, upper-roman, or none)"
        ).into())
}

/// `outside` or `inside`.
pub fn list_style_position(value: &Value) -> Result<&'static str, Message> {
    match value
        .text()
        .map(|text| js_trim(text).to_lowercase())
        .as_deref()
    {
        Some("outside") => Ok("outside"),
        Some("inside") => Ok("inside"),
        _ => Err(format!("invalid listStylePosition: {value} (use outside or inside)").into()),
    }
}

/// The `list-style` shorthand as type and position. A part left out resets to its initial
/// value, `disc` and `outside`.
pub(super) fn parse_list_style(value: &Value) -> Result<(&'static str, &'static str), Message> {
    let Value::Text(text) = value else {
        return Err(format!("invalid listStyle: {value}").into());
    };
    let invalid = || Message::from(format!("invalid listStyle: {text}"));
    let mut kind = None;
    let mut position = None;
    let mut nones = 0;
    for token in words(text) {
        let word = token.to_lowercase();
        if word == "inside" || word == "outside" {
            if position.is_some() {
                return Err(invalid());
            }
            position = Some(if word == "inside" {
                "inside"
            } else {
                "outside"
            });
        } else if word == "none" {
            nones += 1;
        } else if word.contains('(') {
            return Err(
                "list-style-image is not supported; markers are text or drawn shapes".into(),
            );
        } else if kind.is_some() {
            return Err(invalid());
        } else {
            kind = Some(list_style_type(&word.as_str().into())?);
        }
    }
    // A lone `none` is the type, and a second one is the image, which is none here.
    if nones > if kind.is_some() { 1 } else { 2 } {
        return Err(invalid());
    }
    let kind = kind.unwrap_or(if nones > 0 { "none" } else { "disc" });
    Ok((kind, position.unwrap_or("outside")))
}

const ROMAN: [(i64, &str); 13] = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
];

/// A bullet is a shape the engine draws; a counter is text in the item's font.
pub const SHAPES: [(&str, &str); 3] = [("disc", "•"), ("circle", "◦"), ("square", "▪")];

/// The marker text of ordinal `n`. A counter outside the range of its system, such as zero in
/// `lower-alpha`, falls back to decimal.
pub fn counter(kind: &str, n: i64) -> String {
    let mut text = None;
    if kind == "decimal-leading-zero" {
        let sign = if n < 0 { "-" } else { "" };
        text = Some(format!("{sign}{:02}", n.unsigned_abs()));
    } else if kind.ends_with("alpha") || kind.ends_with("latin") {
        if n >= 1 {
            let mut letters = Vec::new();
            let mut rest = n;
            while rest > 0 {
                letters.push(b'a' + ((rest - 1) % 26) as u8);
                rest = (rest - 1) / 26;
            }
            letters.reverse();
            text = String::from_utf8(letters).ok();
        }
    } else if kind.ends_with("roman") && (1..=3999).contains(&n) {
        let mut numerals = String::new();
        let mut rest = n;
        for (value, numeral) in ROMAN {
            while rest >= value {
                numerals.push_str(numeral);
                rest -= value;
            }
        }
        text = Some(numerals);
    }
    let text = text.unwrap_or_else(|| n.to_string());
    if kind.starts_with("upper") {
        text.to_uppercase()
    } else {
        text
    }
}
