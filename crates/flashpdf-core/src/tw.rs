//! Tailwind v4 default utilities for `tw`, with strict rejection and no build step.

use std::fmt::Write;

use crate::compile::InlineStyle;
use crate::css::{property, Cascaded, Value};

/// Font sizes with their line heights, as Tailwind's theme pairs them.
const TEXT: [(&str, &str, &str); 13] = [
    ("xs", "0.75rem", "calc(1 / 0.75)"),
    ("sm", "0.875rem", "calc(1.25 / 0.875)"),
    ("base", "1rem", "calc(1.5 / 1)"),
    ("lg", "1.125rem", "calc(1.75 / 1.125)"),
    ("xl", "1.25rem", "calc(1.75 / 1.25)"),
    ("2xl", "1.5rem", "calc(2 / 1.5)"),
    ("3xl", "1.875rem", "calc(2.25 / 1.875)"),
    ("4xl", "2.25rem", "calc(2.5 / 2.25)"),
    ("5xl", "3rem", "1"),
    ("6xl", "3.75rem", "1"),
    ("7xl", "4.5rem", "1"),
    ("8xl", "6rem", "1"),
    ("9xl", "8rem", "1"),
];
const WEIGHTS: [(&str, &str); 9] = [
    ("thin", "100"),
    ("extralight", "200"),
    ("light", "300"),
    ("normal", "400"),
    ("medium", "500"),
    ("semibold", "600"),
    ("bold", "700"),
    ("extrabold", "800"),
    ("black", "900"),
];
const TRACKING: [(&str, &str); 6] = [
    ("tighter", "-0.05em"),
    ("tight", "-0.025em"),
    ("normal", "0em"),
    ("wide", "0.025em"),
    ("wider", "0.05em"),
    ("widest", "0.1em"),
];
const LEADING: [(&str, &str); 6] = [
    ("none", "1"),
    ("tight", "1.25"),
    ("snug", "1.375"),
    ("normal", "1.5"),
    ("relaxed", "1.625"),
    ("loose", "2"),
];
const RADIUS: [(&str, &str); 10] = [
    ("", "0.25rem"),
    ("none", "0"),
    ("xs", "0.125rem"),
    ("sm", "0.25rem"),
    ("md", "0.375rem"),
    ("lg", "0.5rem"),
    ("xl", "0.75rem"),
    ("2xl", "1rem"),
    ("3xl", "1.5rem"),
    ("4xl", "2rem"),
];
const CONTAINERS: [(&str, &str); 13] = [
    ("3xs", "16rem"),
    ("2xs", "18rem"),
    ("xs", "20rem"),
    ("sm", "24rem"),
    ("md", "28rem"),
    ("lg", "32rem"),
    ("xl", "36rem"),
    ("2xl", "42rem"),
    ("3xl", "48rem"),
    ("4xl", "56rem"),
    ("5xl", "64rem"),
    ("6xl", "72rem"),
    ("7xl", "80rem"),
];
const SHADES: [&str; 11] = [
    "50", "100", "200", "300", "400", "500", "600", "700", "800", "900", "950",
];
/// Utilities that take no value, by name.
const FIXED: &[(&str, &str)] = &[
    ("block", "display:block"),
    ("flex", "display:flex"),
    ("flex-row", "flex-direction:row"),
    ("flex-col", "flex-direction:column"),
    ("flex-wrap", "flex-wrap:wrap"),
    ("flex-nowrap", "flex-wrap:nowrap"),
    ("flex-1", "flex:1"),
    ("flex-auto", "flex:auto"),
    ("flex-initial", "flex:0 auto"),
    ("flex-none", "flex:none"),
    ("grow", "flex-grow:1"),
    ("grow-0", "flex-grow:0"),
    ("shrink", "flex-shrink:1"),
    ("shrink-0", "flex-shrink:0"),
    ("justify-normal", "justify-content:normal"),
    ("justify-start", "justify-content:flex-start"),
    ("justify-end", "justify-content:flex-end"),
    ("justify-center", "justify-content:center"),
    ("justify-between", "justify-content:space-between"),
    ("justify-around", "justify-content:space-around"),
    ("justify-evenly", "justify-content:space-evenly"),
    ("items-start", "align-items:flex-start"),
    ("items-end", "align-items:flex-end"),
    ("items-center", "align-items:center"),
    ("items-stretch", "align-items:stretch"),
    ("self-auto", "align-self:auto"),
    ("self-start", "align-self:flex-start"),
    ("self-end", "align-self:flex-end"),
    ("self-center", "align-self:center"),
    ("self-stretch", "align-self:stretch"),
    ("content-normal", "align-content:normal"),
    ("content-start", "align-content:flex-start"),
    ("content-end", "align-content:flex-end"),
    ("content-center", "align-content:center"),
    ("content-between", "align-content:space-between"),
    ("content-around", "align-content:space-around"),
    ("content-evenly", "align-content:space-evenly"),
    ("content-stretch", "align-content:stretch"),
    ("italic", "font-style:italic"),
    ("not-italic", "font-style:normal"),
    ("underline", "text-decoration-line:underline"),
    ("line-through", "text-decoration-line:line-through"),
    ("no-underline", "text-decoration-line:none"),
    ("uppercase", "text-transform:uppercase"),
    ("lowercase", "text-transform:lowercase"),
    ("capitalize", "text-transform:capitalize"),
    ("normal-case", "text-transform:none"),
    ("text-left", "text-align:left"),
    ("text-center", "text-align:center"),
    ("text-right", "text-align:right"),
    ("text-justify", "text-align:justify"),
    ("text-start", "text-align:start"),
    ("text-end", "text-align:end"),
    ("whitespace-normal", "white-space:normal"),
    ("whitespace-nowrap", "white-space:nowrap"),
    ("whitespace-pre", "white-space:pre"),
    ("whitespace-pre-line", "white-space:pre-line"),
    ("whitespace-pre-wrap", "white-space:pre-wrap"),
    ("border-solid", "border-style:solid"),
    ("border-dashed", "border-style:dashed"),
    ("border-dotted", "border-style:dotted"),
    ("border-none", "border-style:none"),
    ("border-collapse", "border-collapse:collapse"),
    ("border-separate", "border-collapse:separate"),
    ("box-border", "box-sizing:border-box"),
    ("box-content", "box-sizing:content-box"),
    ("list-none", "list-style-type:none"),
    ("list-disc", "list-style-type:disc"),
    ("list-decimal", "list-style-type:decimal"),
    ("list-inside", "list-style-position:inside"),
    ("list-outside", "list-style-position:outside"),
];
const FONTS: [(&str, &str); 3] = [
    (
        "sans",
        "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', 'Noto Sans', Arial, sans-serif, 'Apple Color Emoji', 'Segoe UI Emoji', 'Segoe UI Symbol', 'Noto Color Emoji'",
    ),
    ("serif", "ui-serif, Georgia, Cambria, 'Times New Roman', Times, serif"),
    (
        "mono",
        "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace",
    ),
];
/// Tailwind v4's default palette as OKLCH, shades 50, 100 to 900, and 950.
#[rustfmt::skip]
const PALETTE: [(&str, [&str; 11]); 26] = [
    ("red", ["97.1% 0.013 17.38", "93.6% 0.032 17.717", "88.5% 0.062 18.334", "80.8% 0.114 19.571", "70.4% 0.191 22.216", "63.7% 0.237 25.331", "57.7% 0.245 27.325", "50.5% 0.213 27.518", "44.4% 0.177 26.899", "39.6% 0.141 25.723", "25.8% 0.092 26.042"]),
    ("orange", ["98% 0.016 73.684", "95.4% 0.038 75.164", "90.1% 0.076 70.697", "83.7% 0.128 66.29", "75% 0.183 55.934", "70.5% 0.213 47.604", "64.6% 0.222 41.116", "55.3% 0.195 38.402", "47% 0.157 37.304", "40.8% 0.123 38.172", "26.6% 0.079 36.259"]),
    ("amber", ["98.7% 0.022 95.277", "96.2% 0.059 95.617", "92.4% 0.12 95.746", "87.9% 0.169 91.605", "82.8% 0.189 84.429", "76.9% 0.188 70.08", "66.6% 0.179 58.318", "55.5% 0.163 48.998", "47.3% 0.137 46.201", "41.4% 0.112 45.904", "27.9% 0.077 45.635"]),
    ("yellow", ["98.7% 0.026 102.212", "97.3% 0.071 103.193", "94.5% 0.129 101.54", "90.5% 0.182 98.111", "85.2% 0.199 91.936", "79.5% 0.184 86.047", "68.1% 0.162 75.834", "55.4% 0.135 66.442", "47.6% 0.114 61.907", "42.1% 0.095 57.708", "28.6% 0.066 53.813"]),
    ("lime", ["98.6% 0.031 120.757", "96.7% 0.067 122.328", "93.8% 0.127 124.321", "89.7% 0.196 126.665", "84.1% 0.238 128.85", "76.8% 0.233 130.85", "64.8% 0.2 131.684", "53.2% 0.157 131.589", "45.3% 0.124 130.933", "40.5% 0.101 131.063", "27.4% 0.072 132.109"]),
    ("green", ["98.2% 0.018 155.826", "96.2% 0.044 156.743", "92.5% 0.084 155.995", "87.1% 0.15 154.449", "79.2% 0.209 151.711", "72.3% 0.219 149.579", "62.7% 0.194 149.214", "52.7% 0.154 150.069", "44.8% 0.119 151.328", "39.3% 0.095 152.535", "26.6% 0.065 152.934"]),
    ("emerald", ["97.9% 0.021 166.113", "95% 0.052 163.051", "90.5% 0.093 164.15", "84.5% 0.143 164.978", "76.5% 0.177 163.223", "69.6% 0.17 162.48", "59.6% 0.145 163.225", "50.8% 0.118 165.612", "43.2% 0.095 166.913", "37.8% 0.077 168.94", "26.2% 0.051 172.552"]),
    ("teal", ["98.4% 0.014 180.72", "95.3% 0.051 180.801", "91% 0.096 180.426", "85.5% 0.138 181.071", "77.7% 0.152 181.912", "70.4% 0.14 182.503", "60% 0.118 184.704", "51.1% 0.096 186.391", "43.7% 0.078 188.216", "38.6% 0.063 188.416", "27.7% 0.046 192.524"]),
    ("cyan", ["98.4% 0.019 200.873", "95.6% 0.045 203.388", "91.7% 0.08 205.041", "86.5% 0.127 207.078", "78.9% 0.154 211.53", "71.5% 0.143 215.221", "60.9% 0.126 221.723", "52% 0.105 223.128", "45% 0.085 224.283", "39.8% 0.07 227.392", "30.2% 0.056 229.695"]),
    ("sky", ["97.7% 0.013 236.62", "95.1% 0.026 236.824", "90.1% 0.058 230.902", "82.8% 0.111 230.318", "74.6% 0.16 232.661", "68.5% 0.169 237.323", "58.8% 0.158 241.966", "50% 0.134 242.749", "44.3% 0.11 240.79", "39.1% 0.09 240.876", "29.3% 0.066 243.157"]),
    ("blue", ["97% 0.014 254.604", "93.2% 0.032 255.585", "88.2% 0.059 254.128", "80.9% 0.105 251.813", "70.7% 0.165 254.624", "62.3% 0.214 259.815", "54.6% 0.245 262.881", "48.8% 0.243 264.376", "42.4% 0.199 265.638", "37.9% 0.146 265.522", "28.2% 0.091 267.935"]),
    ("indigo", ["96.2% 0.018 272.314", "93% 0.034 272.788", "87% 0.065 274.039", "78.5% 0.115 274.713", "67.3% 0.182 276.935", "58.5% 0.233 277.117", "51.1% 0.262 276.966", "45.7% 0.24 277.023", "39.8% 0.195 277.366", "35.9% 0.144 278.697", "25.7% 0.09 281.288"]),
    ("violet", ["96.9% 0.016 293.756", "94.3% 0.029 294.588", "89.4% 0.057 293.283", "81.1% 0.111 293.571", "70.2% 0.183 293.541", "60.6% 0.25 292.717", "54.1% 0.281 293.009", "49.1% 0.27 292.581", "43.2% 0.232 292.759", "38% 0.189 293.745", "28.3% 0.141 291.089"]),
    ("purple", ["97.7% 0.014 308.299", "94.6% 0.033 307.174", "90.2% 0.063 306.703", "82.7% 0.119 306.383", "71.4% 0.203 305.504", "62.7% 0.265 303.9", "55.8% 0.288 302.321", "49.6% 0.265 301.924", "43.8% 0.218 303.724", "38.1% 0.176 304.987", "29.1% 0.149 302.717"]),
    ("fuchsia", ["97.7% 0.017 320.058", "95.2% 0.037 318.852", "90.3% 0.076 319.62", "83.3% 0.145 321.434", "74% 0.238 322.16", "66.7% 0.295 322.15", "59.1% 0.293 322.896", "51.8% 0.253 323.949", "45.2% 0.211 324.591", "40.1% 0.17 325.612", "29.3% 0.136 325.661"]),
    ("pink", ["97.1% 0.014 343.198", "94.8% 0.028 342.258", "89.9% 0.061 343.231", "82.3% 0.12 346.018", "71.8% 0.202 349.761", "65.6% 0.241 354.308", "59.2% 0.249 0.584", "52.5% 0.223 3.958", "45.9% 0.187 3.815", "40.8% 0.153 2.432", "28.4% 0.109 3.907"]),
    ("rose", ["96.9% 0.015 12.422", "94.1% 0.03 12.58", "89.2% 0.058 10.001", "81% 0.117 11.638", "71.2% 0.194 13.428", "64.5% 0.246 16.439", "58.6% 0.253 17.585", "51.4% 0.222 16.935", "45.5% 0.188 13.697", "41% 0.159 10.272", "27.1% 0.105 12.094"]),
    ("slate", ["98.4% 0.003 247.858", "96.8% 0.007 247.896", "92.9% 0.013 255.508", "86.9% 0.022 252.894", "70.4% 0.04 256.788", "55.4% 0.046 257.417", "44.6% 0.043 257.281", "37.2% 0.044 257.287", "27.9% 0.041 260.031", "20.8% 0.042 265.755", "12.9% 0.042 264.695"]),
    ("gray", ["98.5% 0.002 247.839", "96.7% 0.003 264.542", "92.8% 0.006 264.531", "87.2% 0.01 258.338", "70.7% 0.022 261.325", "55.1% 0.027 264.364", "44.6% 0.03 256.802", "37.3% 0.034 259.733", "27.8% 0.033 256.848", "21% 0.034 264.665", "13% 0.028 261.692"]),
    ("zinc", ["98.5% 0 none", "96.7% 0.001 286.375", "92% 0.004 286.32", "87.1% 0.006 286.286", "70.5% 0.015 286.067", "55.2% 0.016 285.938", "44.2% 0.017 285.786", "37% 0.013 285.805", "27.4% 0.006 286.033", "21% 0.006 285.885", "14.1% 0.005 285.823"]),
    ("neutral", ["98.5% 0 none", "97% 0 none", "92.2% 0 none", "87% 0 none", "70.8% 0 none", "55.6% 0 none", "43.9% 0 none", "37.1% 0 none", "26.9% 0 none", "20.5% 0 none", "14.5% 0 none"]),
    ("stone", ["98.5% 0.001 106.423", "97% 0.001 106.424", "92.3% 0.003 48.717", "86.9% 0.005 56.366", "70.9% 0.01 56.259", "55.3% 0.013 58.071", "44.4% 0.011 73.639", "37.4% 0.01 67.558", "26.8% 0.007 34.298", "21.6% 0.006 56.043", "14.7% 0.004 49.25"]),
    ("mauve", ["98.5% 0 none", "96% 0.003 325.6", "92.2% 0.005 325.62", "86.5% 0.012 325.68", "71.1% 0.019 323.02", "54.2% 0.034 322.5", "43.5% 0.029 321.78", "36.4% 0.029 323.89", "26.3% 0.024 320.12", "21.2% 0.019 322.12", "14.5% 0.008 326"]),
    ("olive", ["98.8% 0.003 106.5", "96.6% 0.005 106.5", "93% 0.007 106.5", "88% 0.011 106.6", "73.7% 0.021 106.9", "58% 0.031 107.3", "46.6% 0.025 107.3", "39.4% 0.023 107.4", "28.6% 0.016 107.4", "22.8% 0.013 107.4", "15.3% 0.006 107.1"]),
    ("mist", ["98.7% 0.002 197.1", "96.3% 0.002 197.1", "92.5% 0.005 214.3", "87.2% 0.007 219.6", "72.3% 0.014 214.4", "56% 0.021 213.5", "45% 0.017 213.2", "37.8% 0.015 216", "27.5% 0.011 216.9", "21.8% 0.008 223.9", "14.8% 0.004 228.8"]),
    ("taupe", ["98.6% 0.002 67.8", "96% 0.002 17.2", "92.2% 0.005 34.3", "86.8% 0.007 39.5", "71.4% 0.014 41.2", "54.7% 0.021 43.1", "43.8% 0.017 39.3", "36.7% 0.016 35.7", "26.8% 0.011 36.5", "21.4% 0.009 43.1", "14.7% 0.004 49.3"]),
];

