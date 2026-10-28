use super::super::{js_parse, js_space, js_trim, Value};
use crate::css::Message;

/// Sorted by name, for a binary search.
#[rustfmt::skip]
static NAMED: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff), ("antiquewhite", 0xfaebd7), ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4), ("azure", 0xf0ffff), ("beige", 0xf5f5dc), ("bisque", 0xffe4c4),
    ("black", 0x000000), ("blanchedalmond", 0xffebcd), ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2), ("brown", 0xa52a2a), ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0), ("chartreuse", 0x7fff00), ("chocolate", 0xd2691e),
    ("coral", 0xff7f50), ("cornflowerblue", 0x6495ed), ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c), ("cyan", 0x00ffff), ("darkblue", 0x00008b), ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b), ("darkgray", 0xa9a9a9), ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9), ("darkkhaki", 0xbdb76b), ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f), ("darkorange", 0xff8c00), ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000), ("darksalmon", 0xe9967a), ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b), ("darkslategray", 0x2f4f4f), ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1), ("darkviolet", 0x9400d3), ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff), ("dimgray", 0x696969), ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff), ("firebrick", 0xb22222), ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22), ("fuchsia", 0xff00ff), ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff), ("gold", 0xffd700), ("goldenrod", 0xdaa520), ("gray", 0x808080),
    ("green", 0x008000), ("greenyellow", 0xadff2f), ("grey", 0x808080), ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4), ("indianred", 0xcd5c5c), ("indigo", 0x4b0082), ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c), ("lavender", 0xe6e6fa), ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00), ("lemonchiffon", 0xfffacd), ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080), ("lightcyan", 0xe0ffff), ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3), ("lightgreen", 0x90ee90), ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1), ("lightsalmon", 0xffa07a), ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa), ("lightslategray", 0x778899), ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de), ("lightyellow", 0xffffe0), ("lime", 0x00ff00),
    ("limegreen", 0x32cd32), ("linen", 0xfaf0e6), ("magenta", 0xff00ff), ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa), ("mediumblue", 0x0000cd), ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db), ("mediumseagreen", 0x3cb371), ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a), ("mediumturquoise", 0x48d1cc), ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970), ("mintcream", 0xf5fffa), ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5), ("navajowhite", 0xffdead), ("navy", 0x000080), ("oldlace", 0xfdf5e6),
    ("olive", 0x808000), ("olivedrab", 0x6b8e23), ("orange", 0xffa500), ("orangered", 0xff4500),
    ("orchid", 0xda70d6), ("palegoldenrod", 0xeee8aa), ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee), ("palevioletred", 0xdb7093), ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9), ("peru", 0xcd853f), ("pink", 0xffc0cb), ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6), ("purple", 0x800080), ("rebeccapurple", 0x663399), ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f), ("royalblue", 0x4169e1), ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072), ("sandybrown", 0xf4a460), ("seagreen", 0x2e8b57), ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d), ("silver", 0xc0c0c0), ("skyblue", 0x87ceeb), ("slateblue", 0x6a5acd),
    ("slategray", 0x708090), ("slategrey", 0x708090), ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f), ("steelblue", 0x4682b4), ("tan", 0xd2b48c), ("teal", 0x008080),
    ("thistle", 0xd8bfd8), ("tomato", 0xff6347), ("turquoise", 0x40e0d0), ("violet", 0xee82ee),
    ("wheat", 0xf5deb3), ("white", 0xffffff), ("whitesmoke", 0xf5f5f5), ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

pub type Rgb = [u8; 3];

fn opaque(alpha: f64) -> Result<(), Message> {
    if (alpha - 1.0).abs() > 1e-6 {
        return Err("a colour with alpha is not supported; use the opacity property".into());
    }
    Ok(())
}

/// `50%` is 0.5, anything else the number divided by `scale`; NaN for no number.
fn fraction(token: &str, scale: f64) -> f64 {
    match token.strip_suffix('%') {
        Some(amount) => js_parse(amount) / 100.0,
        None => js_parse(token) / scale,
    }
}

/// A channel from 0 to 1 as a byte, NaN kept so the caller can reject it.
fn byte(value: f64) -> f64 {
    // Math.round and f64::round agree here: the value is never negative.
    (value.clamp(0.0, 1.0) * 255.0).round()
}

