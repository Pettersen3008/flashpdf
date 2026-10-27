//! Element tree to protocol records: the port of compile.ts, css/cascade.ts `styleResolver`,
//! and the error helpers of tree.ts and fonts.ts. Messages match the TypeScript ones.

mod table;
mod text;
mod writer;

#[cfg(test)]
mod tests;

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::css::style::{
    box_size, box_style, color, counter, decoration, decoration_color, dimension, face, flex_basis,
    font, kerning, line_height, list_style_position, list_style_type, opacity, point, radius_of,
    spacing, style as normalize, transform, vertical_align, white_space, Box, Dimension, FontSlots,
    LineHeight, Rgb, Style, Transform, WhiteSpace, HELVETICA, MARGINS, SHAPES,
};
use crate::css::{
    js_space, list_key, Cascaded, ElemInput, Inline, Key, Prop, Resolver, Tree, Value,
};
use crate::Direction;
use text::{blank, engine_error, glyph_error, preserve, transform_text};
pub use writer::{Column, Drawing, Flex, FlexItem, Paint, Record, Run, Shape, Sink, Slot, Writer};

macro_rules! p {
    ($name:literal) => {
        const { $crate::css::Prop::of($name) }
    };
}
use p;

/// An element's `style` prop: none, a CSS declaration string, or a style object's entries in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum InlineStyle {
    #[default]
    None,
    Text(String),
    Object(Cascaded),
}

/// What an `<img>` or `<svg>` paints: a raster the host staged, by handle, or a drawing svg.ts
/// parsed. The compiler registers both on first paint, so image slots follow compile order.
#[derive(Clone, Debug, PartialEq)]
pub enum Picture {
    Image(u16),
    Drawing(Drawing),
}

/// `data-bookmark` on a heading.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Bookmark {
    /// The heading's text.
    #[default]
    Auto,
    Title(String),
    /// `false`: left out of the outline.
    Off,
}

/// tree.ts `ElementNode`, validated by the host. Text already holds the page-number tokens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub style: InlineStyle,
    pub children: Vec<Node>,
    pub picture: Option<Picture>,
    pub alt: Option<String>,
    pub href: Option<String>,
    pub bookmark: Bookmark,
    pub start: Option<i64>,
    pub reversed: bool,
    /// The `dir` attribute.
    pub dir: Option<Direction>,
    pub value: Option<i64>,
    pub col_span: Option<u16>,
    pub row_span: Option<u16>,
}

// Text nodes sit in the same Vec as elements, so boxing would add an allocation per element.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Text(String),
    Element(Element),
}

/// A compile error. A bare `PageOverflow` keeps its message, so a box can still recognise it,
/// and remembers the innermost element path for `into_message`. Boxed, so `?` on a
/// `Result<_, Error>` moves one pointer.
#[derive(Clone, Debug, PartialEq)]
pub struct Error(std::boxed::Box<Failure>);

#[derive(Clone, Debug, PartialEq)]
pub struct Failure {
    pub message: String,
    located: bool,
    overflow: Option<Rc<str>>,
}

impl std::ops::Deref for Error {
    type Target = Failure;
    fn deref(&self) -> &Failure {
        &self.0
    }
}

impl std::ops::DerefMut for Error {
    fn deref_mut(&mut self) -> &mut Failure {
        &mut self.0
    }
}

impl Error {
    // Out of line: each call site otherwise inlines the String conversion.
    #[inline(never)]
    pub fn new(message: impl Into<String>) -> Self {
        Self::of(message.into())
    }

    fn of(message: String) -> Self {
        Self(std::boxed::Box::new(Failure {
            message,
            located: false,
            overflow: None,
        }))
    }

    /// tree.ts `locate`: appends the element path once.
    fn locate(mut self, at: &str) -> Self {
        if self.located {
            return self;
        }
        if self.message == "PageOverflow" {
            self.overflow.get_or_insert_with(|| at.into());
            return self;
        }
        self.message.push_str(" on ");
        self.message.push_str(at);
        self.located = true;
        self
    }

    /// tree.ts `pageOverflow`: explains a PageOverflow no box, image, or table claimed.
    pub fn into_message(self) -> String {
        let Failure {
            message, overflow, ..
        } = *self.0;
        if message != "PageOverflow" {
            return message;
        }
        let at = overflow.map_or_else(String::new, |at| format!(" on {at}"));
        format!("PageOverflow: the content does not fit on an empty page; split it into smaller blocks{at}")
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<crate::css::Message> for Error {
    fn from(message: crate::css::Message) -> Self {
        Self::of(message.into())
    }
}

type Result<T> = std::result::Result<T, Error>;
pub type Fonts = HashMap<String, FontSlots>;

/// An element with its normalised style and path, borrowing the record tree.
pub(crate) struct Styled<'a> {
    el: &'a Element,
    style: Rc<Style>,
    /// `<span.total> in <div.row> > <main.invoice>`, for error messages.
    at: Rc<str>,
    children: Vec<StyledNode<'a>>,
}

pub(crate) enum StyledNode<'a> {
    Text(Cow<'a, str>),
    Element(Styled<'a>),
}

struct Computed {
    cascaded: Cascaded,
    style: Rc<Style>,
    children: HashMap<String, usize>,
}

/// cascade.ts `styleResolver`: cascades and normalises each element, caching per parent result,
/// inline style, single-line mode, tag, list defaults, and matched rules.
pub struct Styler {
    resolver: Rc<Resolver>,
    /// The root's result first; each entry's children index into it.
    computed: Vec<Computed>,
}

impl Styler {
    #[cfg(test)]
    pub fn new(sheets: &[&str], width: f64) -> std::result::Result<Self, String> {
        Ok(Self::shared(Rc::new(Resolver::new(sheets, width)?)))
    }

    pub fn shared(resolver: Rc<Resolver>) -> Self {
        Self {
            resolver,
            computed: vec![Computed {
                cascaded: Cascaded::default(),
                style: Rc::default(),
                children: HashMap::new(),
            }],
        }
    }

    pub(crate) fn styles<'a>(
        &mut self,
        nodes: &'a [Node],
        single_line: bool,
    ) -> Result<Vec<StyledNode<'a>>> {
        let mut tree = Tree::default();
        self.level(&mut tree, None, nodes, 0, single_line)
    }

