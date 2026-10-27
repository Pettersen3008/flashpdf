use std::rc::Rc;

use super::parser::{declarations, Registration, Rule};
use super::{evaluate_math, js_number, parse, point, Cascaded, Key, Prop, Stylesheet, Tree, Value};

/// An element's own style: none, a `style` string, or a style object's entries in order.
pub enum Inline<'a> {
    None,
    Text(&'a str),
    Object(Cascaded),
}

const MARGIN: Prop = Prop::of("margin");
const FONT_SIZE: Prop = Prop::of("fontSize");
const FONT_WEIGHT: Prop = Prop::of("fontWeight");
const FONT_STYLE: Prop = Prop::of("fontStyle");
const FONT_FAMILY: Prop = Prop::of("fontFamily");
const TEXT_ALIGN: Prop = Prop::of("textAlign");
const TEXT_DECORATION: Prop = Prop::of("textDecoration");
const COLOR: Prop = Prop::of("color");
const BACKGROUND_COLOR: Prop = Prop::of("backgroundColor");
const VERTICAL_ALIGN: Prop = Prop::of("verticalAlign");
const WHITE_SPACE: Prop = Prop::of("whiteSpace");
const MARGIN_LEFT: Prop = Prop::of("marginLeft");
const BORDER_BOTTOM: Prop = Prop::of("borderBottom");
const BREAK_AFTER: Prop = Prop::of("breakAfter");
const PADDING_LEFT: Prop = Prop::of("paddingLeft");
const LIST_STYLE_TYPE: Prop = Prop::of("listStyleType");

const BOLD: (Prop, &str) = (FONT_WEIGHT, "bold");
const ITALIC: (Prop, &str) = (FONT_STYLE, "italic");
const UNDERLINE: (Prop, &str) = (TEXT_DECORATION, "underline");
const STRIKE: (Prop, &str) = (TEXT_DECORATION, "line-through");
const SMALLER: (Prop, &str) = (FONT_SIZE, "0.83em");
const MONOSPACE: (Prop, &str) = (FONT_FAMILY, "monospace");
const PARAGRAPH: (Prop, &str) = (MARGIN, "1em 0");
const INDENT: (Prop, &str) = (MARGIN, "1em 40px");
const AVOID: (Prop, &str) = (BREAK_AFTER, "avoid");

/// https://github.com/chromium/chromium/blob/main/third_party/blink/renderer/core/html/resources/html.css
fn user_agent(tag: &str) -> &'static [(Prop, &'static str)] {
    match tag {
        "b" | "strong" => &[BOLD],
        "em" | "i" | "cite" | "address" => &[ITALIC],
        "th" => &[BOLD, (TEXT_ALIGN, "center")],
        "p" => &[PARAGRAPH],
        "a" => &[(COLOR, "#0000EE"), UNDERLINE],
        "u" | "ins" => &[UNDERLINE],
        "s" | "del" => &[STRIKE],
        "mark" => &[(BACKGROUND_COLOR, "yellow"), (COLOR, "black")],
        "small" => &[SMALLER],
        "sub" => &[SMALLER, (VERTICAL_ALIGN, "sub")],
        "sup" => &[SMALLER, (VERTICAL_ALIGN, "super")],
        "code" | "kbd" | "samp" => &[MONOSPACE],
        "pre" => &[MONOSPACE, (WHITE_SPACE, "pre"), PARAGRAPH],
        "blockquote" | "figure" => &[INDENT],
        "dd" => &[(MARGIN_LEFT, "40px")],
        "hr" => &[(BORDER_BOTTOM, "1pt solid black"), (MARGIN, "0.5em 0")],
        "h1" => &[(FONT_SIZE, "2em"), BOLD, AVOID, (MARGIN, "0.67em 0")],
        "h2" => &[(FONT_SIZE, "1.5em"), BOLD, AVOID, (MARGIN, "0.83em 0")],
        "h3" => &[(FONT_SIZE, "1.17em"), BOLD, AVOID, (MARGIN, "1em 0")],
        "h4" => &[(FONT_SIZE, "1em"), BOLD, AVOID, (MARGIN, "1.33em 0")],
        "h5" => &[(FONT_SIZE, "0.83em"), BOLD, AVOID, (MARGIN, "1.67em 0")],
        "h6" => &[(FONT_SIZE, "0.67em"), BOLD, AVOID, (MARGIN, "2.33em 0")],
        _ => &[],
    }
}