fn find<'a>(table: &[(&str, &'a str)], name: &str) -> Option<&'a str> {
    table
        .iter()
        .find(|(own, _)| *own == name)
        .map(|(_, value)| *value)
}

/// Tailwind's underscore escape in an arbitrary value. Other escapes reject.
fn arbitrary(value: &str) -> Option<String> {
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    let mut out = String::new();
    let mut chars = inner.char_indices().peekable();
    let mut variable = false;
    while let Some((index, ch)) = chars.next() {
        if inner[index..].starts_with("var(") || inner[index..].starts_with("env(") {
            variable = true;
        }
        if ch == ',' || ch == ')' {
            variable = false;
        }
        match ch {
            '\\' if chars.next()?.1 == '_' => out.push('_'),
            '\\' => return None,
            '_' if !variable => out.push(' '),
            _ => out.push(ch),
        }
    }
    if out.trim().is_empty() {
        return None;
    }
    // Tailwind inserts CSS's required spaces around binary math operators.
    let mut formatted = String::new();
    let mut math = false;
    let mut variable = false;
    let mut quote = None;
    for (i, ch) in out.char_indices() {
        if matches!(ch, '\'' | '"') {
            quote = if quote == Some(ch) { None } else { Some(ch) };
        }
        if out[i..].starts_with("var(") || out[i..].starts_with("env(") {
            variable = true;
        }
        if ch == ',' || ch == ')' {
            variable = false;
        }
        if ["calc(", "min(", "max(", "clamp("]
            .iter()
            .any(|f| out[i..].starts_with(f))
        {
            math = true;
        }
        let previous = out[..i].trim_end().chars().next_back();
        let next = out[i + ch.len_utf8()..].trim_start().chars().next();
        if math
            && !variable
            && quote.is_none()
            && matches!(ch, '+' | '-')
            && previous.is_some_and(|c| c.is_ascii_alphanumeric() || c == ')' || c == '%')
            && next.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '(' | '+' | '-'))
            && !out[..i].ends_with("var(")
            && !out[..i].ends_with("env(")
        {
            formatted.push(' ');
            formatted.push(ch);
            formatted.push(' ');
        } else {
            formatted.push(ch);
        }
    }
    Some(formatted)
}

