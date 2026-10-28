use super::super::{Prop, Value};
use super::color::{color, Rgb};
use super::length::{corner, dimension, inset, point, Dimension};
use super::{
    z_index, Style, BORDER_COLORS, BORDER_STYLES, BORDER_WIDTHS, INSETS, MARGINS, PADDINGS, RADII,
};
use crate::css::Message;

/// `margin` or `padding` in points, top first. An `auto` margin is zero here; `box_size` flags it.
pub fn edges(s: &Style, name: &str, parent: f64) -> Result<[f64; 4], Message> {
    let margin = name == "margin";
    let sides = if margin { MARGINS } else { PADDINGS };
    let mut result = [0.0; 4];
    for (side, prop) in result.iter_mut().zip(sides) {
        let value = s.get(prop).cloned().unwrap_or(Value::Number(0.0));
        *side = if margin && value.is("auto") {
            0.0
        } else {
            point(&value, parent, false)?
        };
    }
    Ok(result)
}

/// Width-like sizes are points (kind 1) or percent of the container's width (kind 2). Heights
/// are points, percents already resolved against the parent's fixed height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub border_box: bool,
    /// One bit per `auto` margin: top 1, right 2, bottom 4, left 8.
    pub auto_margin: u8,
    pub width: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub max_width: Option<Dimension>,
    pub height: Option<f64>,
    pub min_height: Option<f64>,
    pub max_height: Option<f64>,
    pub position: Option<Position>,
}

/// CSS `position` other than `static`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    /// 1 relative, 2 absolute, 3 fixed.
    pub kind: u8,
    /// Top, right, bottom, left; None is `auto`.
    pub inset: [Option<Dimension>; 4],
    /// None is `auto`.
    pub z: Option<i32>,
}

/// The element's `position`, insets, and `z-index`, or None when it is static, which ignores them.
pub fn position(s: &Style, size: f64) -> Result<Option<Position>, Message> {
    let kind = match s.get(prop!("position")) {
        Some(value) if value.is("relative") => 1,
        Some(value) if value.is("absolute") => 2,
        Some(value) if value.is("fixed") => 3,
        _ => return Ok(None),
    };
    let mut sides = [None; 4];
    for (side, prop) in sides.iter_mut().zip(INSETS) {
        if let Some(value) = s.get(prop) {
            *side = inset(value, size, prop.name())?;
        }
    }
    let z = s.get(prop!("zIndex")).map(z_index).transpose()?;
    Ok(Some(Position {
        kind,
        inset: sides,
        z,
    }))
}

/// The box's sizes and auto margins, or None when it has none. `height` is the parent's
/// definite content height, which percent heights need; `width` is false where a table or
/// column-model flex row owns the width.
pub fn box_size(s: &Style, height: Option<f64>, width: bool) -> Result<Option<Size>, Message> {
    let auto_margin = MARGINS
        .iter()
        .enumerate()
        .filter(|(_, prop)| s.get(**prop).is_some_and(|value| value.is("auto")))
        .fold(0, |bits, (index, _)| bits | 1 << index);
    let horizontal = |prop: Prop| {
        s.get(prop)
            .map(|value| dimension(value, prop.name(), true))
            .transpose()
    };
    let vertical = |prop: Prop| -> Result<Option<f64>, Message> {
        let Some(value) = s.get(prop) else {
            return Ok(None);
        };
        let Dimension { kind, value } = dimension(value, prop.name(), true)?;
        if kind == 1 {
            return Ok(Some(value));
        }
        let Some(height) = height else {
            return Err(format!(
                "a percent {} needs a parent with a fixed height; use pt or px",
                prop.name()
            )
            .into());
        };
        Ok(Some(f64::from((height * value / 100.0) as f32)))
    };
    let size = Size {
        border_box: s
            .get(prop!("boxSizing"))
            .is_some_and(|value| value.is("border-box")),
        auto_margin,
        width: if width {
            horizontal(prop!("width"))?
        } else {
            None
        },
        min_width: horizontal(prop!("minWidth"))?,
        max_width: horizontal(prop!("maxWidth"))?,
        height: vertical(prop!("height"))?,
        min_height: vertical(prop!("minHeight"))?,
        max_height: vertical(prop!("maxHeight"))?,
        position: None,
    };
    let sized = [size.width, size.min_width, size.max_width]
        .iter()
        .any(Option::is_some)
        || [size.height, size.min_height, size.max_height]
            .iter()
            .any(Option::is_some);
    Ok((auto_margin != 0 || sized).then_some(size))
}

/// Eight values, clockwise from the top left corner, horizontal then vertical, in points; bit `i`
/// of `percent` makes value `i` a percent of the box's width (even `i`) or height (odd `i`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radius {
    pub values: [f64; 8],
    pub percent: u8,
}

