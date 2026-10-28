//! Element records: the host's JSX tree, which the compiler styles and lowers to layout records.
//!
//! Order: Family and Stylesheet (or StrictStylesheet) records, one Setup, document records (Metadata, PdfA,
//! Attachment), then a Block record per header or footer and one for the body, each followed
//! by its elements, then End.

use std::collections::HashMap;

use super::cursor::Cursor;
use super::opcode::Opcode;
use super::{Commands, Decoder, ProtocolError, MAX_BLOCK_BYTES};
use crate::compile::{
    Bookmark, Compiler, Drawing, Element, Fonts, InlineStyle, Node, Paint, Picture, Record, Shape,
    Sink, Styler,
};
use crate::css::page::{page_setup, Format, Margin};
use crate::css::style::{FontSlots, HELVETICA};
use crate::css::TAGS;
use crate::css::{pages, property_key, resolver, stylesheet, Cascaded, Value};
use crate::{Direction, BUILT_IN};

/// Element levels, as tree.ts counts them: a root is depth 0.
const MAX_DEPTH: usize = 65;
/// Element, text, and drawing bytes per render; the compiler needs a whole block at once.
const MAX_TREE_BYTES: usize = 256 * 1024 * 1024;
/// The render options a Block record's kind names, in the order the host sends them.
const BLOCKS: [&str; 4] = ["header", "footer", "firstPageHeader", "firstPageFooter"];

#[derive(Default)]
pub(super) struct Elements {
    sheets: Vec<String>,
    families: Option<Fonts>,
    compiler: Option<Compiler<Commands>>,
    outline: bool,
    /// A Stylesheet, Text, or Drawing value whose records continue.
    partial: Option<(u8, Vec<u8>)>,
    /// Kind of the block receiving elements; 5 is the body.
    block: Option<u8>,
    roots: Vec<Node>,
    /// Each block's nodes from an Html record, by block kind less one.
    html: [Option<Vec<Node>>; 5],
    /// The `images` entries an HTML `<img src>` may name.
    pictures: HashMap<String, Picture>,
    open: Vec<Element>,
    bytes: usize,
}

impl Elements {
    pub(super) fn compiling(&self) -> bool {
        self.compiler.is_some()
    }

    /// Whether Family or Stylesheet records arrived, so layout records may not follow.
    pub(super) fn started(&self) -> bool {
        self.families.is_some() || !self.sheets.is_empty() || self.partial.is_some()
    }

    pub(super) fn sink(&mut self) -> Option<&mut Commands> {
        self.compiler
            .as_mut()
            .map(|compiler| &mut compiler.writer.sink)
    }

    fn families(&mut self) -> &mut Fonts {
        self.families
            .get_or_insert_with(|| HashMap::from([("Helvetica".into(), HELVETICA)]))
    }

    fn compile(&mut self) -> Result<(), ProtocolError> {
        let Some(kind) = self.block else {
            return Ok(());
        };
        let mut roots = std::mem::take(&mut self.roots);
        if let Some(html) = self.html[usize::from(kind - 1)].take() {
            if !roots.is_empty() {
                return Err(ProtocolError::InvalidNesting);
            }
            roots = html;
        }
        let compiler = self.compiler.as_mut().unwrap();
        let result = match kind {
            5 => compiler.body(&roots, self.outline),
            kind => {
                let option = BLOCKS[usize::from(kind - 1)];
                compiler.furniture(kind % 2 == 0, kind > 2, option, &roots)
            }
        };
        result.map_err(|error| ProtocolError::Message(error.into_message()))
    }

    /// Adds a finished node to the open element, or to the block's roots.
    fn attach(&mut self, node: Node) {
        match self.open.last_mut() {
            Some(parent) => parent.children.push(node),
            None => self.roots.push(node),
        }
    }
}