/// The user-agent rules that depend on nesting: a list inside a list has no vertical margin, and
/// an unordered one cycles through disc, circle, and square as it goes deeper.
fn list_styles(tree: &Tree, node: u32) -> Vec<(Prop, &'static str)> {
    let tag = tree.tag(node);
    if !matches!(tag, "ul" | "ol" | "dl") {
        return Vec::new();
    }
    let mut depth = 0;
    let mut nested = false;
    let mut ancestor = tree.parent(node);
    while let Some(current) = ancestor {
        match tree.tag(current) {
            "ul" | "ol" => depth += 1,
            "dl" => nested = true,
            _ => {}
        }
        ancestor = tree.parent(current);
    }
    let mut styles = vec![(MARGIN, if nested || depth > 0 { "0" } else { "1em 0" })];
    if tag != "dl" {
        styles.push((PADDING_LEFT, "40px"));
        let marker = if tag == "ol" {
            "decimal"
        } else {
            ["disc", "circle", "square"][depth.min(2)]
        };
        styles.push((LIST_STYLE_TYPE, marker));
    }
    styles
}

fn cascade(target: &mut Cascaded, source: &Cascaded) {
    for (key, value) in &source.0 {
        target.set(key.clone(), value.clone());
    }
}

const MAX_VARIABLE_EXPANSIONS: usize = 1024;
const MAX_VARIABLE_OUTPUT: usize = 64 * 1024;

/// JavaScript's `length`: UTF-16 units, which never exceed the UTF-8 length.
fn too_large(text: &str) -> bool {
    text.len() > MAX_VARIABLE_OUTPUT && text.encode_utf16().count() > MAX_VARIABLE_OUTPUT
}

struct Variables<'a> {
    values: &'a Cascaded,
    resolving: Vec<Rc<str>>,
    expansions: &'a mut usize,
}

impl Variables<'_> {
    /// Replaces each top-level `var(--name, fallback)`, matching parentheses so the fallback may
    /// hold `rgb(...)` or a nested `var()`.
    fn resolve(&mut self, value: &str) -> Result<String, String> {
        // Bounds the rescan of each nested fallback.
        if too_large(value) {
            return Err("CSS variable output is too large".into());
        }
        let bytes = value.as_bytes();
        let mut result = String::new();
        let mut index = 0;
        while let Some(start) = value[index..].find("var(").map(|at| index + at) {
            let mut depth = 0usize;
            let mut comma = None;
            let mut end = start + 3;
            while end < bytes.len() {
                match bytes[end] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b',' if depth == 1 && comma.is_none() => comma = Some(end),
                    _ => {}
                }
                end += 1;
            }
            let name = value[start + 4..comma.unwrap_or(end)].trim();
            let valid = name.len() > 2
                && name.starts_with("--")
                && name[2..]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
            if end == bytes.len() || !valid {
                let reference = &value[start..(end + 1).min(bytes.len())];
                return Err(format!("invalid CSS variable reference: {reference}"));
            }
            let fallback = comma.map(|comma| value[comma + 1..end].trim());
            result.push_str(&value[index..start]);
            result.push_str(&self.substitute(name, fallback)?);
            index = end + 1;
        }
        result.push_str(&value[index..]);
        if too_large(&result) {
            return Err("CSS variable output is too large".into());
        }
        Ok(result)
    }

    fn substitute(&mut self, name: &str, fallback: Option<&str>) -> Result<String, String> {
        *self.expansions += 1;
        if *self.expansions > MAX_VARIABLE_EXPANSIONS {
            return Err("CSS variable expansion is too large".into());
        }
        if self.resolving.iter().any(|own| &**own == name) {
            return Err(format!("cyclic CSS variable: {name}"));
        }
        let candidate = match self.values.get(&Key::Custom(name.into())) {
            Some(Value::Text(text)) => text.to_string(),
            Some(&Value::Number(number)) => return Ok(js_number(number)),
            None => match fallback {
                Some(fallback) => fallback.to_string(),
                None => {
                    return Err(format!(
                        "undefined CSS variable: {name} (rules inside an @layer block, such as Tailwind's :root theme, are skipped)"
                    ))
                }
            },
        };
        self.resolving.push(name.into());
        let resolved = self.resolve(&candidate);
        self.resolving.pop();
        resolved
    }
}

/// Matches stylesheet rules against a tree and cascades each element's declarations.
pub struct Resolver {
    /// By specificity, then source order.
    rules: Vec<Rule>,
    properties: Vec<(Rc<str>, Registration)>,
}

