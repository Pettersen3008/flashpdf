use super::super::{js_trim, Value};
use super::border::tokens;
use super::color::{color, Rgb};
use crate::css::Message;

/// The `text-decoration` shorthand.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decoration {
    pub underline: bool,
    pub line_through: bool,
    pub color: Option<String>,
}

/// Lines, the `solid` style, and at most one colour.
pub fn decoration(value: &Value) -> Result<Decoration, Message> {
    let Value::Text(text) = value else {
        return Err(format!("invalid textDecoration: {value}").into());
    };
    let invalid = || {
        Message::from(format!(
            "invalid textDecoration: {text} (use none, underline, or line-through)"
        ))
    };
    let mut result = Decoration::default();
    let mut none = false;
    for token in tokens(text) {
        match token {
            "underline" => result.underline = true,
            "line-through" => result.line_through = true,
            "none" => none = true,
            "overline" => return Err("text-decoration: overline is not supported".into()),
            "dotted" | "dashed" | "double" | "wavy" => {
                return Err(
                    format!("text-decoration: {token} is not supported; lines are solid").into(),
                )
            }
            "solid" => {}
            _ => {
                if result.color.is_some() {
                    return Err(invalid());
                }
                decoration_color(&token.into())?;
                result.color = Some(token.into());
            }
        }
    }
    if none == (result.underline || result.line_through) {
        return Err(invalid());
    }
    Ok(result)
}

/// None for `currentcolor`, the text colour the engine uses when no colour is sent.
pub fn decoration_color(value: &Value) -> Result<Option<Rgb>, Message> {
    if value
        .text()
        .is_some_and(|text| js_trim(text).to_lowercase() == "currentcolor")
    {
        return Ok(None);
    }
    color(value).map(Some)
}
