use super::{point, Style};
use crate::css::{js_trim, number_end, Message, Prop, Value};
use crate::grid::{Grid, GridBreadth, GridItem, GridLine, GridTrack};

const LIMIT: usize = 256;

fn invalid(name: &str, value: impl std::fmt::Display) -> Message {
    format!("invalid {name}: {value} (bounded grid supports numeric lines and at most 256 tracks; named lines, subgrid, masonry, auto-fill, auto-fit, and dense are unsupported)").into()
}

// Split only outside functions, and reject malformed nesting before parsing their contents.
fn split(text: &str, separator: Option<char>) -> Result<Vec<&str>, ()> {
    let mut depth = 0_u8;
    let mut start = 0;
    let mut parts = Vec::new();
    for (at, ch) in text.char_indices() {
        match ch {
            '(' => {
                depth = depth.checked_add(1).filter(|depth| *depth <= 3).ok_or(())?;
            }
            ')' => depth = depth.checked_sub(1).ok_or(())?,
            '[' | ']' => return Err(()),
            _ => {}
        }
        if depth == 0 && separator.map_or_else(|| ch.is_whitespace(), |sep| ch == sep) {
            let part = js_trim(&text[start..at]);
            if separator.is_some() || !part.is_empty() {
                parts.push(part);
            }
            start = at + ch.len_utf8();
        }
    }
    if depth != 0 {
        return Err(());
    }
    let part = js_trim(&text[start..]);
    if separator.is_some() || !part.is_empty() {
        parts.push(part);
    }
    Ok(parts)
}

fn amount(text: &str) -> Option<f32> {
    let end = number_end(text.as_bytes(), true);
    if end == 0 || end != text.len() {
        return None;
    }
    let n = text.parse::<f64>().ok()?;
    (n >= 0.0 && (n as f32).is_finite()).then_some(n as f32)
}

fn breadth(text: &str, size: f64, name: &str) -> Result<GridBreadth, Message> {
    Ok(match text {
        "auto" => GridBreadth::Auto,
        "min-content" => GridBreadth::MinContent,
        "max-content" => GridBreadth::MaxContent,
        _ if text.ends_with("fr") => {
            GridBreadth::Fr(amount(&text[..text.len() - 2]).ok_or_else(|| invalid(name, text))?)
        }
        _ if text.ends_with('%') => GridBreadth::Percent(
            amount(&text[..text.len() - 1]).ok_or_else(|| invalid(name, text))? / 100.0,
        ),
        _ => {
            let n = point(&text.into(), size, false).map_err(|_| invalid(name, text))? as f32;
            if !n.is_finite() {
                return Err(invalid(name, text));
            }
            GridBreadth::Length(n)
        }
    })
}

fn track(text: &str, size: f64, name: &str) -> Result<GridTrack, Message> {
    if let Some(inner) = text
        .strip_prefix("minmax(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let parts = split(inner, Some(',')).map_err(|_| invalid(name, text))?;
        if parts.len() != 2 {
            return Err(invalid(name, text));
        }
        let min = breadth(parts[0], size, name)?;
        let max = breadth(parts[1], size, name)?;
        if matches!(min, GridBreadth::Fr(_)) {
            return Err(invalid(name, text));
        }
        return Ok(GridTrack { min, max });
    }
    let max = breadth(text, size, name)?;
    Ok(GridTrack {
        min: if matches!(max, GridBreadth::Fr(_)) {
            GridBreadth::Auto
        } else {
            max
        },
        max,
    })
}

