//! A strict HTML reader for the element subset FlashPDF renders. It builds the same nodes as
//! element records, so the cascade and compile are shared.
//!
//! It takes what HTML lets an author leave out: the end tags of `p`, `li`, `dt`, `dd`, table
//! sections, rows, and cells, an implied `tbody`, and the `html`, `head`, and `body` wrappers.
//! Anything a browser would repair, such as misnested or missing end tags, rejects with its
//! line and column instead.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::compile::{Bookmark, Element, InlineStyle, Node, Picture};
use crate::css::TAGS;
use crate::Direction;

/// Element levels, as element records count them.
const MAX_DEPTH: usize = 65;
const VOID: [&str; 4] = ["img", "br", "hr", "meta"];
const WRAPPERS: [&str; 3] = ["html", "head", "body"];
/// Start tags that end an open `p`.
const P_CLOSERS: [&str; 25] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "div",
    "dl",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];
const ENTITIES: [(&str, char); 44] = [
    ("amp", '&'),
    ("lt", '<'),
    ("gt", '>'),
    ("quot", '"'),
    ("apos", '\''),
    ("nbsp", '\u{a0}'),
    ("shy", '\u{ad}'),
    ("copy", '©'),
    ("reg", '®'),
    ("trade", '™'),
    ("euro", '€'),
    ("pound", '£'),
    ("yen", '¥'),
    ("cent", '¢'),
    ("sect", '§'),
    ("para", '¶'),
    ("deg", '°'),
    ("plusmn", '±'),
    ("times", '×'),
    ("divide", '÷'),
    ("micro", 'µ'),
    ("middot", '·'),
    ("bull", '•'),
    ("hellip", '…'),
    ("ndash", '–'),
    ("mdash", '—'),
    ("minus", '−'),
    ("lsquo", '‘'),
    ("rsquo", '’'),
    ("sbquo", '‚'),
    ("ldquo", '“'),
    ("rdquo", '”'),
    ("bdquo", '„'),
    ("laquo", '«'),
    ("raquo", '»'),
    ("thinsp", '\u{2009}'),
    ("ensp", '\u{2002}'),
    ("emsp", '\u{2003}'),
    ("zwsp", '\u{200b}'),
    ("zwnj", '\u{200c}'),
    ("zwj", '\u{200d}'),
    ("frac12", '½'),
    ("frac14", '¼'),
    ("frac34", '¾'),
];

/// The nodes and `<style>` sheets of one HTML string.
pub struct Document {
    pub nodes: Vec<Node>,
    pub styles: Vec<String>,
}

/// `images` maps an `<img src>` to the picture the host staged under that name.
pub fn parse(source: &str, images: &HashMap<String, Picture>) -> Result<Document, String> {
    let source: Cow<str> = if source.contains('\r') {
        source.replace("\r\n", "\n").replace('\r', "\n").into()
    } else {
        source.into()
    };
    let mut parser = Parser {
        source: &source,
        at: 0,
        images,
        open: Vec::new(),
        roots: Vec::new(),
        styles: Vec::new(),
    };
    parser
        .run()
        .map_err(|(message, at)| format!("{message} at {}", position(&source, at)))?;
    Ok(Document {
        nodes: parser.roots,
        styles: parser.styles,
    })
}

type Failure = (String, usize);

struct Open {
    element: Element,
    at: usize,
}

struct Parser<'a> {
    source: &'a str,
    at: usize,
    images: &'a HashMap<String, Picture>,
    open: Vec<Open>,
    roots: Vec<Node>,
    styles: Vec<String>,
}

fn fail<T>(message: impl Into<String>, at: usize) -> Result<T, Failure> {
    Err((message.into(), at))
}

/// Whether an open element's end tag is implied by the start tag `next`.
fn closed_by(open: &str, next: &str) -> bool {
    match open {
        "li" => next == "li",
        "dt" | "dd" => matches!(next, "dt" | "dd"),
        "p" => P_CLOSERS.contains(&next),
        "td" | "th" => matches!(next, "td" | "th" | "tr" | "tbody" | "thead" | "tfoot"),
        "tr" => matches!(next, "tr" | "tbody" | "thead" | "tfoot"),
        "thead" | "tbody" | "tfoot" => matches!(next, "tbody" | "tfoot"),
        "head" => next == "body",
        _ => false,
    }
}