fn arbitrary_value(value: &str) -> Option<String> {
    let value = arbitrary(value)?;
    Some(match value.split_once(':') {
        Some(("length" | "color" | "number" | "percentage" | "family-name", rest)) => rest.into(),
        _ => value,
    })
}

/// Tailwind accepts canonical quarter steps, not 01, 1.00, or exponents.
fn numeric(value: &str, step: f64) -> bool {
    let Ok(number) = value.parse::<f64>() else {
        return false;
    };
    number.is_finite()
        && number >= 0.0
        && (number / step).fract() == 0.0
        && number.to_string() == value
}

/// A multiple of the 0.25rem spacing unit, `px`, or an arbitrary value.
fn spacing(value: &str) -> Option<String> {
    if value == "px" {
        return Some("1px".into());
    }
    if let Some(value) = arbitrary_value(value) {
        return Some(value);
    }
    numeric(value, 0.25).then(|| format!("calc(0.25rem * {value})"))
}

/// A spacing value, a fraction, `full`, `auto`, or a container size.
fn size(value: &str, containers: bool, auto: bool) -> Option<String> {
    if let Some(value) = arbitrary_value(value) {
        return Some(value);
    }
    if let Some((top, bottom)) = value.split_once('/') {
        let valid = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        return (valid(top)
            && valid(bottom)
            && numeric(top, 1.0)
            && numeric(bottom, 1.0)
            && bottom != "0")
            .then(|| format!("calc({top} / {bottom} * 100%)"));
    }
    match value {
        "full" => Some("100%".into()),
        "auto" if auto => Some("auto".into()),
        _ => containers
            .then(|| find(&CONTAINERS, value))
            .flatten()
            .map(Into::into)
            .or_else(|| spacing(value)),
    }
}

