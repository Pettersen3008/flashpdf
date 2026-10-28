use crate::css::{
    style::{color_alpha, spacing},
    Message,
};

#[derive(Clone, Debug, PartialEq)]
pub struct Effects {
    pub(crate) source: [String; 2],
    pub(crate) em: f64,
    pub(crate) current: [u8; 3],
    pub(crate) shadows: Vec<Shadow>,
    pub(crate) gradient: Option<Gradient>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shadow {
    pub offset: [f32; 2],
    pub spread: f32,
    pub color: [u8; 3],
    pub inset: bool,
    pub alpha: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Position(pub f32, pub bool);
impl Position {
    pub fn resolve(self, length: f32) -> f32 {
        if self.1 {
            self.0 * length / 100.0
        } else {
            self.0
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Shape {
    Linear(f32),
    Corner([f32; 2]),
    Radial {
        circle: bool,
        size: &'static str,
        radii: Vec<Position>,
        at: [Position; 2],
    },
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Gradient {
    pub shape: Shape,
    pub stops: Vec<([u8; 3], Option<Position>)>,
}

// Splits only at the top level, keeping functional colours intact.
fn split(source: &str, comma: bool) -> Result<Vec<&str>, Message> {
    let mut depth = 0i32;
    let mut start = 0;
    let mut parts = Vec::new();
    for (at, ch) in source.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return Err("unbalanced paint function".into());
        }
        if depth == 0
            && if comma {
                ch == ','
            } else {
                ch.is_ascii_whitespace()
            }
        {
            if comma || start < at {
                parts.push(source[start..at].trim());
            }
            start = at + ch.len_utf8();
        }
    }
    if depth != 0 {
        return Err("unbalanced paint function".into());
    }
    if comma || start < source.len() {
        parts.push(source[start..].trim());
    }
    Ok(parts)
}

#[inline(never)]
fn length(token: &str, em: f64) -> Result<f64, Message> {
    if token == "normal" {
        return Err("normal is not a paint length".into());
    }
    spacing(&token.into(), em, "paint length")
}

#[inline(never)]
fn position(token: &str, em: f64) -> Result<Position, Message> {
    if let Some(number) = token.strip_suffix('%') {
        let n: f64 = number
            .parse()
            .map_err(|_| Message::from("invalid percentage"))?;
        if !n.is_finite() || n.abs() > 1_000_000.0 {
            return Err("percentage out of range".into());
        }
        Ok(Position(n as f32, true))
    } else {
        Ok(Position(length(token, em)? as f32, false))
    }
}

#[inline(never)]
fn opaque_color(token: &str) -> Result<[u8; 3], Message> {
    let (rgb, alpha) = color_alpha(&token.into())?;
    if alpha != 1.0 {
        return Err("non-opaque gradient stops are not supported".into());
    }
    Ok(rgb)
}

fn shadows(source: &str, em: f64, current: [u8; 3]) -> Result<Vec<Shadow>, Message> {
    if source == "none" {
        return Ok(Vec::new());
    }
    let parts = split(source, true)?;
    if parts.len() > 64 {
        return Err("at most 64 shadows are supported".into());
    }
    parts
        .into_iter()
        .map(|part| {
            let tokens = split(part, false)?;
            let mut inset = false;
            let mut rgb = None;
            let mut lengths = Vec::new();
            let mut ended = false;
            for token in tokens {
                if token == "inset" && !inset {
                    inset = true;
                    ended |= !lengths.is_empty();
                } else if let Ok(length) = length(token, em) {
                    if ended {
                        return Err("shadow lengths must be consecutive".into());
                    }
                    lengths.push(length as f32);
                } else if rgb.is_none() {
                    rgb = Some(color_alpha(&token.into())?);
                    ended |= !lengths.is_empty();
                } else {
                    return Err("invalid shadow".into());
                }
            }
            if !(2..=4).contains(&lengths.len()) {
                return Err("shadow needs two offsets, optional zero blur and spread".into());
            }
            if lengths.get(2).is_some_and(|blur| *blur != 0.0) {
                return Err("blur is not supported; use box-shadow with blur radius 0".into());
            }
            Ok(Shadow {
                offset: [lengths[0], lengths[1]],
                spread: lengths.get(3).copied().unwrap_or(0.0),
                color: rgb.unwrap_or((current, 1.0)).0,
                alpha: (rgb.unwrap_or((current, 1.0)).1 * 255.0).round() as u8,
                inset,
            })
        })
        .collect()
}

#[inline(never)]
fn angle(token: &str) -> Option<f32> {
    [
        ("deg", 1.0),
        ("grad", 0.9),
        ("rad", 180.0 / std::f64::consts::PI),
        ("turn", 360.0),
    ]
    .into_iter()
    .find_map(|(unit, scale)| {
        token
            .strip_suffix(unit)?
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(|n| ((n % (360.0 / scale)) * scale) as f32)
    })
}

fn center(tokens: &[&str], em: f64) -> Result<[Position; 2], Message> {
    let middle = Position(50.0, true);
    let keyword = |token: &str, vertical: bool| match token {
        "center" => Some(middle),
        "left" if !vertical => Some(Position(0.0, true)),
        "right" if !vertical => Some(Position(100.0, true)),
        "top" if vertical => Some(Position(0.0, true)),
        "bottom" if vertical => Some(Position(100.0, true)),
        _ => None,
    };
    if tokens.is_empty() || tokens.len() > 2 {
        return Err(
            "radial position needs one or two lengths, percentages or side keywords".into(),
        );
    }
    let mut tokens = tokens.to_vec();
    if ["top", "bottom"].contains(&tokens[0])
        || tokens.get(1).is_some_and(|s| ["left", "right"].contains(s))
    {
        if tokens.len() == 1 {
            return Ok([middle, keyword(tokens[0], true).unwrap()]);
        }
        if !tokens
            .iter()
            .all(|t| ["left", "right", "top", "bottom", "center"].contains(t))
        {
            return Err("reordered radial position needs side keywords".into());
        }
        tokens.swap(0, 1);
    }
    Ok([
        keyword(tokens[0], false)
            .map(Ok)
            .unwrap_or_else(|| position(tokens[0], em))?,
        tokens
            .get(1)
            .map(|token| {
                keyword(token, true)
                    .map(Ok)
                    .unwrap_or_else(|| position(token, em))
            })
            .transpose()?
            .unwrap_or(middle),
    ])
}

fn gradient(source: &str, em: f64) -> Result<Gradient, Message> {
    let (radial, inner) = if let Some(s) = source.strip_prefix("linear-gradient(") {
        (false, s)
    } else if let Some(s) = source.strip_prefix("radial-gradient(") {
        (true, s)
    } else {
        return Err("use linear-gradient or radial-gradient; conic, repeating gradients and background images are not supported".into());
    };
    let inner = inner
        .strip_suffix(')')
        .ok_or_else(|| Message::from("invalid gradient"))?;
    let parts = split(inner, true)?;
    if parts.len() < 2 || parts.len() > 256 {
        return Err("gradient needs 2 to 256 colour stops".into());
    }
    let header = split(parts[0], false)?;
    let mut skip = false;
    let shape = if radial {
        let has_header = header.iter().any(|t| {
            [
                "circle",
                "ellipse",
                "at",
                "closest-side",
                "farthest-side",
                "closest-corner",
                "farthest-corner",
            ]
            .contains(t)
        }) || position(header.first().copied().unwrap_or(""), em).is_ok();
        let mut circle = false;
        let mut size = "farthest-corner";
        let mut radii = Vec::new();
        let mut at = [Position(50.0, true); 2];
        if has_header {
            skip = true;
            let index = header
                .iter()
                .position(|t| *t == "at")
                .unwrap_or(header.len());
            if index < header.len() {
                at = center(&header[index + 1..], em)?;
            }
            let mut shape_seen = false;
            let mut size_seen = false;
            for token in &header[..index] {
                match *token {
                    "circle" | "ellipse" if !shape_seen => {
                        circle = *token == "circle";
                        shape_seen = true;
                    }
                    "closest-side" | "farthest-side" | "closest-corner" | "farthest-corner"
                        if !size_seen =>
                    {
                        size = match *token {
                            "closest-side" => "closest-side",
                            "farthest-side" => "farthest-side",
                            "closest-corner" => "closest-corner",
                            _ => "farthest-corner",
                        };
                        size_seen = true;
                    }
                    _ => radii.push(position(token, em)?),
                }
            }
            if !radii.is_empty() {
                if size_seen || radii.len() > 2 || radii.iter().any(|p| p.0 < 0.0) {
                    return Err("invalid radial size".into());
                }
                if !shape_seen {
                    circle = radii.len() == 1;
                }
                if (circle && (radii.len() != 1 || radii[0].1)) || (!circle && radii.len() != 2) {
                    return Err(
                        "circle needs one length; ellipse needs two lengths or percentages".into(),
                    );
                }
            }
        }
        Shape::Radial {
            circle,
            size,
            radii,
            at,
        }
    } else if let Some(a) = angle(parts[0]) {
        skip = true;
        Shape::Linear(a)
    } else if header.first() == Some(&"to") {
        skip = true;
        let mut dir = [0.0; 2];
        for token in &header[1..] {
            match *token {
                "left" if dir[0] == 0.0 => dir[0] = -1.0,
                "right" if dir[0] == 0.0 => dir[0] = 1.0,
                "top" if dir[1] == 0.0 => dir[1] = 1.0,
                "bottom" if dir[1] == 0.0 => dir[1] = -1.0,
                _ => return Err("invalid linear direction".into()),
            }
        }
        if dir == [0.0; 2] {
            return Err("missing linear direction".into());
        }
        Shape::Corner(dir)
    } else {
        Shape::Linear(180.0)
    };
    let mut stops = Vec::new();
    for part in &parts[usize::from(skip)..] {
        let tokens = split(part, false)?;
        if tokens.is_empty() || tokens.len() > 3 {
            return Err("invalid colour stop".into());
        }
        let rgb = opaque_color(tokens[0])?;
        if tokens.len() == 1 {
            stops.push((rgb, None));
        } else {
            for token in &tokens[1..] {
                stops.push((rgb, Some(position(token, em)?)));
            }
        }
    }
    if stops.len() > 256 {
        return Err("at most 256 expanded gradient stops are supported".into());
    }
    if stops.len() < 2 {
        return Err(
            "gradient needs at least two colour stops; transition hints are not supported".into(),
        );
    }
    Ok(Gradient { shape, stops })
}

pub(crate) fn parse(
    shadow: &str,
    image: &str,
    em: f64,
    current: [u8; 3],
) -> Result<Effects, Message> {
    let shadow = shadow.to_ascii_lowercase();
    let image = image.to_ascii_lowercase();
    let shadows = shadows(shadow.trim(), em, current)
        .map_err(|e| Message::from(format!("box-shadow: {shadow}: {e}")))?;
    let gradient = if image.trim() == "none" {
        None
    } else {
        if split(&image, true)?.len() != 1 {
            return Err(format!(
                "background-image: {image}: multiple background layers are not supported"
            )
            .into());
        }
        Some(
            gradient(image.trim(), em)
                .map_err(|e| Message::from(format!("background-image: {image}: {e}")))?,
        )
    };
    Ok(Effects {
        source: [shadow, image],
        em,
        current,
        shadows,
        gradient,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_multiple_shadows_when_parsed_then_keeps_signed_lengths_alpha_inset_and_current_color()
    {
        let effects = parse(
            "2px -3pt 0 4pt rgba(255,0,0,.5), inset 0 2pt",
            "none",
            12.0,
            [0, 0, 255],
        )
        .unwrap();
        assert_eq!(effects.shadows[0].offset, [1.5, -3.0]);
        assert_eq!(effects.shadows[0].spread, 4.0);
        assert_eq!(effects.shadows[0].alpha, 128);
        assert!(effects.shadows[1].inset);
        assert_eq!(effects.shadows[1].color, [0, 0, 255]);
        for bad in [
            "0 1pt 2pt red",
            "0 0 -1pt",
            "0 0 0 1pt 2pt",
            "0 red 1pt",
            "normal normal",
        ] {
            assert!(parse(bad, "none", 12.0, [0; 3]).is_err(), "{bad}");
        }
    }

    #[test]
    fn given_gradients_when_parsed_then_keeps_geometry_and_rejects_unsupported_paint() {
        let effects = parse(
            "none",
            "linear-gradient(to top right, red -10pt, rgb(0 255 0) 20% 40%, blue)",
            12.0,
            [0; 3],
        )
        .unwrap();
        let gradient = effects.gradient.unwrap();
        assert_eq!(gradient.shape, Shape::Corner([1.0, 1.0]));
        assert_eq!(gradient.stops.len(), 4);
        for bad in [
            "conic-gradient(red,blue)",
            "repeating-linear-gradient(red,blue)",
            "linear-gradient(red,blue), radial-gradient(red,blue)",
            "linear-gradient(transparent,blue)",
            "linear-gradient(rgba(0 0 0 / .9999999),blue)",
            "linear-gradient(red normal,blue)",
            "linear-gradient(#f008,blue)",
            "linear-gradient(red,50%,blue)",
            "radial-gradient(circle 10%,red,blue)",
            "linear-gradient(to right left,red,blue)",
        ] {
            assert!(parse("none", bad, 12.0, [0; 3]).is_err(), "{bad}");
        }
    }
}