    fn level<'a>(
        &mut self,
        tree: &mut Tree,
        parent: Option<u32>,
        nodes: &'a [Node],
        computed: usize,
        single_line: bool,
    ) -> Result<Vec<StyledNode<'a>>> {
        let inputs: Vec<ElemInput> = nodes
            .iter()
            .filter_map(|node| match node {
                Node::Element(el) => Some(ElemInput {
                    tag: &el.tag,
                    id: el.id.as_deref(),
                    classes: el.classes.iter().map(String::as_str).collect(),
                    empty: el
                        .children
                        .iter()
                        .all(|child| matches!(child, Node::Text(text) if text.is_empty())),
                }),
                Node::Text(_) => None,
            })
            .collect();
        let mut next = tree.add_siblings(parent, &inputs)?;
        let mut result = Vec::with_capacity(nodes.len());
        for node in nodes {
            result.push(match node {
                Node::Text(text) => StyledNode::Text(Cow::Borrowed(text)),
                Node::Element(el) => {
                    let styled = self.visit(tree, next, el, computed, single_line)?;
                    next += 1;
                    StyledNode::Element(styled)
                }
            });
        }
        Ok(result)
    }

    fn visit<'a>(
        &mut self,
        tree: &mut Tree,
        node: u32,
        el: &'a Element,
        parent: usize,
        single_line: bool,
    ) -> Result<Styled<'a>> {
        let at: Rc<str> = tree.path(node).into();
        let matched = self.resolver.matched(tree, node);
        let mut key = format!("{single_line}\n{}\n{}\n", el.tag, list_key(tree, node));
        for index in &matched {
            key.push_str(&index.to_string());
            key.push(',');
        }
        key.push('\n');
        inline_key(&el.style, &mut key);
        let computed = match self.computed[parent].children.get(&key) {
            Some(&index) => index,
            None => {
                let parent_computed = &self.computed[parent];
                let font_size = match parent_computed.style.get(p!("fontSize")) {
                    Some(Value::Number(size)) => Some(*size),
                    _ => None,
                };
                let inline = match &el.style {
                    InlineStyle::None => Inline::None,
                    InlineStyle::Text(text) => Inline::Text(text),
                    InlineStyle::Object(entries) => Inline::Object(entries.clone()),
                };
                let cascaded = self
                    .resolver
                    .cascaded(
                        tree,
                        node,
                        &matched,
                        &inline,
                        &parent_computed.cascaded,
                        font_size,
                        single_line,
                    )
                    .and_then(|cascaded| {
                        let style = normalize(&cascaded, &parent_computed.style)?;
                        Ok((cascaded, style))
                    });
                let (cascaded, style) =
                    cascaded.map_err(|message| Error::new(message).locate(&at))?;
                self.computed.push(Computed {
                    cascaded,
                    style: Rc::new(style),
                    children: HashMap::new(),
                });
                let index = self.computed.len() - 1;
                self.computed[parent].children.insert(key, index);
                index
            }
        };
        let children = self.level(tree, Some(node), &el.children, computed, single_line)?;
        Ok(Styled {
            el,
            style: self.computed[computed].style.clone(),
            at,
            children,
        })
    }
}

/// The content of an inline style, standing in for the object identity cascade.ts keys on.
fn inline_key(value: &InlineStyle, key: &mut String) {
    match value {
        InlineStyle::None => key.push('n'),
        InlineStyle::Text(text) => {
            key.push('t');
            key.push_str(text);
        }
        InlineStyle::Object(entries) => {
            key.push('o');
            for (name, value) in &entries.0 {
                let tag = match name {
                    Key::Prop(_) => 'p',
                    Key::Custom(_) => 'c',
                    Key::Other(_) => 'x',
                };
                key.push(tag);
                key.push_str(name.name());
                key.push('\0');
                match value {
                    Value::Text(text) => {
                        key.push('s');
                        key.push_str(text);
                    }
                    Value::Number(number) => {
                        key.push_str(&format!("#{:x}", number.to_bits()));
                    }
                }
                key.push('\0');
            }
        }
    }
}

/// Inherited text state, plus the layout flags that do not inherit.
#[derive(Clone)]
struct Ctx {
    size: f64,
    /// 0 left, 1 center, 2 right, 3 justify, 4 start, 5 end.
    align: u8,
    /// CSS `direction`, or `Auto` from `dir="auto"`.
    direction: Direction,
    color: Rgb,
    family: FontSlots,
    bold: bool,
    italic: bool,
    kerning: bool,
    underline: bool,
    line_through: bool,
    /// None is the text colour.
    decoration_color: Option<Rgb>,
    line_height: Option<LineHeight>,
    letter_spacing: f64,
    word_spacing: f64,
    transform: Transform,
    white_space: WhiteSpace,
    /// Baseline shift in points, up positive; `sub` and `sup` add to their parent's.
    rise: f64,
    /// The background of the nearest enclosing `<mark>`.
    background: Option<Rgb>,
    /// The nearest enclosing `<a href>`.
    link: Option<Rc<str>>,
    /// Not inherited by CSS, but each element paints its own, so the product down the tree is the effective value.
    opacity: f64,
    /// Set on the direct children of a `ul` or `ol`, whose items number themselves from it.
    list: Option<Rc<Cell<List>>>,
    /// The list item being compiled, which its first paragraph empties by taking the marker.
    marker: Option<Rc<RefCell<Option<Run>>>>,
    orphans: f64,
    widows: f64,
    in_row: bool,
    /// Set only for the direct children of a table row.
    cell: Option<CellPaint>,
    item: bool,
    in_column: bool,
    height: Option<f64>,
}

#[derive(Clone, Copy)]
struct List {
    next: i64,
    step: i64,
    ordered: bool,
}

/// A cell's collapsed borders, and its row's background when rowSpan joins the row to others.
#[derive(Clone, Copy, Default)]
struct CellPaint {
    border: Option<[f64; 4]>,
    border_color: Option<[Rgb; 4]>,
    background: Option<Rgb>,
}

const ROOT: Ctx = Ctx {
    size: 12.0,
    align: 4,
    direction: Direction::Ltr,
    color: [0, 0, 0],
    family: HELVETICA,
    bold: false,
    italic: false,
    kerning: true,
    underline: false,
    line_through: false,
    decoration_color: None,
    line_height: None,
    letter_spacing: 0.0,
    word_spacing: 0.0,
    transform: Transform::None,
    white_space: WhiteSpace::Normal,
    rise: 0.0,
    background: None,
    link: None,
    opacity: 1.0,
    list: None,
    marker: None,
    orphans: 2.0,
    widows: 2.0,
    in_row: false,
    cell: None,
    item: false,
    in_column: false,
    height: None,
};

/// The ids links name, and whether to bookmark headings.
struct Navigation {
    /// Each id a link names, in link order, with its element's path.
    targets: Vec<(String, Rc<str>)>,
    index: HashMap<String, usize>,
    outline: bool,
    /// The targets whose anchor record is written.
    anchored: HashSet<usize>,
    /// Anchor records written, against the engine's cap.
    count: usize,
}

/// Destinations and bookmarks per render, matching the engine's cap.
const MAX_ANCHORS: usize = 10_000;
const MAX_TITLE: usize = 255;
const INLINE_TAGS: [&str; 19] = [
    "span", "b", "strong", "em", "i", "cite", "a", "br", "small", "sub", "sup", "u", "ins", "s",
    "del", "mark", "code", "kbd", "samp",
];
const INLINE_ONLY: &str = "accepts only text, <br>, and inline elements such as <span>, <b>, <a>, and <code> without box styles";
const NBSP: &str = "\u{a0}";
/// Blocks with no line of text of their own, so a marker before them takes a line of its own.
const MARKER_ALONE: [&str; 5] = ["ul", "ol", "table", "img", "hr"];
const VERTICAL_ALIGN: &str =
    "vertical-align applies only to inline elements and <td>, <th>, <tr>, and row groups";
const TABLE_PARTS: [&str; 6] = ["td", "th", "tr", "thead", "tbody", "tfoot"];
/// Tags whose `display: flex` lays out their children.
const BLOCK_TAGS: [&str; 17] = [
    "div",
    "main",
    "section",
    "article",
    "header",
    "footer",
    "nav",
    "aside",
    "address",
    "blockquote",
    "figure",
    "figcaption",
    "ul",
    "ol",
    "li",
    "td",
    "th",
];
const SIZES: [Prop; 6] = [
    p!("width"),
    p!("minWidth"),
    p!("maxWidth"),
    p!("height"),
    p!("minHeight"),
    p!("maxHeight"),
];
const BLACK: Rgb = [0, 0, 0];
const EMPTY: Box = Box {
    margin: [0.0; 4],
    padding: [0.0; 4],
    border: [0.0; 4],
    background: None,
    border_color: [BLACK; 4],
    border_style: [0; 4],
    radius: None,
    opacity: 1.0,
    size: None,
};

