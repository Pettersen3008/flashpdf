use super::super::{words, Cascaded, Key, Prop, Value};
use super::border::parse_border;
use super::decoration::decoration;
use super::length::{edge_values, flex_shorthand};
use super::list::parse_list_style;
use super::{
    Style, BORDER_COLORS, BORDER_SIDES, BORDER_STYLES, BORDER_WIDTHS, MARGINS, PADDINGS, RADII,
};
use crate::css::Message;

const COLOR: Prop = prop!("color");
const MARGIN: Prop = prop!("margin");
const PADDING: Prop = prop!("padding");
const BORDER: Prop = prop!("border");
const BORDER_RADIUS: Prop = prop!("borderRadius");
const FLEX: Prop = prop!("flex");
const BACKGROUND: Prop = prop!("background");
const TEXT_DECORATION: Prop = prop!("textDecoration");
const LIST_STYLE: Prop = prop!("listStyle");
/// Each logical shorthand with its start and end sides, for horizontal left-to-right text:
/// inline runs from left to right, block from top to bottom.
const LOGICAL: [(Prop, [Prop; 2]); 4] = [
    (prop!("marginInline"), [MARGINS[3], MARGINS[1]]),
    (prop!("marginBlock"), [MARGINS[0], MARGINS[2]]),
    (prop!("paddingInline"), [PADDINGS[3], PADDINGS[1]]),
    (prop!("paddingBlock"), [PADDINGS[0], PADDINGS[2]]),
];

/// `initial` values; a property missing here is simply absent by default.
fn initial(key: &Key) -> Option<Value> {
    let Key::Prop(prop) = *key else {
        return None;
    };
    let name = prop.name();
    let value: Value = match name {
        "color" => "black".into(),
        "fontSize" => 12.0.into(),
        "fontWeight" => "normal".into(),
        "fontFamily" => "Helvetica".into(),
        "textAlign" => "start".into(),
        "direction" => "ltr".into(),
        "textDecoration" => "none".into(),
        "lineHeight" => "normal".into(),
        "orphans" | "widows" => 2.0.into(),
        "backgroundColor" => "transparent".into(),
        "opacity" | "flexShrink" => 1.0.into(),
        "flexGrow" => 0.0.into(),
        "flexBasis" => "auto".into(),
        _ if MARGINS.contains(&prop)
            || PADDINGS.contains(&prop)
            || BORDER_WIDTHS.contains(&prop)
            || RADII.contains(&prop) =>
        {
            0.0.into()
        }
        _ if BORDER_COLORS.contains(&prop) => "currentcolor".into(),
        // A border without a style is solid here, where CSS draws none.
        _ if BORDER_STYLES.contains(&prop) => "solid".into(),
        _ => return None,
    };
    Some(value)
}

/// Absent, these follow the parent through the compiler's text state, so `inherit` leaves them absent.
fn from_parent(key: &Key) -> bool {
    matches!(key, Key::Prop(prop) if prop.inherited() || *prop == prop!("lineHeight"))
}

fn border_side(prop: Prop) -> Option<usize> {
    BORDER_SIDES.iter().position(|&side| side == prop)
}

fn longhands(key: &Key) -> Vec<Key> {
    let props: Vec<Prop> = match *key {
        Key::Prop(MARGIN) => MARGINS.into(),
        Key::Prop(PADDING) => PADDINGS.into(),
        Key::Prop(BORDER) => (0..4)
            .flat_map(|side| {
                [
                    BORDER_WIDTHS[side],
                    BORDER_COLORS[side],
                    BORDER_STYLES[side],
                ]
            })
            .collect(),
        Key::Prop(prop) if border_side(prop).is_some() => {
            let side = border_side(prop).unwrap_or(0);
            vec![
                BORDER_WIDTHS[side],
                BORDER_COLORS[side],
                BORDER_STYLES[side],
            ]
        }
        Key::Prop(prop) if prop == prop!("borderWidth") => BORDER_WIDTHS.into(),
        Key::Prop(prop) if prop == prop!("borderColor") => BORDER_COLORS.into(),
        Key::Prop(prop) if prop == prop!("borderStyle") => BORDER_STYLES.into(),
        Key::Prop(BORDER_RADIUS) => RADII.into(),
        Key::Prop(FLEX) => vec![prop!("flexGrow"), prop!("flexShrink"), prop!("flexBasis")],
        Key::Prop(BACKGROUND) => vec![prop!("backgroundColor")],
        Key::Prop(prop) => match LOGICAL.iter().find(|(own, _)| *own == prop) {
            Some((_, sides)) => sides.to_vec(),
            None => vec![prop],
        },
        _ => return vec![key.clone()],
    };
    props.into_iter().map(Key::Prop).collect()
}

fn keyword(item: &Value) -> Option<String> {
    item.text()
        .map(|text| super::super::js_trim(text).to_lowercase())
}

/// `text.replace(/\bcurrentcolor\b/gi, current)`.
fn replace_current(text: &str, current: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let bytes = text.as_bytes();
    let word = |at: usize| {
        bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    };
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for (at, found) in lower.match_indices("currentcolor") {
        let end = at + found.len();
        if (at == 0 || !word(at - 1)) && !word(end) {
            result.push_str(&text[last..at]);
            result.push_str(current);
            last = end;
        }
    }
    result.push_str(&text[last..]);
    result
}