/// A palette colour such as `gray-200`, `black`, `white`, `transparent`, or an arbitrary one.
fn color(value: &str) -> Option<String> {
    match value {
        "black" => return Some("#000".into()),
        "white" => return Some("#fff".into()),
        "transparent" => return Some("transparent".into()),
        _ => {}
    }
    if let Some(value) = arbitrary_value(value) {
        return Some(value);
    }
    let (family, shade) = value.rsplit_once('-')?;
    let shade = SHADES.iter().position(|own| *own == shade)?;
    let (_, shades) = PALETTE.iter().find(|(own, _)| *own == family)?;
    Some(format!("oklch({})", shades[shade]))
}

/// Tailwind infers lengths from units and math, and treats ambiguous values as colours.
fn length_like(value: &str) -> bool {
    if let Some((hint, _)) = value.split_once(':') {
        return hint == "length" || hint == "percentage";
    }
    ["calc(", "min(", "max(", "clamp("]
        .iter()
        .any(|f| value.starts_with(f))
        || ["px", "pt", "pc", "rem", "em", "in", "cm", "mm", "Q", "%"]
            .iter()
            .any(|unit| {
                value
                    .strip_suffix(unit)
                    .is_some_and(|n| n.parse::<f64>().is_ok())
            })
}

/// The declarations of one utility, or None when Tailwind has no such utility.
fn utility(name: &str, out: &mut String) -> Option<()> {
    if let Some(fixed) = find(FIXED, name) {
        out.push_str(fixed);
        out.push(';');
        return Some(());
    }
    let mut put = |property: &str, value: &str| {
        let _ = write!(out, "{property}:{value};");
    };
    // `[property:value]` sets any property.
    if let Some(declaration) = arbitrary(name) {
        let (_, value) = declaration.split_once(':')?;
        let (property, _) = name[1..name.len() - 1].split_once(':')?;
        if property.is_empty()
            || value.trim().is_empty()
            || !property.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b == b'-'
                    || property.starts_with("--") && (b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return None;
        }
        if property.starts_with("--tw-") {
            return None;
        }
        crate::css::property(property, "").ok()?;
        put(property, value);
        return Some(());
    }
    let (negative, name) = match name.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, name),
    };
    if name.ends_with('-') {
        return None;
    }
    let (prefix, value) = name.split_once('-').unwrap_or((name, ""));
    let sides: &[&str] = match prefix {
        "p" | "m" => &[""],
        "px" | "mx" => &["-inline"],
        "py" | "my" => &["-block"],
        "pt" | "mt" => &["-top"],
        "pr" | "mr" => &["-right"],
        "pb" | "mb" => &["-bottom"],
        "pl" | "ml" => &["-left"],
        _ => &[],
    };
    if !sides.is_empty() {
        let property = if prefix.starts_with('p') {
            "padding"
        } else {
            "margin"
        };
        let value = match value {
            "auto" => {
                if property == "margin" && !negative {
                    "auto".into()
                } else {
                    return None;
                }
            }
            _ => spacing(value)?,
        };
        if negative && property == "padding" {
            return None;
        }
        let value = if negative {
            format!("calc({value} * -1)")
        } else {
            value
        };
        for side in sides {
            put(&format!("{property}{side}"), &value);
        }
        return Some(());
    }
    if negative && prefix != "tracking" {
        return None;
    }
    match prefix {
        "gap" => match value.split_once('-') {
            Some(("x", value)) => put("column-gap", &spacing(value)?),
            Some(("y", value)) => put("row-gap", &spacing(value)?),
            _ => put("gap", &spacing(value)?),
        },
        "w" => put("width", &size(value, true, true)?),
        "h" => put("height", &size(value, false, true)?),
        "size" => {
            let value = size(value, false, true)?;
            put("width", &value);
            put("height", &value);
        }
        "basis" => put("flex-basis", &size(value, true, true)?),
        "flex" => put(
            "flex",
            &arbitrary_value(value).or_else(|| numeric(value, 1.0).then(|| value.into()))?,
        ),
        "grow" | "shrink" => put(
            if prefix == "grow" {
                "flex-grow"
            } else {
                "flex-shrink"
            },
            &arbitrary_value(value).or_else(|| numeric(value, 1.0).then(|| value.into()))?,
        ),
        "min" | "max" => {
            let (axis, value) = value.split_once('-')?;
            let property = match axis {
                "w" => "width",
                "h" => "height",
                _ => return None,
            };
            let value = match value {
                "none" if prefix == "max" => "none".into(),
                _ => size(value, axis == "w", prefix == "min")?,
            };
            put(&format!("{prefix}-{property}"), &value);
        }
        "text" => {
            if let Some((_, size, height)) = TEXT.iter().find(|(own, ..)| *own == value) {
                put("font-size", size);
                put("line-height", height);
            } else if arbitrary(value).is_some_and(|v| length_like(&v)) {
                put("font-size", &arbitrary_value(value)?);
            } else {
                put("color", &color(value)?);
            }
        }
        "font" => {
            if let Some(weight) = find(&WEIGHTS, value) {
                put("font-weight", weight);
            } else if let Some(family) = find(&FONTS, value) {
                put("font-family", family);
            } else {
                let raw = arbitrary(value)?;
                let val = arbitrary_value(value)?;
                if !raw.starts_with("family-name:")
                    && (raw.split_once(':').is_some_and(|(hint, _)| {
                        ["number", "length", "percentage", "color"].contains(&hint)
                    }) || val.parse::<f64>().is_ok()
                        || ["calc(", "min(", "max(", "clamp("]
                            .iter()
                            .any(|f| raw.starts_with(f)))
                {
                    put("font-weight", &val);
                } else {
                    put("font-family", &val);
                }
            }
        }
        "leading" => {
            let height = find(&LEADING, value)
                .map(Into::into)
                .or_else(|| spacing(value))?;
            put("line-height", &height);
        }
        "tracking" => {
            let val = find(&TRACKING, value)
                .map(Into::into)
                .or_else(|| arbitrary_value(value))?;
            let val = if negative {
                format!("calc({val} * -1)")
            } else {
                val
            };
            put("letter-spacing", &val);
        }
        "bg" => put("background-color", &color(value)?),
        "decoration" => put("text-decoration-color", &color(value)?),
        "opacity" => {
            let opacity = arbitrary_value(value)
                .or_else(|| numeric(value, 0.25).then(|| format!("{value}%")))?;
            put("opacity", &opacity);
        }
        "rounded" => {
            let (corners, value): (&[&str], &str) =
                match value.split_once('-').unwrap_or((value, "")) {
                    ("t", rest) => (&["top-left", "top-right"], rest),
                    ("r", rest) => (&["top-right", "bottom-right"], rest),
                    ("b", rest) => (&["bottom-right", "bottom-left"], rest),
                    ("l", rest) => (&["top-left", "bottom-left"], rest),
                    ("tl", rest) => (&["top-left"], rest),
                    ("tr", rest) => (&["top-right"], rest),
                    ("br", rest) => (&["bottom-right"], rest),
                    ("bl", rest) => (&["bottom-left"], rest),
                    _ => (&[], value),
                };
            let radius = match value {
                // Tailwind CLI --minify lowers infinity to this finite CSS length.
                "full" => "3.40282e38px".into(),
                _ => find(&RADIUS, value)
                    .map(Into::into)
                    .or_else(|| arbitrary_value(value))?,
            };
            if corners.is_empty() {
                put("border-radius", &radius);
            }
            for corner in corners {
                put(&format!("border-{corner}-radius"), &radius);
            }
        }
        "border" => {
            let (sides, value): (&[&str], &str) = match value.split_once('-').unwrap_or((value, ""))
            {
                ("x", rest) => (&["-left", "-right"], rest),
                ("y", rest) => (&["-top", "-bottom"], rest),
                ("t", rest) => (&["-top"], rest),
                ("r", rest) => (&["-right"], rest),
                ("b", rest) => (&["-bottom"], rest),
                ("l", rest) => (&["-left"], rest),
                _ => (&[""], value),
            };
            let width = match value {
                "" => Some("1px".to_owned()),
                _ if numeric(value, 1.0) => Some(format!("{value}px")),
                _ => arbitrary(value)
                    .filter(|v| length_like(v))
                    .and_then(|_| arbitrary_value(value)),
            };
            match width {
                Some(width) => {
                    for side in sides {
                        put(&format!("border{side}-style"), "solid");
                        put(&format!("border{side}-width"), &width);
                    }
                }
                None if sides == [""] => put("border-color", &color(value)?),
                None => {
                    for side in sides {
                        put(&format!("border{side}-color"), &color(value)?);
                    }
                }
            }
        }
        _ => return None,
    }
    Some(())
}