fn justify(value: Option<&Value>) -> u8 {
    match value.and_then(Value::text).unwrap_or("normal") {
        "flex-end" | "end" => 1,
        "center" => 2,
        "space-between" => 3,
        "space-around" => 4,
        "space-evenly" => 5,
        _ => 0,
    }
}

fn flex_align(value: Option<&Value>) -> u8 {
    match value.and_then(Value::text).unwrap_or("auto") {
        "stretch" => 1,
        "flex-start" | "start" => 2,
        "flex-end" | "end" => 3,
        "center" => 4,
        _ => 0,
    }
}

fn get(s: &Style, prop: Prop) -> Option<&Value> {
    s.get(prop)
}

fn is(s: &Style, prop: Prop, text: &str) -> bool {
    s.get(prop).is_some_and(|value| value.is(text))
}

fn number(s: &Style, prop: Prop) -> Option<f64> {
    match s.get(prop) {
        Some(Value::Number(value)) => Some(*value),
        _ => None,
    }
}

fn inline_tag(tag: &str) -> bool {
    INLINE_TAGS.contains(&tag)
}

/// `flex` and `align-self` size and align a flex item; anywhere else they would be
/// silently dropped, so they are rejected instead.
fn row_only(s: &Style, in_row: bool) -> Result<()> {
    if in_row {
        return Ok(());
    }
    if [p!("flexGrow"), p!("flexShrink"), p!("flexBasis")]
        .iter()
        .any(|prop| s.get(*prop).is_some())
    {
        return Err(Error::new("flex applies only to a flex item"));
    }
    if s.get(p!("alignSelf")).is_some() {
        return Err(Error::new("align-self applies only to a flex item"));
    }
    Ok(())
}

/// Sizes or an auto margin make a layout node; `box-sizing` alone changes nothing.
fn sized(s: &Style) -> bool {
    SIZES.iter().any(|prop| s.get(*prop).is_some())
        || MARGINS.iter().any(|prop| is(s, *prop, "auto"))
}

/// Rejects the sizes and auto margins an image or a table part cannot honour.
fn sizeless(s: &Style, tag: &str, keep: &[&str]) -> Result<()> {
    for prop in SIZES.iter().chain(&[p!("boxSizing")]) {
        if s.get(*prop).is_some() && !keep.contains(&prop.name()) {
            return Err(Error::new(format!(
                "{} applies to blocks and flex items, not <{tag}>",
                prop.name()
            )));
        }
    }
    if MARGINS.iter().any(|prop| is(s, *prop, "auto")) {
        return Err(Error::new(format!(
            "auto margins apply to blocks and flex items, not <{tag}>"
        )));
    }
    Ok(())
}

/// Margins collapse in block flow, not around flex items or table cells.
fn collapses(context: &Ctx) -> bool {
    !context.in_row && !context.in_column && !context.item && context.cell.is_none()
}

fn vertical_box_align(value: &Value) -> bool {
    value.is("top") || value.is("middle") || value.is("bottom")
}

/// `start` and `end` as left or right where the direction is known; under `dir="auto"`
/// the engine resolves them once it has found the first strong character.
fn physical(align: u8, direction: Direction) -> u8 {
    match (align, direction) {
        (4, Direction::Ltr) | (5, Direction::Rtl) => 0,
        (4, Direction::Rtl) | (5, Direction::Ltr) => 2,
        _ => align,
    }
}

/// `dir` is the element's attribute, which its own CSS `direction` overrides.
fn inherit(s: &Style, current: &Ctx, fonts: &Fonts, dir: Option<Direction>) -> Result<Ctx> {
    let direction = match s.get(p!("direction")).and_then(Value::text) {
        Some("rtl") => Direction::Rtl,
        Some(_) => Direction::Ltr,
        None => dir.unwrap_or(current.direction),
    };
    let selected = font(s, current.family, current.bold, current.italic, fonts)?;
    let (underline, line_through) = match s.get(p!("textDecoration")) {
        None => (current.underline, current.line_through),
        Some(value) => {
            let decorated = decoration(value)?;
            (decorated.underline, decorated.line_through)
        }
    };
    let size = match s.get(p!("fontSize")) {
        None => current.size,
        Some(value) => point(value, current.size, true)?,
    };
    let align = match s.get(p!("textAlign")).and_then(Value::text) {
        None => current.align,
        Some(align) => ["left", "center", "right", "justify", "start", "end"]
            .iter()
            .position(|own| *own == align)
            .map_or(255, |index| index as u8),
    };
    Ok(Ctx {
        size,
        align,
        direction,
        color: s.get(p!("color")).map_or(Ok(current.color), color)?,
        family: selected.family,
        bold: selected.weight,
        italic: selected.italic,
        kerning: s
            .get(p!("fontKerning"))
            .map_or(Ok(current.kerning), kerning)?,
        underline,
        line_through,
        decoration_color: match s.get(p!("textDecorationColor")) {
            None => current.decoration_color,
            Some(value) => decoration_color(value)?,
        },
        line_height: match s.get(p!("lineHeight")) {
            None => current.line_height,
            Some(value) => line_height(value, size)?,
        },
        letter_spacing: match s.get(p!("letterSpacing")) {
            None => current.letter_spacing,
            Some(value) => spacing(value, size, "letterSpacing")?,
        },
        word_spacing: match s.get(p!("wordSpacing")) {
            None => current.word_spacing,
            Some(value) => spacing(value, size, "wordSpacing")?,
        },
        transform: s
            .get(p!("textTransform"))
            .map_or(Ok(current.transform), transform)?,
        white_space: s
            .get(p!("whiteSpace"))
            .map_or(Ok(current.white_space), white_space)?,
        rise: current.rise
            + match s.get(p!("verticalAlign")) {
                Some(value) if !vertical_box_align(value) => vertical_align(value, current.size)?,
                _ => 0.0,
            },
        background: current.background,
        link: current.link.clone(),
        opacity: match s.get(p!("opacity")) {
            None => current.opacity,
            Some(value) => current.opacity * opacity(value)?,
        },
        list: None,
        marker: current.marker.clone(),
        orphans: number(s, p!("orphans")).unwrap_or(current.orphans),
        widows: number(s, p!("widows")).unwrap_or(current.widows),
        in_row: false,
        cell: None,
        item: false,
        in_column: false,
        height: None,
    })
}

fn run(state: &Ctx, text: impl Into<String>, hard_break: bool) -> Run {
    let (font, oblique) = face(&state.family, state.bold, state.italic);
    let decorated = state.underline || state.line_through;
    Run {
        font: font.unwrap_or(0),
        size: state.size,
        line_height: match state.line_height {
            None => 0.0,
            Some(LineHeight::Pt(pt)) => pt,
            Some(LineHeight::Factor(factor)) => factor * state.size,
        },
        color: state.color,
        text: text.into(),
        hard_break,
        underline: state.underline,
        line_through: state.line_through,
        decoration_color: if decorated {
            state.decoration_color
        } else {
            None
        },
        link: state.link.clone(),
        opacity: state.opacity,
        oblique,
        letter_spacing: state.letter_spacing,
        word_spacing: state.word_spacing,
        kerning: state.kerning,
        rise: state.rise,
        background: state.background,
        marker: 0,
        white_space: state.white_space,
        transform: state.transform,
    }
}