/// `Math.min` and `Math.max`, which return NaN when either side is NaN.
fn min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.min(right)
    }
}
fn max(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        f64::NAN
    } else {
        left.max(right)
    }
}

fn hex(source: &str) -> Result<Option<Rgb>, Message> {
    let Some(digits) = source.strip_prefix('#') else {
        return Ok(None);
    };
    let valid = digits
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !valid || ![3, 4, 6, 8].contains(&digits.len()) {
        return Ok(None);
    }
    let digits: String = if digits.len() <= 4 {
        digits.chars().flat_map(|digit| [digit, digit]).collect()
    } else {
        digits.into()
    };
    let pair = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).unwrap_or(0);
    if digits.len() == 8 {
        opaque(f64::from(pair(6)) / 255.0)?;
    }
    Ok(Some([pair(0), pair(2), pair(4)]))
}

fn hsl(hue: f64, saturation: f64, lightness: f64) -> [f64; 3] {
    let h = ((hue % 360.0) + 360.0) % 360.0;
    let a = saturation * min(lightness, 1.0 - lightness);
    let channel = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        byte(lightness - a * max(-1.0, min(min(k - 3.0, 9.0 - k), 1.0)))
    };
    [channel(0.0), channel(8.0), channel(4.0)]
}

/// OKLab to sRGB bytes, clipped per channel to the gamut; see https://bottosson.github.io/posts/oklab/.
/// libm, not the platform's, keeps the bytes identical everywhere.
fn oklab(lightness: f64, a: f64, b: f64) -> [f64; 3] {
    let cube = |value: f64| libm::pow(value, 3.0);
    let l = cube(lightness + 0.3963377774 * a + 0.2158037573 * b);
    let m = cube(lightness - 0.1055613458 * a - 0.0638541728 * b);
    let s = cube(lightness - 0.0894841775 * a - 1.291485548 * b);
    let encode = |linear: f64| {
        byte(if linear <= 0.0031308 {
            12.92 * linear
        } else {
            1.055 * libm::pow(linear, 1.0 / 2.4) - 0.055
        })
    };
    [
        encode(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
        encode(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
        encode(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
    ]
}

/// An OKLab a/b axis or OKLCH chroma, where 100% is 0.4.
fn axis(token: &str) -> f64 {
    fraction(token, 1.0) * if token.ends_with('%') { 0.4 } else { 1.0 }
}

fn hue(token: &str) -> f64 {
    js_parse(token.strip_suffix("deg").unwrap_or(token))
}

fn functional(source: &str) -> Result<Option<Rgb>, Message> {
    let Some(open) = source.find('(') else {
        return Ok(None);
    };
    let kind = &source[..open];
    let Some(inner) = source[open + 1..].strip_suffix(')') else {
        return Ok(None);
    };
    let known = ["rgb", "rgba", "hsl", "hsla", "oklab", "oklch"].contains(&kind);
    // `.` in the TypeScript pattern stops at line terminators.
    if !known || inner.contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
        return Ok(None);
    }
    let parts: Vec<&str> = inner
        .split(|c: char| js_space(c) || c == ',' || c == '/')
        .filter(|part| !part.is_empty())
        .collect();
    let [first, second, third, rest @ ..] = parts.as_slice() else {
        return Ok(None);
    };
    if rest.len() > 1 {
        return Ok(None);
    }
    if let Some(alpha) = rest.first() {
        opaque(fraction(alpha, 1.0))?;
    }
    let rgb = if kind.starts_with("rgb") {
        [first, second, third].map(|token| byte(fraction(token, 255.0)))
    } else if kind.starts_with("hsl") {
        hsl(hue(first), fraction(second, 100.0), fraction(third, 100.0))
    } else if kind == "oklab" {
        oklab(fraction(first, 1.0), axis(second), axis(third))
    } else {
        let angle = hue(third) * std::f64::consts::PI / 180.0;
        oklab(
            fraction(first, 1.0),
            axis(second) * libm::cos(angle),
            axis(second) * libm::sin(angle),
        )
    };
    Ok((!rgb.iter().any(|channel| channel.is_nan())).then(|| rgb.map(|channel| channel as u8)))
}

/// sRGB bytes to OKLab; see https://bottosson.github.io/posts/oklab/.
fn to_oklab(rgb: Rgb) -> [f64; 3] {
    let [r, g, b] = rgb.map(|channel| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            libm::pow((value + 0.055) / 1.055, 2.4)
        }
    });
    let l = libm::cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    let m = libm::cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    let s = libm::cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    [
        0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
    ]
}

