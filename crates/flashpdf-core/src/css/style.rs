//! Style normalisation: shorthands expanded, `inherit`, `initial`, and `currentcolor` resolved,
//! and every value the compiler reads eagerly validated. Values stay loosely typed, as the
//! compiler validates the rest where it reads them.

use crate::css::Message;
mod border;
mod boxes;
mod color;
mod decoration;
mod font;
mod length;
mod list;
mod normalize;

pub use boxes::{box_size, box_style, radius_of, Box};
pub use color::{color, Rgb};
pub use decoration::{decoration, decoration_color};
pub use font::{bold, face, font, italic, FontSlots, HELVETICA};
pub use length::{
    corner, dimension, edge_values, flex_basis, line_height, opacity, point, spacing, Dimension,
    LineHeight,
};
pub use list::{counter, list_style_position, list_style_type, SHAPES};

use super::{js_number, js_parse, js_trim, words, Cascaded, Key, Prop, Value};

/// A normalised style: values by property, in the order a JavaScript object keeps its keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style(pub Vec<(Key, Value)>);

impl Style {
    // Out of line: a prop scan inlined at each of the hundreds of call sites cost ~5 KB of wasm.
    #[inline(never)]
    pub fn get(&self, prop: Prop) -> Option<&Value> {
        self.0
            .iter()
            .find(|(own, _)| matches!(own, Key::Prop(own) if *own == prop))
            .map(|(_, value)| value)
    }
    pub fn key(&self, key: &Key) -> Option<&Value> {
        self.0
            .iter()
            .find(|(own, _)| own == key)
            .map(|(_, value)| value)
    }
    #[inline(never)]
    pub fn set(&mut self, prop: Prop, value: Value) {
        self.set_key(Key::Prop(prop), value);
    }
    /// An existing key keeps its place, as in a JavaScript object.
    pub fn set_key(&mut self, key: Key, value: Value) {
        match self.0.iter_mut().find(|(own, _)| *own == key) {
            Some(entry) => entry.1 = value,
            None => self.0.push((key, value)),
        }
    }
    pub fn remove(&mut self, prop: Prop) {
        self.remove_key(&Key::Prop(prop));
    }
    pub fn remove_key(&mut self, key: &Key) {
        self.0.retain(|(own, _)| own != key);
    }
}

pub(crate) const MARGINS: [Prop; 4] = [
    prop!("marginTop"),
    prop!("marginRight"),
    prop!("marginBottom"),
    prop!("marginLeft"),
];
pub(crate) const PADDINGS: [Prop; 4] = [
    prop!("paddingTop"),
    prop!("paddingRight"),
    prop!("paddingBottom"),
    prop!("paddingLeft"),
];
pub(crate) const BORDER_SIDES: [Prop; 4] = [
    prop!("borderTop"),
    prop!("borderRight"),
    prop!("borderBottom"),
    prop!("borderLeft"),
];
pub(crate) const BORDER_WIDTHS: [Prop; 4] = [
    prop!("borderTopWidth"),
    prop!("borderRightWidth"),
    prop!("borderBottomWidth"),
    prop!("borderLeftWidth"),
];
pub(crate) const BORDER_COLORS: [Prop; 4] = [
    prop!("borderTopColor"),
    prop!("borderRightColor"),
    prop!("borderBottomColor"),
    prop!("borderLeftColor"),
];
/// Absent means solid, as for a border written with only a width.
pub(crate) const BORDER_STYLES: [Prop; 4] = [
    prop!("borderTopStyle"),
    prop!("borderRightStyle"),
    prop!("borderBottomStyle"),
    prop!("borderLeftStyle"),
];
/// Clockwise from the top left.
pub(crate) const RADII: [Prop; 4] = [
    prop!("borderTopLeftRadius"),
    prop!("borderTopRightRadius"),
    prop!("borderBottomRightRadius"),
    prop!("borderBottomLeftRadius"),
];