/// Appends a run, merging it into the previous one when nothing about its paint differs.
fn push(runs: &mut Vec<Run>, next: Run) {
    if let Some(last) = runs.last_mut() {
        if !last.hard_break
            && last.font == next.font
            && last.size == next.size
            && last.line_height == next.line_height
            && last.underline == next.underline
            && last.line_through == next.line_through
            && last.decoration_color == next.decoration_color
            && last.link == next.link
            && last.opacity == next.opacity
            && last.oblique == next.oblique
            && last.letter_spacing == next.letter_spacing
            && last.word_spacing == next.word_spacing
            && last.kerning == next.kerning
            && last.rise == next.rise
            && last.background == next.background
            && last.white_space == next.white_space
            && last.transform == next.transform
            && last.color == next.color
        {
            last.text.push_str(&next.text);
            last.hard_break = next.hard_break;
            return;
        }
    }
    runs.push(next);
}

/// A `br` paints nothing, so a box or layout property on it would be dropped.
fn line_break(node: &Styled, context: &Ctx) -> Result<Run> {
    let key = node.style.0.iter().find_map(|(key, _)| match key {
        Key::Prop(prop) if !prop.inherited() => Some(prop.name()),
        Key::Other(name) => Some(&**name),
        _ => None,
    });
    if let Some(key) = key {
        return Err(
            Error::new(format!("<br> takes no {key}; style the text around it")).locate(&node.at),
        );
    }
    Ok(run(context, "", true))
}

/// Bounds of a box's content height; equal when `height` fixes it.
fn heights(value: Option<&Box>) -> (f64, Option<f64>) {
    let Some((value, size)) = value.and_then(|value| Some((value, value.size?))) else {
        return (0.0, None);
    };
    let frame = value.padding[0] + value.padding[2] + value.border[0] + value.border[2];
    let inner = |amount: f64| {
        if size.border_box {
            (amount - frame).max(0.0)
        } else {
            amount
        }
    };
    let min = inner(size.min_height.unwrap_or(0.0));
    let max = size.max_height.map(inner);
    let Some(height) = size.height else {
        return (min, max);
    };
    let fixed = inner(height).min(max.unwrap_or(f64::INFINITY)).max(min);
    (fixed, Some(fixed))
}

/// The definite height children see: only a fixed one.
fn fixed((min, max): (f64, Option<f64>)) -> Option<f64> {
    (Some(min) == max).then_some(min)
}

fn cell_box(value: Option<Box>, cell: Option<CellPaint>) -> Option<Box> {
    let Some(cell) = cell.filter(|cell| cell.border.is_some() || cell.background.is_some()) else {
        return value;
    };
    let base = value.unwrap_or(EMPTY);
    Some(Box {
        border: cell.border.unwrap_or(base.border),
        border_color: cell.border_color.unwrap_or(base.border_color),
        background: base.background.or(cell.background),
        ..base
    })
}

/// A column-model track: a width, or a grow factor with a zero basis, or with `auto` and no
/// width. None for any other flex, which needs the CSS flex algorithm.
fn track(s: &Style) -> Result<Option<Column>> {
    let grow = number(s, p!("flexGrow")).unwrap_or(0.0);
    let basis = get(s, p!("flexBasis"))
        .map(flex_basis)
        .transpose()?
        .flatten();
    if basis.is_some_and(|basis| basis.value > 0.0) {
        return Ok(None);
    }
    let width = get(s, p!("width"));
    if grow > 0.0 && (basis.is_some() || width.is_none()) {
        return Ok(Some(Column {
            kind: 1,
            value: grow,
        }));
    }
    if let (Some(width), true) = (width, grow == 0.0) {
        let Dimension { kind, value } = dimension(width, "width", false)?;
        return Ok(Some(Column {
            kind: if kind == 2 { 2 } else { 0 },
            value,
        }));
    }
    let flexed = [p!("flexGrow"), p!("flexShrink"), p!("flexBasis")]
        .iter()
        .any(|prop| s.get(*prop).is_some());
    Ok((!flexed).then_some(Column {
        kind: 1,
        value: 1.0,
    }))
}

fn column(node: &StyledNode) -> Result<Column> {
    match node {
        StyledNode::Text(_) => Ok(Column {
            kind: 1,
            value: 1.0,
        }),
        StyledNode::Element(el) => element_column(el),
    }
}

fn element_column(el: &Styled) -> Result<Column> {
    track(&el.style)?.ok_or_else(|| {
        Error::new(
            "a table column takes a width or a flex grow factor, not a flex basis or flex: none",
        )
        .locate(&el.at)
    })
}

/// Earlier releases measured flex rows as columns: an item without flex or width took an equal
/// share. A row keeps that model, so its output is unchanged, until it or an item uses a
/// property the model cannot express. A column without one stays block flow.
fn css_flex(s: &Style, items: &[&StyledNode], column: bool) -> Result<bool> {
    let own = [
        p!("justifyContent"),
        p!("alignItems"),
        p!("height"),
        p!("minHeight"),
        p!("maxHeight"),
    ];
    if own.iter().any(|prop| s.get(*prop).is_some()) {
        return Ok(true);
    }
    for item in items {
        let StyledNode::Element(el) = item else {
            continue;
        };
        let s = &el.style;
        if s.get(p!("alignSelf")).is_some() || MARGINS.iter().any(|prop| is(s, *prop, "auto")) {
            return Ok(true);
        }
        let flexed = if column {
            [p!("flexGrow"), p!("flexShrink"), p!("flexBasis")]
                .iter()
                .any(|prop| s.get(*prop).is_some())
        } else {
            [
                p!("minWidth"),
                p!("maxWidth"),
                p!("height"),
                p!("minHeight"),
                p!("maxHeight"),
                p!("boxSizing"),
            ]
            .iter()
            .any(|prop| s.get(*prop).is_some())
                || track(s)?.is_none()
        };
        if flexed {
            return Ok(true);
        }
    }
    Ok(false)
}

fn plain(nodes: &[StyledNode], out: &mut String) {
    for node in nodes {
        match node {
            StyledNode::Text(text) => out.push_str(text),
            StyledNode::Element(el) if el.el.tag == "br" => out.push(' '),
            StyledNode::Element(el) => plain(&el.children, out),
        }
    }
}

/// HTML ignores one newline right after `<pre>`, so a template literal can start on its own line.
fn without_leading_newline<'a>(children: &'a [StyledNode<'a>]) -> Option<Vec<StyledNode<'a>>> {
    let Some(StyledNode::Text(first)) = children.first() else {
        return None;
    };
    let rest = first
        .strip_prefix("\r\n")
        .or_else(|| first.strip_prefix('\n'))?;
    let mut result = vec![StyledNode::Text(Cow::Owned(rest.into()))];
    for child in &children[1..] {
        result.push(child.borrow());
    }
    Some(result)
}

impl<'a> StyledNode<'a> {
    /// A shallow copy that shares the element's data.
    fn borrow(&self) -> StyledNode<'a> {
        match self {
            Self::Text(text) => Self::Text(text.clone()),
            Self::Element(el) => Self::Element(Styled {
                el: el.el,
                style: el.style.clone(),
                at: el.at.clone(),
                children: el.children.iter().map(StyledNode::borrow).collect(),
            }),
        }
    }
}

