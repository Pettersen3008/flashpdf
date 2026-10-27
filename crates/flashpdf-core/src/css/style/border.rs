use super::super::{js_space, js_trim, Value};
use super::length::point;
use crate::css::Message;

const BORDER_STYLES: [&str; 10] = [
    "none", "hidden", "dotted", "dashed", "solid", "double", "groove", "ridge", "inset", "outset",
];

/// `value.match(/[^\s()]+(?:\([^()]*\))?/g)`: whitespace splits outside parentheses, so
/// `rgb(0, 0, 0)` stays one token.
pub(super) fn tokens(value: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find(|c: char| !js_space(c) && c != '(' && c != ')') {
        rest = &rest[start..];
        let mut end = rest
            .find(|c: char| js_space(c) || c == '(' || c == ')')
            .unwrap_or(rest.len());
        if rest[end..].starts_with('(') {
            if let Some(close) = rest[end + 1..].find(['(', ')']) {
                if rest.as_bytes()[end + 1 + close] == b')' {
                    end += close + 2;
                }
            }
        }
        result.push(&rest[..end]);
        rest = &rest[end..];
    }
    result
}

/// A border line as the box reads it: `none`, `solid`, `dashed`, or `dotted`. `hidden` draws
/// like `none`. A border with no style at all is solid, unlike CSS, where it is `none`.
pub(super) fn line(value: &Value) -> Result<&'static str, Message> {
    let word = value.text().map(|text| js_trim(text).to_lowercase());
    match word.as_deref() {
        Some("none" | "hidden") => Ok("none"),
        Some("solid") => Ok("solid"),
        Some("dashed") => Ok("dashed"),
        Some("dotted") => Ok("dotted"),
        Some(word) if BORDER_STYLES.contains(&word) => Err(format!(
            "unsupported border style: {word} (only none, solid, dashed, and dotted are drawn)"
        )
        .into()),
        _ => Err(format!("invalid border style: {value}").into()),
    }
}

/// The width, colour, and line of a `border` shorthand. Only the width is matched by shape;
/// whatever is left over is the colour, so an unpaintable colour reports itself later instead
/// of being ignored.
pub(super) fn parse_border(value: &Value) -> Result<(Value, Value, &'static str), Message> {
    let text = match value {
        Value::Number(_) => return Ok((value.clone(), "black".into(), "solid")),
        Value::Text(text) => text,
    };
    let mut width = None;
    let mut paint = None;
    let mut style = "solid";
    for token in tokens(text) {
        if token == "none" || token == "hidden" {
            return Ok((Value::Number(0.0), "black".into(), style));
        }
        if BORDER_STYLES.contains(&token) {
            style = line(&token.into())?;
        } else if width.is_none() && token.starts_with(|c: char| c == '.' || c.is_ascii_digit()) {
            width = Some(token);
        } else if paint.is_none() {
            paint = Some(token);
        } else {
            return Err(format!("invalid border: {text}").into());
        }
    }
    let Some(width) = width else {
        return Err(format!("invalid border: {text}").into());
    };
    let width = Value::from(width);
    point(&width, 12.0, false)?;
    Ok((width, paint.unwrap_or("black").into(), style))
}