/// Expands every shorthand into its longhands in cascade order, so the later declaration wins.
/// `inherit` and `initial` resolve against `parent`, the parent's normalized style, and
/// `currentcolor` becomes the element's own colour.
pub(super) fn normalize(value: &Cascaded, parent: &Style) -> Result<Style, Message> {
    let declared = value.get(&Key::Prop(COLOR));
    let own = declared.and_then(keyword);
    let current = match (own.as_deref(), declared) {
        (None | Some("initial"), _) | (_, None) => "black".into(),
        (Some("inherit" | "currentcolor"), _) => parent
            .get(COLOR)
            .map_or_else(|| "black".into(), Value::to_string),
        (_, Some(declared)) => declared.to_string(),
    };
    let paint = |item: &Value| match item {
        Value::Text(text) if text.to_ascii_lowercase().contains("currentcolor") => {
            Value::from(replace_current(text, &current).as_str())
        }
        _ => item.clone(),
    };
    let mut result = Style::default();
    for (key, raw) in &value.0 {
        let word = if *key == Key::Prop(COLOR) && own.as_deref() == Some("currentcolor") {
            Some("inherit".into())
        } else {
            keyword(raw)
        };
        let item = paint(raw);
        if let Some(word @ ("inherit" | "initial")) = word.as_deref() {
            for longhand in longhands(key) {
                let next = if word == "initial" {
                    initial(&longhand)
                } else {
                    parent.key(&longhand).cloned().or_else(|| {
                        if from_parent(&longhand) {
                            None
                        } else {
                            initial(&longhand)
                        }
                    })
                };
                match next {
                    Some(next) => result.set_key(longhand, paint(&next)),
                    None => result.remove_key(&longhand),
                }
            }
            continue;
        }
        let Key::Prop(prop) = *key else {
            result.set_key(key.clone(), item);
            continue;
        };
        if prop == MARGIN || prop == PADDING {
            let values = edge_values(&item, prop.name())?;
            let sides = if prop == MARGIN { MARGINS } else { PADDINGS };
            for (side, value) in sides.into_iter().zip(values) {
                result.set(side, value);
            }
        } else if let Some((_, [start, end])) = LOGICAL.iter().find(|(own, _)| *own == prop) {
            let values = match &item {
                Value::Text(text) => words(text).into_iter().map(Value::from).collect(),
                Value::Number(_) => vec![item.clone()],
            };
            if values.len() > 2 {
                return Err(format!("invalid {}: {item}", prop.name()).into());
            }
            result.set(*start, values[0].clone());
            result.set(
                *end,
                values.last().cloned().unwrap_or_else(|| values[0].clone()),
            );
        } else if prop == BORDER || border_side(prop).is_some() {
            let (width, paint, line) = parse_border(&item)?;
            let sides = match border_side(prop) {
                Some(side) => side..side + 1,
                None => 0..4,
            };
            for side in sides {
                result.set(BORDER_WIDTHS[side], width.clone());
                result.set(BORDER_COLORS[side], paint.clone());
                result.set(BORDER_STYLES[side], line.into());
            }
        } else if prop == prop!("borderWidth") || prop == prop!("borderColor") {
            let sides = if prop == prop!("borderWidth") {
                BORDER_WIDTHS
            } else {
                BORDER_COLORS
            };
            for side in sides {
                result.set(side, item.clone());
            }
        } else if prop == prop!("borderStyle") {
            let values = edge_values(&item, "border-style")?;
            for (side, value) in BORDER_STYLES.into_iter().zip(values) {
                result.set(side, value);
            }
        } else if prop == BORDER_RADIUS {
            // Horizontal radii, then after a slash the vertical ones, each as margin gives its four sides.
            let parts: Vec<Value> = match &item {
                Value::Text(text) => text.split('/').map(Value::from).collect(),
                Value::Number(_) => vec![item.clone()],
            };
            if parts.len() > 2 {
                return Err(format!("invalid border-radius: {item}").into());
            }
            let across = edge_values(&parts[0], "border-radius")?;
            let down = parts
                .get(1)
                .map(|part| edge_values(part, "border-radius"))
                .transpose()?;
            for (index, corner) in RADII.into_iter().enumerate() {
                let radius = match &down {
                    Some(down) => {
                        Value::from(format!("{} {}", across[index], down[index]).as_str())
                    }
                    None => across[index].clone(),
                };
                result.set(corner, radius);
            }
        } else if prop == TEXT_DECORATION {
            // The shorthand resets the colour, so a later `text-decoration: underline` drops an earlier one.
            let color = decoration(&item)?.color;
            result.set(TEXT_DECORATION, item);
            result.set(
                prop!("textDecorationColor"),
                color.as_deref().unwrap_or("currentcolor").into(),
            );
        } else if prop == LIST_STYLE {
            // The shorthand resets both longhands, so a later `list-style: inside` brings back `disc`.
            let (kind, position) = parse_list_style(&item)?;
            result.set(prop!("listStyleType"), kind.into());
            result.set(prop!("listStylePosition"), position.into());
        } else if prop == FLEX {
            let (grow, shrink, basis) = flex_shorthand(&item)?;
            result.set(prop!("flexGrow"), grow.into());
            result.set(prop!("flexShrink"), shrink.into());
            result.set(prop!("flexBasis"), basis);
        } else if prop == BACKGROUND {
            result.set(prop!("backgroundColor"), item);
        } else {
            result.set(prop, item);
        }
    }
    Ok(result)
}
