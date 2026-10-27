use super::super::{js_trim, number, number_end, parse_number, unit_points, words, Value};

/// `value.trim().split(/\s+/)` for text, the value alone otherwise.
pub(super) fn tokens(value: &Value) -> Vec<Value> {
    match value {
        Value::Text(text) => words(text).into_iter().map(Value::from).collect(),
        Value::Number(_) => vec![value.clone()],
    }
}

/// One to four values spread over the sides, top first, as `margin` gives them.
pub fn edge_values(value: &Value, name: &str) -> Result<[Value; 4], String> {
    let values = tokens(value);
    let at = |index: usize| values[index].clone();
    Ok(match values.len() {
        1 => [at(0), at(0), at(0), at(0)],
        2 => [at(0), at(1), at(0), at(1)],
        3 => [at(0), at(1), at(2), at(1)],
        4 => [at(0), at(1), at(2), at(3)],
        _ => return Err(format!("invalid {name}: {value}")),
    })
}

/// Whether `text` is all `\d+(\.\d*)?|\.\d+`, without an exponent.
pub(super) fn plain_number(text: &str) -> bool {
    !text.is_empty() && number_end(text.as_bytes(), false) == text.len()
}

/// A length in points; a number is points already. `em` scales `parent`.
pub fn point(value: &Value, parent: f64, positive: bool) -> Result<f64, String> {
    match value {
        Value::Number(amount) => number(*amount, positive),
        Value::Text(text) => super::super::point(text, parent, positive),
    }
}

/// One radius: points, or a percent of the box side, which only the engine can resolve.
pub type CornerRadius = (f64, bool);

/// A corner's radius, horizontal then vertical; one value serves both.
pub fn corner(value: &Value, parent: f64) -> Result<[CornerRadius; 2], String> {
    let tokens = tokens(value);
    if tokens.len() > 2 {
        return Err(format!("invalid border radius: {value}"));
    }
    let radius = |token: &Value| -> Result<CornerRadius, String> {
        let percent = token.text().and_then(|text| text.strip_suffix('%'));
        match percent {
            Some(amount)
                if !amount.is_empty() && number_end(amount.as_bytes(), true) == amount.len() =>
            {
                Ok((number(parse_number(amount), false)?, true))
            }
            _ => Ok((point(token, parent, false)?, false)),
        }
    };
    Ok([
        radius(&tokens[0])?,
        radius(tokens.get(1).unwrap_or(&tokens[0]))?,
    ])
}

/// A number or percent, clamped to 0 through 1 as CSS does.
pub fn opacity(value: &Value) -> Result<f64, String> {
    let amount = match value {
        Value::Number(amount) => *amount,
        Value::Text(text) => {
            let text = js_trim(text);
            let (digits, scale) = match text.strip_suffix('%') {
                Some(digits) => (digits, 100.0),
                None => (text, 1.0),
            };
            if plain_number(digits) {
                parse_number(digits) / scale
            } else {
                f64::NAN
            }
        }
    };
    if amount.is_nan() {
        return Err(format!("invalid opacity: {value}"));
    }
    Ok(amount.clamp(0.0, 1.0))
}

/// `letter-spacing` or `word-spacing` in points, either sign, bounded by the largest page side.
pub fn spacing(value: &Value, size: f64, name: &str) -> Result<f64, String> {
    if value.is("normal") {
        return Ok(0.0);
    }
    let (negative, magnitude) = match value {
        Value::Number(amount) => (*amount < 0.0, Value::Number(amount.abs())),
        Value::Text(text) => {
            let trimmed = js_trim(text);
            (
                trimmed.starts_with('-'),
                Value::from(trimmed.strip_prefix('-').unwrap_or(trimmed)),
            )
        }
    };
    let amount = point(&magnitude, size, false)
        .map_err(|_| format!("invalid {name}: {value} (use normal or a length)"))?
        * if negative { -1.0 } else { 1.0 };
    if amount.abs() > 14_400.0 {
        return Err(format!("invalid {name}: {value}"));
    }
    Ok(amount)
}

/// A width-like size: points (kind 1) or percent of the container's width (kind 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dimension {
    pub kind: u8,
    pub value: f64,
}

