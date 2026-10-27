use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use super::whitespace;

/// Every element tag, in the order `tree.ts` lists them.
pub const TAGS: &[&str] = &[
    "main",
    "div",
    "section",
    "article",
    "header",
    "footer",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "blockquote",
    "address",
    "figure",
    "figcaption",
    "nav",
    "aside",
    "p",
    "span",
    "b",
    "strong",
    "em",
    "i",
    "cite",
    "small",
    "sub",
    "sup",
    "u",
    "ins",
    "s",
    "del",
    "mark",
    "code",
    "kbd",
    "samp",
    "pre",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "a",
    "img",
    "svg",
    "hr",
    "br",
];

pub(super) fn tag_index(tag: &str) -> Option<u8> {
    TAGS.iter()
        .position(|own| *own == tag)
        .map(|index| index as u8)
}

const MAX_SELECTOR_DEPTH: usize = 16;

/// Pseudo-classes and CSS 2 pseudo-elements that match nothing in a PDF: no pointer, focus, form, or generated content.
const NEVER: &[&str] = &[
    "before",
    "after",
    "first-line",
    "first-letter",
    "hover",
    "active",
    "focus",
    "focus-visible",
    "focus-within",
    "visited",
    "target",
    "target-within",
    "local-link",
    "checked",
    "indeterminate",
    "default",
    "enabled",
    "disabled",
    "required",
    "optional",
    "read-only",
    "read-write",
    "placeholder-shown",
    "valid",
    "invalid",
    "user-valid",
    "user-invalid",
    "in-range",
    "out-of-range",
    "autofill",
    "fullscreen",
    "modal",
    "open",
    "popover-open",
    "picture-in-picture",
    "paused",
    "playing",
    "muted",
    "seeking",
    "buffering",
    "stalled",
    "volume-locked",
    "current",
    "past",
    "future",
    "host",
    "host-context",
    "-moz-focusring",
    "-moz-ui-invalid",
    "-webkit-autofill",
    "-moz-placeholder",
    "-ms-input-placeholder",
];

#[derive(Debug)]
enum Pseudo {
    Root,
    Empty,
    Nth {
        a: i64,
        b: i64,
        last: bool,
        of_type: bool,
    },
    Is(Vec<Complex>),
    Where(Vec<Complex>),
    Not(Vec<Complex>),
}

#[derive(Debug)]
struct Compound {
    /// Unique among a resolver's selectors; the matcher caches verdicts under it.
    id: u32,
    tag: Option<u8>,
    ids: Vec<Box<str>>,
    classes: Vec<Box<str>>,
    pseudos: Vec<Pseudo>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Combinator {
    Descendant,
    Child,
    Next,
    Subsequent,
}

/// Compounds joined by combinators: `combinators[i]` sits between `compounds[i]` and `compounds[i + 1]`.
#[derive(Debug)]
pub struct Complex {
    compounds: Vec<Compound>,
    combinators: Vec<Combinator>,
}

pub type Specificity = [u32; 3];

/// What the rightmost compound demands of every element it matches: an id, else a class,
/// else a tag. `Any` holds only pseudo-classes or `*`, so every element is a candidate.
pub(super) enum Bucket<'a> {
    Id(&'a str),
    Class(&'a str),
    Tag(u8),
    Any,
}

impl Complex {
    pub(super) fn bucket(&self) -> Bucket<'_> {
        let last = &self.compounds[self.compounds.len() - 1];
        if let Some(id) = last.ids.first() {
            Bucket::Id(id)
        } else if let Some(class) = last.classes.first() {
            Bucket::Class(class)
        } else {
            last.tag.map_or(Bucket::Any, Bucket::Tag)
        }
    }
}

/// End of the escape at the backslash `index`: up to six hex digits and one whitespace, or one
/// other character. None for a backslash that ends the source or precedes a newline.
fn escape_end(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let next = *bytes.get(index + 1)?;
    if next == b'\n' {
        return None;
    }
    if !next.is_ascii_hexdigit() {
        let width = source[index + 1..].chars().next().map_or(1, char::len_utf8);
        return Some(index + 1 + width);
    }
    let mut end = index + 2;
    while end < index + 7 && bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
        end += 1;
    }
    if bytes.get(end).copied().is_some_and(whitespace) {
        end += 1;
    }
    Some(end)
}