// Tailwind 4.3.3 property order, restricted to this CSS subset and grouped radius keys.
const ORDER: &[&str] = &[
    "position",
    "inset",
    "inset-inline",
    "inset-block",
    "top",
    "right",
    "bottom",
    "left",
    "z-index",
    "margin",
    "margin-inline",
    "margin-block",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "box-sizing",
    "display",
    "height",
    "max-height",
    "min-height",
    "width",
    "max-width",
    "min-width",
    "flex",
    "flex-shrink",
    "flex-grow",
    "flex-basis",
    "border-collapse",
    "border-spacing",
    "list-style-position",
    "list-style-type",
    "break-before",
    "break-inside",
    "break-after",
    "flex-direction",
    "flex-wrap",
    "align-content",
    "align-items",
    "justify-content",
    "gap",
    "column-gap",
    "row-gap",
    "align-self",
    "border-radius",
    "border-top-radius",
    "border-right-radius",
    "border-bottom-radius",
    "border-left-radius",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-right-radius",
    "border-bottom-left-radius",
    "border-width",
    "border-inline-width",
    "border-block-width",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "border-style",
    "border-inline-style",
    "border-block-style",
    "border-top-style",
    "border-right-style",
    "border-bottom-style",
    "border-left-style",
    "border-color",
    "border-inline-color",
    "border-block-color",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "background-color",
    "padding",
    "padding-inline",
    "padding-block",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "text-align",
    "vertical-align",
    "font-family",
    "font-size",
    "line-height",
    "font-weight",
    "letter-spacing",
    "white-space",
    "color",
    "text-transform",
    "font-style",
    "font-stretch",
    "text-decoration-line",
    "text-decoration-color",
    "opacity",
    "box-shadow",
    "outline",
    "outline-width",
    "outline-offset",
    "outline-color",
];