/// `color-mix(in srgb | oklab, <color> [<percentage>], <color> [<percentage>])` of opaque
/// colours, or `transparent` at 0%. A mix that keeps any transparency rejects, as alpha does.
fn mix(source: &str) -> Result<Option<Rgb>, Message> {
    let Some(inner) = source["color-mix(".len()..].strip_suffix(')') else {
        return Ok(None);
    };
    let mut depth = 0usize;
    let parts: Vec<&str> = inner
        .split(|c: char| {
            depth = match c {
                '(' => depth + 1,
                ')' => depth.saturating_sub(1),
                _ => depth,
            };
            c == ',' && depth == 0
        })
        .map(js_trim)
        .collect();
    let [space, first, second] = parts.as_slice() else {
        return Ok(None);
    };
    let oklab = match *space {
        "in srgb" => false,
        "in oklab" => true,
        _ => return Err(format!("unsupported color-mix space: {space}; use srgb or oklab").into()),
    };
    // A colour and an optional percentage, either first.
    let part = |text: &str| -> Result<(Option<Rgb>, Option<f64>), Message> {
        let (head, tail) = match text.rfind(|c: char| js_space(c)) {
            Some(at) if text.ends_with('%') && !text[at..].contains(')') => {
                (&text[..at], Some(&text[at + 1..]))
            }
            _ => match text.split_once(|c: char| js_space(c)) {
                Some((amount, rest)) if amount.ends_with('%') => (rest, Some(amount)),
                _ => (text, None),
            },
        };
        let amount = tail.map(|amount| fraction(amount, 1.0));
        if amount.is_some_and(|amount| !(0.0..=1.0).contains(&amount)) {
            return Err(format!("invalid color-mix percentage: {text}").into());
        }
        let head = js_trim(head);
        let rgb = (head != "transparent")
            .then(|| color(&Value::Text(head.into())))
            .transpose()?;
        Ok((rgb, amount))
    };
    let ((one, p1), (two, p2)) = (part(first)?, part(second)?);
    let (p1, p2) = match (p1, p2) {
        (None, None) => (0.5, 0.5),
        (Some(p1), None) => (p1, 1.0 - p1),
        (None, Some(p2)) => (1.0 - p2, p2),
        (Some(p1), Some(p2)) => (p1, p2),
    };
    let total = p1 + p2;
    if total <= 0.0 {
        return Err(format!("invalid color-mix percentages: {source}").into());
    }
    // Percentages summing under 100% scale the alpha down too.
    let alpha = |rgb: Option<Rgb>| f64::from(u8::from(rgb.is_some()));
    opaque((p1 * alpha(one) + p2 * alpha(two)) / total * total.min(1.0))?;
    let (weight, one, two) = match (one, two) {
        (Some(one), Some(two)) => (p2 / total, one, two),
        (Some(one), None) => (0.0, one, one),
        (None, Some(two)) => (1.0, two, two),
        (None, None) => unreachable!(),
    };
    let blend = |a: f64, b: f64| a + (b - a) * weight;
    let rgb = if oklab {
        let (a, b) = (to_oklab(one), to_oklab(two));
        self::oklab(blend(a[0], b[0]), blend(a[1], b[1]), blend(a[2], b[2]))
    } else {
        [0, 1, 2].map(|i| byte(blend(f64::from(one[i]), f64::from(two[i])) / 255.0))
    };
    Ok(Some(rgb.map(|channel| channel as u8)))
}

pub fn color(value: &Value) -> Result<Rgb, Message> {
    let Value::Text(text) = value else {
        return Err(format!("invalid color: {value}").into());
    };
    let source = js_trim(text).to_lowercase();
    let rgb = match NAMED.binary_search_by(|(name, _)| name.cmp(&source.as_str())) {
        Ok(index) => {
            let [_, red, green, blue] = NAMED[index].1.to_be_bytes();
            Some([red, green, blue])
        }
        Err(_) => match hex(&source)? {
            Some(rgb) => Some(rgb),
            None if source.starts_with("color-mix(") => mix(&source)?,
            None => functional(&source)?,
        },
    };
    rgb.ok_or_else(|| {
        if source == "transparent" {
            "transparent applies only to background and border color".into()
        } else {
            format!("unsupported color: {source}").into()
        }
    })
}