/// The ids that `<a href="#id">` names, each with its element. Every one must be on exactly one element.
fn link_targets(nodes: &[StyledNode]) -> Result<Navigation> {
    #[derive(Default)]
    struct Walk {
        links: Vec<(String, Rc<str>)>,
        linked: HashSet<String>,
        owners: HashMap<String, Rc<str>>,
        twins: HashMap<String, Rc<str>>,
    }
    fn walk(walk_state: &mut Walk, list: &[StyledNode]) {
        for node in list {
            let StyledNode::Element(node) = node else {
                continue;
            };
            if let Some(id) = node
                .el
                .href
                .as_deref()
                .and_then(|href| href.strip_prefix('#'))
            {
                if walk_state.linked.insert(id.into()) {
                    walk_state.links.push((id.into(), node.at.clone()));
                }
            }
            if let Some(id) = &node.el.id {
                if walk_state.owners.contains_key(id) {
                    walk_state.twins.insert(id.clone(), node.at.clone());
                } else {
                    walk_state.owners.insert(id.clone(), node.at.clone());
                }
            }
            walk(walk_state, &node.children);
        }
    }
    let mut state = Walk::default();
    walk(&mut state, nodes);
    let mut targets = Vec::with_capacity(state.links.len());
    for (id, link) in state.links {
        let Some(owner) = state.owners.get(&id) else {
            return Err(
                Error::new(format!("link to #{id}: no element has id=\"{id}\"")).locate(&link),
            );
        };
        if let Some(twin) = state.twins.get(&id) {
            return Err(Error::new(format!(
                "link to #{id}: id=\"{id}\" is on more than one element"
            ))
            .locate(twin));
        }
        targets.push((id, owner.clone()));
    }
    let index = targets
        .iter()
        .enumerate()
        .map(|(index, (id, _))| (id.clone(), index))
        .collect();
    Ok(Navigation {
        targets,
        index,
        outline: false,
        anchored: HashSet::new(),
        count: 0,
    })
}

/// Compiles element records through a `Writer` into a `Sink`, styling each block of input first.
pub struct Compiler<S> {
    styler: Styler,
    fonts: Fonts,
    pub writer: Writer<S>,
    /// Set only while compiling the document body.
    nav: Option<Navigation>,
    page_tokens: bool,
}

impl<S: Sink> Compiler<S> {
    pub fn new(styler: Styler, fonts: Fonts, sink: S) -> Self {
        Self {
            styler,
            fonts,
            writer: Writer::new(sink),
            nav: None,
            page_tokens: false,
        }
    }

    /// A header or footer block: render.ts's furniture loop. `option` names the render option.
    pub fn furniture(
        &mut self,
        footer: bool,
        first: bool,
        option: &str,
        nodes: &[Node],
    ) -> Result<()> {
        let result: Result<()> = (|| {
            self.writer.repeat_start(footer, first, option)?;
            let styled = self.styler.styles(nodes, true)?;
            self.page_tokens = true;
            let result = self.compile(&styled, &ROOT, 0);
            self.page_tokens = false;
            result?;
            self.writer.repeat_end()
        })();
        result.map_err(|error| {
            if error.message == "PageOverflow" {
                Error::new(format!(
                    "PageOverflow: with {option}, the header and footer take more than half the page"
                ))
            } else {
                error
            }
        })
    }

    /// Compiles the document body. A link target or a bookmarked heading gets an `anchor` record.
    pub fn body(&mut self, nodes: &[Node], outline: bool) -> Result<()> {
        let styled = self.styler.styles(nodes, false)?;
        let mut nav = link_targets(&styled)?;
        nav.outline = outline;
        self.nav = Some(nav);
        self.compile(&styled, &ROOT, 0)?;
        let nav = self.nav.take().unwrap();
        for (index, (id, owner)) in nav.targets.iter().enumerate() {
            if !nav.anchored.contains(&index) {
                return Err(Error::new(format!(
                    "link to #{id}: its element cannot hold a destination; put the id on a block, heading, paragraph, image, table, or table cell"
                ))
                .locate(owner));
            }
        }
        Ok(())
    }

    pub fn end(&mut self) -> Result<()> {
        self.writer.end()
    }

    fn compile(&mut self, nodes: &[StyledNode], context: &Ctx, depth: usize) -> Result<()> {
        for node in nodes {
            match node {
                StyledNode::Text(text) => {
                    self.paragraph(vec![run(context, text.as_ref(), false)], context)?
                }
                StyledNode::Element(el) => self.compile_element(el, context, depth)?,
            }
        }
        Ok(())
    }

    fn compile_element(&mut self, el: &Styled, context: &Ctx, depth: usize) -> Result<()> {
        self.element(el, context, depth).map_err(|error| {
            if error.located {
                return error;
            }
            let error = glyph_error(&error.message).map_or(error, Error::new);
            let error = engine_error(&error.message).map_or(error, Error::new);
            error.locate(&el.at)
        })
    }

    /// Runs of an inline node; None for a layout node (box, page break, or its own alignment).
    fn inline_runs(&self, node: &StyledNode, context: &Ctx) -> Result<Option<Vec<Run>>> {
        let node = match node {
            StyledNode::Text(text) => return Ok(Some(vec![run(context, text.as_ref(), false)])),
            StyledNode::Element(el) => el,
        };
        let tag = node.el.tag.as_str();
        if !inline_tag(tag) {
            return Ok(None);
        }
        if tag == "br" {
            return Ok(Some(vec![line_break(node, context)?]));
        }
        (|| {
            let s = &node.style;
            if let Some(value) = s
                .get(p!("verticalAlign"))
                .filter(|value| vertical_box_align(value))
            {
                vertical_align(value, context.size)?;
            }
            row_only(s, context.in_row)?;
            // A `<mark>` paints its background behind each line of its text, so only box edges make it a box.
            let paint = box_style(s, context.size, 1.0)?;
            let highlight = tag == "mark"
                && paint.is_some_and(|paint| {
                    paint
                        .margin
                        .iter()
                        .chain(&paint.padding)
                        .chain(&paint.border)
                        .all(|edge| *edge == 0.0)
                });
            if (paint.is_some() && !highlight)
                || sized(s)
                || is(s, p!("breakBefore"), "page")
                || is(s, p!("breakAfter"), "page")
            {
                return Ok(None);
            }
            let mut next = inherit(s, context, &self.fonts, node.el.dir)?;
            if let Some(background) = paint
                .filter(|_| highlight)
                .and_then(|paint| paint.background)
            {
                next.background = Some(background);
            }
            if next.align != context.align {
                return Ok(None);
            }
            if let Some(href) = &node.el.href {
                next.link = Some(href.as_str().into());
            }
            self.collect(&node.children, &next)
        })()
        .map_err(|error| error.locate(&node.at))
    }

    /// Runs of consecutive inline children; None as soon as one is a layout node.
    fn collect(&self, children: &[StyledNode], context: &Ctx) -> Result<Option<Vec<Run>>> {
        let mut runs = Vec::new();
        for child in children {
            let Some(part) = self.inline_runs(child, context)? else {
                return Ok(None);
            };
            for item in part {
                push(&mut runs, item);
            }
        }
        Ok(Some(runs))
    }

    fn paragraph(&mut self, runs: Vec<Run>, state: &Ctx) -> Result<()> {
        if !self.page_tokens
            && runs
                .iter()
                .any(|run| run.text.contains([crate::PAGE_NUMBER, crate::TOTAL_PAGES]))
        {
            return Err(Error::new(
                "page tokens are only valid in a header or footer",
            ));
        }
        // A word may span runs, so each run sees the text before it.
        let mut before = String::new();
        let transformed = runs
            .into_iter()
            .map(|item| {
                let text = transform_text(&item.text, item.transform, &before);
                before = if item.hard_break {
                    String::new()
                } else {
                    item.text.clone()
                };
                Run { text, ..item }
            })
            .collect();
        let mut shown = preserve(transformed);
        // The first paragraph of a list item takes its marker, which hangs before that paragraph's first line.
        if let Some(marker) = state
            .marker
            .as_ref()
            .and_then(|marker| marker.borrow_mut().take())
        {
            shown.insert(0, marker);
        }
        self.writer.paragraph(
            shown,
            physical(state.align, state.direction) | (state.direction as u8) << 3,
            state.orphans,
            state.widows,
        )
    }