fn compare_names(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    for i in 0..a.len().min(b.len()) {
        if a[i].is_ascii_digit() && b[i].is_ascii_digit() {
            let digits = |s: &[u8]| s.iter().take_while(|c| c.is_ascii_digit()).count();
            let (an, bn) = (&a[i..i + digits(&a[i..])], &b[i..i + digits(&b[i..])]);
            let n = |s: &[u8]| {
                std::str::from_utf8(s)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap_or(f64::INFINITY)
            };
            let cmp = n(an).total_cmp(&n(bn)).then_with(|| an.cmp(bn));
            if !cmp.is_eq() {
                return cmp;
            }
        } else if a[i] != b[i] {
            return a[i].cmp(&b[i]);
        }
    }
    a.len().cmp(&b.len())
}

fn order(name: &str, css: &str) -> (Vec<usize>, usize) {
    let mut props = Vec::new();
    let mut count = 0;
    // Border axes compile to logical edge declarations in Tailwind.
    let grouped = [("border-x", "border-inline"), ("border-y", "border-block")]
        .iter()
        .find(|(prefix, _)| {
            name == *prefix
                || name
                    .strip_prefix(prefix)
                    .is_some_and(|r| r.starts_with('-'))
        });
    for declaration in css.split(';').filter(|s| !s.is_empty()) {
        count += 1;
        let (prop, _) = declaration.split_once(':').unwrap();
        let grouped_prop;
        let prop = if let Some((_, group)) = grouped {
            let suffix = prop.rsplit_once('-').unwrap().1;
            grouped_prop = format!("{group}-{suffix}");
            grouped_prop.as_str()
        } else {
            prop
        };
        if let Some(index) = ORDER.iter().position(|own| *own == prop) {
            props.push(index);
        }
    }
    if grouped.is_some() {
        count /= 2;
    }
    props.sort_unstable();
    props.dedup();
    // Tailwind counts its non-inheriting implementation variables too; tw emits only CSS values.
    if name.starts_with("leading-")
        || name.starts_with("tracking-")
        || name.starts_with("-tracking-")
        || (name.starts_with("font-") && css.starts_with("font-weight:"))
        || [
            "border-solid",
            "border-dashed",
            "border-dotted",
            "border-none",
        ]
        .contains(&name)
    {
        count += 1;
    }
    (props, count)
}