/// Whether an element's end tag may be left for its parent's end tag, or the end of input.
fn optional_end(tag: &str) -> bool {
    matches!(
        tag,
        "li" | "dt"
            | "dd"
            | "p"
            | "td"
            | "th"
            | "tr"
            | "thead"
            | "tbody"
            | "tfoot"
            | "html"
            | "head"
            | "body"
    )
}

/// Phrasing elements, which a browser closes on its own to honour an implied end tag above them.
fn phrasing(tag: &str) -> bool {
    matches!(
        tag,
        "span"
            | "b"
            | "strong"
            | "em"
            | "i"
            | "cite"
            | "small"
            | "sub"
            | "sup"
            | "u"
            | "ins"
            | "s"
            | "del"
            | "mark"
            | "code"
            | "kbd"
            | "samp"
            | "a"
    )
}

impl Parser<'_> {
    fn run(&mut self) -> Result<(), Failure> {
        let source = self.source;
        while self.at < source.len() {
            let rest = &source[self.at..];
            if let Some(comment) = rest.strip_prefix("<!--") {
                let Some(end) = comment.find("-->") else {
                    return fail("unclosed comment", self.at);
                };
                self.at += 4 + end + 3;
            } else if rest.starts_with("<!") {
                let end = rest.find('>').map_or(rest.len(), |end| end + 1);
                let doctype = &rest[..end];
                if !doctype.eq_ignore_ascii_case("<!doctype html>")
                    || !self.roots.is_empty()
                    || !self.open.is_empty()
                {
                    return fail(
                        "only <!DOCTYPE html> may start with <! , before any element",
                        self.at,
                    );
                }
                self.at += end;
            } else if rest.starts_with("</") {
                self.end_tag()?;
            } else if rest.starts_with('<') {
                self.start_tag()?;
            } else {
                let end = rest.find('<').unwrap_or(rest.len());
                let at = self.at;
                let text = decode(&rest[..end], at)?;
                self.at += end;
                self.text(text, at)?;
            }
        }
        while let Some(open) = self.open.last() {
            if !optional_end(&open.element.tag) {
                let (tag, at) = (open.element.tag.clone(), open.at);
                return fail(format!("<{tag}> is never closed; add </{tag}>"), at);
            }
            self.pop();
        }
        Ok(())
    }

    fn text(&mut self, text: Cow<str>, at: usize) -> Result<(), Failure> {
        let blank = text
            .chars()
            .all(|character| character.is_ascii_whitespace());
        let top = self.open.last().map(|open| open.element.tag.as_str());
        // Whitespace between rows and around the document holds no content, as in a browser.
        let structural = [
            "html", "head", "body", "table", "thead", "tbody", "tfoot", "tr",
        ];
        if blank && top.is_none_or(|tag| structural.contains(&tag)) {
            return Ok(());
        }
        if top == Some("head") {
            return fail("text in <head>", at);
        }
        let parent = self
            .open
            .iter_mut()
            .rev()
            .find(|open| !WRAPPERS.contains(&open.element.tag.as_str()));
        match parent {
            None => fail("text outside an element; wrap it in one", at),
            Some(open) => {
                if !crate::win_ansi::valid(&text, false) {
                    return fail("unsupported control character", at);
                }
                open.element.children.push(Node::Text(text.into_owned()));
                Ok(())
            }
        }
    }

    /// Attaches the innermost open element to its parent; a wrapper hands over its children.
    fn pop(&mut self) {
        let open = self.open.pop().unwrap();
        let nodes = if WRAPPERS.contains(&open.element.tag.as_str()) {
            open.element.children
        } else {
            vec![Node::Element(open.element)]
        };
        match self.open.last_mut() {
            Some(parent) => parent.element.children.extend(nodes),
            None => self.roots.extend(nodes),
        }
    }

    fn name(&mut self) -> String {
        let rest = &self.source[self.at..];
        let length = rest
            .bytes()
            .position(|byte| !byte.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        self.at += length;
        rest[..length].to_ascii_lowercase()
    }

    fn skip_space(&mut self) {
        let rest = &self.source[self.at..];
        self.at += rest.len()
            - rest
                .trim_start_matches(|c: char| c.is_ascii_whitespace())
                .len();
    }

    fn end_tag(&mut self) -> Result<(), Failure> {
        let at = self.at;
        self.at += 2;
        let name = self.name();
        self.skip_space();
        if name.is_empty() || !self.source[self.at..].starts_with('>') {
            return fail("invalid end tag", at);
        }
        self.at += 1;
        if VOID.contains(&name.as_str()) {
            return fail(
                format!("<{name}> has no end tag in HTML; remove </{name}>"),
                at,
            );
        }
        if !self.open.iter().any(|open| open.element.tag == name) {
            return fail(format!("</{name}> closes no open element"), at);
        }
        loop {
            let top = &self.open.last().unwrap().element.tag;
            if *top == name {
                self.pop();
                return Ok(());
            }
            if !optional_end(top) {
                let open = self.open.last().unwrap();
                return fail(
                    format!(
                        "</{name}> while <{top}> from {} is open; close it first",
                        position(self.source, open.at)
                    ),
                    at,
                );
            }
            self.pop();
        }
    }

    fn attributes(
        &mut self,
        tag: &str,
        at: usize,
    ) -> Result<(Vec<(String, String)>, bool), Failure> {
        let mut attributes: Vec<(String, String)> = Vec::new();
        loop {
            self.skip_space();
            let rest = &self.source[self.at..];
            if rest.is_empty() || rest.starts_with('<') {
                return fail(format!("<{tag}> is missing its >"), at);
            }
            if rest.starts_with('>') {
                self.at += 1;
                return Ok((attributes, false));
            }
            if rest.starts_with("/>") {
                self.at += 2;
                return Ok((attributes, true));
            }
            let length = rest
                .find(|c: char| {
                    c.is_ascii_whitespace() || matches!(c, '"' | '\'' | '>' | '/' | '=' | '<')
                })
                .unwrap_or(rest.len());
            if length == 0 {
                return fail(format!("invalid character in <{tag}>"), self.at);
            }
            let name = rest[..length].to_ascii_lowercase();
            self.at += length;
            self.skip_space();
            let mut value = String::new();
            if self.source[self.at..].starts_with('=') {
                self.at += 1;
                self.skip_space();
                let rest = &self.source[self.at..];
                let start = self.at;
                let raw = match rest.chars().next() {
                    Some(quote @ ('"' | '\'')) => {
                        let Some(end) = rest[1..].find(quote) else {
                            return fail(format!("unclosed {name} value in <{tag}>"), start);
                        };
                        self.at += end + 2;
                        &rest[1..end + 1]
                    }
                    _ => {
                        let end = rest
                            .find(|c: char| {
                                c.is_ascii_whitespace()
                                    || matches!(c, '"' | '\'' | '=' | '<' | '>' | '`')
                            })
                            .unwrap_or(rest.len());
                        if end == 0 {
                            return fail(format!("{name} needs a value in <{tag}>"), start);
                        }
                        self.at += end;
                        &rest[..end]
                    }
                };
                value = decode(raw, start)?.into_owned();
            }
            if attributes.iter().any(|(other, _)| *other == name) {
                return fail(format!("duplicate attribute {name} on <{tag}>"), at);
            }
            attributes.push((name, value));
        }
    }

    fn start_tag(&mut self) -> Result<(), Failure> {
        let at = self.at;
        self.at += 1;
        let tag = self.name();
        if tag.is_empty() {
            return fail("< starts no tag; write &lt; for a literal <", at);
        }
        let (attributes, self_closing) = self.attributes(&tag, at)?;
        let void = VOID.contains(&tag.as_str());
        if self_closing && !void {
            return fail(
                format!("<{tag} /> does not close in HTML; write <{tag}></{tag}>"),
                at,
            );
        }
        let in_head = self
            .open
            .last()
            .is_some_and(|open| open.element.tag == "head");
        if tag == "style" {
            if let Some((name, _)) = attributes.first() {
                return fail(format!("unsupported attribute {name} on <style>"), at);
            }
            let rest = &self.source[self.at..];
            let end = rest
                .match_indices("</")
                .map(|(index, _)| index)
                .find(|index| {
                    rest.as_bytes()[*index..]
                        .get(..8)
                        .is_some_and(|close| close.eq_ignore_ascii_case(b"</style>"))
                });
            let Some(end) = end else {
                return fail("<style> is never closed; add </style>", at);
            };
            self.styles.push(rest[..end].to_owned());
            self.at += end + 8;
            return Ok(());
        }
        if tag == "meta" {
            let charset = matches!(&attributes[..], [(name, value)] if name == "charset" && value.eq_ignore_ascii_case("utf-8"));
            if !in_head || !charset {
                return fail("only <meta charset=\"utf-8\"> in <head> is supported", at);
            }
            return Ok(());
        }
        if WRAPPERS.contains(&tag.as_str()) {
            self.imply(&tag, at)?;
            let order = |name: &str| WRAPPERS.iter().position(|wrapper| *wrapper == name);
            let inside = self.open.last().and_then(|open| order(&open.element.tag));
            let valid = match tag.as_str() {
                "html" => self.open.is_empty() && self.roots.is_empty(),
                _ => {
                    (inside == Some(0) || self.open.is_empty() && self.roots.is_empty())
                        && !(tag == "head" && self.seen_body())
                }
            };
            if !valid {
                return fail(format!("<{tag}> must wrap the whole document"), at);
            }
            if let Some((name, _)) = attributes.first() {
                return fail(format!("unsupported attribute {name} on <{tag}>"), at);
            }
            return self.push(tag, at);
        }
        if in_head {
            return fail(format!("<{tag}> belongs in <body>"), at);
        }
        let Some(&known) = TAGS.iter().find(|known| **known == tag) else {
            return fail(format!("unsupported element <{tag}>{}", hint(&tag)), at);
        };
        if known == "svg" {
            return fail(
                "inline <svg> is not supported in HTML; pass the SVG bytes in the images option and use <img src>",
                at,
            );
        }
        self.imply(&tag, at)?;
        if tag == "tr"
            && self
                .open
                .last()
                .is_some_and(|open| open.element.tag == "table")
        {
            self.push("tbody".into(), at)?;
        }
        self.push(tag, at)?;
        let element = &mut self.open.last_mut().unwrap().element;
        for (name, value) in attributes {
            attribute(element, &name, value, self.images)
                .map_err(|message| (format!("{message} on <{}>", element.tag), at))?;
        }
        if element.tag == "img" && element.picture.is_none() {
            return fail("<img> needs a src naming an entry in the images option", at);
        }
        if void {
            self.pop();
        } else if element.tag == "pre" && self.source[self.at..].starts_with('\n') {
            self.at += 1;
        }
        Ok(())
    }

    fn seen_body(&self) -> bool {
        self.open.iter().any(|open| open.element.tag == "body")
    }

    /// Ends the open elements whose end tag `tag` implies, and rejects one a browser would end
    /// through open phrasing elements.
    fn imply(&mut self, tag: &str, at: usize) -> Result<(), Failure> {
        while self
            .open
            .last()
            .is_some_and(|open| closed_by(&open.element.tag, tag))
        {
            self.pop();
        }
        let inner = self
            .open
            .iter()
            .rev()
            .find(|open| !phrasing(&open.element.tag));
        if let Some(open) = inner.filter(|open| closed_by(&open.element.tag, tag)) {
            let top = &self.open.last().unwrap().element.tag;
            return fail(
                format!(
                    "<{tag}> would end <{}> from {} while <{top}> is open; close <{top}> first",
                    open.element.tag,
                    position(self.source, open.at)
                ),
                at,
            );
        }
        Ok(())
    }

    fn push(&mut self, tag: String, at: usize) -> Result<(), Failure> {
        if self.open.len() >= MAX_DEPTH {
            return fail("nesting exceeds 64", at);
        }
        self.open.push(Open {
            element: Element {
                tag,
                ..Element::default()
            },
            at,
        });
        Ok(())
    }
}