impl Decoder {
    pub(super) fn element_record(
        &mut self,
        code: Opcode,
        payload: &[u8],
    ) -> Result<(), ProtocolError> {
        let elements = &mut self.elements;
        if let Some((open, _)) = &elements.partial {
            if *open != code as u8 {
                return Err(ProtocolError::InvalidNesting);
            }
        }
        let before_setup = elements.compiler.is_none() && self.commands.page.is_none();
        let in_block = elements.block.is_some();
        match code {
            Opcode::Family if before_setup => {
                let mut cursor = Cursor::new(payload);
                let name = cursor.alt()?.to_owned();
                let count = (self.commands.fonts.len() + BUILT_IN) as u32;
                let mut slot = |required: bool| -> Result<Option<u32>, ProtocolError> {
                    match u32::from(cursor.take(1)?[0]) {
                        255 if !required => Ok(None),
                        slot if (BUILT_IN as u32..count).contains(&slot) => Ok(Some(slot)),
                        _ => Err(ProtocolError::UnknownFont),
                    }
                };
                let regular = slot(true)?.unwrap();
                let slots = FontSlots {
                    regular,
                    bold: slot(false)?,
                    italic: slot(false)?,
                    bold_italic: slot(false)?,
                };
                cursor.done()?;
                let families = elements.families();
                if name.trim() != name || name.is_empty() || name.contains(',') {
                    return Err(ProtocolError::InvalidValue("font family"));
                }
                if families.insert(name, slots).is_some() {
                    return Err(ProtocolError::InvalidValue("font family"));
                }
                Ok(())
            }
            Opcode::Stylesheet | Opcode::StrictStylesheet if before_setup => {
                let Some(bytes) = chunk(elements, code, payload)? else {
                    return Ok(());
                };
                let source = String::from_utf8(bytes).map_err(|_| ProtocolError::InvalidUtf8)?;
                if matches!(code, Opcode::StrictStylesheet) {
                    stylesheet(&source)
                        .map_err(|message| ProtocolError::Message(message.into()))?;
                }
                elements.sheets.push(source);
                Ok(())
            }
            Opcode::NamedImage if before_setup => {
                let Some(bytes) = chunk(elements, code, payload)? else {
                    return Ok(());
                };
                let mut cursor = Cursor::new(&bytes);
                let name = cursor.utf8()?.to_owned();
                let picture = match cursor.take(1)?[0] {
                    0 => {
                        let handle = cursor.u16()?;
                        cursor.done()?;
                        if usize::from(handle) >= self.commands.staged.len() {
                            return Err(ProtocolError::UnknownImage);
                        }
                        Picture::Image(handle)
                    }
                    1 => Picture::Drawing(drawing(&bytes[4 + name.len() + 1..])?),
                    _ => return Err(ProtocolError::InvalidValue("image kind")),
                };
                if name.is_empty() || elements.pictures.insert(name, picture).is_some() {
                    return Err(ProtocolError::InvalidValue("image name"));
                }
                Ok(())
            }
            Opcode::Html if before_setup => {
                let Some(bytes) = chunk(elements, code, payload)? else {
                    return Ok(());
                };
                count(elements, bytes.len())?;
                let (&kind, source) = bytes.split_first().ok_or(ProtocolError::TruncatedRecord)?;
                let slot = match kind {
                    0 => 4,
                    1..=4 => usize::from(kind - 1),
                    _ => return Err(ProtocolError::InvalidValue("block")),
                };
                let source = std::str::from_utf8(source).map_err(|_| ProtocolError::InvalidUtf8)?;
                let document =
                    crate::html::parse(source, &elements.pictures).map_err(|message| {
                        ProtocolError::Message(match BLOCKS.get(slot) {
                            Some(option) => format!("{message} in the {option} HTML"),
                            None => message,
                        })
                    })?;
                if elements.html[slot].replace(document.nodes).is_some() {
                    return Err(ProtocolError::InvalidNesting);
                }
                elements.sheets.extend(document.styles);
                Ok(())
            }
            Opcode::Setup if before_setup && elements.partial.is_none() => self.setup(payload),
            Opcode::Block if elements.compiler.is_some() && elements.open.is_empty() => {
                let mut cursor = Cursor::new(payload);
                let kind = match cursor.take(1)?[0] {
                    0 => 5,
                    kind @ 1..=4 => kind,
                    _ => return Err(ProtocolError::InvalidValue("block")),
                };
                cursor.done()?;
                // Headers and footers in option order, then the body once.
                if elements.block.is_some_and(|last| kind <= last) {
                    return Err(ProtocolError::InvalidNesting);
                }
                elements.compile()?;
                elements.block = Some(kind);
                Ok(())
            }
            Opcode::Open if in_block => {
                if elements.open.len() >= MAX_DEPTH {
                    return Err(ProtocolError::NestingTooDeep);
                }
                count(elements, payload.len())?;
                let staged = self.commands().staged.len();
                let element = open(payload, staged)?;
                let elements = &mut self.elements;
                if let Some(parent) = elements.open.last() {
                    if matches!(parent.tag.as_str(), "img" | "svg" | "hr" | "br") {
                        return Err(ProtocolError::InvalidNesting);
                    }
                }
                elements.open.push(element);
                Ok(())
            }
            Opcode::Text if in_block => {
                count(elements, payload.len())?;
                let Some(bytes) = chunk(elements, code, payload)? else {
                    return Ok(());
                };
                let text = String::from_utf8(bytes).map_err(|_| ProtocolError::InvalidUtf8)?;
                if !crate::win_ansi::valid(&text, true) {
                    return Err(ProtocolError::ControlCharacter);
                }
                if let Some(parent) = elements.open.last() {
                    if matches!(parent.tag.as_str(), "img" | "svg" | "hr" | "br") {
                        return Err(ProtocolError::InvalidNesting);
                    }
                }
                elements.attach(Node::Text(text));
                Ok(())
            }
            Opcode::Drawing if in_block => {
                count(elements, payload.len())?;
                let Some(bytes) = chunk(elements, code, payload)? else {
                    return Ok(());
                };
                let element = elements
                    .open
                    .last_mut()
                    .filter(|element| {
                        matches!(element.tag.as_str(), "img" | "svg")
                            && element.picture.is_none()
                            && element.children.is_empty()
                    })
                    .ok_or(ProtocolError::InvalidNesting)?;
                element.picture = Some(Picture::Drawing(drawing(&bytes)?));
                Ok(())
            }
            Opcode::Close if in_block && payload.is_empty() => {
                let element = elements.open.pop().ok_or(ProtocolError::InvalidNesting)?;
                elements.attach(Node::Element(element));
                Ok(())
            }
            Opcode::End
                if payload.is_empty()
                    && elements.block == Some(5)
                    && elements.open.is_empty()
                    && elements.partial.is_none() =>
            {
                elements.compile()?;
                elements.block = None;
                let compiler = elements.compiler.as_mut().unwrap();
                compiler
                    .end()
                    .map_err(|error| ProtocolError::Message(error.into_message()))
            }
            _ => Err(ProtocolError::InvalidNesting),
        }
    }