impl Resolver {
    /// `width` is the page content width in points that `@media` width conditions compare.
    pub fn new(sheets: &[&str], width: f64) -> Result<Self, String> {
        let mut sheet = Stylesheet::default();
        for source in sheets {
            parse(source, Some(width), &mut sheet)?;
        }
        sheet.rules.sort_by(|a, b| {
            a.specificity
                .cmp(&b.specificity)
                .then(a.order.cmp(&b.order))
        });
        Ok(Self {
            rules: sheet.rules,
            properties: sheet.properties,
        })
    }

    /// The cascaded declarations of element `node`, with var(), calc(), and the font size
    /// resolved: what normalisation reads. `parent` is the parent's result and
    /// `parent_font_size` its normalised font size. Errors name the element path.
    pub fn cascade(
        &self,
        tree: &mut Tree,
        node: u32,
        inline: &Inline,
        parent: &Cascaded,
        parent_font_size: Option<f64>,
        single_line: bool,
    ) -> Result<Cascaded, String> {
        self.cascaded(tree, node, inline, parent, parent_font_size, single_line)
            .map_err(|message| format!("{message} on {}", tree.path(node)))
    }

    fn cascaded(
        &self,
        tree: &mut Tree,
        node: u32,
        inline: &Inline,
        parent: &Cascaded,
        parent_font_size: Option<f64>,
        single_line: bool,
    ) -> Result<Cascaded, String> {
        let matched: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|rule| tree.matches(&rule.selector, node))
            .collect();
        let mut result = Cascaded::default();
        for (key, value) in &parent.0 {
            let inherits = match key {
                Key::Prop(prop) => prop.inherited(),
                Key::Custom(name) => self
                    .properties
                    .iter()
                    .find(|(own, _)| own == name)
                    .is_none_or(|(_, own)| own.inherits),
                Key::Other(_) => false,
            };
            if inherits {
                result.0.push((key.clone(), value.clone()));
            }
        }
        // A registered `initial-value` is the value of a property nothing sets, ahead of any var() fallback.
        for (name, registration) in &self.properties {
            let key = Key::Custom(name.clone());
            if let (Some(initial), None) = (&registration.initial, result.get(&key)) {
                result.0.push((key, Value::Text(initial.clone())));
            }
        }
        let defaults = user_agent(tree.tag(node))
            .iter()
            .copied()
            .chain(list_styles(tree, node));
        for (prop, value) in defaults {
            result.set(Key::Prop(prop), Value::Text(value.into()));
        }
        // Single-line furniture has no default block spacing; author margins still reject.
        if single_line {
            result.remove(&Key::Prop(MARGIN));
        }
        let own = match inline {
            Inline::None => Default::default(),
            Inline::Text(text) => declarations(text, " in inline style")?,
            Inline::Object(entries) => super::Declarations {
                normal: entries.clone(),
                important: Cascaded::default(),
            },
        };
        for rule in &matched {
            cascade(&mut result, &rule.declarations()?.normal);
        }
        cascade(&mut result, &own.normal);
        for rule in &matched {
            cascade(&mut result, &rule.declarations()?.important);
        }
        cascade(&mut result, &own.important);
        let mut expansions = 0;
        for index in 0..result.0.len() {
            let (key, Value::Text(text)) = &result.0[index] else {
                continue;
            };
            if !text.contains("var(") {
                continue;
            }
            let resolving = match key {
                Key::Custom(name) => vec![name.clone()],
                _ => Vec::new(),
            };
            let resolved = Variables {
                values: &result,
                resolving,
                expansions: &mut expansions,
            }
            .resolve(text)?;
            result.0[index].1 = Value::Text(resolved.into());
        }
        let parent_size = parent_font_size.unwrap_or(12.0);
        if let Some((_, value)) = result
            .0
            .iter_mut()
            .find(|(key, _)| *key == Key::Prop(FONT_SIZE))
        {
            // `inherit` and `initial` resolve in normalisation.
            if let Value::Text(text) = value {
                if !matches!(
                    text.trim().to_ascii_lowercase().as_str(),
                    "inherit" | "initial"
                ) {
                    let size = point(
                        &evaluate_math("fontSize", text, parent_size)?,
                        parent_size,
                        true,
                    )?;
                    *value = Value::Number(size);
                }
            }
        }
        let em = match result.get(&Key::Prop(FONT_SIZE)) {
            Some(&Value::Number(size)) => size,
            _ => parent_size,
        };
        for (key, value) in &mut result.0 {
            if let (Key::Prop(_) | Key::Other(_), Value::Text(text)) = (&*key, &*value) {
                let std::borrow::Cow::Owned(evaluated) = evaluate_math(key.name(), text, em)?
                else {
                    continue;
                };
                *value = Value::Text(evaluated.into());
            }
        }
        Ok(result)
    }
}
