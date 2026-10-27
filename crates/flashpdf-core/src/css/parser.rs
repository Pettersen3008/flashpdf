use std::cell::OnceCell;
use std::ops::Range;
use std::rc::Rc;

use super::selector::{quoted, selector, specificity, split, Complex, Specificity};
use super::{json_string, point, position, property, whitespace, Declarations, Value};

/// The value without a trailing `!important`, if it has one.
fn important(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    let word = b"important";
    if bytes.len() < word.len() + 1 || !bytes[bytes.len() - word.len()..].eq_ignore_ascii_case(word)
    {
        return None;
    }
    let before = value[..value.len() - word.len()].trim_end();
    before.strip_suffix('!').map(str::trim_end)
}

pub(super) fn declarations(source: &str, context: &str) -> Result<Declarations, String> {
    let mut result = Declarations::default();
    for item in source.split(';') {
        if item.trim().is_empty() {
            continue;
        }
        let colon = item.find(':').filter(|&colon| colon > 0);
        let Some(colon) = colon else {
            return Err(format!("invalid CSS declaration: {}{context}", item.trim()));
        };
        let key = property(item[..colon].trim(), context)?;
        let value = item[colon + 1..].trim();
        let (target, value) = match important(value) {
            Some(value) => (&mut result.important, value),
            None => (&mut result.normal, value),
        };
        target.set(key, Value::Text(value.into()));
    }
    Ok(result)
}

fn invalid(source: &str, index: usize) -> String {
    format!("invalid CSS stylesheet at {}", position(source, index))
}

/// Blanks comments to whitespace of the same UTF-16 length, keeping newlines, so error positions match the source.
fn without_comments(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut index = 0;
    loop {
        let Some(start) = source[index..].find("/*").map(|at| index + at) else {
            result.push_str(&source[index..]);
            return result;
        };
        result.push_str(&source[index..start]);
        let Some(end) = source[start + 2..].find("*/").map(|at| start + 2 + at) else {
            result.push_str(&source[start..]);
            return result;
        };
        for character in source[start..end + 2].chars() {
            match character {
                '\n' => result.push('\n'),
                _ => (0..character.len_utf16()).for_each(|_| result.push(' ')),
            }
        }
        index = end + 2;
    }
}