    /// Folds the render options into `@page`, then sets up the page, the cascade, and the compiler.
    fn setup(&mut self, payload: &[u8]) -> Result<(), ProtocolError> {
        let mut cursor = Cursor::new(payload);
        let outline = match cursor.take(1)?[0] {
            0 | 1 => payload[0] == 1,
            _ => return Err(ProtocolError::InvalidValue("outline flag")),
        };
        let format = match cursor.take(1)?[0] {
            0 => None,
            1 => Some(Format::Named(
                value(&mut cursor)?.ok_or(ProtocolError::InvalidValue("page format"))?,
            )),
            2 => Some(Format::Size {
                width: value(&mut cursor)?,
                height: value(&mut cursor)?,
            }),
            _ => return Err(ProtocolError::InvalidValue("page format")),
        };
        let orientation = value(&mut cursor)?;
        let margin = match cursor.take(1)?[0] {
            0 => None,
            1 => Some(Margin::All(
                value(&mut cursor)?.ok_or(ProtocolError::InvalidValue("margin"))?,
            )),
            2 => Some(Margin::Sides([
                value(&mut cursor)?,
                value(&mut cursor)?,
                value(&mut cursor)?,
                value(&mut cursor)?,
            ])),
            _ => return Err(ProtocolError::InvalidValue("margin")),
        };
        cursor.done()?;
        let elements = &mut self.elements;
        let page = pages(&elements.sheets)
            .and_then(|pages| {
                page_setup(
                    format.as_ref(),
                    orientation.as_ref(),
                    margin.as_ref(),
                    &pages,
                )
            })
            .map_err(|message| ProtocolError::Message(message.into()))?;
        let [top, right, bottom, left] = page.margin;
        self.commands.set_page(
            [page.width as f32, page.height as f32],
            [top as f32, right as f32, bottom as f32, left as f32],
        )?;
        let width = page.width - right - left;
        let cascade = resolver(&elements.sheets, width)
            .map_err(|message| ProtocolError::Message(message.into()))?;
        let families = elements.families().clone();
        elements.outline = outline;
        elements.compiler = Some(Compiler::new(
            Styler::shared(cascade),
            families,
            std::mem::take(&mut self.commands),
        ));
        Ok(())
    }
}