fn tracks(value: &Value, size: f64, name: &str, template: bool) -> Result<Vec<GridTrack>, Message> {
    if let Value::Number(n) = value {
        if !(*n as f32).is_finite() || *n < 0.0 {
            return Err(invalid(name, value));
        }
        let n = *n as f32;
        return Ok(vec![GridTrack {
            min: GridBreadth::Length(n),
            max: GridBreadth::Length(n),
        }]);
    }
    let text = value.to_string().to_ascii_lowercase();
    let text = js_trim(&text);
    if template && text == "none" {
        return Ok(Vec::new());
    }
    let parts = split(text, None).map_err(|_| invalid(name, value))?;
    if parts.is_empty() || parts.len() > LIMIT {
        return Err(invalid(name, value));
    }
    let mut result = Vec::new();
    for part in parts {
        if let Some(inner) = part
            .strip_prefix("repeat(")
            .and_then(|s| s.strip_suffix(')'))
        {
            let args = split(inner, Some(',')).map_err(|_| invalid(name, value))?;
            if !template || args.len() != 2 {
                return Err(invalid(name, value));
            }
            let count_text = args[0].strip_prefix('+').unwrap_or(args[0]);
            if !count_text.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid(name, value));
            }
            let count = count_text
                .parse::<usize>()
                .ok()
                .filter(|n| *n > 0 && *n <= LIMIT)
                .ok_or_else(|| invalid(name, value))?;
            let repeated = split(args[1], None).map_err(|_| invalid(name, value))?;
            if repeated.is_empty() || repeated.len() > (LIMIT - result.len()) / count {
                return Err(invalid(name, value));
            }
            let repeated = repeated
                .into_iter()
                .map(|part| track(part, size, name))
                .collect::<Result<Vec<_>, _>>()?;
            for _ in 0..count {
                result.extend_from_slice(&repeated);
            }
        } else {
            if result.len() == LIMIT {
                return Err(invalid(name, value));
            }
            result.push(track(part, size, name)?);
        }
    }
    Ok(result)
}

fn line(value: &Value, name: &str) -> Result<GridLine, Message> {
    let text = value.to_string().to_ascii_lowercase();
    let parts: Vec<_> = text.split_whitespace().collect();
    if parts == ["auto"] {
        return Ok(GridLine::Auto);
    }
    let (span, digits) = match parts.as_slice() {
        [digits] => (false, *digits),
        ["span", digits] | [digits, "span"] => (true, *digits),
        _ => return Err(invalid(name, value)),
    };
    let unsigned = digits.strip_prefix(['+', '-']).unwrap_or(digits);
    if unsigned.is_empty() || !unsigned.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(name, value));
    }
    let n = digits
        .parse::<i16>()
        .ok()
        .filter(|n| *n != 0 && n.unsigned_abs() <= LIMIT as u16)
        .ok_or_else(|| invalid(name, value))?;
    if span {
        if n < 0 {
            return Err(invalid(name, value));
        }
        Ok(GridLine::Span(n as u16))
    } else {
        Ok(GridLine::Line(n))
    }
}

pub(super) fn placement(value: &Value, name: &str) -> Result<[Value; 2], Message> {
    let text = value.to_string();
    let parts: Vec<_> = text.split('/').collect();
    if parts.is_empty() || parts.len() > 2 {
        return Err(invalid(name, value));
    }
    let result = [
        Value::from(js_trim(parts[0])),
        Value::from(js_trim(parts.get(1).copied().unwrap_or("auto"))),
    ];
    for value in &result {
        line(value, name)?;
    }
    Ok(result)
}

fn alignment(s: &Style, prop: Prop, content: bool, auto: bool) -> Result<u8, Message> {
    let Some(value) = s.get(prop) else {
        return Ok(if content || auto { 0 } else { 5 });
    };
    let text = value.to_string().to_ascii_lowercase();
    let text = js_trim(&text);
    let code = if content {
        match text {
            "stretch" | "normal" => 0,
            "start" | "flex-start" => 1,
            "end" | "flex-end" => 2,
            "center" => 3,
            "space-between" => 4,
            "space-around" => 5,
            "space-evenly" => 6,
            _ => return Err(invalid(prop.name(), value)),
        }
    } else {
        match text {
            "auto" if auto => 0,
            "stretch" => 1,
            "normal" => 5,
            "start" | "flex-start" => 2,
            "end" | "flex-end" => 3,
            "center" => 4,
            _ => return Err(invalid(prop.name(), value)),
        }
    };
    Ok(code)
}