/// Index just past the `}` matching the `{` at `open`; braces inside strings do not count, and an
/// escape such as the `\'` in Tailwind's `.\[\&_svg\:not\(\[class\*\=\'size-\'\]\)\]` opens no string.
fn closing(source: &str, open: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'"' | b'\'' => index = quoted(bytes, index),
            b'{' => depth += 1,
            b'}' => {
                depth = depth.wrapping_sub(1);
                if depth == 0 {
                    return Ok(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    Err(invalid(source, open))
}

/// Index of the `{` or `;` that ends the at-rule prelude starting at `start`.
fn prelude_end(source: &str, start: usize) -> Result<usize, String> {
    let bytes = source.as_bytes();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'"' | b'\'' => index = quoted(bytes, index),
            b';' | b'{' => return Ok(index),
            b'}' => break,
            _ => {}
        }
        index += 1;
    }
    Err(invalid(source, start))
}

/// One `@page` declaration, in source order.
#[derive(Debug, PartialEq)]
pub struct PageDeclaration {
    pub name: String,
    pub value: String,
    pub important: bool,
    /// ` in @page at 1:1`, appended to errors about this declaration.
    pub at: String,
}

const PAGE_PROPERTIES: [&str; 6] = [
    "size",
    "margin",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
];

/// Reads the `@page` rule at `start` into `pages`.
fn page_rule(source: &str, start: usize, pages: &mut Vec<PageDeclaration>) -> Result<(), String> {
    let open = source[start..]
        .find('{')
        .map(|at| start + at)
        .ok_or_else(|| invalid(source, start))?;
    let at = format!(" in @page at {}", position(source, start));
    let selector = source[start + 5..open].trim();
    if !selector.is_empty() {
        return Err(format!(
            "unsupported @page selector: {selector} (one size and margin apply to every page){at}"
        ));
    }
    let end = closing(source, open)?;
    let body = &source[open + 1..end - 1];
    let bytes = body.as_bytes();
    let margin_box = bytes.iter().enumerate().find_map(|(index, &byte)| {
        let length = bytes[index + 1..]
            .iter()
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_' || **b == b'-')
            .count();
        (byte == b'@' && length > 0).then(|| &body[index..index + 1 + length])
    });
    if let Some(margin_box) = margin_box {
        return Err(format!(
            "unsupported @page margin box: {margin_box} (use the header and footer options){at}"
        ));
    }
    for item in body.split(';') {
        if item.trim().is_empty() {
            continue;
        }
        let Some(colon) = item.find(':').filter(|&colon| colon > 0) else {
            return Err(format!("invalid CSS declaration: {}{at}", item.trim()));
        };
        let name = item[..colon].trim();
        if !PAGE_PROPERTIES.contains(&name) {
            return Err(format!(
                "unsupported @page property: {name} (only size and margin apply){at}"
            ));
        }
        let value = item[colon + 1..].trim();
        let stripped = important(value);
        pages.push(PageDeclaration {
            name: name.into(),
            value: stripped.unwrap_or(value).into(),
            important: stripped.is_some(),
            at: at.clone(),
        });
    }
    Ok(())
}

/// `(width >= 48rem)`, `(min-width: 30em)`, and the like as (operator, length).
fn width_feature(term: &str) -> Option<(&'static str, &str)> {
    let inner = term.strip_prefix('(')?.strip_suffix(')')?;
    let inner = inner.trim_start();
    fn colon(rest: &str) -> Option<&str> {
        rest.trim_start().strip_prefix(':')
    }
    let mut candidates: Vec<(&'static str, &str)> = Vec::new();
    if let Some(rest) = inner.strip_prefix("min-width").and_then(colon) {
        candidates.push((">=", rest));
    } else if let Some(rest) = inner.strip_prefix("max-width").and_then(colon) {
        candidates.push(("<=", rest));
    } else if let Some(rest) = inner.strip_prefix("width") {
        let rest = rest.trim_start();
        if let Some(rest) = rest.strip_prefix(':') {
            candidates.push(("=", rest));
        }
        for operator in [">=", "<=", ">", "<", "="] {
            if let Some(rest) = rest.strip_prefix(operator) {
                candidates.push((operator, rest));
            }
        }
    }
    // The regex `\s*([^()]+?)\s*` backtracks to the first operator that leaves a value.
    candidates.into_iter().find_map(|(operator, rest)| {
        if rest.is_empty() || rest.contains(['(', ')']) {
            return None;
        }
        let trimmed = rest.trim();
        Some((
            operator,
            if trimmed.is_empty() {
                rest.char_indices()
                    .last()
                    .map_or(rest, |(at, _)| &rest[at..])
            } else {
                trimmed
            },
        ))
    })
}

/// Whether an `@media` prelude matches a page `width` points wide: `print` and `all` match,
/// `screen` does not, and a width condition compares the width. With no `width`, a width
/// condition counts as matching, so validation checks the rules it guards. Errors for anything else.
fn media(prelude: &str, width: Option<f64>) -> Result<bool, String> {
    let lower = prelude.to_lowercase();
    for alternative in lower.split(',') {
        let query = alternative.trim();
        let (negated, query) = match query.split_once(|c: char| c.is_whitespace()) {
            Some((modifier @ ("not" | "only"), rest)) => (modifier == "not", rest.trim_start()),
            _ => (false, query),
        };
        let mut matched = true;
        for term in and_terms(query) {
            let holds = match term {
                "all" | "print" => true,
                "screen" => false,
                _ => {
                    let (operator, value) = width_feature(term)
                        .ok_or("only all, print, screen, and width conditions are supported")?;
                    let limit = point(value, 12.0, false)?;
                    width.is_none_or(|width| match operator {
                        ">=" => width >= limit,
                        "<=" => width <= limit,
                        ">" => width > limit,
                        "<" => width < limit,
                        _ => width == limit,
                    })
                }
            };
            if !holds {
                matched = false;
                break;
            }
        }
        if matched != negated {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `query.split(/\s+and\s+/)`.
fn and_terms(query: &str) -> Vec<&str> {
    let bytes = query.as_bytes();
    let space = |byte: u8| byte.is_ascii_whitespace() || byte == b'\x0b';
    let mut terms = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if space(bytes[index]) {
            let after = index + bytes[index..].iter().take_while(|&&b| space(b)).count();
            if bytes[after..].starts_with(b"and") {
                let next = after + 3;
                let end = next + bytes[next..].iter().take_while(|&&b| space(b)).count();
                if end > next {
                    terms.push(&query[start..index]);
                    start = end;
                    index = end;
                    continue;
                }
            }
            index = after;
        } else {
            index += 1;
        }
    }
    terms.push(&query[start..]);
    terms
}

/// An `@property` rule: whether the value inherits, and the value of an element that sets none.
#[derive(Clone, Debug, PartialEq)]
pub struct Registration {
    pub inherits: bool,
    pub initial: Option<Rc<str>>,
}

fn register(name: &str, body: &str) -> Result<Registration, String> {
    let mut descriptors: Vec<(String, &str)> = Vec::new();
    for item in body.split(';') {
        if let Some(colon) = item.find(':').filter(|&colon| colon > 0) {
            let key = item[..colon].trim().to_lowercase();
            let value = item[colon + 1..].trim();
            match descriptors.iter_mut().find(|(own, _)| *own == key) {
                Some(entry) => entry.1 = value,
                None => descriptors.push((key, value)),
            }
        }
    }
    let get = |key: &str| {
        descriptors
            .iter()
            .find(|(own, _)| own == key)
            .map(|(_, value)| *value)
    };
    let inherits = get("inherits");
    let custom = name.len() > 2
        && name.starts_with("--")
        && name[2..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if !custom || get("syntax").is_none() || !matches!(inherits, Some("true" | "false")) {
        return Err(format!(
            "invalid @property {name}: it needs syntax and inherits"
        ));
    }
    Ok(Registration {
        inherits: inherits == Some("true"),
        initial: get("initial-value")
            .filter(|value| !value.is_empty())
            .map(Into::into),
    })
}

/// A rule body, parsed on first use so rules that never match cost nothing and cannot fail a render.
#[derive(Debug)]
struct Body {
    source: Rc<str>,
    range: Range<usize>,
    parsed: OnceCell<Rc<Declarations>>,
}

#[derive(Debug)]
pub struct Rule {
    pub(super) selector: Complex,
    pub order: usize,
    pub(super) specificity: Specificity,
    /// ` in selector ".a" at 1:1`, appended to errors about this rule.
    pub at: String,
    /// Set for a rule in an `@media` query outside the subset: `declarations()` returns it as the
    /// error, so a render reports it where the rule matches.
    pub blocked: Option<Rc<str>>,
    body: Rc<Body>,
}

impl Rule {
    pub fn declarations(&self) -> Result<Rc<Declarations>, String> {
        if let Some(blocked) = &self.blocked {
            return Err(format!("{blocked}{}", self.at));
        }
        if let Some(parsed) = self.body.parsed.get() {
            return Ok(parsed.clone());
        }
        let parsed = Rc::new(declarations(
            &self.body.source[self.body.range.clone()],
            &self.at,
        )?);
        Ok(self.body.parsed.get_or_init(|| parsed).clone())
    }
}

/// What parsing one or more stylesheets collects.
#[derive(Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
    pub pages: Vec<PageDeclaration>,
    /// `@property` registrations in first-registration order.
    pub properties: Vec<(Rc<str>, Registration)>,
    /// Numbers the selector compounds across every sheet.
    compounds: u32,
}

const MAX_MEDIA_DEPTH: usize = 16;

struct Context<'a> {
    clean: Rc<str>,
    sheet: &'a mut Stylesheet,
    /// The page content width in points that `@media` width conditions compare.
    width: Option<f64>,
}

impl Context<'_> {
    /// Parses the rules in `clean[start..stop]`. `@media` blocks recurse and `@property` registers;
    /// every other at-rule (`@layer`, `@supports`, `@import`, ...) is skipped whole.
    fn block(
        &mut self,
        start: usize,
        stop: usize,
        depth: usize,
        blocked: Option<Rc<str>>,
    ) -> Result<(), String> {
        let clean = self.clean.clone();
        let bytes = clean.as_bytes();
        let mut index = start;
        while index < stop {
            if whitespace(bytes[index]) {
                index += 1;
                continue;
            }
            if bytes[index] == b'@' {
                let length = bytes[index + 1..]
                    .iter()
                    .take(31)
                    .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_' || **b == b'-')
                    .count();
                let name = clean[index + 1..index + 1 + length].to_ascii_lowercase();
                let open = prelude_end(&clean, index)?;
                let brace = bytes[open] == b'{';
                let end = if brace {
                    closing(&clean, open)?
                } else {
                    open + 1
                };
                if brace && name == "page" {
                    page_rule(&clean, index, &mut self.sheet.pages)?;
                } else if brace && name == "media" {
                    if depth >= MAX_MEDIA_DEPTH {
                        return Err(format!(
                            "@media nests deeper than {MAX_MEDIA_DEPTH} levels at {}",
                            position(&clean, index)
                        ));
                    }
                    let prelude = clean[index + 6..open].trim();
                    let (applies, reason) = match media(prelude, self.width) {
                        Ok(applies) => (applies, blocked.clone()),
                        Err(message) => (
                            true,
                            blocked.clone().or_else(|| {
                                Some(
                                    format!("unsupported media query \"{prelude}\" ({message})")
                                        .into(),
                                )
                            }),
                        ),
                    };
                    if applies {
                        self.block(open + 1, end - 1, depth + 1, reason)?;
                    }
                } else if brace && name == "property" && depth == 0 {
                    let custom = clean[index + 9..open].trim();
                    let registration = register(custom, &clean[open + 1..end - 1])?;
                    let properties = &mut self.sheet.properties;
                    match properties.iter_mut().find(|(own, _)| &**own == custom) {
                        Some(entry) => entry.1 = registration,
                        None => properties.push((custom.into(), registration)),
                    }
                }
                index = end;
                continue;
            }
            let open = clean[index..].find('{').map(|at| index + at);
            let open = match open {
                Some(open) if open > index && open < stop && !clean[index..open].contains('}') => {
                    open
                }
                _ => return Err(invalid(&clean, index)),
            };
            let end = closing(&clean, open)?;
            let body = Rc::new(Body {
                source: clean.clone(),
                range: open + 1..end - 1,
                parsed: OnceCell::new(),
            });
            let at = format!(" at {}", position(&clean, index));
            let selectors = split(&clean[index..open]);
            index = end;
            for raw in selectors {
                let text = raw.trim();
                let at = format!(" in selector {}{at}", json_string(text));
                let Some(compiled) = selector(text, &at, &mut self.sheet.compounds)? else {
                    continue;
                };
                self.sheet.rules.push(Rule {
                    specificity: specificity(&compiled),
                    selector: compiled,
                    order: self.sheet.rules.len(),
                    at,
                    blocked: blocked.clone(),
                    body: body.clone(),
                });
            }
        }
        Ok(())
    }
}

/// Adds the rules, `@page` declarations, and `@property` registrations of `source` to `sheet`.
/// `width` is the page content width `@media` width conditions compare; None matches them all.
pub fn parse(source: &str, width: Option<f64>, sheet: &mut Stylesheet) -> Result<(), String> {
    let clean: Rc<str> = without_comments(source).into();
    let length = clean.len();
    Context {
        clean,
        sheet,
        width,
    }
    .block(0, length, 0, None)
}