/// Collects a value split across records: each payload is a continuation byte, then bytes.
fn chunk(
    elements: &mut Elements,
    code: Opcode,
    payload: &[u8],
) -> Result<Option<Vec<u8>>, ProtocolError> {
    let (&more, bytes) = payload
        .split_first()
        .ok_or(ProtocolError::TruncatedRecord)?;
    let (_, value) = elements
        .partial
        .get_or_insert_with(|| (code as u8, Vec::new()));
    if value.len() + bytes.len() > MAX_BLOCK_BYTES {
        return Err(ProtocolError::BlockTooLarge);
    }
    value.extend_from_slice(bytes);
    match more {
        0 => Ok(elements.partial.take().map(|(_, value)| value)),
        1 => Ok(None),
        _ => Err(ProtocolError::InvalidValue("continuation flag")),
    }
}

fn count(elements: &mut Elements, bytes: usize) -> Result<(), ProtocolError> {
    elements.bytes += bytes;
    if elements.bytes > MAX_TREE_BYTES {
        return Err(ProtocolError::BlockTooLarge);
    }
    Ok(())
}

/// A string or number the host passed through unchanged; 0 is `undefined`.
fn value(cursor: &mut Cursor) -> Result<Option<Value>, ProtocolError> {
    Ok(match cursor.take(1)?[0] {
        0 => None,
        1 => Some(Value::Text(cursor.utf8()?.into())),
        2 => Some(Value::Number(cursor.f64()?)),
        _ => return Err(ProtocolError::InvalidValue("value kind")),
    })
}

const ID: u16 = 1;
const ALT: u16 = 1 << 1;
const HREF: u16 = 1 << 2;
const TITLE: u16 = 1 << 3;
const NO_BOOKMARK: u16 = 1 << 4;
const START: u16 = 1 << 5;
const REVERSED: u16 = 1 << 6;
const VALUE: u16 = 1 << 7;
const COL_SPAN: u16 = 1 << 8;
const ROW_SPAN: u16 = 1 << 9;
const RASTER: u16 = 1 << 10;
const STYLE_TEXT: u16 = 1 << 11;
const STYLE_OBJECT: u16 = 1 << 12;
const DIR_LTR: u16 = 1 << 13;
const DIR_RTL: u16 = 1 << 14;
const DIR_AUTO: u16 = 1 << 15;