/// A `width` or img `height`: points (kind 1) or percent of the container width (kind 2).
pub fn dimension(value: &Value, name: &str, zero: bool) -> Result<Dimension, String> {
    let mut percent = false;
    let amount = match value {
        Value::Number(amount) => *amount,
        Value::Text(text) if &**text == "0" => 0.0,
        Value::Text(text) => {
            let end = number_end(text.as_bytes(), false);
            let unit = &text[end..];
            percent = unit == "%";
            let scale = if percent {
                Some(1.0)
            } else {
                unit_points(unit)
            };
            match scale {
                Some(scale) if end > 0 => parse_number(&text[..end]) * scale,
                _ => f64::NAN,
            }
        }
    };
    // The engine takes f32, so a long digit string must not round to Infinity.
    let single = amount as f32;
    if !(amount > 0.0 || (zero && amount == 0.0)) || !single.is_finite() {
        return Err(format!("invalid {name}: {value}"));
    }
    Ok(Dimension {
        kind: if percent && amount > 0.0 { 2 } else { 1 },
        value: f64::from(single),
    })
}

/// `auto` (None), or a zero, length, or percent basis.
pub fn flex_basis(value: &Value) -> Result<Option<Dimension>, String> {
    if value.is("auto") {
        return Ok(None);
    }
    if value.is("content") {
        return Err("flex-basis: content is not supported; use auto".into());
    }
    dimension(value, "flex-basis", true).map(Some)
}

/// The `flex` shorthand's longhands, grow, shrink, and basis: `none`, `auto`, or grow and shrink
/// factors with a basis before or after them. Factors alone have a zero basis; a basis alone
/// grows and shrinks by 1.
pub(super) fn flex_shorthand(value: &Value) -> Result<(f64, f64, Value), String> {
    let text = match value {
        Value::Number(amount) => return Ok((number(*amount, false)?, 1.0, Value::Number(0.0))),
        Value::Text(text) => js_trim(text).to_lowercase(),
    };
    let invalid = || format!("invalid flex: {value}");
    match text.as_str() {
        "" => return Err(invalid()),
        "none" => return Ok((0.0, 0.0, "auto".into())),
        "auto" => return Ok((1.0, 1.0, "auto".into())),
        _ => {}
    }
    let mut tokens = words(&text);
    let mut basis = (!plain_number(tokens[0])).then(|| tokens.remove(0));
    let mut rest = tokens.into_iter();
    let grow = rest.next();
    let mut shrink = rest.next();
    let mut last = rest.next();
    let mut extra: Vec<&str> = rest.collect();
    // A second token that is no number, or any third token, is the basis.
    if shrink.is_some_and(|token| !plain_number(token)) && last.is_none() {
        last = shrink.take();
    }
    if let Some(last) = last {
        if basis.is_some() {
            extra.push(last);
        }
        basis = Some(last);
    }
    if !extra.is_empty()
        || [grow, shrink]
            .iter()
            .flatten()
            .any(|token| !plain_number(token))
    {
        return Err(invalid());
    }
    let basis = basis.map_or(Value::Number(0.0), Value::from);
    flex_basis(&basis)?;
    let factor = |token: Option<&str>| number(token.map_or(1.0, parse_number), false);
    Ok((factor(grow)?, factor(shrink)?, basis))
}

/// A unitless number inherits as a factor, a length or percent as points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineHeight {
    Factor(f64),
    Pt(f64),
}

/// None is `normal`.
pub fn line_height(value: &Value, size: f64) -> Result<Option<LineHeight>, String> {
    let text = match value {
        Value::Number(amount) => return Ok(Some(LineHeight::Factor(number(*amount, true)?))),
        Value::Text(text) if &**text == "normal" => return Ok(None),
        Value::Text(text) => text,
    };
    let trimmed = js_trim(text);
    let (digits, percent) = match trimmed.strip_suffix('%') {
        Some(digits) => (digits, true),
        None => (trimmed, false),
    };
    if !plain_number(digits) {
        return Ok(Some(LineHeight::Pt(point(value, size, true)?)));
    }
    let amount = number(parse_number(digits), true)?;
    Ok(Some(if percent {
        LineHeight::Pt(number(amount * size / 100.0, true)?)
    } else {
        LineHeight::Factor(amount)
    }))
}