/// The corner radii, or None when every corner is square.
pub fn radius_of(s: &Style, parent: f64) -> Result<Option<Radius>, Message> {
    let mut radius = Radius {
        values: [0.0; 8],
        percent: 0,
    };
    for (index, prop) in RADII.into_iter().enumerate() {
        let value = s.get(prop).cloned().unwrap_or(Value::Number(0.0));
        for (half, (amount, percent)) in corner(&value, parent)?.into_iter().enumerate() {
            let at = index * 2 + half;
            radius.values[at] = amount;
            radius.percent |= u8::from(percent) << at;
        }
    }
    Ok(radius
        .values
        .iter()
        .any(|&value| value != 0.0)
        .then_some(radius))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Box {
    pub margin: [f64; 4],
    pub padding: [f64; 4],
    pub border: [f64; 4],
    pub background: Option<Rgb>,
    pub paint_effects: Option<std::rc::Rc<crate::Effects>>,
    pub border_color: [Rgb; 4],
    /// Per side, top first: 0 solid, 1 dashed, 2 dotted.
    pub border_style: [u8; 4],
    pub radius: Option<Radius>,
    /// 0 to 1, with every ancestor's opacity multiplied in.
    pub opacity: f64,
    pub size: Option<Size>,
    pub effects: Option<std::rc::Rc<EffectInput>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectInput {
    pub transform: String,
    pub origin: String,
    pub clip: String,
    pub overflow: bool,
    pub font_size: f32,
    pub(crate) image: bool,
}

pub fn effects(s: &Style, size: f64) -> Result<Option<std::rc::Rc<EffectInput>>, Message> {
    if !s.0.iter().any(|(key, _)| matches!(key, crate::css::Key::Prop(prop)
        if [prop!("transform"), prop!("transformOrigin"), prop!("clipPath"), prop!("overflow")].contains(prop))) {
        return Ok(None);
    }
    let text = |prop, initial: &str| {
        s.get(prop)
            .map_or_else(|| initial.to_owned(), ToString::to_string)
    };
    let input = EffectInput {
        transform: text(prop!("transform"), "none"),
        origin: text(prop!("transformOrigin"), "50% 50%"),
        clip: text(prop!("clipPath"), "none"),
        overflow: s
            .get(prop!("overflow"))
            .is_some_and(|value| value.is("hidden")),
        font_size: size as f32,
        image: false,
    };
    crate::BoxEffects::parse_inner(
        &input.transform,
        &input.origin,
        &input.clip,
        input.overflow,
        input.font_size,
        false,
    )?;
    let active = input.transform != "none" || input.clip != "none" || input.overflow;
    Ok(active.then(|| std::rc::Rc::new(input)))
}

/// The box's edges, paint, and radii, or None when the style sets none of them.
// ponytail: no memo per style and font size as the TypeScript has; add one if profiles show it.
pub fn box_style(s: &Style, parent: f64, opacity: f64) -> Result<Option<Box>, Message> {
    let boxed = [MARGINS, PADDINGS, BORDER_WIDTHS, BORDER_COLORS]
        .iter()
        .flatten()
        .chain([
            &prop!("backgroundColor"),
            &prop!("backgroundImage"),
            &prop!("boxShadow"),
        ])
        .chain(&RADII)
        .any(|prop| s.get(*prop).is_some());
    if !boxed {
        return Ok(None);
    }
    let lines = BORDER_STYLES.map(|prop| s.get(prop).cloned().unwrap_or_else(|| "solid".into()));
    let mut border = [0.0; 4];
    for (index, width) in border.iter_mut().enumerate() {
        let paint = s.get(BORDER_COLORS[index]);
        if let Some(value) = s.get(BORDER_WIDTHS[index]) {
            if !paint.is_some_and(|paint| paint.is("transparent")) && !lines[index].is("none") {
                *width = point(value, parent, false)?;
            }
        }
    }
    let mut border_color = [[0; 3]; 4];
    for (rgb, prop) in border_color.iter_mut().zip(BORDER_COLORS) {
        if let Some(paint) = s.get(prop).filter(|paint| !paint.is("transparent")) {
            *rgb = color(paint)?;
        }
    }
    let border_style = lines.map(|line| match line.text() {
        Some("dashed") => 1,
        Some("dotted") => 2,
        _ => 0,
    });
    let radius = radius_of(s, parent)?;
    // The engine's `uniform_border`: one stroke around the box draws all four borders.
    let uniform = border.iter().all(|&width| width == border[0])
        && (border[0] == 0.0
            || (border_color.iter().all(|rgb| *rgb == border_color[0])
                && border_style.iter().all(|&line| line == border_style[0])));
    if radius.is_some() && !uniform {
        return Err("border-radius needs one width, colour, and style on all four borders, or none, because a curved border is drawn as one stroke".into());
    }
    let margin = edges(s, "margin", parent)?;
    let padding = edges(s, "padding", parent)?;
    let background = s
        .get(prop!("backgroundColor"))
        .filter(|paint| !paint.is("transparent"))
        .map(color)
        .transpose()?;
    let effects = if s.get(prop!("boxShadow")).is_some_and(|v| !v.is("none"))
        || s.get(prop!("backgroundImage"))
            .is_some_and(|v| !v.is("none"))
    {
        let effects = crate::backgrounds::parse(
            s.get(prop!("boxShadow"))
                .and_then(Value::text)
                .unwrap_or("none"),
            s.get(prop!("backgroundImage"))
                .and_then(Value::text)
                .unwrap_or("none"),
            parent,
            s.get(prop!("color"))
                .map(color)
                .transpose()?
                .unwrap_or([0; 3]),
        )?;
        (!effects.shadows.is_empty() || effects.gradient.is_some())
            .then(|| std::rc::Rc::new(effects))
    } else {
        None
    };
    Ok(Some(Box {
        margin,
        padding,
        border,
        background,
        paint_effects: effects,
        border_color,
        border_style,
        radius,
        opacity,
        size: None,
        effects: None,
    }))
}