pub fn container(s: &Style, font_size: f64) -> Result<Grid, Message> {
    let axis = |prop: Prop, template| {
        s.get(prop).map_or_else(
            || {
                if template {
                    Ok(Vec::new())
                } else {
                    Ok(vec![GridTrack {
                        min: GridBreadth::Auto,
                        max: GridBreadth::Auto,
                    }])
                }
            },
            |v| tracks(v, font_size, prop.name(), template),
        )
    };
    let column_flow = match s.get(prop!("gridAutoFlow")) {
        None => false,
        Some(v) if v.is("row") => false,
        Some(v) if v.is("column") => true,
        Some(v) => return Err(invalid("grid-auto-flow", v)),
    };
    let gap = |prop: Prop| -> Result<f32, Message> {
        let Some(v) = s.get(prop) else {
            return Ok(0.0);
        };
        let n = point(v, font_size, false).map_err(|_| invalid(prop.name(), v))? as f32;
        if !n.is_finite() {
            return Err(invalid(prop.name(), v));
        }
        Ok(n)
    };
    Ok(Grid {
        columns: axis(prop!("gridTemplateColumns"), true)?,
        rows: axis(prop!("gridTemplateRows"), true)?,
        auto_columns: axis(prop!("gridAutoColumns"), false)?,
        auto_rows: axis(prop!("gridAutoRows"), false)?,
        column_flow,
        boxed: false,
        justify_items: alignment(s, prop!("justifyItems"), false, false)?,
        align_items: alignment(s, prop!("alignItems"), false, false)?,
        justify_content: alignment(s, prop!("justifyContent"), true, false)?,
        align_content: alignment(s, prop!("alignContent"), true, false)?,
        column_gap: gap(prop!("columnGap"))?,
        row_gap: gap(prop!("rowGap"))?,
        min_height: 0.0,
        max_height: None,
    })
}

