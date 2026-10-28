//! CSS box effects in points, with top-left origins and downward-positive y axes.

use crate::vector::{parse_path, PathOp};

const MAX_INPUT: usize = 64 * 1024;
const MAX_ITEMS: usize = 1024;
// Bounds CSS scalars, computed coordinates and matrix entries in points.
const LIMIT: f32 = 1_000_000.0;
const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

type Result<T> = std::result::Result<T, crate::css::Message>;

#[inline(never)]
fn invalid() -> crate::css::Message {
    "unsupported, invalid or over-limit value".into()
}

#[inline(never)]
fn bounded(value: f32) -> Result<f32> {
    if value.is_finite() && value.abs() <= LIMIT {
        Ok(value)
    } else {
        Err(invalid())
    }
}

fn trim(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_ascii() && crate::css::whitespace(c as u8))
}

#[inline(never)]
fn words(text: &str) -> Vec<&str> {
    text.split(|c: char| c.is_ascii() && crate::css::whitespace(c as u8))
        .filter(|s| !s.is_empty())
        .collect()
}

fn source(text: &str) -> Result<&str> {
    if text.len() > MAX_INPUT {
        return Err(invalid());
    }
    let text = trim(text);
    if text.is_empty() {
        Err(invalid())
    } else {
        Ok(text)
    }
}

// CSS numbers require a digit after the decimal point, unlike SVG path numbers.
#[inline(never)]
fn quantity(text: &str) -> Result<(f32, &str)> {
    let bytes = text.as_bytes();
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let end = sign + crate::css::number_end(&bytes[sign..], true);
    if end == sign
        || text[..end].ends_with('.')
        || text[..end].contains(".e")
        || text[..end].contains(".E")
    {
        return Err(invalid());
    }
    let value = text[..end].parse::<f64>().map_err(|_| invalid())? as f32;
    Ok((bounded(value)?, &text[end..]))
}