/// End of the name at `start`: letters, digits, `_`, `-`, and escapes.
pub(super) fn name_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = start;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'\\' {
            match escape_end(source, index) {
                Some(end) => index = end,
                None => break,
            }
        } else if byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' {
            index += 1;
        } else {
            break;
        }
    }
    index
}

/// CSS escapes: `\/` is `/`, `\32 ` is `2`; an invalid code point is U+FFFD.
pub(super) fn unescape(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut characters = name.char_indices().peekable();
    while let Some((at, character)) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.peek() {
            None | Some((_, '\n')) => result.push('\\'),
            Some(&(_, next)) if next.is_ascii_hexdigit() => {
                let digits = name[at + 1..]
                    .bytes()
                    .take(6)
                    .take_while(u8::is_ascii_hexdigit)
                    .count();
                for _ in 0..digits {
                    characters.next();
                }
                if characters
                    .peek()
                    .is_some_and(|&(_, c)| c.is_ascii() && whitespace(c as u8))
                {
                    characters.next();
                }
                let code = u32::from_str_radix(&name[at + 1..at + 1 + digits], 16).unwrap_or(0);
                result.push(match char::from_u32(code) {
                    Some(decoded) if code != 0 => decoded,
                    _ => '\u{fffd}',
                });
            }
            Some(&(_, next)) => {
                characters.next();
                result.push(next);
            }
        }
    }
    result
}

/// Index of the quote closing the string that opens at `start`.
pub(super) fn quoted(source: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < source.len() {
        if source[index] == b'\\' {
            index += 1;
        } else if source[index] == source[start] {
            return index;
        }
        index += 1;
    }
    source.len()
}