/// The 1-based line and column of a byte offset.
fn position(source: &str, at: usize) -> String {
    let before = &source[..at];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |index| index + 1);
    format!("{line}:{}", before[start..].chars().count() + 1)
}

fn hint(tag: &str) -> &'static str {
    match tag {
        "colgroup" | "col" => {
            ": each column's width comes from its first <th>/<td> without colspan"
        }
        "caption" => ": captions are not supported; use a <p> before the table",
        _ => "",
    }
}

fn attribute(
    element: &mut Element,
    name: &str,
    value: String,
    images: &HashMap<String, Picture>,
) -> Result<(), String> {
    let tag = element.tag.as_str();
    let integer = |name: &str| {
        value
            .trim()
            .parse::<i32>()
            .map(i64::from)
            .map_err(|_| format!("{name} must be an integer"))
    };
    let span = |name: &str, max: u16| {
        value
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|span| (1..=max).contains(span))
            .ok_or_else(|| format!("{name} must be an integer from 1 to {max}"))
    };
    match name {
        "id" => element.id = Some(value),
        "class" => {
            element.classes = value
                .split(crate::css::js_space)
                .filter(|class| !class.is_empty())
                .map(str::to_owned)
                .collect()
        }
        "style" => element.style = InlineStyle::Text(value),
        "tw" => element.tw = Some(value),
        "dir" => {
            element.dir = Some(match value.as_str() {
                "ltr" => Direction::Ltr,
                "rtl" => Direction::Rtl,
                "auto" => Direction::Auto,
                _ => return Err("dir must be \"ltr\", \"rtl\", or \"auto\"".into()),
            })
        }
        "src" if tag == "img" => {
            let picture = images
                .get(&value)
                .ok_or_else(|| format!("src {value:?} names no entry in the images option"))?;
            element.picture = Some(picture.clone());
        }
        "alt" if tag == "img" => {
            if value.chars().any(char::is_control) {
                return Err("alt must be a string without control characters".into());
            }
            element.alt = Some(value);
        }
        "href" if tag == "a" => element.href = Some(href(value)?),
        "data-bookmark" if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") => {
            element.bookmark = if value.is_empty() {
                Bookmark::Off
            } else if value.trim().is_empty()
                || value.chars().count() > 255
                || value.chars().any(char::is_control)
            {
                return Err(
                    "data-bookmark must be 1 to 255 characters without control characters, or empty"
                        .into(),
                );
            } else {
                Bookmark::Title(value)
            }
        }
        "start" if tag == "ol" => element.start = Some(integer("start")?),
        "reversed" if tag == "ol" => {
            if !value.is_empty() && value != "reversed" {
                return Err("reversed takes no value".into());
            }
            element.reversed = true;
        }
        "value" if tag == "li" => element.value = Some(integer("value")?),
        "colspan" if matches!(tag, "td" | "th") => element.col_span = Some(span("colspan", 256)?),
        "rowspan" if matches!(tag, "td" | "th") => {
            element.row_span = Some(span("rowspan", u16::MAX)?)
        }
        _ => return Err(format!("unsupported attribute {name}")),
    }
    Ok(())
}