pub fn kerning(value: &Value) -> Result<bool, Message> {
    if !(value.is("auto") || value.is("normal") || value.is("none")) {
        return Err(format!("invalid fontKerning: {value} (use auto, normal, or none)").into());
    }
    Ok(!value.is("none"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhiteSpace {
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
}

pub fn white_space(value: &Value) -> Result<WhiteSpace, Message> {
    Ok(match value.text() {
        Some("normal") => WhiteSpace::Normal,
        Some("nowrap") => WhiteSpace::Nowrap,
        Some("pre") => WhiteSpace::Pre,
        Some("pre-wrap") => WhiteSpace::PreWrap,
        Some("pre-line") => WhiteSpace::PreLine,
        _ => {
            return Err(format!(
                "invalid whiteSpace: {value} (use normal, nowrap, pre, pre-wrap, or pre-line)"
            )
            .into())
        }
    })
}

/// The baseline shift of `sub` and `super` in points upward, as a fraction of the parent's font size.
pub fn vertical_align(value: &Value, parent: f64) -> Result<f64, Message> {
    let shift = match value.text() {
        Some("baseline") => 0.0,
        Some("sub") => -0.2,
        Some("super") => 0.33,
        _ => {
            return Err(
                format!("invalid verticalAlign: {value} (use baseline, sub, or super)").into(),
            )
        }
    };
    Ok(shift * parent)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

pub fn transform(value: &Value) -> Result<Transform, Message> {
    Ok(match value.text() {
        Some("none") => Transform::None,
        Some("uppercase") => Transform::Uppercase,
        Some("lowercase") => Transform::Lowercase,
        Some("capitalize") => Transform::Capitalize,
        _ => {
            return Err(format!(
                "invalid textTransform: {value} (use none, uppercase, lowercase, or capitalize)"
            )
            .into())
        }
    })
}

/// `border-spacing` as one or two lengths; true when either is nonzero.
pub fn border_spacing(value: &Value) -> Result<bool, Message> {
    let lengths = match value {
        Value::Number(_) => vec![value.clone()],
        Value::Text(text) => words(text).into_iter().map(Value::from).collect(),
    };
    if lengths.len() > 2 {
        return Err(format!("invalid borderSpacing: {value}").into());
    }
    let mut spaced = false;
    for length in &lengths {
        spaced |= point(length, 12.0, false)? != 0.0;
    }
    Ok(spaced)
}

const ALIGN: [&str; 7] = [
    "normal",
    "stretch",
    "flex-start",
    "start",
    "flex-end",
    "end",
    "center",
];
const KEYWORDS: [(Prop, &[&str]); 5] = [
    (
        prop!("alignContent"),
        &[
            "normal",
            "stretch",
            "flex-start",
            "start",
            "flex-end",
            "end",
            "center",
            "space-between",
            "space-around",
            "space-evenly",
        ],
    ),
    (
        prop!("justifyContent"),
        &[
            "normal",
            "flex-start",
            "start",
            "flex-end",
            "end",
            "center",
            "space-between",
            "space-around",
            "space-evenly",
        ],
    ),
    (prop!("alignItems"), &ALIGN),
    (
        prop!("alignSelf"),
        &[
            "auto",
            "normal",
            "stretch",
            "flex-start",
            "start",
            "flex-end",
            "end",
            "center",
        ],
    ),
    (prop!("boxSizing"), &["content-box", "border-box"]),
];
/// Each size's initial value, which reads as absent.
const INITIAL_SIZE: [(Prop, &str); 6] = [
    (prop!("width"), "auto"),
    (prop!("height"), "auto"),
    (prop!("minWidth"), "auto"),
    (prop!("minHeight"), "auto"),
    (prop!("maxWidth"), "none"),
    (prop!("maxHeight"), "none"),
];

/// Whether `value` is unset or one of the strings in `allowed`.
fn absent_or(value: Option<&Value>, allowed: &[&str]) -> bool {
    value.is_none_or(|value| allowed.iter().any(|word| value.is(word)))
}

/// `value` is an element's cascaded declarations and `parent` the parent's normalised style,
/// which `inherit` reads.
pub fn style(value: &Cascaded, parent: &Style) -> Result<Style, Message> {
    for (key, _) in &value.0 {
        match key {
            Key::Custom(_) => {}
            Key::Other(name) => return Err(format!("unsupported style property: {name}").into()),
            Key::Prop(prop) => {
                if let Some(reason) = prop.unsupported() {
                    return Err(
                        format!("unsupported style property: {} ({reason})", prop.name()).into(),
                    );
                }
            }
        }
    }
    // Validated after normalize, so `inherit` and `initial` have become values.
    let mut result = normalize::normalize(value, parent)?;
    if !absent_or(result.get(prop!("display")), &["block", "flex"]) {
        return Err("invalid display".into());
    }
    if !absent_or(result.get(prop!("flexDirection")), &["row", "column"]) {
        return Err("invalid flex direction".into());
    }
    if !absent_or(
        result.get(prop!("textAlign")),
        &["left", "center", "right", "justify", "start", "end"],
    ) {
        return Err("invalid text alignment".into());
    }
    if !absent_or(result.get(prop!("direction")), &["ltr", "rtl"]) {
        return Err("invalid direction (use ltr or rtl)".into());
    }
    for prop in [
        prop!("breakBefore"),
        prop!("breakAfter"),
        prop!("breakInside"),
    ] {
        let inside = prop == prop!("breakInside");
        let allowed: &[&str] = if inside {
            &["auto", "avoid"]
        } else {
            &["auto", "avoid", "page"]
        };
        let Some(value) = result.get(prop) else {
            continue;
        };
        // Without columns or regions, avoiding a page break is avoiding any break.
        if value.is("avoid-page") {
            result.set(prop, "avoid".into());
        } else if !inside
            && ["left", "right", "recto", "verso"]
                .iter()
                .any(|side| value.is(side))
        {
            return Err(format!(
                "{}: {value} is not supported (pages have no sides; use page)",
                prop.name()
            )
            .into());
        } else if !absent_or(Some(value), allowed) {
            return Err(format!("invalid {}", prop.name()).into());
        }
    }
    for prop in [prop!("orphans"), prop!("widows")] {
        let Some(value) = result.get(prop) else {
            continue;
        };
        let lines = match value {
            Value::Number(lines) => Some(*lines),
            Value::Text(text) => {
                let digits = js_trim(text);
                (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                    .then(|| js_parse(digits))
            }
        };
        let whole =
            lines.filter(|lines| lines.is_finite() && lines.trunc() == *lines && *lines >= 1.0);
        let Some(whole) = whole else {
            let shown = lines.map_or_else(|| value.to_string(), js_number);
            return Err(format!(
                "invalid {}: {shown} (use a whole number of lines from 1)",
                prop.name()
            )
            .into());
        };
        // No paragraph record holds 65,535 lines, so the cap changes nothing.
        result.set(prop, whole.min(65_535.0).into());
    }
    if !absent_or(
        result.get(prop!("borderCollapse")),
        &["collapse", "separate"],
    ) {
        return Err("invalid borderCollapse".into());
    }
    if let Some(value) = result.get(prop!("borderSpacing")) {
        border_spacing(value)?;
    }
    match result.get(prop!("flexWrap")) {
        Some(value) if value.is("wrap-reverse") => return Err(
            "flex-wrap: wrap-reverse is not supported; put the last row's items first and use wrap"
                .into(),
        ),
        // `nowrap` is the initial value, so it reads as absent.
        Some(value) if value.is("nowrap") => result.remove(prop!("flexWrap")),
        Some(value) if !value.is("wrap") => return Err(format!("invalid flexWrap: {value}").into()),
        _ => {}
    }
    for (prop, allowed) in KEYWORDS {
        if let Some(value) = result
            .get(prop)
            .filter(|value| !absent_or(Some(value), allowed))
        {
            let value = value.to_string();
            return Err(if value.contains("baseline") {
                format!(
                    "{}: {value} is not supported; align text with center or flex-start",
                    prop.name()
                )
            } else {
                format!("invalid {}: {value}", prop.name())
            }
            .into());
        }
    }
    for prop in [prop!("flexGrow"), prop!("flexShrink")] {
        let Some(value) = result.get(prop) else {
            continue;
        };
        let factor = match value {
            Value::Number(factor) => *factor,
            Value::Text(text) => js_parse(text),
        };
        let single = factor as f32;
        if factor.is_nan() || factor < 0.0 || !single.is_finite() {
            return Err(format!("invalid {}: {value}", prop.name()).into());
        }
        result.set(prop, f64::from(single).into());
    }
    if let Some(value) = result.get(prop!("flexBasis")) {
        flex_basis(value)?;
    }
    for (prop, initial) in INITIAL_SIZE {
        match result.get(prop) {
            Some(value) if value.is(initial) => result.remove(prop),
            Some(value) => {
                dimension(value, prop.name(), true)?;
            }
            None => {}
        }
    }
    let get = |name: Prop| result.get(name);
    for prop in [prop!("rowGap"), prop!("columnGap")] {
        if let Some(value) = get(prop) {
            point(value, 12.0, false)?;
        }
    }
    if let Some(value) = get(prop!("fontSize")) {
        point(value, 12.0, true)?;
    }
    if let Some(value) = get(prop!("fontWeight")) {
        bold(value)?;
    }
    if let Some(value) = get(prop!("fontStyle")) {
        italic(value)?;
    }
    if let Some(value) = get(prop!("fontKerning")) {
        kerning(value)?;
    }
    if let Some(value) = get(prop!("letterSpacing")) {
        spacing(value, 12.0, "letterSpacing")?;
    }
    if let Some(value) = get(prop!("wordSpacing")) {
        spacing(value, 12.0, "wordSpacing")?;
    }
    if let Some(value) = get(prop!("textTransform")) {
        transform(value)?;
    }
    if let Some(value) = get(prop!("textDecorationColor")) {
        decoration_color(value)?;
    }
    if let Some(value) = get(prop!("whiteSpace")) {
        white_space(value)?;
    }
    if let Some(value) = get(prop!("verticalAlign")) {
        if !["top", "middle", "bottom"]
            .iter()
            .any(|word| value.is(word))
        {
            vertical_align(value, 12.0)?;
        }
    }
    if let Some(value) = get(prop!("listStyleType")) {
        list_style_type(value)?;
    }
    if let Some(value) = get(prop!("listStylePosition")) {
        list_style_position(value)?;
    }
    if let Some(value) = get(prop!("lineHeight")) {
        line_height(value, 12.0)?;
    }
    if let Some(value) = get(prop!("fontFamily")) {
        if value.text().is_none_or(|family| js_trim(family).is_empty()) {
            return Err("fontFamily must name at least one family".into());
        }
    }
    if let Some(value) = get(prop!("textDecoration")) {
        decoration(value)?;
    }
    if let Some(value) = get(prop!("opacity")) {
        opacity(value)?;
    }
    for prop in BORDER_STYLES {
        if let Some(value) = result.get(prop) {
            let line = border::line(value)?;
            result.set(prop, line.into());
        }
    }
    for prop in RADII {
        if let Some(value) = result.get(prop) {
            corner(value, 12.0)?;
        }
    }
    if let Some(background) = result.get(prop!("backgroundColor")).and_then(Value::text) {
        let lower = background.to_ascii_lowercase();
        if lower.contains("gradient(") || lower.contains("url(") {
            return Err(format!(
                "unsupported background: {background} (gradients and background images are not drawn; use a solid colour)"
            ).into());
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
