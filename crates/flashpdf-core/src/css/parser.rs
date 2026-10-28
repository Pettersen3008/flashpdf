use crate::css::Message;
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

pub(super) fn declarations(source: &str, context: &str) -> Result<Declarations, Message> {
    let mut result = Declarations::default();
    for item in source.split(';') {
        if item.trim().is_empty() {
            continue;
        }
        let colon = item.find(':').filter(|&colon| colon > 0);
        let Some(colon) = colon else {
            return Err(format!("invalid CSS declaration: {}{context}", item.trim()).into());
        };
        let name = item[..colon].trim();
        // A vendor-prefixed property targets one browser engine; the standard one beside it counts.
        if ["-webkit-", "-moz-", "-ms-", "-o-"].iter().any(|prefix| {
            name.len() > prefix.len()
                && name.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
        }) {
            continue;
        }
        let key = property(name, context)?;
        let value = item[colon + 1..].trim();
        let (target, value) = match important(value) {
            Some(value) => (&mut result.important, value),
            None => (&mut result.normal, value),
        };
        target.set(key, Value::Text(value.into()));
    }
    Ok(result)
}

fn invalid(source: &str, index: usize) -> Message {
    format!("invalid CSS stylesheet at {}", position(source, index)).into()
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
fn closing(source: &str, open: usize) -> Result<usize, Message> {
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
fn prelude_end(source: &str, start: usize) -> Result<usize, Message> {
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
fn page_rule(source: &str, start: usize, pages: &mut Vec<PageDeclaration>) -> Result<(), Message> {
    let open = source[start..]
        .find('{')
        .map(|at| start + at)
        .ok_or_else(|| invalid(source, start))?;
    let at = format!(" in @page at {}", position(source, start));
    let selector = source[start + 5..open].trim();
    if !selector.is_empty() {
        return Err(format!(
            "unsupported @page selector: {selector} (one size and margin apply to every page){at}"
        )
        .into());
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
        )
        .into());
    }
    for item in body.split(';') {
        if item.trim().is_empty() {
            continue;
        }
        let Some(colon) = item.find(':').filter(|&colon| colon > 0) else {
            return Err(format!("invalid CSS declaration: {}{at}", item.trim()).into());
        };
        let name = item[..colon].trim();
        if !PAGE_PROPERTIES.contains(&name) {
            return Err(format!(
                "unsupported @page property: {name} (only size and margin apply){at}"
            )
            .into());
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
fn media(prelude: &str, width: Option<f64>) -> Result<bool, Message> {
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

fn register(name: &str, body: &str) -> Result<Registration, Message> {
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
        return Err(format!("invalid @property {name}: it needs syntax and inherits").into());
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
    /// First-declaration ranks from outermost to innermost layer, ending in the implicit sublayer.
    pub layer: Vec<usize>,
    body: Rc<Body>,
}

impl Rule {
    pub fn declarations(&self) -> Result<Rc<Declarations>, Message> {
        if let Some(blocked) = &self.blocked {
            return Err(format!("{blocked}{}", self.at).into());
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
    /// Cascade layer names, dotted when nested, in the order they first appear.
    layers: Vec<Rc<str>>,
    /// Numbers the selector compounds across every sheet.
    compounds: u32,
    selector_bytes: usize,
}

const MAX_MEDIA_DEPTH: usize = 16;

struct Context<'a> {
    clean: Rc<str>,
    sheet: &'a mut Stylesheet,
    /// The page content width in points that `@media` width conditions compare.
    width: Option<f64>,
}

/// Where a block's rules go: the `@media` reason that blocks them, and their cascade layer.
#[derive(Clone, Default)]
struct Scope {
    blocked: Option<Rc<str>>,
    layer: Option<Rc<str>>,
}

impl Context<'_> {
    /// Layer `name` inside `scope`, dotted when nested, registered on first sight.
    fn layer(&mut self, scope: &Scope, name: &str) -> Result<Rc<str>, Message> {
        if !name.starts_with('\0')
            && !name.split('.').all(|part| {
                !part.is_empty()
                    && !part.contains('\\')
                    && super::selector::name_end(part, 0) == part.len()
                    && !part.as_bytes()[0].is_ascii_digit()
                    && !["initial", "inherit", "unset", "revert", "revert-layer"]
                        .contains(&part.to_ascii_lowercase().as_str())
            })
        {
            return Err(format!("unsupported CSS layer name: {name}").into());
        }
        let full: Rc<str> = match &scope.layer {
            Some(outer) => format!("{outer}.{name}").into(),
            None => name.into(),
        };
        if full.split('.').count() > MAX_MEDIA_DEPTH {
            return Err(format!("CSS layers nest deeper than {MAX_MEDIA_DEPTH} levels").into());
        }
        for end in full
            .match_indices('.')
            .map(|(at, _)| at)
            .chain(std::iter::once(full.len()))
        {
            let prefix: Rc<str> = full[..end].into();
            if !self.sheet.layers.contains(&prefix) {
                self.sheet.layers.push(prefix);
            }
        }
        Ok(full)
    }

    fn rank(&self, scope: &Scope) -> Vec<usize> {
        let mut rank = Vec::new();
        if let Some(layer) = &scope.layer {
            for end in layer
                .match_indices('.')
                .map(|(at, _)| at)
                .chain(std::iter::once(layer.len()))
            {
                rank.push(
                    self.sheet
                        .layers
                        .iter()
                        .position(|own| **own == layer[..end])
                        .unwrap(),
                );
            }
        }
        // Each layer's own declarations follow all of its sublayers.
        rank.push(usize::MAX);
        rank
    }

    /// The scope inside the block at-rule at `index`, or None when its block does not apply.
    /// `@media` and `@supports` evaluate, `@layer` names a layer, and every other at-rule
    /// (`@import`, `@font-face`, ...) is skipped whole.
    fn at_rule(
        &mut self,
        index: usize,
        open: usize,
        depth: usize,
        scope: &Scope,
    ) -> Result<Option<Scope>, Message> {
        let clean = self.clean.clone();
        let length = clean.as_bytes()[index + 1..]
            .iter()
            .take(31)
            .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_' || **b == b'-')
            .count();
        let name = clean[index + 1..index + 1 + length].to_ascii_lowercase();
        let prelude = clean[index + 1 + length..open].trim();
        if !matches!(name.as_str(), "media" | "supports" | "layer") {
            return Ok(None);
        }
        if depth >= MAX_MEDIA_DEPTH {
            return Err(format!(
                "@{name} nests deeper than {MAX_MEDIA_DEPTH} levels at {}",
                position(&clean, index)
            )
            .into());
        }
        let mut inner = scope.clone();
        match name.as_str() {
            "media" => match media(prelude, self.width) {
                Ok(false) => return Ok(None),
                Ok(true) => {}
                Err(message) => {
                    inner.blocked = inner.blocked.or_else(|| {
                        Some(format!("unsupported media query \"{prelude}\" ({message})").into())
                    })
                }
            },
            "supports" if !supports(prelude, 0)? => return Ok(None),
            "supports" => {}
            // An anonymous layer is its own, unnamed in the source.
            _ if prelude.is_empty() => {
                let anonymous = format!("\0{}", self.sheet.layers.len());
                inner.layer = Some(self.layer(scope, &anonymous)?);
            }
            _ => inner.layer = Some(self.layer(scope, prelude)?),
        }
        Ok(Some(inner))
    }

    /// Parses the rules in `clean[start..stop]`; `@page` and `@property` register.
    fn block(
        &mut self,
        start: usize,
        stop: usize,
        depth: usize,
        scope: &Scope,
    ) -> Result<(), Message> {
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
                } else if brace && name == "property" && depth == 0 {
                    let custom = clean[index + 9..open].trim();
                    let registration = register(custom, &clean[open + 1..end - 1])?;
                    let properties = &mut self.sheet.properties;
                    match properties.iter_mut().find(|(own, _)| &**own == custom) {
                        Some(entry) => entry.1 = registration,
                        None => properties.push((custom.into(), registration)),
                    }
                } else if !brace && name == "layer" {
                    // `@layer theme, base;` fixes the order before any block.
                    for layer in clean[index + 6..open].split(',') {
                        self.layer(scope, layer.trim())?;
                    }
                } else if brace {
                    if let Some(inner) = self.at_rule(index, open, depth, scope)? {
                        self.block(open + 1, end - 1, depth + 1, &inner)?;
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
            self.style(&clean[index..open], index, open, end, depth, scope)?;
            index = end;
        }
        Ok(())
    }

    /// A style rule: `selectors` at `index`, its body `open..end`. Nested rules (`&:hover { }`,
    /// `.b { }`) and nested conditional blocks retain declaration source order.
    fn style(
        &mut self,
        selectors: &str,
        index: usize,
        open: usize,
        end: usize,
        depth: usize,
        scope: &Scope,
    ) -> Result<(), Message> {
        let clean = self.clean.clone();
        if !clean[open + 1..end - 1].contains('{') {
            return self.rules(selectors, index, clean.clone(), open + 1..end - 1, scope);
        }
        let mut own = String::new();
        let mut at = open + 1;
        while at < end - 1 {
            match prelude_end(&clean, at).ok().filter(|stop| *stop < end - 1) {
                Some(brace) if clean.as_bytes()[brace] == b'{' => {
                    let close = closing(&clean, brace)?;
                    let start = at + clean[at..brace].len() - clean[at..brace].trim_start().len();
                    let length = own.len();
                    self.rules(
                        selectors,
                        index,
                        std::mem::take(&mut own).into(),
                        0..length,
                        scope,
                    )?;
                    if clean.as_bytes()[start] == b'@' {
                        if let Some(inner) = self.at_rule(start, brace, depth, scope)? {
                            self.style(selectors, start, brace, close, depth + 1, &inner)?;
                        }
                    } else if depth >= MAX_MEDIA_DEPTH {
                        return Err(format!(
                            "nested rules go deeper than {MAX_MEDIA_DEPTH} levels at {}",
                            position(&clean, start)
                        )
                        .into());
                    } else {
                        let inner = nest(selectors, clean[start..brace].trim())?;
                        self.style(&inner, start, brace, close, depth + 1, scope)?;
                    }
                    at = close;
                }
                Some(semicolon) => {
                    own.push_str(&clean[at..=semicolon]);
                    at = semicolon + 1;
                }
                None => {
                    own.push_str(&clean[at..end - 1]);
                    at = end - 1;
                }
            }
        }
        let length = own.len();
        self.rules(selectors, index, own.into(), 0..length, scope)
    }

    /// Adds a rule per selector in `selectors`, all sharing the body `source[range]`.
    fn rules(
        &mut self,
        selectors: &str,
        index: usize,
        source: Rc<str>,
        range: Range<usize>,
        scope: &Scope,
    ) -> Result<(), Message> {
        let empty = source[range.clone()].trim().is_empty();
        let body = Rc::new(Body {
            source,
            range,
            parsed: OnceCell::new(),
        });
        let at = format!(" at {}", position(&self.clean, index));
        let layer = self.rank(scope);
        for raw in split(selectors) {
            let text = raw.trim();
            // Include rule overhead so many tiny selectors cannot bypass the budget.
            self.sheet.selector_bytes = self
                .sheet
                .selector_bytes
                .saturating_add(text.len().saturating_mul(2).saturating_add(256));
            if self.sheet.selector_bytes > 1024 * 1024 {
                return Err("CSS selector budget exceeds 1 MiB".into());
            }
            let at = format!(" in selector {}{at}", json_string(text));
            let Some(compiled) = selector(text, &at, &mut self.sheet.compounds)? else {
                continue;
            };
            if empty {
                continue;
            }
            self.sheet.rules.push(Rule {
                specificity: specificity(&compiled),
                selector: compiled,
                order: self.sheet.rules.len(),
                at,
                blocked: scope.blocked.clone(),
                layer: layer.clone(),
                body: body.clone(),
            });
        }
        Ok(())
    }
}

/// A nested selector list resolved against its parent's: `&` stands for the parent, and a
/// selector without one is its descendant.
fn nest(parent: &str, nested: &str) -> Result<String, Message> {
    let parent = parent.trim();
    const MAX_EXPANSION: usize = 64 * 1024;
    if parent.len() > MAX_EXPANSION || nested.len() > MAX_EXPANSION {
        return Err("nested selector expansion exceeds 64 KiB".into());
    }
    let is = format!(":is({parent})");
    let mut result = String::new();
    for item in split(nested) {
        if !result.is_empty() {
            result.push_str(", ");
        }
        let item = item.trim();
        let mut replaced = String::new();
        let mut start = 0;
        let mut index = 0;
        while index < item.len() {
            match item.as_bytes()[index] {
                b'\\' => index += 1,
                b'"' | b'\'' => index = quoted(item.as_bytes(), index),
                b'&' => {
                    replaced.push_str(&item[start..index]);
                    replaced.push_str(&is);
                    start = index + 1;
                }
                _ => {}
            }
            if replaced.len() > MAX_EXPANSION {
                return Err("nested selector expansion exceeds 64 KiB".into());
            }
            index += 1;
        }
        if start == 0 {
            replaced.push_str(&is);
            replaced.push(' ');
        }
        replaced.push_str(&item[start..]);
        result.push_str(&replaced);
        if result.len() > MAX_EXPANSION {
            return Err("nested selector expansion exceeds 64 KiB".into());
        }
    }
    Ok(result)
}

/// Whether FlashPDF renders what an `@supports` condition tests: a declaration holds when its
/// property is supported and its value renders, `not`, `and`, `or`, and parentheses combine
/// them, and any other test, such as `selector()`, does not hold.
fn supports(condition: &str, nesting: usize) -> Result<bool, Message> {
    if nesting >= MAX_MEDIA_DEPTH {
        return Err(
            format!("@supports condition nests deeper than {MAX_MEDIA_DEPTH} levels").into(),
        );
    }
    let condition = condition.trim();
    let bytes = condition.as_bytes();
    let mut depth = 0usize;
    let mut terms = Vec::new();
    let mut operator = None;
    let mut start = 0;
    let mut index = 0;
    let mut first_close = None;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'"' | b'\'' => {
                index = quoted(bytes, index);
                if index == bytes.len() {
                    return Ok(false);
                }
            }
            b'(' => depth += 1,
            b')' => {
                let Some(next) = depth.checked_sub(1) else {
                    return Ok(false);
                };
                depth = next;
                if depth == 0 && first_close.is_none() {
                    first_close = Some(index);
                }
            }
            byte if depth == 0 && whitespace(byte) && index >= start => {
                let after = condition[index + 1..].trim_start();
                for word in ["and", "or"] {
                    let joined = after
                        .get(..word.len())
                        .is_some_and(|text| text.eq_ignore_ascii_case(word))
                        && after[word.len()..].starts_with(|c: char| c.is_whitespace() || c == '(');
                    if joined {
                        if operator.is_some_and(|own| own != word) {
                            return Ok(false);
                        }
                        operator = Some(word);
                        terms.push(&condition[start..index]);
                        start = condition.len() - after.len() + word.len();
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    if depth != 0 {
        return Ok(false);
    }
    let not = condition
        .get(..3)
        .is_some_and(|text| text.eq_ignore_ascii_case("not"))
        && condition[3..].starts_with(char::is_whitespace);
    if not {
        return if operator.is_some() {
            Ok(false)
        } else {
            supports(&condition[3..], nesting + 1).map(|value| !value)
        };
    }
    if let Some(operator) = operator {
        terms.push(&condition[start..]);
        let mut result = operator == "and";
        for term in terms {
            let value = supports(term, nesting + 1)?;
            if operator == "and" {
                result &= value;
            } else {
                result |= value;
            }
        }
        return Ok(result);
    }
    if first_close != bytes.len().checked_sub(1) {
        return Ok(false);
    }
    let Some(inner) = condition
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .map(str::trim)
    else {
        return Ok(false);
    };
    if inner.starts_with('(')
        || inner
            .get(..3)
            .is_some_and(|text| text.eq_ignore_ascii_case("not"))
    {
        return supports(inner, nesting + 1);
    }
    Ok(inner
        .split_once(':')
        .is_some_and(|(name, value)| super::renders(name.trim(), value.trim())))
}

/// Adds the rules, `@page` declarations, and `@property` registrations of `source` to `sheet`.
/// `width` is the page content width `@media` width conditions compare; None matches them all.
pub fn parse(source: &str, width: Option<f64>, sheet: &mut Stylesheet) -> Result<(), Message> {
    let clean: Rc<str> = without_comments(source).into();
    let length = clean.len();
    Context {
        clean,
        sheet,
        width,
    }
    .block(0, length, 0, &Scope::default())
}