/// The links element records accept, without the URL normalisation JSX gets from the host.
fn href(value: String) -> Result<String, String> {
    let printable = !value.is_empty() && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte));
    if let Some(id) = value.strip_prefix('#') {
        if !printable || id.is_empty() || id.len() > 255 {
            return Err(format!(
                "a link to an id is # and 1 to 255 printable ASCII characters, not {value:?}"
            ));
        }
        return Ok(value);
    }
    let lower = value.to_ascii_lowercase();
    let web = ["http://", "https://"].iter().any(|scheme| {
        lower
            .strip_prefix(scheme)
            .is_some_and(|rest| rest.starts_with(|c: char| !matches!(c, '/' | '\\' | '?' | '#')))
    });
    if !printable || !(web || lower.starts_with("mailto:") && value.len() > 7) {
        return Err(format!(
            "href must be an absolute http, https, or mailto URL in printable ASCII, not {value:?}"
        ));
    }
    Ok(value)
}

/// Replaces character references; a `&` that starts none rejects.
fn decode(raw: &str, offset: usize) -> Result<Cow<'_, str>, Failure> {
    if !raw.contains('&') {
        return Ok(raw.into());
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(index) = rest.find('&') {
        out.push_str(&rest[..index]);
        let at = offset + (raw.len() - rest.len()) + index;
        let after = &rest[index + 1..];
        let end = after.find(';').filter(|end| *end <= 32);
        let reference = end.map(|end| &after[..end]);
        let character = reference.and_then(|name| match name.strip_prefix('#') {
            Some(number) => {
                let code = match number.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16),
                    None => number.parse(),
                };
                code.ok()
                    .and_then(char::from_u32)
                    .filter(|c| !c.is_control() || c.is_ascii_whitespace())
            }
            None => ENTITIES
                .iter()
                .find(|(entity, _)| *entity == name)
                .map(|(_, character)| *character),
        });
        let Some(character) = character else {
            return fail(
                match reference {
                    Some(name) => format!("unknown character reference &{name};"),
                    None => "& starts no character reference; write &amp;".into(),
                },
                at,
            );
        };
        out.push(character);
        rest = &after[end.unwrap() + 1..];
    }
    out.push_str(rest);
    Ok(out.into())
}

#[cfg(test)]
mod tests;