/// An Open record: tag, flags naming the attributes that follow, id, classes, style, attributes.
fn open(payload: &[u8], staged: usize) -> Result<Element, ProtocolError> {
    let mut cursor = Cursor::new(payload);
    let tag = *TAGS
        .get(usize::from(cursor.take(1)?[0]))
        .ok_or(ProtocolError::InvalidValue("tag"))?;
    let flags = cursor.u16()?;
    let allowed = ID
        | STYLE_TEXT
        | STYLE_OBJECT
        | DIR_LTR
        | DIR_RTL
        | DIR_AUTO
        | match tag {
            "img" => ALT | RASTER,
            "svg" => ALT,
            "a" => HREF,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => TITLE | NO_BOOKMARK,
            "ol" => START | REVERSED,
            "li" => VALUE,
            "td" | "th" => COL_SPAN | ROW_SPAN,
            _ => 0,
        };
    let both = |pair: u16| flags & pair == pair;
    if flags & !allowed != 0
        || both(TITLE | NO_BOOKMARK)
        || both(STYLE_TEXT | STYLE_OBJECT)
        || (flags & (DIR_LTR | DIR_RTL | DIR_AUTO)).count_ones() > 1
    {
        return Err(ProtocolError::InvalidValue("element attributes"));
    }
    let has = |flag: u16| flags & flag != 0;
    let mut element = Element {
        tag: tag.into(),
        ..Element::default()
    };
    if has(ID) {
        element.id = Some(cursor.utf8()?.into());
    }
    for _ in 0..cursor.u16()? {
        let class = cursor.utf8()?;
        if class.is_empty() || class.chars().any(crate::css::js_space) {
            return Err(ProtocolError::InvalidValue("class name"));
        }
        element.classes.push(class.into());
    }
    if has(STYLE_TEXT) {
        element.style = InlineStyle::Text(cursor.utf8()?.into());
    } else if has(STYLE_OBJECT) {
        let mut entries = Cascaded::default();
        for _ in 0..cursor.u16()? {
            let key = property_key(cursor.utf8()?);
            let value = value(&mut cursor)?.ok_or(ProtocolError::InvalidValue("style value"))?;
            entries.set(key, value);
        }
        element.style = InlineStyle::Object(entries);
    }
    if has(ALT) {
        element.alt = Some(cursor.alt()?.into());
    }
    if has(HREF) {
        element.href = Some(cursor.uri()?.into());
    }
    if has(TITLE) {
        let title = cursor.alt()?;
        if title.trim().is_empty() || title.chars().count() > 255 {
            return Err(ProtocolError::InvalidValue("bookmark"));
        }
        element.bookmark = Bookmark::Title(title.into());
    } else if has(NO_BOOKMARK) {
        element.bookmark = Bookmark::Off;
    }
    let mut integer = || -> Result<i64, ProtocolError> {
        Ok(i64::from(i32::from_le_bytes(
            cursor.take(4)?.try_into().unwrap(),
        )))
    };
    if has(START) {
        element.start = Some(integer()?);
    }
    element.reversed = has(REVERSED);
    element.dir = [
        (DIR_LTR, Direction::Ltr),
        (DIR_RTL, Direction::Rtl),
        (DIR_AUTO, Direction::Auto),
    ]
    .into_iter()
    .find(|(flag, _)| has(*flag))
    .map(|(_, direction)| direction);
    if has(VALUE) {
        element.value = Some(integer()?);
    }
    if has(COL_SPAN) {
        element.col_span = Some(cursor.u16()?).filter(|span| (1..=256).contains(span));
        element
            .col_span
            .ok_or(ProtocolError::InvalidValue("colSpan"))?;
    }
    if has(ROW_SPAN) {
        element.row_span = Some(cursor.u16()?).filter(|span| *span >= 1);
        element
            .row_span
            .ok_or(ProtocolError::InvalidValue("rowSpan"))?;
    }
    if has(RASTER) {
        let handle = cursor.u16()?;
        if usize::from(handle) >= staged {
            return Err(ProtocolError::UnknownImage);
        }
        element.picture = Some(Picture::Image(handle));
    }
    cursor.done()?;
    Ok(element)
}