pub fn item(s: &Style) -> Result<GridItem, Message> {
    let axis = |start: Prop, end: Prop| -> Result<[GridLine; 2], Message> {
        let read = |prop: Prop| {
            s.get(prop)
                .map_or(Ok(GridLine::Auto), |v| line(v, prop.name()))
        };
        Ok([read(start)?, read(end)?])
    };
    Ok(GridItem {
        column: axis(prop!("gridColumnStart"), prop!("gridColumnEnd"))?,
        row: axis(prop!("gridRowStart"), prop!("gridRowEnd"))?,
        justify: alignment(s, prop!("justifySelf"), false, true)?,
        align: alignment(s, prop!("alignSelf"), false, true)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::{style::style, stylesheet, Cascaded, Key};

    fn normalized(entries: &[(&str, Value)]) -> Result<Style, Message> {
        style(
            &Cascaded(
                entries
                    .iter()
                    .map(|(name, v)| (Key::Prop(Prop::of(name)), v.clone()))
                    .collect(),
            ),
            &Style::default(),
        )
    }

    #[test]
    fn given_track_functions_and_auto_tracks_when_parsing_then_expands_and_normalizes_breadths() {
        let s = normalized(&[
            ("display", "grid".into()),
            (
                "gridTemplateColumns",
                "12pt 16px 25% 2fr repeat(2, minmax(0, 1fr) auto) min-content max-content".into(),
            ),
            (
                "gridAutoRows",
                "minmax(min-content, max-content) 4pt".into(),
            ),
            ("gridAutoFlow", "column".into()),
            ("gap", "3pt 8px".into()),
            ("justifyItems", "center".into()),
            ("alignItems", "end".into()),
            ("justifyContent", "space-between".into()),
            ("alignContent", "stretch".into()),
        ])
        .unwrap();
        let g = container(&s, 12.0).unwrap();
        assert_eq!(g.columns.len(), 10);
        assert_eq!(g.columns[0].max, GridBreadth::Length(12.0));
        assert_eq!(g.columns[1].max, GridBreadth::Length(12.0));
        assert_eq!(g.columns[2].max, GridBreadth::Percent(0.25));
        assert_eq!(
            g.columns[3],
            GridTrack {
                min: GridBreadth::Auto,
                max: GridBreadth::Fr(2.0)
            }
        );
        assert_eq!(
            g.columns[4],
            GridTrack {
                min: GridBreadth::Length(0.0),
                max: GridBreadth::Fr(1.0)
            }
        );
        assert_eq!(
            g.auto_rows[0],
            GridTrack {
                min: GridBreadth::MinContent,
                max: GridBreadth::MaxContent
            }
        );
        assert_eq!((g.column_flow, g.column_gap, g.row_gap), (true, 6.0, 3.0));
        assert_eq!(
            (
                g.justify_items,
                g.align_items,
                g.justify_content,
                g.align_content
            ),
            (4, 3, 4, 0)
        );
        assert_eq!((g.min_height, g.max_height), (0.0, None));
    }

    #[test]
    fn given_placement_shorthands_when_cascading_then_resets_ends_and_keeps_later_longhands() {
        let s = normalized(&[
            ("gridColumnEnd", "span 4".into()),
            ("gridColumn", "-2".into()),
            ("gridRow", "2 / span 3".into()),
            ("gridRowStart", 4.0.into()),
            ("justifySelf", "end".into()),
            ("alignSelf", "center".into()),
        ])
        .unwrap();
        assert_eq!(
            item(&s).unwrap(),
            GridItem {
                column: [GridLine::Line(-2), GridLine::Auto],
                row: [GridLine::Line(4), GridLine::Span(3)],
                justify: 3,
                align: 4,
            }
        );
        let inherited = style(
            &Cascaded(vec![(Key::Prop(prop!("gridRow")), "inherit".into())]),
            &s,
        )
        .unwrap();
        assert_eq!(item(&inherited).unwrap().row, item(&s).unwrap().row);
        let reset =
            normalized(&[("gridRow", "2 / 4".into()), ("gridRow", "initial".into())]).unwrap();
        assert_eq!(item(&reset).unwrap().row, [GridLine::Auto; 2]);
    }

    #[test]
    fn given_bounded_tracks_and_placement_when_parsing_then_accepts_256_and_rejects_257() {
        assert_eq!(
            tracks(
                &"repeat(256, 1fr)".into(),
                12.0,
                "grid-template-columns",
                true
            )
            .unwrap()
            .len(),
            256
        );
        for value in [
            "repeat(257, 1fr)",
            "repeat(129, 1fr 2fr)",
            "repeat(256, 1fr) 1pt",
            "repeat(0, 1fr)",
        ] {
            assert!(
                tracks(&value.into(), 12.0, "grid-template-columns", true).is_err(),
                "{value}"
            );
        }
        for value in ["256", "-256", "span 256", "256 span"] {
            assert!(line(&value.into(), "grid-row-start").is_ok(), "{value}");
        }
        for value in [
            "257", "-257", "span 257", "0", "span 0", "span -1", "1.5", "span", "name",
        ] {
            assert!(line(&value.into(), "grid-row-start").is_err(), "{value}");
        }
    }

    #[test]
    fn given_unsupported_or_malformed_grid_values_when_validating_unmatched_rules_then_names_property(
    ) {
        for (property, value) in [
            ("display", "inline-grid"),
            ("grid-template-columns", "subgrid"),
            ("grid-template-rows", "masonry"),
            ("grid-template-columns", "[a] 1fr"),
            ("grid-template-columns", "repeat(auto-fill, 1fr)"),
            ("grid-template-columns", "repeat(auto-fit, 1fr)"),
            ("grid-template-columns", "minmax(1fr, 2fr)"),
            ("grid-template-columns", "repeat(2, repeat(2, 1fr))"),
            ("grid-template-columns", "minmax(0, 1fr"),
            ("grid-template-columns", "-1px"),
            ("grid-template-columns", "-1e-100fr"),
            ("grid-template-columns", "-1e-100%"),
            ("grid-template-columns", "1fr,"),
            ("grid-auto-flow", "row dense"),
            ("grid-auto-rows", "repeat(2, 1fr)"),
            ("grid-row", "1 / 2 / 3"),
            ("grid-row-start", "span 0"),
            ("justify-items", "baseline"),
            ("justify-self", "space-between"),
            ("grid-template-areas", "\"a\""),
            ("grid-area", "a"),
        ] {
            let error = stylesheet(&format!(".unmatched {{ {property}: {value}; }}")).unwrap_err();
            assert!(
                error
                    .replace('-', "")
                    .to_ascii_lowercase()
                    .contains(&property.replace('-', "")),
                "{error}"
            );
        }
    }

    #[test]
    fn given_negative_or_nonfinite_numeric_tracks_when_parsing_then_rejects_before_f32_rounding() {
        for n in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            -1e-100,
            -1.0,
            1e40,
        ] {
            assert!(
                tracks(&n.into(), 12.0, "grid-template-rows", true).is_err(),
                "{n}"
            );
        }
    }
}