#[inline(never)]
fn number(text: &str) -> Result<f32> {
    let (value, unit) = quantity(text)?;
    if unit.is_empty() {
        Ok(value)
    } else {
        Err(invalid())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Length {
    points: f32,
    percent: f32,
}

impl Length {
    const ZERO: Self = Self {
        points: 0.0,
        percent: 0.0,
    };
    const CENTER: Self = Self {
        points: 0.0,
        percent: 0.5,
    };

    #[inline(never)]
    fn parse(text: &str, font_size: f32) -> Result<Self> {
        let (value, unit) = quantity(text)?;
        if unit == "%" {
            return Ok(Self {
                points: 0.0,
                percent: value / 100.0,
            });
        }
        if unit.is_empty() {
            return if value == 0.0 {
                Ok(Self::ZERO)
            } else {
                Err(invalid())
            };
        }
        let lower = unit.to_ascii_lowercase();
        let unit = if lower == "q" { "Q" } else { &lower };
        let scale = if unit == "em" {
            f64::from(font_size)
        } else {
            crate::css::unit_points(unit).ok_or_else(invalid)?
        };
        Ok(Self {
            points: bounded((f64::from(value) * scale) as f32)?,
            percent: 0.0,
        })
    }

    fn nonnegative(text: &str, font_size: f32) -> Result<Self> {
        let value = Self::parse(text, font_size)?;
        if value.points < 0.0 || value.percent < 0.0 {
            Err(invalid())
        } else {
            Ok(value)
        }
    }

    #[inline(never)]
    fn resolve(self, size: f32) -> Result<f32> {
        bounded(self.points + self.percent * size)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Step {
    Matrix([f32; 6]),
    Translate(Length, Length),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Transform {
    steps: Vec<Step>,
    specified: bool,
}

#[inline(never)]
fn multiply(a: [f32; 6], b: [f32; 6]) -> Result<[f32; 6]> {
    let result = [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ];
    for value in result {
        bounded(value)?;
    }
    Ok(result)
}

fn angle(text: &str) -> Result<f32> {
    let (value, unit) = quantity(text)?;
    let scale = match unit.to_ascii_lowercase().as_str() {
        "deg" => std::f32::consts::PI / 180.0,
        "grad" => std::f32::consts::PI / 200.0,
        "turn" => 2.0 * std::f32::consts::PI,
        "rad" => 1.0,
        "" if value == 0.0 => 1.0,
        _ => return Err(invalid()),
    };
    bounded(value * scale)
}

// Returns one function and its suffix, respecting quoted path data.
#[inline(never)]
fn function(text: &str) -> Result<(&str, &str, &str)> {
    let open = text.find('(').ok_or_else(invalid)?;
    let name = &text[..open];
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_alphabetic()) {
        return Err(invalid());
    }
    let mut quote = None;
    let mut escaped = false;
    for (at, c) in text[open + 1..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            quote = Some(c);
            continue;
        }
        if c == '(' {
            return Err(invalid());
        }
        if c == ')' {
            let close = open + 1 + at;
            return Ok((name, &text[open + 1..close], trim(&text[close + 1..])));
        }
    }
    Err(invalid())
}

impl Transform {
    #[inline(never)]
    pub(crate) fn parse(text: &str, font_size: f32) -> Result<Self> {
        let mut rest = source(text)?;
        if rest.eq_ignore_ascii_case("none") {
            return Ok(Self {
                steps: Vec::new(),
                specified: false,
            });
        }
        let mut steps = Vec::new();
        while !rest.is_empty() {
            if steps.len() >= MAX_ITEMS {
                return Err(invalid());
            }
            let (name, body, suffix) = function(rest)?;
            let args: Vec<&str> = body.split(',').map(trim).collect();
            let mut matrix = IDENTITY;
            let step = match (name.to_ascii_lowercase().as_str(), args.as_slice()) {
                ("matrix", [a, b, c, d, e, f]) => Step::Matrix([
                    number(a)?,
                    number(b)?,
                    number(c)?,
                    number(d)?,
                    bounded(number(e)? * 0.75)?,
                    bounded(number(f)? * 0.75)?,
                ]),
                ("translate", [x]) | ("translatex", [x]) => {
                    Step::Translate(Length::parse(x, font_size)?, Length::ZERO)
                }
                ("translatey", [y]) => Step::Translate(Length::ZERO, Length::parse(y, font_size)?),
                ("translate", [x, y]) => {
                    Step::Translate(Length::parse(x, font_size)?, Length::parse(y, font_size)?)
                }
                ("scale", [x]) => {
                    matrix[0] = number(x)?;
                    matrix[3] = matrix[0];
                    Step::Matrix(matrix)
                }
                ("scale", [x, y]) => {
                    matrix[0] = number(x)?;
                    matrix[3] = number(y)?;
                    Step::Matrix(matrix)
                }
                ("scalex", [x]) => {
                    matrix[0] = number(x)?;
                    Step::Matrix(matrix)
                }
                ("scaley", [y]) => {
                    matrix[3] = number(y)?;
                    Step::Matrix(matrix)
                }
                ("rotate", [a]) => {
                    let a = angle(a)?;
                    let (s, c) = (
                        libm::sin(f64::from(a)) as f32,
                        libm::cos(f64::from(a)) as f32,
                    );
                    Step::Matrix([c, s, -s, c, 0.0, 0.0])
                }
                ("skew", [x]) | ("skewx", [x]) => {
                    matrix[2] = bounded(crate::vector::tangent(f64::from(angle(x)?)) as f32)?;
                    Step::Matrix(matrix)
                }
                ("skewy", [y]) => {
                    matrix[1] = bounded(crate::vector::tangent(f64::from(angle(y)?)) as f32)?;
                    Step::Matrix(matrix)
                }
                ("skew", [x, y]) => {
                    matrix[2] = bounded(crate::vector::tangent(f64::from(angle(x)?)) as f32)?;
                    matrix[1] = bounded(crate::vector::tangent(f64::from(angle(y)?)) as f32)?;
                    Step::Matrix(matrix)
                }
                _ => return Err(invalid()),
            };
            if let Step::Matrix(matrix) = step {
                multiply(IDENTITY, matrix)?;
            }
            steps.push(step);
            rest = suffix;
        }
        let result = Self {
            steps,
            specified: true,
        };
        result.resolve(0.0, 0.0)?;
        Ok(result)
    }

    pub(crate) fn resolve(&self, width: f32, height: f32) -> Result<[f32; 6]> {
        let mut result = IDENTITY;
        for step in &self.steps {
            let matrix = match step {
                Step::Matrix(matrix) => *matrix,
                Step::Translate(x, y) => {
                    [1.0, 0.0, 0.0, 1.0, x.resolve(width)?, y.resolve(height)?]
                }
            };
            result = multiply(result, matrix)?;
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Origin([Length; 2]);

#[inline(never)]
fn keyword(text: &str, horizontal: bool) -> Option<Length> {
    match text.to_ascii_lowercase().as_str() {
        "center" => Some(Length::CENTER),
        "left" if horizontal => Some(Length::ZERO),
        "top" if !horizontal => Some(Length::ZERO),
        "right" if horizontal => Some(Length {
            points: 0.0,
            percent: 1.0,
        }),
        "bottom" if !horizontal => Some(Length {
            points: 0.0,
            percent: 1.0,
        }),
        _ => None,
    }
}

#[inline(never)]
fn position(tokens: &[&str], font_size: f32, offsets: bool) -> Result<Origin> {
    if let [one] = tokens {
        if let Some(y) = keyword(one, false) {
            if !one.eq_ignore_ascii_case("center") {
                return Ok(Origin([Length::CENTER, y]));
            }
        }
        return Ok(Origin([
            keyword(one, true)
                .map(Ok)
                .unwrap_or_else(|| Length::parse(one, font_size))?,
            Length::CENTER,
        ]));
    }
    if let [x, y] = tokens {
        if let (Some(x), Some(y)) = (keyword(x, true), keyword(y, false)) {
            return Ok(Origin([x, y]));
        }
        if let (Some(y), Some(x)) = (keyword(x, false), keyword(y, true)) {
            return Ok(Origin([x, y]));
        }
        let x = keyword(x, true)
            .map(Ok)
            .unwrap_or_else(|| Length::parse(x, font_size))?;
        let y = keyword(y, false)
            .map(Ok)
            .unwrap_or_else(|| Length::parse(y, font_size))?;
        return Ok(Origin([x, y]));
    }
    if offsets && (3..=4).contains(&tokens.len()) {
        let group = |tokens: &[&str], horizontal| -> Result<Length> {
            let first = tokens.first().ok_or_else(invalid)?;
            let mut edge = keyword(first, horizontal).ok_or_else(invalid)?;
            if let [_, offset] = tokens {
                if first.eq_ignore_ascii_case("center") {
                    return Err(invalid());
                }
                let offset = Length::parse(offset, font_size)?;
                let sign = if edge.percent == 1.0 { -1.0 } else { 1.0 };
                edge.points += sign * offset.points;
                edge.percent += sign * offset.percent;
            } else if tokens.len() != 1 {
                return Err(invalid());
            }
            Ok(edge)
        };
        for split in 1..tokens.len() {
            for reverse in [false, true] {
                if let (Ok(a), Ok(b)) = (
                    group(&tokens[..split], !reverse),
                    group(&tokens[split..], reverse),
                ) {
                    return Ok(Origin(if reverse { [b, a] } else { [a, b] }));
                }
            }
        }
    }
    Err(invalid())
}

impl Origin {
    #[inline(never)]
    pub(crate) fn parse(text: &str, font_size: f32) -> Result<Self> {
        let tokens = words(source(text)?);
        let xy = if tokens.len() == 3 {
            if quantity(tokens[2])?.1 == "%" {
                return Err(invalid());
            }
            let z = Length::parse(tokens[2], font_size)?;
            if z != Length::ZERO {
                return Err(invalid());
            }
            &tokens[..2]
        } else {
            &tokens
        };
        position(xy, font_size, false)
    }

    #[inline(never)]
    pub(crate) fn resolve(self, width: f32, height: f32) -> Result<[f32; 2]> {
        Ok([self.0[0].resolve(width)?, self.0[1].resolve(height)?])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Radius {
    Length(Length),
    Closest,
    Farthest,
}

impl Radius {
    fn parse(text: &str, font_size: f32) -> Result<Self> {
        match text.to_ascii_lowercase().as_str() {
            "closest-side" => Ok(Self::Closest),
            "farthest-side" => Ok(Self::Farthest),
            _ => Ok(Self::Length(Length::nonnegative(text, font_size)?)),
        }
    }

    fn resolve(self, size: f32, center: f32) -> Result<f32> {
        match self {
            Self::Length(length) => length.resolve(size),
            Self::Closest => bounded(center.abs().min((size - center).abs())),
            Self::Farthest => bounded(center.abs().max((size - center).abs())),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClipPath {
    None,
    BorderBox,
    Inset {
        edges: [Length; 4],
        radii: [[Length; 4]; 2],
    },
    Circle(Radius, Origin),
    Ellipse([Radius; 2], Origin),
    Polygon(Vec<[Length; 2]>, bool),
    Path(Vec<PathOp>, bool),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedClip {
    pub(crate) operations: Vec<PathOp>,
    pub(crate) outline: Option<([f32; 4], crate::Radii)>,
    pub(crate) even_odd: bool,
}

#[inline(never)]
fn four(tokens: &[&str], font_size: f32, positive: bool) -> Result<[Length; 4]> {
    let mut values = Vec::new();
    for token in tokens {
        values.push(if positive {
            Length::nonnegative(token, font_size)?
        } else {
            Length::parse(token, font_size)?
        });
    }
    Ok(match values.as_slice() {
        [a] => [*a; 4],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => return Err(invalid()),
    })
}

fn fill_rule(body: &str) -> (&str, bool) {
    if let Some((head, tail)) = body.split_once(',') {
        if trim(head).eq_ignore_ascii_case("evenodd") {
            return (trim(tail), true);
        }
        if trim(head).eq_ignore_ascii_case("nonzero") {
            return (trim(tail), false);
        }
    }
    (trim(body), false)
}

fn css_string(text: &str) -> Result<String> {
    let mut chars = text.chars();
    let quote = chars
        .next()
        .filter(|c| matches!(c, '\'' | '"'))
        .ok_or_else(invalid)?;
    let mut decoded = String::new();
    while let Some(c) = chars.next() {
        if c == quote {
            return if trim(chars.as_str()).is_empty() {
                Ok(decoded)
            } else {
                Err(invalid())
            };
        }
        if matches!(c, '\n' | '\r' | '\x0c' | '\0') {
            return Err(invalid());
        }
        if c != '\\' {
            decoded.push(c);
            continue;
        }
        let c = chars.next().ok_or_else(invalid)?;
        if c == '\r' {
            if chars.as_str().starts_with('\n') {
                chars.next();
            }
            continue;
        }
        if matches!(c, '\n' | '\x0c') {
            continue;
        }
        if c.is_ascii_hexdigit() {
            let mut digits = String::from(c);
            while digits.len() < 6 && chars.as_str().starts_with(|c: char| c.is_ascii_hexdigit()) {
                digits.push(chars.next().unwrap());
            }
            let value = u32::from_str_radix(&digits, 16).map_err(|_| invalid())?;
            let c = char::from_u32(value)
                .filter(|c| *c != '\0')
                .ok_or_else(invalid)?;
            decoded.push(c);
            if chars
                .as_str()
                .starts_with(|c: char| c.is_ascii() && crate::css::whitespace(c as u8))
            {
                let c = chars.next();
                if c == Some('\r') && chars.as_str().starts_with('\n') {
                    chars.next();
                }
            }
        } else {
            decoded.push(c);
        }
    }
    Err(invalid())
}

impl ClipPath {
    #[inline(never)]
    pub(crate) fn parse(text: &str, font_size: f32) -> Result<Self> {
        let text = source(text)?;
        if text.eq_ignore_ascii_case("none") {
            return Ok(Self::None);
        }
        if text.eq_ignore_ascii_case("border-box") {
            return Ok(Self::BorderBox);
        }
        let prefix_box = text
            .get(..10)
            .is_some_and(|s| s.eq_ignore_ascii_case("border-box"))
            && text
                .as_bytes()
                .get(10)
                .is_some_and(|b| crate::css::whitespace(*b));
        let text = if prefix_box { trim(&text[10..]) } else { text };
        let (name, body, suffix) = function(text)?;
        if !suffix.is_empty() && (prefix_box || !suffix.eq_ignore_ascii_case("border-box")) {
            return Err(invalid());
        }
        match name.to_ascii_lowercase().as_str() {
            "inset" => {
                let tokens = words(body);
                let round = tokens.iter().position(|s| s.eq_ignore_ascii_case("round"));
                let edges = four(&tokens[..round.unwrap_or(tokens.len())], font_size, false)?;
                let radii = if let Some(round) = round {
                    let joined = tokens[round + 1..].join(" ");
                    let parts: Vec<_> = joined.split('/').collect();
                    let x = four(&words(parts[0]), font_size, true)?;
                    let y = match parts.as_slice() {
                        [_] => x,
                        [_, y] => four(&words(y), font_size, true)?,
                        _ => return Err(invalid()),
                    };
                    [x, y]
                } else {
                    [[Length::ZERO; 4]; 2]
                };
                Ok(Self::Inset { edges, radii })
            }
            "circle" | "ellipse" => {
                let tokens = words(body);
                let at = tokens.iter().position(|s| s.eq_ignore_ascii_case("at"));
                let center = if let Some(at) = at {
                    position(&tokens[at + 1..], font_size, true)?
                } else {
                    Origin([Length::CENTER; 2])
                };
                let sizes = &tokens[..at.unwrap_or(tokens.len())];
                if name.eq_ignore_ascii_case("circle") {
                    let radius = match sizes {
                        [] => Radius::Closest,
                        [r] => Radius::parse(r, font_size)?,
                        _ => return Err(invalid()),
                    };
                    Ok(Self::Circle(radius, center))
                } else {
                    let radii = match sizes {
                        [] => [Radius::Closest; 2],
                        [x, y] => [Radius::parse(x, font_size)?, Radius::parse(y, font_size)?],
                        _ => return Err(invalid()),
                    };
                    Ok(Self::Ellipse(radii, center))
                }
            }
            "polygon" => {
                let (body, even_odd) = fill_rule(body);
                let mut points = Vec::new();
                for pair in body.split(',') {
                    if points.len() >= MAX_ITEMS {
                        return Err(invalid());
                    }
                    let tokens = words(pair);
                    let [x, y] = tokens.as_slice() else {
                        return Err(invalid());
                    };
                    points.push([Length::parse(x, font_size)?, Length::parse(y, font_size)?]);
                }
                if points.len() < 3 {
                    return Err(invalid());
                }
                Ok(Self::Polygon(points, even_odd))
            }
            "path" => {
                let (body, even_odd) = fill_rule(body);
                let mut operations = parse_path(&css_string(body)?)?;
                for op in &mut operations {
                    match op {
                        PathOp::Move(p) | PathOp::Line(p) => {
                            for v in p {
                                *v *= 0.75;
                            }
                        }
                        PathOp::Cubic(p) => {
                            for v in p {
                                *v *= 0.75;
                            }
                        }
                        PathOp::Close => {}
                    }
                }
                Ok(Self::Path(operations, even_odd))
            }
            _ => Err(invalid()),
        }
    }

    pub(crate) fn resolve(&self, width: f32, height: f32) -> Result<Option<ResolvedClip>> {
        let mut operations = Vec::new();
        let mut outline = None;
        let mut even_odd = false;
        match self {
            Self::None => return Ok(None),
            Self::BorderBox => {
                outline = Some(([0.0, 0.0, width, height], [[0.0; 4]; 2]));
            }
            Self::Path(path, rule) => {
                operations = path.clone();
                even_odd = *rule;
            }
            Self::Polygon(points, rule) => {
                even_odd = *rule;
                for (index, point) in points.iter().enumerate() {
                    let point = [point[0].resolve(width)?, point[1].resolve(height)?];
                    operations.push(if index == 0 {
                        PathOp::Move(point)
                    } else {
                        PathOp::Line(point)
                    });
                }
                operations.push(PathOp::Close);
            }
            Self::Circle(radius, origin) => {
                let [x, y] = origin.resolve(width, height)?;
                let r = match radius {
                    Radius::Length(length) => {
                        length.resolve(libm::sqrtf((width * width + height * height) / 2.0))?
                    }
                    Radius::Closest => x
                        .abs()
                        .min((width - x).abs())
                        .min(y.abs())
                        .min((height - y).abs()),
                    Radius::Farthest => x
                        .abs()
                        .max((width - x).abs())
                        .max(y.abs())
                        .max((height - y).abs()),
                };
                outline = Some(([x - r, y - r, 2.0 * r, 2.0 * r], [[r; 4]; 2]));
            }
            Self::Ellipse(radii, origin) => {
                let [x, y] = origin.resolve(width, height)?;
                let (rx, ry) = (radii[0].resolve(width, x)?, radii[1].resolve(height, y)?);
                outline = Some(([x - rx, y - ry, 2.0 * rx, 2.0 * ry], [[rx; 4], [ry; 4]]));
            }
            Self::Inset { edges, radii } => {
                let mut edges = [
                    edges[0].resolve(height)?,
                    edges[1].resolve(width)?,
                    edges[2].resolve(height)?,
                    edges[3].resolve(width)?,
                ];
                for (first, second, size) in [(0, 2, height), (3, 1, width)] {
                    let sum = edges[first] + edges[second];
                    if sum > size {
                        let scale = size / sum;
                        edges[first] *= scale;
                        edges[second] *= scale;
                    }
                }
                let (w, h) = (
                    (width - edges[3] - edges[1]).max(0.0),
                    (height - edges[0] - edges[2]).max(0.0),
                );
                let mut r = [[0.0; 4]; 2];
                for axis in 0..2 {
                    for corner in 0..4 {
                        r[axis][corner] =
                            radii[axis][corner].resolve(if axis == 0 { width } else { height })?;
                    }
                }
                let mut scale = 1.0_f32;
                for (size, sum) in [
                    (w, r[0][0] + r[0][1]),
                    (w, r[0][3] + r[0][2]),
                    (h, r[1][0] + r[1][3]),
                    (h, r[1][1] + r[1][2]),
                ] {
                    if sum > 0.0 {
                        scale = scale.min(size / sum);
                    }
                }
                for axis in &mut r {
                    for radius in axis {
                        *radius *= scale;
                    }
                }
                outline = Some(([edges[3], edges[0], w, h], r));
            }
        }
        let outline = if let Some(([x, y, w, h], mut radii)) = outline {
            if w == 0.0 || h == 0.0 {
                None
            } else {
                for coordinate in [x, y, x + w, y + h] {
                    bounded(coordinate)?;
                }
                let [horizontal, vertical] = &mut radii;
                for (rx, ry) in horizontal.iter_mut().zip(vertical) {
                    if *rx == 0.0 || *ry == 0.0 {
                        *rx = 0.0;
                        *ry = 0.0;
                    }
                }
                Some((
                    [x, y, w, h],
                    crate::Radii {
                        values: std::array::from_fn(|i| radii[i % 2][i / 2]),
                        percent: 0,
                    },
                ))
            }
        } else {
            None
        };
        for op in &operations {
            match op {
                PathOp::Move(p) | PathOp::Line(p) => {
                    for v in p {
                        bounded(*v)?;
                    }
                }
                PathOp::Cubic(p) => {
                    for v in p {
                        bounded(*v)?;
                    }
                }
                PathOp::Close => {}
            }
        }
        Ok(Some(ResolvedClip {
            operations,
            outline,
            even_odd,
        }))
    }
}

/// Validated CSS box effects. Percentages resolve against the final border box.
/// Values are limited to 64 KiB, 1024 functions/vertices, and magnitudes of
/// 1,000,000 for CSS scalars, resolved point coordinates and matrix entries.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxEffects {
    transform: Transform,
    origin: Origin,
    clip: ClipPath,
    overflow: bool,
    pub(crate) image: bool,
}

impl BoxEffects {
    pub fn parse(
        transform: &str,
        origin: &str,
        clip: &str,
        overflow: bool,
        font_size: f32,
    ) -> std::result::Result<Self, String> {
        Self::parse_inner(transform, origin, clip, overflow, font_size, false)
            .map(std::rc::Rc::unwrap_or_clone)
            .map_err(String::from)
    }

    #[inline(never)]
    pub(crate) fn parse_inner(
        transform: &str,
        origin: &str,
        clip: &str,
        overflow: bool,
        font_size: f32,
        image: bool,
    ) -> Result<std::rc::Rc<Self>> {
        if !(0.0..=LIMIT).contains(&font_size) {
            return Err("invalid effects font size".into());
        }
        let error =
            |property, value: &str, error| format!("invalid {property}: {value:?}: {error}");
        Ok(std::rc::Rc::new(Self {
            transform: Transform::parse(transform, font_size)
                .map_err(|e| error("transform", transform, e))?,
            origin: Origin::parse(origin, font_size)
                .map_err(|e| error("transform-origin", origin, e))?,
            clip: ClipPath::parse(clip, font_size).map_err(|e| error("clip-path", clip, e))?,
            overflow,
            image,
        }))
    }

    pub(crate) fn capacity(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.transform.steps.capacity() * std::mem::size_of::<Step>()
            + match &self.clip {
                ClipPath::Polygon(points, _) => {
                    points.capacity() * std::mem::size_of::<[Length; 2]>()
                }
                ClipPath::Path(ops, _) => ops.capacity() * std::mem::size_of::<PathOp>(),
                _ => 0,
            }
    }

    pub(crate) fn transformed(&self) -> bool {
        self.transform.specified
    }
    pub(crate) fn border_box(&self) -> bool {
        matches!(self.clip, ClipPath::BorderBox)
    }

    pub(crate) fn overflow(&self) -> bool {
        self.overflow
    }

    pub(crate) fn image(&self) -> bool {
        self.image
    }

    pub(crate) fn transform(&self, width: f32, height: f32) -> Result<[f32; 6]> {
        dimensions(width, height)?;
        let mut matrix = self.transform.resolve(width, height)?;
        let [x, y] = self.origin.resolve(width, height)?;
        matrix[4] = bounded(matrix[4] + x - matrix[0] * x - matrix[2] * y)?;
        matrix[5] = bounded(matrix[5] + y - matrix[1] * x - matrix[3] * y)?;
        Ok(matrix)
    }

    pub(crate) fn clip(&self, width: f32, height: f32) -> Result<Option<ResolvedClip>> {
        dimensions(width, height)?;
        self.clip.resolve(width, height)
    }
}

#[inline(never)]
fn dimensions(width: f32, height: f32) -> Result<()> {
    bounded(width)?;
    bounded(height)?;
    if width < 0.0 || height < 0.0 {
        Err(invalid())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effects(transform: &str, origin: &str, clip: &str) -> BoxEffects {
        BoxEffects::parse(transform, origin, clip, false, 12.0).unwrap()
    }

    fn near(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (actual, expected) in actual.iter().zip(expected) {
            assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
        }
    }

    #[test]
    fn given_transform_functions_when_resolving_then_postmultiplies_about_the_final_origin() {
        let effect = effects(
            "translate(25%, 1em) scale(2) rotate(90deg)",
            "50% 50%",
            "none",
        );
        assert!(effect.transformed());
        near(
            &effect.transform(200.0, 100.0).unwrap(),
            &[0.0, 2.0, -2.0, 0.0, 250.0, -138.0],
        );
        near(
            &effects("matrix(1,0,0,1,20,-4)", "0 0", "none")
                .transform(100.0, 50.0)
                .unwrap(),
            &[1.0, 0.0, 0.0, 1.0, 15.0, -3.0],
        );
    }

    #[test]
    fn given_singular_transforms_when_parsing_then_preserves_them_for_the_painter() {
        for text in [
            "scale(0)",
            "matrix(1,2,2,4,0,0)",
            "scaleX(0) rotate(.25turn)",
        ] {
            let matrix = effects(text, "50% 50%", "none")
                .transform(80.0, 40.0)
                .unwrap();
            assert!((matrix[0] * matrix[3] - matrix[1] * matrix[2]).abs() < 0.001);
        }
        assert!(!effects("none", "50% 50%", "none").transformed());
    }

    #[test]
    fn given_css_lengths_when_parsing_then_reuses_point_units_and_defers_percentages() {
        for (text, expected) in [
            ("-2px", -1.5),
            ("1EM", 12.0),
            ("2rem", 24.0),
            ("1in", 72.0),
            ("2.54cm", 72.0),
            ("25.4mm", 72.0),
            ("101.6q", 72.0),
            ("6pc", 72.0),
            ("1e2%", 200.0),
            ("-0", 0.0),
        ] {
            near(
                &[Length::parse(text, 12.0).unwrap().resolve(200.0).unwrap()],
                &[expected],
            );
        }
        for text in [
            "1",
            "1.px",
            "1.e2px",
            "1 px",
            "NaNpx",
            "infpx",
            "1e99px",
            "calc(1px + 1%)",
            "1vh",
        ] {
            assert!(Length::parse(text, 12.0).is_err(), "{text}");
        }
    }

    #[test]
    fn given_origin_keywords_when_parsing_then_obeys_axis_order_and_validates_unused_origins() {
        for (text, expected) in [
            ("top", [100.0, 0.0]),
            ("bottom right", [200.0, 100.0]),
            ("center left", [0.0, 50.0]),
            ("25% -2pt 0em", [50.0, -2.0]),
        ] {
            near(
                &Origin::parse(text, 12.0)
                    .unwrap()
                    .resolve(200.0, 100.0)
                    .unwrap(),
                &expected,
            );
        }
        for text in ["top 1px", "left right", "left 1px top", "0 0 1px", "0 0 0%"] {
            let error = BoxEffects::parse("none", text, "none", false, 12.0).unwrap_err();
            assert!(error.contains("transform-origin"), "{error}");
            assert!(error.contains(text), "{error}");
        }
    }

    #[test]
    fn given_edge_offsets_when_resolving_shape_positions_then_offsets_from_the_named_edges() {
        for text in ["right 10% bottom 2pt", "bottom 2pt right 10%"] {
            let clip = effects("none", "center", &format!("ellipse(2pt 3pt at {text})"))
                .clip(200.0, 100.0)
                .unwrap()
                .unwrap();
            let (rect, radii) = clip.outline.unwrap();
            assert_eq!(rect, [178.0, 95.0, 4.0, 6.0]);
            assert_eq!(radii.values, [2.0, 3.0, 2.0, 3.0, 2.0, 3.0, 2.0, 3.0]);
        }
        let clip = effects("none", "center", "circle(1pt at left 10px center)")
            .clip(200.0, 100.0)
            .unwrap()
            .unwrap();
        let (rect, radii) = clip.outline.unwrap();
        assert_eq!(rect, [6.5, 49.0, 2.0, 2.0]);
        assert_eq!(radii.values, [1.0; 8]);
    }

    #[test]
    fn given_circle_percentages_when_resolving_then_uses_the_normalized_box_diagonal() {
        let clip = effects("none", "center", "circle(50% at 25% 75%)")
            .clip(80.0, 60.0)
            .unwrap()
            .unwrap();
        let (rect, radii) = clip.outline.unwrap();
        let radius = libm::sqrtf(5000.0) * 0.5;
        near(
            &rect,
            &[20.0 - radius, 45.0 - radius, 2.0 * radius, 2.0 * radius],
        );
        near(&radii.values, &[radius; 8]);
        assert_eq!(radii.percent, 0);
        assert!(clip.operations.is_empty());
        let clip = effects(
            "none",
            "center",
            "ellipse(farthest-side closest-side at 25% 75%)",
        )
        .clip(80.0, 60.0)
        .unwrap()
        .unwrap();
        let (rect, radii) = clip.outline.unwrap();
        assert_eq!(rect, [-40.0, 30.0, 120.0, 30.0]);
        assert_eq!(
            radii.values,
            [60.0, 15.0, 60.0, 15.0, 60.0, 15.0, 60.0, 15.0]
        );
    }

    #[test]
    fn given_inset_radii_when_resolving_then_scales_overlapping_corners_against_the_inset_rectangle(
    ) {
        let clip = effects(
            "none",
            "center",
            "inset(10% 20% round 50% / 25%) border-box",
        )
        .clip(200.0, 100.0)
        .unwrap()
        .unwrap();
        // Radius percentages use the reference box; the resulting 100pt radii shrink to 60pt.
        let (rect, radii) = clip.outline.unwrap();
        assert_eq!(rect, [40.0, 10.0, 120.0, 80.0]);
        near(
            &radii.values,
            &[60.0, 15.0, 60.0, 15.0, 60.0, 15.0, 60.0, 15.0],
        );
        assert_eq!(radii.percent, 0);
        assert!(clip.operations.is_empty());
    }

    #[test]
    fn given_empty_shapes_when_resolving_then_returns_an_empty_clip_instead_of_no_clip() {
        for text in [
            "inset(75% 0 50% 0)",
            "circle(0)",
            "ellipse(0 1pt)",
            "path('')",
        ] {
            let clip = effects("none", "center", text)
                .clip(100.0, 100.0)
                .unwrap()
                .unwrap();
            assert!(clip.operations.is_empty(), "{text}");
            assert!(clip.outline.is_none(), "{text}");
        }
        assert!(effects("none", "center", "none")
            .clip(100.0, 100.0)
            .unwrap()
            .is_none());
        assert!(effects("none", "center", "border-box").border_box());
    }

    #[test]
    fn given_outline_extents_when_resolving_then_limits_coordinates_instead_of_dimensions() {
        for text in [
            "circle(1000000pt at 0 0)",
            "ellipse(1000000pt 1000000pt at 0 0)",
            "inset(-1000000pt)",
        ] {
            let clip = effects("none", "center", text)
                .clip(0.0, 0.0)
                .unwrap()
                .unwrap();
            assert_eq!(
                clip.outline.unwrap().0,
                [-1_000_000.0, -1_000_000.0, 2_000_000.0, 2_000_000.0],
                "{text}"
            );
            assert!(clip.operations.is_empty());
        }
        let clip = effects("none", "center", "border-box")
            .clip(1_000_000.0, 1_000_000.0)
            .unwrap()
            .unwrap();
        assert_eq!(
            clip.outline,
            Some((
                [0.0, 0.0, 1_000_000.0, 1_000_000.0],
                crate::Radii {
                    values: [0.0; 8],
                    percent: 0
                }
            ))
        );
        for text in [
            "circle(1pt at -1000000pt 0)",
            "circle(1pt at 1000000pt 0)",
            "ellipse(1pt 1pt at 0 -1000000pt)",
            "ellipse(1pt 1pt at 0 1000000pt)",
            "inset(-1pt)",
        ] {
            assert!(
                effects("none", "center", text)
                    .clip(1_000_000.0, 1_000_000.0)
                    .is_err(),
                "{text}"
            );
        }
    }

    #[test]
    fn given_polygons_and_paths_when_resolving_then_preserves_fill_rules_and_css_pixel_coordinates()
    {
        let clip = effects("none", "center", "polygon(evenodd, 0 0, 100% 0, 50% 100%)")
            .clip(100.0, 80.0)
            .unwrap()
            .unwrap();
        assert!(clip.even_odd);
        assert!(clip.outline.is_none());
        assert_eq!(
            clip.operations,
            vec![
                PathOp::Move([0.0, 0.0]),
                PathOp::Line([100.0, 0.0]),
                PathOp::Line([50.0, 80.0]),
                PathOp::Close
            ]
        );
        let clip = effects("none", "center", r#"path(evenodd, '\4d 0 0 L20 0 20 20z')"#)
            .clip(1.0, 1.0)
            .unwrap()
            .unwrap();
        assert!(clip.even_odd);
        assert!(clip.outline.is_none());
        assert_eq!(
            clip.operations,
            vec![
                PathOp::Move([0.0, 0.0]),
                PathOp::Line([15.0, 0.0]),
                PathOp::Line([15.0, 15.0]),
                PathOp::Close
            ]
        );
    }

    #[test]
    fn given_unsupported_or_malformed_effects_when_parsing_then_reports_the_property_and_value() {
        for text in [
            "translate(1px 2px)",
            "translate(1px,)",
            "rotate(1)",
            "rotate(1deg,2deg)",
            "matrix(1,0,0,1,0)",
            "scale(NaN)",
            "scale(1000001)",
            "translateZ(0)",
            "url(x)",
            "skew(90deg)",
        ] {
            let error = BoxEffects::parse(text, "center", "none", false, 12.0).unwrap_err();
            assert!(error.contains("transform:"), "{error}");
            assert!(error.contains(text), "{error}");
        }
        for text in [
            "url('#id')",
            "circle(1px) content-box",
            "padding-box",
            "circle(-1px)",
            "ellipse(1px)",
            "circle(1pt at top 10px)",
            "inset(1px round -1px)",
            "inset(1px round 2px /)",
            "polygon(0 0, 1pt 1pt)",
            "path('L0 0')",
            "path('M0 0',evenodd)",
            "border-box circle(1px) border-box",
            "éééééé(0)",
        ] {
            let error = BoxEffects::parse("none", "center", text, false, 12.0).unwrap_err();
            assert!(error.contains("clip-path:"), "{error}");
        }
        assert!(effects("translate(1000%)", "center", "none")
            .transform(1_000_000.0, 100.0)
            .is_err());
        assert!(effects("none", "center", "none")
            .clip(f32::NAN, 100.0)
            .is_err());
    }

    #[test]
    fn given_svg_shorthand_and_relative_commands_when_parsing_then_normalizes_curves_and_reopens_closed_subpaths(
    ) {
        let path = parse_path("M1 2h3v4q3 3 6 0t6 0c1 2 3 4 5 6s3 4 5 6z l2 3").unwrap();
        assert_eq!(
            &path[..3],
            &[
                PathOp::Move([1.0, 2.0]),
                PathOp::Line([4.0, 2.0]),
                PathOp::Line([4.0, 6.0])
            ]
        );
        assert_eq!(path[3], PathOp::Cubic([6.0, 8.0, 8.0, 8.0, 10.0, 6.0]));
        assert_eq!(path[4], PathOp::Cubic([12.0, 4.0, 14.0, 4.0, 16.0, 6.0]));
        assert_eq!(path[6], PathOp::Cubic([23.0, 14.0, 24.0, 16.0, 26.0, 18.0]));
        assert_eq!(
            &path[7..],
            &[
                PathOp::Close,
                PathOp::Move([1.0, 2.0]),
                PathOp::Line([3.0, 5.0])
            ]
        );
    }

    #[test]
    fn given_svg_arcs_when_parsing_then_converts_quarter_turns_and_preserves_the_endpoint() {
        let path = parse_path("M10 0A10 10 0 0 1 0 10").unwrap();
        assert_eq!(path.len(), 2);
        let PathOp::Cubic(points) = &path[1] else {
            panic!()
        };
        near(points, &[10.0, 5.522_848, 5.522_848, 10.0, 0.0, 10.0]);
        let path = parse_path("M0 0 a1 2 45 0110 10 A0 2 0 0 0 20 20").unwrap();
        assert_eq!(path.last(), Some(&PathOp::Line([20.0, 20.0])));
        assert_eq!(
            parse_path("M0 0 A1 1 0 1 1 0 0").unwrap(),
            vec![PathOp::Move([0.0, 0.0])]
        );
    }

    #[test]
    fn given_invalid_svg_path_data_when_parsing_then_rejects_partial_commands_and_bad_separators() {
        for text in [
            "L0 0",
            "M,0 0",
            "M0,,0",
            "M0 0,",
            "M0 0,L1 1",
            "M0 0X1 1",
            "M0 0L1",
            "M0 0A1 1 0 2 0 1 1",
            "M0 0A-1 1 0 0 0 1 1",
            "M0 0z1 2",
            "M0 0L1e99 0",
            "M0 0L1000001 0",
            "M0 0LNaN 0",
        ] {
            assert!(parse_path(text).is_err(), "{text}");
        }
    }
}