/// Utilities follow Tailwind's property order, declaration count, then numeric name order.
pub fn declarations(utilities: &str) -> Result<String, String> {
    if utilities.len() > 16_384 {
        return Err("tw exceeds 16384 bytes".into());
    }
    let mut rules = Vec::new();
    for name in utilities.split_ascii_whitespace() {
        if rules.len() >= 256 {
            return Err("tw exceeds 256 utilities".into());
        }
        if name.len() > 1024 {
            return Err("tw utility exceeds 1024 bytes".into());
        }
        let mut stack = Vec::new();
        let mut quote = None;
        for byte in name.bytes() {
            if matches!(byte, b'\'' | b'"') {
                quote = if quote == Some(byte) {
                    None
                } else if quote.is_none() {
                    Some(byte)
                } else {
                    quote
                };
            }
            if quote.is_some() && !matches!(byte, b';' | b'{' | b'}' | b'!' | 0..=31 | 127) {
                continue;
            }
            match byte {
                b'[' | b'(' => {
                    if stack.len() == 16 {
                        return Err(format!("tw nesting exceeds 16: {name}"));
                    }
                    stack.push(byte);
                }
                b']' if stack.pop() == Some(b'[') => {}
                b')' if stack.pop() == Some(b'(') => {}
                b':' if stack.is_empty() => return Err(format!("tw takes no variants: {name}")),
                b']' | b')' | b';' | b'{' | b'}' | b'!' | 0..=31 | 127 => {
                    return Err(format!("invalid tw utility: {name}"))
                }
                _ => {}
            }
        }
        if !stack.is_empty() || quote.is_some() || name.contains("/*") || name.contains("*/") {
            return Err(format!("invalid tw utility: {name}"));
        }
        let mut css = String::new();
        utility(name, &mut css).ok_or_else(|| format!("unknown tw utility: {name}"))?;
        let (props, count) = order(name, &css);
        rules.push((props, count, name, css));
    }
    rules.sort_by(|a, b| {
        let mut left = a.0.iter();
        let mut right = b.0.iter();
        let cmp = loop {
            match (left.next(), right.next()) {
                (Some(a), Some(b)) if a == b => continue,
                (Some(a), Some(b)) => break a.cmp(b),
                (Some(_), None) => break std::cmp::Ordering::Less,
                (None, Some(_)) => break std::cmp::Ordering::Greater,
                (None, None) => break std::cmp::Ordering::Equal,
            }
        };
        cmp.then_with(|| b.1.cmp(&a.1))
            .then_with(|| compare_names(a.2, b.2))
    });
    Ok(rules.into_iter().map(|(_, _, _, css)| css).collect())
}

#[cfg(test)]
mod tests;

/// An element's `style` with the `tw` declarations before it, so `style` wins a conflict.
pub fn merge(style: &InlineStyle, utilities: &str) -> Result<InlineStyle, String> {
    let text = declarations(utilities)?;
    Ok(match style {
        InlineStyle::None => InlineStyle::Text(text),
        InlineStyle::Text(style) => InlineStyle::Text(text + style),
        InlineStyle::Object(object) => {
            let mut merged = Cascaded::default();
            for item in text.split(';').filter(|item| !item.is_empty()) {
                let (name, value) = item.split_once(':').unwrap();
                let key = property(name, "").map_err(String::from)?;
                merged.set(key, Value::Text(value.into()));
            }
            for (key, value) in &object.0 {
                merged.set(key.clone(), value.clone());
            }
            InlineStyle::Object(merged)
        }
    })
}