/// svg.ts's `Drawing`: f64 scalars, f32 path coordinates, which the engine only stores as f32.
fn drawing(bytes: &[u8]) -> Result<Drawing, ProtocolError> {
    let mut cursor = Cursor::new(bytes);
    let f64s = |cursor: &mut Cursor, count: usize| -> Result<Vec<f64>, ProtocolError> {
        (0..count).map(|_| cursor.f64()).collect()
    };
    let size = f64s(&mut cursor, 6)?;
    let align = match cursor.take(1)?[0] {
        0 => None,
        1 => {
            let factors = f64s(&mut cursor, 2)?;
            Some([factors[0], factors[1]])
        }
        _ => return Err(ProtocolError::InvalidValue("drawing alignment")),
    };
    let slice = flag(&mut cursor, "drawing slice")?;
    let count = cursor.u32()?;
    let mut shapes = Vec::new();
    for _ in 0..count {
        let matrix = f64s(&mut cursor, 6)?;
        let fill = paint(&mut cursor)?;
        let even_odd = flag(&mut cursor, "fill rule")?;
        let fill_opacity = cursor.f64()?;
        let stroke = paint(&mut cursor)?;
        let stroke_width = cursor.f64()?;
        let [cap, join] = [cursor.take(1)?[0], cursor.take(1)?[0]];
        if cap > 2 || join > 2 {
            return Err(ProtocolError::InvalidValue("stroke line style"));
        }
        let miter = cursor.f64()?;
        let dashes = usize::from(cursor.take(1)?[0]);
        let dashes = f64s(&mut cursor, dashes)?;
        let dash_offset = cursor.f64()?;
        let stroke_opacity = cursor.f64()?;
        let ops = cursor.u32()? as usize;
        let ops = cursor.take(ops)?.to_vec();
        let mut points = Vec::new();
        for &op in &ops {
            let arity = [2, 2, 6, 0]
                .get(usize::from(op))
                .ok_or(ProtocolError::InvalidValue("path operator"))?;
            for _ in 0..*arity {
                let value = f32::from_le_bytes(cursor.take(4)?.try_into().unwrap());
                points.push(f64::from(value));
            }
        }
        shapes.push(Shape {
            matrix: matrix.try_into().unwrap(),
            fill,
            even_odd,
            fill_opacity,
            stroke,
            stroke_width,
            cap,
            join,
            miter,
            dashes,
            dash_offset,
            stroke_opacity,
            ops,
            points,
        });
    }
    cursor.done()?;
    Ok(Drawing {
        width: size[0],
        height: size[1],
        view_box: [size[2], size[3], size[4], size[5]],
        align,
        slice,
        shapes,
    })
}

fn flag(cursor: &mut Cursor, name: &'static str) -> Result<bool, ProtocolError> {
    match cursor.take(1)?[0] {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ProtocolError::InvalidValue(name)),
    }
}

/// 0 is none, 1 an RGB colour, 2 `currentColor`.
fn paint(cursor: &mut Cursor) -> Result<Option<Paint>, ProtocolError> {
    Ok(match cursor.take(1)?[0] {
        0 => None,
        1 => Some(Paint::Rgb(cursor.take(3)?.try_into().unwrap())),
        2 => Some(Paint::Current),
        _ => return Err(ProtocolError::InvalidValue("paint")),
    })
}

/// The compiler writes layout records straight into the decoder that validates them.
impl Sink for Commands {
    fn record(&mut self, record: &Record, payload: &[u8]) -> Result<(), String> {
        self.read_record(record.opcode(), payload)
            .map_err(|error| error.to_string())
    }

    fn add_image(&mut self, bytes: Vec<u8>) -> Result<u16, String> {
        Commands::add_image(self, bytes).map_err(|error| error.to_string())
    }

    fn add_raster(&mut self, handle: u16) -> Result<u16, String> {
        let bytes = self
            .staged
            .get_mut(usize::from(handle))
            .and_then(Option::take)
            .ok_or_else(|| ProtocolError::UnknownImage.to_string())?;
        Commands::add_image(self, bytes).map_err(|error| error.to_string())
    }
}