    /// Inside a flex row or a kept-together group every break is avoided already, so the engine ignores `avoid` there.
    fn page_break(&mut self, s: &Style, after: bool, depth: usize) -> Result<()> {
        let prop = if after {
            p!("breakAfter")
        } else {
            p!("breakBefore")
        };
        if is(s, prop, "avoid") {
            self.writer.avoid()?;
        }
        if !is(s, prop, "page") {
            return Ok(());
        }
        if depth > 0 {
            return Err(Error::new(format!(
                "nested {} is not implemented",
                if after { "break-after" } else { "break-before" }
            )));
        }
        self.writer.page_break()
    }

    /// In block flow the writer holds a box's vertical margins so they collapse with adjoining ones
    /// (CSS 2.1 8.3.1). A flex item, table cell, or row keeps its margins, and its children's, inside.
    fn boxed(
        &mut self,
        value: Option<Box>,
        collapses: bool,
        avoid: bool,
        body: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let [top, right, bottom, left] = value.map_or([0.0; 4], |value| value.margin);
        let inner = value.map(|value| {
            if collapses {
                Box {
                    margin: [0.0, right, 0.0, left],
                    ..value
                }
            } else {
                value
            }
        });
        let drawn = inner.filter(|inner| {
            !collapses
                || inner.background.is_some()
                || inner.size.is_some()
                || inner
                    .margin
                    .iter()
                    .chain(&inner.padding)
                    .chain(&inner.border)
                    .any(|edge| *edge != 0.0)
        });
        if collapses && top != 0.0 {
            self.writer.margin(top);
        }
        let result: Result<()> = (|| {
            if let Some(inner) = drawn {
                let through = collapses
                    && inner.size.is_none()
                    && inner.padding[0] == 0.0
                    && inner.border[0] == 0.0;
                self.writer.box_start(inner, through)?;
            }
            body(self)?;
            if let Some(inner) = drawn {
                self.writer
                    .box_end(collapses && inner.padding[2] == 0.0 && inner.border[2] == 0.0)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if error.message == "PageOverflow" {
                if avoid {
                    return Err(Error::new(
                        "PageOverflow: the box is taller than a page; drop break-inside: avoid",
                    ));
                }
                if drawn.is_some() {
                    let size = value.and_then(|value| value.size);
                    let limited =
                        size.is_some_and(|size| size.height.or(size.max_height).is_some());
                    return Err(Error::new(if limited {
                        "PageOverflow: a box with a height or max-height moves whole, and this one is taller than a page"
                    } else {
                        "PageOverflow: use document margins or split this decorated box"
                    }));
                }
            }
            return Err(error);
        }
        if collapses && bottom != 0.0 {
            self.writer.margin(bottom);
        }
        Ok(())
    }

    /// The box an element paints and sizes. A column-model row or a table owns its items' width.
    fn layout_box(s: &Style, size: f64, context: &Ctx) -> Result<Option<Box>> {
        let own = get(s, p!("opacity")).map_or(Ok(1.0), opacity)?;
        let paint = box_style(s, size, context.opacity * own)?;
        let sizes = box_size(s, context.height, !context.in_row || context.item)?;
        Ok(match sizes {
            Some(sizes) => Some(Box {
                size: Some(sizes),
                ..paint.unwrap_or(EMPTY)
            }),
            None => paint,
        })
    }

    /// Block flow: consecutive inline children form one paragraph; layout nodes stand alone.
    fn flow(
        &mut self,
        children: &[StyledNode],
        context: &Ctx,
        depth: usize,
        gap: f64,
    ) -> Result<()> {
        let mut count = 0;
        let mut runs: Vec<Run> = Vec::new();
        let separate = |this: &mut Self, count: &mut usize| -> Result<()> {
            *count += 1;
            if *count > 1 && gap != 0.0 {
                this.writer.spacer(gap)?;
            }
            Ok(())
        };
        // Whitespace between block-level children makes no line, as in CSS, unless it is preformatted.
        let shows = |runs: &[Run]| {
            runs.iter().any(|item| {
                item.hard_break
                    || !blank(&item.text)
                    || (!item.text.is_empty()
                        && !matches!(item.white_space, WhiteSpace::Normal | WhiteSpace::Nowrap))
            })
        };
        for child in children {
            if let Some(part) = self.inline_runs(child, context)? {
                for item in part {
                    push(&mut runs, item);
                }
                continue;
            }
            if shows(&runs) {
                separate(self, &mut count)?;
                self.paragraph(std::mem::take(&mut runs), context)?;
            }
            runs.clear();
            separate(self, &mut count)?;
            // A flex item's margins, and its children's, never collapse with its siblings'.
            if context.in_column {
                self.writer.flush()?;
            }
            self.compile(std::slice::from_ref(child), context, depth)?;
            if context.in_column {
                self.writer.flush()?;
            }
        }
        if shows(&runs) {
            separate(self, &mut count)?;
            self.paragraph(runs, context)?;
        }
        Ok(())
    }

    /// Writes the destination that a link to `node.id` or the outline needs, before the node's content.
    /// The engine places it at the top of the next line, so it moves with the node across a page break.
    fn anchor(&mut self, node: &Styled) -> Result<()> {
        let Some(nav) = &mut self.nav else {
            return Ok(());
        };
        let target = node
            .el
            .id
            .as_ref()
            .and_then(|id| nav.index.get(id).copied());
        let tag = node.el.tag.as_bytes();
        let level = match tag {
            [b'h', level @ b'1'..=b'6'] if nav.outline && node.el.bookmark != Bookmark::Off => {
                level - b'0'
            }
            _ => 0,
        };
        let title = match (&node.el.bookmark, level) {
            (_, 0) => String::new(),
            (Bookmark::Title(title), _) => title.clone(),
            _ => {
                let mut text = String::new();
                plain(&node.children, &mut text);
                // `replace(/\s+/g, " ").trim()`
                let words: Vec<&str> = text
                    .split(js_space)
                    .filter(|word| !word.is_empty())
                    .collect();
                words.join(" ").chars().take(MAX_TITLE).collect()
            }
        };
        if target.is_none() && title.is_empty() {
            return Ok(());
        }
        nav.count += 1;
        if nav.count > MAX_ANCHORS {
            return Err(Error::new(format!(
                "more than {MAX_ANCHORS} link targets and bookmarks"
            )));
        }
        if let Some(target) = target {
            nav.anchored.insert(target);
        }
        let name = target.map_or("", |_| node.el.id.as_deref().unwrap_or(""));
        let level = if title.is_empty() { 0 } else { level };
        let name = name.to_owned();
        self.writer.anchor(&name, level, &title)
    }

    /// `ul` and `ol` start the counter their items number themselves from; an `li` takes the next
    /// number and, unless its type is `none`, owes its first paragraph a marker.
    fn list_state(node: &Styled, next: &mut Ctx, context: &Ctx) -> Result<()> {
        let tag = node.el.tag.as_str();
        if tag == "ul" || tag == "ol" {
            for child in &node.children {
                let other = match child {
                    StyledNode::Text(text) => !blank(text),
                    StyledNode::Element(el) => el.el.tag != "li",
                };
                if other {
                    return Err(Error::new(format!("<{tag}> accepts only <li>")));
                }
            }
            let count = node
                .children
                .iter()
                .filter(|child| matches!(child, StyledNode::Element(_)))
                .count() as i64;
            next.list = Some(Rc::new(Cell::new(List {
                next: node
                    .el
                    .start
                    .unwrap_or(if node.el.reversed { count } else { 1 }),
                step: if node.el.reversed { -1 } else { 1 },
                ordered: tag == "ol",
            })));
        } else if tag == "li" {
            let Some(list) = &context.list else {
                return Err(Error::new("<li> belongs in a <ul> or <ol>"));
            };
            let mut state = list.get();
            if node.el.value.is_some() && !state.ordered {
                return Err(Error::new("value applies only to an <li> in an <ol>"));
            }
            let ordinal = node.el.value.unwrap_or(state.next);
            state.next = ordinal + state.step;
            list.set(state);
            let kind =
                list_style_type(get(&node.style, p!("listStyleType")).unwrap_or(&"disc".into()))?;
            let inside = list_style_position(
                get(&node.style, p!("listStylePosition")).unwrap_or(&"outside".into()),
            )? == "inside";
            let text = SHAPES.iter().find(|(name, _)| *name == kind).map_or_else(
                || format!("{}. ", counter(kind, ordinal)),
                |(_, shape)| (*shape).into(),
            );
            // A marker is plain text in the item's font, whatever the item inherits.
            let marker = (kind != "none").then(|| Run {
                underline: false,
                line_through: false,
                decoration_color: None,
                link: None,
                background: None,
                rise: 0.0,
                marker: if inside { 2 } else { 1 },
                white_space: WhiteSpace::Normal,
                transform: Transform::None,
                ..run(next, text, false)
            });
            next.marker = Some(Rc::new(RefCell::new(marker)));
        }
        Ok(())
    }

    fn element(&mut self, node: &Styled, context: &Ctx, depth: usize) -> Result<()> {
        let s: &Style = &node.style;
        let tag = node.el.tag.as_str();
        self.anchor(node)?;
        if s.get(p!("verticalAlign")).is_some() && !TABLE_PARTS.contains(&tag) && !inline_tag(tag) {
            return Err(Error::new(VERTICAL_ALIGN));
        }
        if (s.get(p!("justifyContent")).is_some() || s.get(p!("alignItems")).is_some())
            && (!is(s, p!("display"), "flex") || !BLOCK_TAGS.contains(&tag))
        {
            return Err(Error::new(
                "justify-content and align-items apply only to a flex container",
            ));
        }
        match tag {
            "div" | "main" | "section" | "article" | "header" | "footer" | "ul" | "ol" | "li"
            | "dl" | "dt" | "dd" | "blockquote" | "address" | "figure" | "figcaption" | "nav"
            | "aside" | "td" | "th" => self.block(node, s, context, depth),
            "a" | "span" | "b" | "strong" | "em" | "i" | "cite" | "small" | "sub" | "sup" | "u"
            | "ins" | "s" | "del" | "mark" | "code" | "kbd" | "samp" | "pre" | "p" | "h1"
            | "h2" | "h3" | "h4" | "h5" | "h6" => self.text_block(node, s, context, depth),
            "img" | "svg" => self.picture(node, s, context, depth),
            "br" => {
                let item = line_break(node, context)?;
                self.paragraph(vec![item], context)
            }
            "thead" | "tbody" | "tfoot" | "tr" => {
                Err(Error::new(format!("<{tag}> belongs in a <table>")))
            }
            "table" => self.table(node, context, depth),
            "hr" => {
                row_only(s, context.in_row)?;
                self.page_break(s, false, depth)?;
                let size = inherit(s, context, &self.fonts, node.el.dir)?.size;
                let paint = Self::layout_box(s, size, context)?;
                self.boxed(paint, collapses(context), false, |_| Ok(()))?;
                self.page_break(s, true, depth)
            }
            _ => Ok(()),
        }
    }

    fn block(&mut self, node: &Styled, s: &Style, context: &Ctx, depth: usize) -> Result<()> {
        let tag = node.el.tag.as_str();
        if (tag == "td" || tag == "th") && context.cell.is_none() {
            return Err(Error::new(format!(
                "<{tag}> belongs in a <tr> inside a <table>"
            )));
        }
        if context.cell.is_some() {
            sizeless(s, tag, &["width"])?;
        }
        row_only(s, context.in_row)?;
        self.page_break(s, false, depth)?;
        let mut next = inherit(s, context, &self.fonts, node.el.dir)?;
        Self::list_state(node, &mut next, context)?;
        let owed = if tag == "li" {
            next.marker.clone()
        } else {
            None
        };
        let paint = cell_box(Self::layout_box(s, next.size, context)?, context.cell);
        let avoid = is(s, p!("breakInside"), "avoid");
        // A CSS flex item is one element, so one without a box of its own gets an empty one.
        let value = paint.or(context.item.then_some(EMPTY));
        self.boxed(value, collapses(context), avoid, |this| {
            let flex = is(s, p!("display"), "flex");
            let row = flex && get(s, p!("flexDirection")).is_none_or(|value| value.is("row"));
            let inner = Ctx {
                height: fixed(heights(paint.as_ref())),
                in_column: flex && !row,
                ..next.clone()
            };
            let owes = || owed.as_ref().is_some_and(|marker| marker.borrow().is_some());
            let first = node.children.iter().find(|child| match child {
                StyledNode::Element(_) => true,
                StyledNode::Text(text) => !blank(text),
            });
            if owes() && matches!(first, Some(StyledNode::Element(el)) if MARKER_ALONE.contains(&el.el.tag.as_str())) {
                this.paragraph(vec![run(&inner, NBSP, false)], &inner)?;
            }
            let gap = get(s, p!("gap")).map_or(Ok(0.0), |gap| point(gap, next.size, false))?;
            // A whitespace-only text child is no flex item, as in CSS.
            let items: Vec<&StyledNode> = node
                .children
                .iter()
                .filter(|child| !matches!(child, StyledNode::Text(text) if blank(text)))
                .collect();
            if flex && !items.is_empty() && css_flex(s, &items, !row)? {
                this.flexbox(s, &items, &next, paint.as_ref(), depth)?;
            } else if row && !items.is_empty() {
                // A gap is a fixed-width empty column between items, so flex grow shares what is left.
                let spacer = Column { kind: 0, value: gap };
                let mut columns = Vec::with_capacity(items.len() * 2);
                for (index, item) in items.iter().enumerate() {
                    if index > 0 && gap != 0.0 {
                        columns.push(spacer);
                    }
                    columns.push(column(item)?);
                }
                this.writer.row_start(columns, Vec::new())?;
                let cell = Ctx {
                    in_row: true,
                    ..next.clone()
                };
                for (index, child) in items.iter().enumerate() {
                    if index > 0 && gap != 0.0 {
                        this.writer.spacer(0.0)?;
                    }
                    this.compile(std::slice::from_ref(*child), &cell, depth + 1)?;
                }
                this.writer.row_end()?;
            } else if (context.in_row && !context.item) || avoid {
                this.writer.stack_start(gap, collapses(context))?;
                this.flow(&node.children, &inner, depth + 1, 0.0)?;
                this.writer.stack_end(collapses(context))?;
            } else {
                this.flow(&node.children, &inner, depth, gap)?;
            }
            if owes() {
                this.paragraph(vec![run(&inner, NBSP, false)], &inner)?;
            }
            Ok(())
        })?;
        self.page_break(s, true, depth)
    }

    /// A CSS flex container: one record for the container and its items, then one element per item.
    fn flexbox(
        &mut self,
        s: &Style,
        items: &[&StyledNode],
        text: &Ctx,
        layout: Option<&Box>,
        depth: usize,
    ) -> Result<()> {
        let column = is(s, p!("flexDirection"), "column");
        let (min_height, max_height) = heights(layout);
        let justify = justify(get(s, p!("justifyContent")));
        let mut specs = Vec::with_capacity(items.len());
        for item in items {
            specs.push(match item {
                StyledNode::Text(_) => FlexItem {
                    grow: 0.0,
                    shrink: 1.0,
                    basis: None,
                    align: 0,
                },
                StyledNode::Element(el) => FlexItem {
                    grow: number(&el.style, p!("flexGrow")).unwrap_or(0.0),
                    shrink: number(&el.style, p!("flexShrink")).unwrap_or(1.0),
                    basis: get(&el.style, p!("flexBasis"))
                        .map(flex_basis)
                        .transpose()?
                        .flatten(),
                    align: flex_align(get(&el.style, p!("alignSelf"))),
                },
            });
        }
        if column {
            let flexed = specs
                .iter()
                .position(|spec| spec.grow > 0.0 || spec.basis.is_some());
            if let Some(flexed) = flexed {
                let error = Error::new(
                    "flex-grow and flex-basis size flex row items; give a column item a height",
                );
                return Err(match items[flexed] {
                    StyledNode::Element(el) => error.locate(&el.at),
                    StyledNode::Text(_) => error,
                });
            }
            // Without a height the column is as tall as its items, so there is no space to distribute.
            let size = layout.and_then(|layout| layout.size);
            if justify != 0
                && size.is_none_or(|size| size.height.is_none())
                && size
                    .and_then(|size| size.min_height)
                    .is_none_or(|min| min == 0.0)
            {
                return Err(Error::new(
                    "justify-content in a flex column needs a height or min-height on the column",
                ));
            }
        }
        let align = flex_align(get(s, p!("alignItems")));
        let gap = get(s, p!("gap")).map_or(Ok(0.0), |gap| point(gap, text.size, false))?;
        self.writer.flex_start(
            Flex {
                column,
                justify,
                align,
                gap,
                min_height,
                max_height,
            },
            specs,
        )?;
        let inner = Ctx {
            in_row: true,
            item: true,
            height: fixed((min_height, max_height)),
            ..text.clone()
        };
        for child in items {
            self.compile(std::slice::from_ref(*child), &inner, depth + 1)?;
        }
        self.writer.flex_end()
    }

    fn text_block(&mut self, node: &Styled, s: &Style, context: &Ctx, depth: usize) -> Result<()> {
        let tag = node.el.tag.as_str();
        row_only(s, context.in_row)?;
        self.page_break(s, false, depth)?;
        let next = inherit(s, context, &self.fonts, node.el.dir)?;
        let mut inner = next.clone();
        if let Some(href) = &node.el.href {
            inner.link = Some(href.as_str().into());
        }
        let stripped = if tag == "pre" {
            without_leading_newline(&node.children)
        } else {
            None
        };
        let runs = self.collect(stripped.as_deref().unwrap_or(&node.children), &inner)?;
        // A link around layout nodes (an image, a card) flows them as a block; every run inside links.
        if runs.is_none() && tag != "a" {
            return Err(Error::new(format!("<{tag}> {INLINE_ONLY}")));
        }
        let paint = box_style(s, next.size, next.opacity)?;
        if let Some(align) =
            get(s, p!("verticalAlign")).filter(|value| paint.is_some() && !value.is("baseline"))
        {
            return Err(Error::new(format!(
                "vertical-align: {align} shifts text within a line, and <{tag}> has box styles that make it a block"
            )));
        }
        let own = Self::layout_box(s, next.size, context)?;
        let avoid = is(s, p!("breakInside"), "avoid");
        let value = own.or((context.item && runs.is_none()).then_some(EMPTY));
        self.boxed(value, collapses(context), avoid, |this| {
            if avoid {
                this.writer.stack_start(0.0, false)?;
            }
            match runs {
                None => this.flow(
                    &node.children,
                    &inner,
                    if avoid { depth + 1 } else { depth },
                    0.0,
                )?,
                Some(runs) => this.paragraph(runs, &next)?,
            }
            if avoid {
                this.writer.stack_end(false)?;
            }
            Ok(())
        })?;
        self.page_break(s, true, depth)
    }

    fn picture(&mut self, node: &Styled, s: &Style, context: &Ctx, depth: usize) -> Result<()> {
        let tag = node.el.tag.as_str();
        row_only(s, context.in_row)?;
        sizeless(s, tag, &["width", "height", "maxWidth", "maxHeight"])?;
        for prop in [p!("height"), p!("maxHeight")] {
            if get(s, prop)
                .and_then(Value::text)
                .is_some_and(|value| value.ends_with('%'))
            {
                return Err(Error::new(format!(
                    "percent {} has no container height; use pt or px, or set width",
                    prop.name()
                )));
            }
        }
        self.page_break(s, false, depth)?;
        let next = inherit(s, context, &self.fonts, node.el.dir)?;
        let length = |prop: Prop| {
            get(s, prop)
                .map(|value| dimension(value, prop.name(), false))
                .transpose()
        };
        let width = length(p!("width"))?;
        if context.item && width.is_some_and(|width| width.kind == 2) {
            return Err(Error::new(
                "an image flex item takes a width in pt or px; wrap it to size it by percent",
            ));
        }
        let height = length(p!("height"))?.map(|height| height.value);
        let max_width = length(p!("maxWidth"))?;
        if context.item && max_width.is_some_and(|width| width.kind == 2) {
            return Err(Error::new(
                "an image flex item takes a maxWidth in pt or px; wrap it to size it by percent",
            ));
        }
        let max_height = length(p!("maxHeight"))?.map(|height| height.value);
        if radius_of(s, next.size)?.is_some() {
            return Err(Error::new(format!(
                "border-radius does not clip <{tag}>: the picture stays rectangular, so remove the radius"
            )));
        }
        let paint = box_style(s, next.size, next.opacity)?;
        self.boxed(paint, collapses(context), false, |this| {
            let result: Result<()> = (|| {
                // A drawing carries its opacity in its shapes, where an image takes it from the page.
                let (slot, alpha) = match &node.el.picture {
                    Some(Picture::Drawing(drawing)) => {
                        let fitted = writer::frame(drawing, width, height)?;
                        let bytes = writer::encode(drawing, fitted, next.color, next.opacity)?;
                        (this.writer.drawing(bytes)?, 1.0)
                    }
                    Some(Picture::Image(handle)) => (this.writer.raster(*handle)?, next.opacity),
                    None => return Err(Error::new("unknown image")),
                };
                let alt = node.el.alt.as_deref().unwrap_or("");
                this.writer.image(
                    slot,
                    width,
                    height,
                    max_width,
                    max_height,
                    alt,
                    context.link.clone(),
                    alpha,
                )
            })();
            result.map_err(|error| match error.message.as_str() {
                "PageOverflow" => {
                    Error::new("PageOverflow: the image is taller than the page; reduce its height")
                }
                "ImageTooWide" => {
                    Error::new("the image is wider than its container; reduce width or height")
                }
                _ => error,
            })
        })?;
        self.page_break(s, true, depth)
    }
}