/// Index of the bracket closing the `(` or `[` at `open`.
fn bracket_end(source: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < source.len() {
        match source[index] {
            b'\\' => index += 1,
            b'"' | b'\'' => index = quoted(source, index),
            b'(' | b'[' => depth += 1,
            b')' | b']' => {
                depth = depth.wrapping_sub(1);
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Splits at the commas outside brackets, strings, and escapes: `\,` belongs to a class name.
pub(super) fn split(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0isize;
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 1,
            b'"' | b'\'' => index = quoted(bytes, index),
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b',' if depth == 0 => {
                parts.push(&source[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(&source[start.min(source.len())..]);
    parts
}

/// `odd`, `even`, `B`, or `An+B` as (a, b); None for anything else, `of <selector>` included.
fn nth(source: &str) -> Option<(i64, i64)> {
    let text = source.trim().to_lowercase();
    match text.as_str() {
        "odd" => return Some((2, 1)),
        "even" => return Some((2, 0)),
        _ => {}
    }
    let bytes = text.as_bytes();
    let signed = |at: usize| match bytes.get(at) {
        Some(b'-') => (-1, at + 1),
        Some(b'+') => (1, at + 1),
        _ => (1, at),
    };
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let value = |from: usize, count: usize| text[from..from + count].parse::<i64>().unwrap_or(0);
    let (sign, start) = signed(0);
    let count = digits(start);
    if count > 9 {
        return None;
    }
    let after = start + count;
    if after == bytes.len() {
        return (count > 0).then(|| (0, sign * value(start, count)));
    }
    if bytes[after] != b'n' {
        return None;
    }
    let a = sign * if count == 0 { 1 } else { value(start, count) };
    let mut index = after + 1;
    if index == bytes.len() {
        return Some((a, 0));
    }
    let skip = |mut at: usize| {
        while bytes
            .get(at)
            .is_some_and(|b| b.is_ascii_whitespace() || *b == b'\x0b')
        {
            at += 1;
        }
        at
    };
    index = skip(index);
    let sign = match bytes.get(index) {
        Some(b'+') => 1,
        Some(b'-') => -1,
        _ => return None,
    };
    index = skip(index + 1);
    let count = digits(index);
    ((1..=9).contains(&count) && index + count == bytes.len())
        .then(|| (a, sign * value(index, count)))
}

struct Parse<'a> {
    at: &'a str,
    /// The first feature outside the subset; it rejects the selector unless the selector can never match.
    why: Option<String>,
}

impl Parse<'_> {
    fn bad(&self) -> String {
        format!("invalid CSS selector{}", self.at)
    }
    fn why(&mut self, reason: impl FnOnce() -> String) {
        if self.why.is_none() {
            self.why = Some(reason());
        }
    }
}

struct Parser<'a> {
    source: &'a str,
    ids: &'a mut u32,
}

type Step<T> = Result<(T, usize), String>;

impl Parser<'_> {
    fn bytes(&self) -> &[u8] {
        self.source.as_bytes()
    }

    /// The `:pseudo` at `start` as the pseudo-classes it adds, or None when it can match no rendered element.
    fn pseudo(
        &mut self,
        start: usize,
        depth: usize,
        parse: &mut Parse,
    ) -> Step<Option<Vec<Pseudo>>> {
        let element = self.bytes().get(start + 1) == Some(&b':');
        let from = start + if element { 2 } else { 1 };
        let end = name_end(self.source, from);
        if end == from {
            return Err(parse.bad());
        }
        let name = unescape(&self.source[from..end]).to_lowercase();
        let mut index = end;
        let mut args = None;
        if self.bytes().get(index) == Some(&b'(') {
            let close = bracket_end(self.bytes(), index).ok_or_else(|| parse.bad())?;
            args = Some(&self.source[index + 1..close]);
            index = close + 1;
        }
        if element || NEVER.contains(&name.as_str()) {
            return Ok((None, index));
        }
        let (place, kind) = name.split_once('-').unwrap_or((&name, ""));
        if matches!(place, "first" | "last" | "only")
            && matches!(kind, "child" | "of-type")
            && args.is_none()
        {
            let of_type = kind == "of-type";
            let mut found = Vec::new();
            for last in [false, true] {
                if place != if last { "first" } else { "last" } {
                    found.push(Pseudo::Nth {
                        a: 0,
                        b: 1,
                        last,
                        of_type,
                    });
                }
            }
            return Ok((Some(found), index));
        }
        let counted = name
            .strip_prefix("nth-")
            .map(|rest| {
                rest.strip_prefix("last-")
                    .map_or((false, rest), |rest| (true, rest))
            })
            .filter(|(_, kind)| matches!(*kind, "child" | "of-type"));
        if let (Some((last, kind)), Some(args)) = (counted, args) {
            if let Some((a, b)) = nth(args) {
                let of_type = kind == "of-type";
                return Ok((
                    Some(vec![Pseudo::Nth {
                        a,
                        b,
                        last,
                        of_type,
                    }]),
                    index,
                ));
            }
            parse.why(|| {
                format!(
                    ":{name}({}) (use An+B, odd, or even; \"of <selector>\" is not implemented)",
                    args.trim()
                )
            });
            return Ok((Some(Vec::new()), index));
        }
        if args.is_none() && (name == "root" || name == "empty") {
            let found = if name == "root" {
                Pseudo::Root
            } else {
                Pseudo::Empty
            };
            return Ok((Some(vec![found]), index));
        }
        if let (Some(args), "is" | "where" | "not") = (args, name.as_str()) {
            if depth >= MAX_SELECTOR_DEPTH {
                return Err(format!(
                    ":{name}() nests deeper than {MAX_SELECTOR_DEPTH} levels{}",
                    parse.at
                ));
            }
            let list = self.alternatives(args, depth + 1, parse)?;
            return Ok(match (name.as_str(), list.is_empty()) {
                ("not", true) => (Some(Vec::new()), index),
                ("not", false) => (Some(vec![Pseudo::Not(list)]), index),
                (_, true) => (None, index),
                ("is", false) => (Some(vec![Pseudo::Is(list)]), index),
                _ => (Some(vec![Pseudo::Where(list)]), index),
            });
        }
        parse.why(|| {
            format!(
                ":{name}{} (not implemented)",
                if args.is_some() { "()" } else { "" }
            )
        });
        Ok((Some(Vec::new()), index))
    }

    /// The arguments that can match something. One that cannot drops out, so `:not(:hover)` is true and `:is(:hover)` is not.
    fn alternatives(
        &mut self,
        source: &str,
        depth: usize,
        parse: &mut Parse,
    ) -> Result<Vec<Complex>, String> {
        let mut found = Vec::new();
        if source.trim().is_empty() {
            return Ok(found);
        }
        for item in split(source) {
            let mut own = Parse {
                at: parse.at,
                why: None,
            };
            let mut parser = Parser {
                source: item.trim(),
                ids: &mut *self.ids,
            };
            if let Some(pieces) = parser.complex(depth, &mut own)? {
                found.push(pieces);
                if parse.why.is_none() {
                    parse.why = own.why;
                }
            }
        }
        Ok(found)
    }

    /// A type or `*`, then `#id`, `.class`, `:pseudo`, and `[attribute]` parts; None if no rendered element can match it.
    fn compound(
        &mut self,
        start: usize,
        depth: usize,
        parse: &mut Parse,
    ) -> Step<Option<Compound>> {
        let mut compound = Compound {
            id: 0,
            tag: None,
            ids: Vec::new(),
            classes: Vec::new(),
            pseudos: Vec::new(),
        };
        let mut index = start;
        let mut never = false;
        match self.bytes().get(index) {
            Some(b'*') => index += 1,
            Some(byte) if byte.is_ascii_alphabetic() => {
                let end = name_end(self.source, index);
                // HTML type selectors are case-insensitive; JSX tags are lowercase.
                let tag = unescape(&self.source[index..end]).to_lowercase();
                compound.tag = tag_index(&tag);
                never = compound.tag.is_none();
                index = end;
            }
            _ => {}
        }
        if self.bytes().get(index) == Some(&b'|') {
            parse.why(|| "namespaces (FlashPDF elements have none)".into());
            index = if self.bytes().get(index + 1) == Some(&b'*') {
                index + 2
            } else {
                name_end(self.source, index + 1)
            };
        }
        loop {
            match self.bytes().get(index) {
                Some(&mark @ (b'#' | b'.')) => {
                    let end = name_end(self.source, index + 1);
                    if end == index + 1 {
                        return Err(parse.bad());
                    }
                    let name = unescape(&self.source[index + 1..end]).into_boxed_str();
                    if mark == b'#' {
                        &mut compound.ids
                    } else {
                        &mut compound.classes
                    }
                    .push(name);
                    index = end;
                }
                Some(b':') => {
                    let (added, end) = self.pseudo(index, depth, parse)?;
                    match added {
                        Some(added) => compound.pseudos.extend(added),
                        None => never = true,
                    }
                    index = end;
                }
                Some(b'[') => {
                    let end = bracket_end(self.bytes(), index).ok_or_else(|| parse.bad())?;
                    parse.why(|| "attribute selectors (match a class or id instead)".into());
                    index = end + 1;
                }
                _ => break,
            }
        }
        if index == start {
            return Err(parse.bad());
        }
        if never {
            return Ok((None, index));
        }
        compound.id = *self.ids;
        *self.ids += 1;
        Ok((Some(compound), index))
    }

    /// Compounds joined by combinators; None when any compound can match no rendered element.
    fn complex(&mut self, depth: usize, parse: &mut Parse) -> Result<Option<Complex>, String> {
        let mut complex = Complex {
            compounds: Vec::new(),
            combinators: Vec::new(),
        };
        let mut never = false;
        let mut index = 0;
        let skip = |bytes: &[u8], mut at: usize| {
            while bytes.get(at).copied().is_some_and(whitespace) {
                at += 1;
            }
            at
        };
        loop {
            let (part, end) = self.compound(index, depth, parse)?;
            match part {
                Some(part) => complex.compounds.push(part),
                None => never = true,
            }
            let after = skip(self.bytes(), end);
            if after == self.source.len() {
                break;
            }
            let combinator = match self.bytes()[after] {
                b'>' => Combinator::Child,
                b'+' => Combinator::Next,
                b'~' => Combinator::Subsequent,
                _ if after > end => Combinator::Descendant,
                _ => return Err(parse.bad()),
            };
            index = if combinator == Combinator::Descendant {
                after
            } else {
                skip(self.bytes(), after + 1)
            };
            complex.combinators.push(combinator);
        }
        Ok((!never).then_some(complex))
    }
}

/// None when no rendered element can match, so the rule is skipped; an error for what the subset lacks.
/// `at` ends each message; `ids` numbers the compounds.
pub(super) fn selector(source: &str, at: &str, ids: &mut u32) -> Result<Option<Complex>, String> {
    let mut parse = Parse { at, why: None };
    let pieces = Parser { source, ids }.complex(0, &mut parse)?;
    match (&pieces, parse.why) {
        (Some(_), Some(why)) => Err(format!("unsupported CSS selector: {why}{at}")),
        _ => Ok(pieces),
    }
}

/// `:is()` and `:not()` count their most specific argument, `:where()` nothing, other pseudo-classes a class.
pub(super) fn specificity(complex: &Complex) -> Specificity {
    let mut total = [0; 3];
    for part in &complex.compounds {
        let mut add = |weight: Specificity| {
            for place in 0..3 {
                total[place] += weight[place];
            }
        };
        add([
            part.ids.len() as u32,
            part.classes.len() as u32,
            u32::from(part.tag.is_some()),
        ]);
        for pseudo in &part.pseudos {
            add(match pseudo {
                Pseudo::Where(_) => [0; 3],
                Pseudo::Is(list) | Pseudo::Not(list) => list
                    .iter()
                    .map(specificity)
                    .reduce(|a, b| if a < b { b } else { a })
                    .unwrap_or([0; 3]),
                _ => [0, 1, 0],
            });
        }
    }
    total
}

/// An element as a stylesheet sees it, before the tree places it.
pub struct ElemInput<'a> {
    pub tag: &'a str,
    pub id: Option<&'a str>,
    pub classes: Vec<&'a str>,
    /// No children but zero-length text.
    pub empty: bool,
}

/// An element as selectors see it: its identity and place among the element siblings, with text nodes not counted.
pub(super) struct Elem {
    pub tag: u8,
    pub id: Option<Box<str>>,
    pub classes: Vec<Box<str>>,
    pub parent: Option<u32>,
    previous: Option<u32>,
    /// 1-based place among the element siblings, and how many there are.
    index: u32,
    count: u32,
    /// The same among the siblings with this tag.
    type_index: u32,
    type_count: u32,
    empty: bool,
}

/// FxHash: the memo keys are small integers, so SipHash's flood resistance buys nothing.
#[derive(Default)]
struct Fx(u64);
impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}
type Memo = HashMap<(u32, u32), bool, BuildHasherDefault<Fx>>;

/// The elements of one render. Verdicts are cached per (compound, element), which keeps
/// combinators polynomial.
#[derive(Default)]
pub struct Tree {
    pub(super) elems: Vec<Elem>,
    memo: Memo,
    before: Memo,
}

impl Tree {
    /// Adds the element children of `parent`, or the roots, and returns the index of the first.
    pub fn add_siblings(
        &mut self,
        parent: Option<u32>,
        siblings: &[ElemInput],
    ) -> Result<u32, String> {
        let first = self.elems.len() as u32;
        let tags = siblings
            .iter()
            .map(|input| {
                tag_index(input.tag).ok_or_else(|| format!("unsupported element: {}", input.tag))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut totals = [0u32; TAGS.len()];
        for &tag in &tags {
            totals[usize::from(tag)] += 1;
        }
        let mut seen = [0u32; TAGS.len()];
        for (offset, (input, &tag)) in siblings.iter().zip(&tags).enumerate() {
            seen[usize::from(tag)] += 1;
            self.elems.push(Elem {
                tag,
                id: input.id.map(Into::into),
                classes: input.classes.iter().map(|&class| class.into()).collect(),
                parent,
                previous: (offset > 0).then(|| first + offset as u32 - 1),
                index: offset as u32 + 1,
                count: siblings.len() as u32,
                type_index: seen[usize::from(tag)],
                type_count: totals[usize::from(tag)],
                empty: input.empty,
            });
        }
        Ok(first)
    }

    pub(super) fn parent(&self, node: u32) -> Option<u32> {
        self.elems[node as usize].parent
    }

    pub(super) fn tag(&self, node: u32) -> &'static str {
        TAGS[usize::from(self.elems[node as usize].tag)]
    }

    fn pseudo(&mut self, pseudo: &Pseudo, node: u32) -> bool {
        let elem = &self.elems[node as usize];
        match pseudo {
            Pseudo::Root => elem.parent.is_none(),
            Pseudo::Empty => elem.empty,
            &Pseudo::Nth {
                a,
                b,
                last,
                of_type,
            } => {
                let (index, count) = if of_type {
                    (elem.type_index, elem.type_count)
                } else {
                    (elem.index, elem.count)
                };
                let gap = i64::from(if last { count - index + 1 } else { index }) - b;
                if a == 0 {
                    gap == 0
                } else {
                    gap % a == 0 && gap / a >= 0
                }
            }
            Pseudo::Is(list) | Pseudo::Where(list) => {
                list.iter().any(|item| self.matches(item, node))
            }
            Pseudo::Not(list) => !list.iter().any(|item| self.matches(item, node)),
        }
    }

    fn part(&mut self, part: &Compound, node: u32) -> bool {
        let elem = &self.elems[node as usize];
        let plain = part.tag.is_none_or(|tag| tag == elem.tag)
            && part.ids.iter().all(|id| elem.id.as_ref() == Some(id))
            && part
                .classes
                .iter()
                .all(|class| elem.classes.contains(class));
        plain && part.pseudos.iter().all(|pseudo| self.pseudo(pseudo, node))
    }

    /// Whether a sibling before `node` matches `complex` up to compound `index`; each sibling the
    /// walk crosses caches the verdict, so it is linear.
    fn earlier(&mut self, complex: &Complex, index: usize, node: u32) -> bool {
        let key = complex.compounds[index].id;
        let mut crossed = Vec::new();
        let mut verdict = false;
        let mut at = Some(node);
        while let Some(current) = at {
            if let Some(&known) = self.before.get(&(key, current)) {
                verdict = known;
                break;
            }
            crossed.push(current);
            let previous = self.elems[current as usize].previous;
            if previous.is_some_and(|previous| self.from(complex, index, previous)) {
                verdict = true;
                break;
            }
            at = previous;
        }
        for sibling in crossed {
            self.before.insert((key, sibling), verdict);
        }
        verdict
    }

    fn at(&mut self, complex: &Complex, index: usize, node: u32) -> bool {
        if !self.part(&complex.compounds[index], node) {
            return false;
        }
        if index == 0 {
            return true;
        }
        let elem = &self.elems[node as usize];
        let (parent, previous) = (elem.parent, elem.previous);
        match complex.combinators[index - 1] {
            Combinator::Child => parent.is_some_and(|parent| self.from(complex, index - 1, parent)),
            Combinator::Next => {
                previous.is_some_and(|previous| self.from(complex, index - 1, previous))
            }
            Combinator::Subsequent => self.earlier(complex, index - 1, node),
            Combinator::Descendant => {
                let mut ancestor = parent;
                while let Some(current) = ancestor {
                    if self.from(complex, index - 1, current) {
                        return true;
                    }
                    ancestor = self.elems[current as usize].parent;
                }
                false
            }
        }
    }

    /// Right to left, with one cached verdict per (compound, element) so backtracking stays polynomial.
    fn from(&mut self, complex: &Complex, index: usize, node: u32) -> bool {
        // The last compound has one caller per element, so it needs no cache.
        if index == complex.compounds.len() - 1 {
            return self.at(complex, index, node);
        }
        let key = (complex.compounds[index].id, node);
        if let Some(&known) = self.memo.get(&key) {
            return known;
        }
        let verdict = self.at(complex, index, node);
        self.memo.insert(key, verdict);
        verdict
    }

    pub(super) fn matches(&mut self, complex: &Complex, node: u32) -> bool {
        self.from(complex, complex.compounds.len() - 1, node)
    }

    /// `<span.total> in <div.row> > <main.invoice>`, for error messages.
    pub fn path(&self, node: u32) -> String {
        let mut result = label(&self.elems[node as usize]);
        let mut ancestor = self.parent(node);
        let mut first = true;
        while let Some(current) = ancestor {
            result.push_str(if first { " in " } else { " > " });
            result.push_str(&label(&self.elems[current as usize]));
            first = false;
            ancestor = self.parent(current);
        }
        result
    }
}

/// `<div#id.a.b>`.
pub(super) fn label(elem: &Elem) -> String {
    let mut result = format!("<{}", TAGS[usize::from(elem.tag)]);
    if let Some(id) = elem.id.as_deref().filter(|id| !id.is_empty()) {
        result.push('#');
        result.push_str(id);
    }
    for class in &elem.classes {
        result.push('.');
        result.push_str(class);
    }
    result.push('>');
    result
}
