//! Port of protocol.ts `ProtocolWriter`: collapses held margins, defers box and stack starts,
//! and encodes each record exactly as the TypeScript writer does, so the decoder validates it.

use std::collections::HashMap;
use std::rc::Rc;

use super::Error;
use crate::css::style::{Box, Dimension, Rgb, Transform, WhiteSpace};

/// A column: kind 0 points, 1 a grow factor, 2 percent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    pub kind: u8,
    pub value: f64,
}

/// A table row's column: a cell that starts there, more of the cell to its left, or part of a cell from an earlier row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Slot {
    Cell { align: u8, rows: u16 },
    Join,
    Covered,
}

/// `align_content` 0 is `stretch`, else a `justify` code plus 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flex {
    pub column: bool,
    pub wrap: bool,
    pub justify: u8,
    pub align: u8,
    pub align_content: u8,
    pub gap: f64,
    pub row_gap: f64,
    pub min_height: f64,
    pub max_height: Option<f64>,
}

/// `basis` None is `auto`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlexItem {
    pub grow: f64,
    pub shrink: f64,
    pub basis: Option<Dimension>,
    pub align: u8,
}

/// One styled stretch of a paragraph; `hard_break` ends the line after it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub structure: u32,
    pub font: u32,
    pub size: f64,
    /// Points; 0 is `normal`.
    pub line_height: f64,
    pub color: Rgb,
    pub text: String,
    pub hard_break: bool,
    pub underline: bool,
    pub line_through: bool,
    pub decoration_color: Option<Rgb>,
    pub link: Option<Rc<str>>,
    pub opacity: f64,
    pub oblique: bool,
    pub letter_spacing: f64,
    pub word_spacing: f64,
    pub kerning: bool,
    pub rise: f64,
    pub background: Option<Rgb>,
    /// 1 hangs a list marker before the first line and 2 starts the line with it.
    pub marker: u8,
    pub white_space: WhiteSpace,
    pub transform: Transform,
}

/// One protocol record, as protocol.ts writes it. Each lives only until it is encoded.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum Record {
    Spacer(f64),
    StackStart(f64),
    StackEnd,
    RowStart(Vec<Column>, Vec<Slot>),
    RowEnd,
    FlexStart(Flex, Vec<FlexItem>),
    FlexEnd,
    PageBreak,
    /// The header or footer block that follows; `first` is the first page's.
    Repeat {
        footer: bool,
        first: bool,
        /// A `position: fixed` box, which paints on every page and reserves no height.
        fixed: bool,
    },
    Paragraph {
        runs: Vec<Run>,
        align: u8,
        orphans: f64,
        widows: f64,
    },
    TableStart(u16, u16, Column),
    TableEnd,
    Image {
        structure: u32,
        slot: u16,
        width: Option<Dimension>,
        height: Option<f64>,
        max_width: Option<Dimension>,
        max_height: Option<f64>,
        link: Option<Rc<str>>,
        alt: String,
        opacity: f64,
    },
    BoxStart(Box),
    BoxEnd,
    Anchor {
        name: String,
        level: u8,
        title: String,
    },
    Avoid,
    End,
}

/// Where records go: the decoder's `read_record(record.opcode(), payload)` in a render.
pub trait Sink {
    fn pdfua(&self) -> bool {
        false
    }
    fn structure(&mut self, _nodes: Vec<super::StructureNode>) {}
    fn record(&mut self, record: &Record, payload: &[u8]) -> Result<(), String>;
    /// Registers a drawing's encoded bytes and returns its image slot.
    fn add_image(&mut self, bytes: Vec<u8>) -> Result<u16, String>;
    /// Registers the raster the host staged as `handle` and returns its image slot.
    fn add_raster(&mut self, handle: u16) -> Result<u16, String>;
    /// The variation axes of the font in `slot`; none for a built-in or static font.
    fn axes(&mut self, _slot: u32) -> Vec<[u8; 4]> {
        Vec::new()
    }
    /// The slot of the font in `slot` at these user axis values, each clamped to its axis.
    fn instance(&mut self, slot: u32, _values: &[([u8; 4], f32)]) -> Result<u32, String> {
        Ok(slot)
    }
}

/// The largest payload, which the decoder also enforces.
const MAX_RECORD: usize = 64 * 1024;
const MAX_PAGES: u32 = 10_000;

impl Record {
    pub fn opcode(&self) -> u8 {
        match self {
            Self::Spacer(_) => 2,
            Self::StackStart(_) => 3,
            Self::StackEnd => 4,
            Self::RowStart(..) => 5,
            Self::RowEnd => 6,
            Self::PageBreak => 7,
            Self::Repeat { footer: true, .. } => 8,
            Self::Repeat { footer: false, .. } => 13,
            Self::Paragraph { .. } => 9,
            Self::TableStart(..) => 10,
            Self::TableEnd => 11,
            Self::Image { .. } => 12,
            Self::BoxStart(_) => 17,
            Self::BoxEnd => 18,
            Self::Anchor { .. } => 22,
            Self::Avoid => 23,
            Self::FlexStart(..) => 24,
            Self::FlexEnd => 25,
            Self::End => 255,
        }
    }

    /// The payload bytes, exactly as protocol.ts writes them.
    #[inline(never)]
    pub fn encode(&self, out: &mut Vec<u8>) {
        let mut b = Bytes(out);
        match self {
            Self::Spacer(value) | Self::StackStart(value) => b.f32(*value),
            Self::RowStart(columns, slots) => {
                b.u16(columns.len() as f64);
                for (index, column) in columns.iter().enumerate() {
                    b.u8(column.kind);
                    b.f32(column.value);
                    match slots
                        .get(index)
                        .copied()
                        .unwrap_or(Slot::Cell { align: 0, rows: 1 })
                    {
                        Slot::Join => b.u8(8),
                        Slot::Covered => b.u8(16),
                        Slot::Cell { align, rows } if rows > 1 => {
                            b.u8(4 | align);
                            b.u16(f64::from(rows));
                        }
                        Slot::Cell { align, .. } => b.u8(align),
                    }
                }
            }
            Self::FlexStart(flex, items) => {
                b.u8(u8::from(flex.column) | u8::from(flex.wrap) << 1);
                b.u8(flex.justify);
                b.u8(flex.align);
                b.f32(flex.gap);
                b.f32(flex.min_height);
                b.height(flex.max_height);
                b.f32(flex.row_gap);
                b.u8(flex.align_content);
                b.u16(items.len() as f64);
                for item in items {
                    b.f32(item.grow);
                    b.f32(item.shrink);
                    b.size(item.basis);
                    b.u8(item.align);
                }
            }
            Self::Repeat { first, fixed, .. } => b.u8(if *fixed { 2 } else { u8::from(*first) }),
            Self::Paragraph {
                runs,
                align,
                orphans,
                widows,
            } => paragraph(&mut b, runs, *align, *orphans, *widows),
            Self::TableStart(header, footer, width) => {
                b.u16(f64::from(*header));
                b.u16(f64::from(*footer));
                b.u8(width.kind);
                b.f32(width.value);
            }
            Self::Image {
                slot,
                width,
                height,
                max_width,
                max_height,
                link,
                alt,
                opacity,
                ..
            } => {
                b.u16(f64::from(*slot));
                for width in [width, max_width] {
                    match width {
                        Some(width) => {
                            b.u8(width.kind);
                            b.f32(width.value);
                        }
                        None => b.u8(0),
                    }
                }
                for height in [height, max_height] {
                    match height {
                        Some(height) => {
                            b.u8(1);
                            b.f32(*height);
                        }
                        None => b.u8(0),
                    }
                }
                match link {
                    Some(link) => {
                        b.u8(1);
                        b.text(link);
                    }
                    None => b.u8(0),
                }
                b.text(alt);
                b.u8(alpha(*opacity));
            }
            Self::BoxStart(value) => box_start(&mut b, value),
            Self::Anchor { name, level, title } => {
                b.text(name);
                b.u8(*level);
                b.text(title);
            }
            Self::StackEnd
            | Self::RowEnd
            | Self::FlexEnd
            | Self::PageBreak
            | Self::TableEnd
            | Self::BoxEnd
            | Self::Avoid
            | Self::End => {}
        }
    }
}

/// The engine's alpha byte; 255 is opaque. `Math.round`, which rounds halves up.
pub(super) fn alpha(opacity: f64) -> u8 {
    js_u8((opacity * 255.0 + 0.5).floor())
}

/// `DataView.setUint8`'s ToUint8.
fn js_u8(value: f64) -> u8 {
    js_u16(value) as u8
}

/// `DataView.setUint16`'s ToUint16.
fn js_u16(value: f64) -> u16 {
    if value.is_finite() {
        value.trunc().rem_euclid(65536.0) as u16
    } else {
        0
    }
}

struct Bytes<'a>(&'a mut Vec<u8>);

impl Bytes<'_> {
    #[inline(never)]
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    #[inline(never)]
    fn u16(&mut self, value: f64) {
        self.0.extend_from_slice(&js_u16(value).to_le_bytes());
    }
    #[inline(never)]
    fn f32(&mut self, value: f64) {
        self.0.extend_from_slice(&(value as f32).to_le_bytes());
    }
    #[inline(never)]
    fn text(&mut self, value: &str) {
        self.0
            .extend_from_slice(&(value.len() as u32).to_le_bytes());
        self.0.extend_from_slice(value.as_bytes());
    }
    /// Kind 0 is `auto`.
    fn size(&mut self, value: Option<Dimension>) {
        match value {
            Some(value) => {
                self.u8(value.kind);
                self.f32(value.value);
            }
            None => self.u8(0),
        }
    }
    fn height(&mut self, value: Option<f64>) {
        self.size(value.map(|value| Dimension { kind: 1, value }));
    }
}

fn paragraph(b: &mut Bytes, runs: &[Run], align: u8, orphans: f64, widows: f64) {
    let mut links: Vec<&str> = Vec::new();
    for run in runs {
        if let Some(link) = run.link.as_deref() {
            if !links.contains(&link) {
                links.push(link);
            }
        }
    }
    b.u8(align);
    b.u16(orphans);
    b.u16(widows);
    b.u16(links.len() as f64);
    for link in &links {
        b.text(link);
    }
    b.u16(runs.len() as f64);
    for run in runs {
        b.u8(run.font as u8);
        b.f32(run.size);
        for channel in run.color {
            b.u8(channel);
        }
        let translucent = alpha(run.opacity) != 255;
        let flags = u16::from(run.hard_break)
            | u16::from(run.underline) << 1
            | u16::from(run.line_through) << 2
            | u16::from(run.link.is_some()) << 3
            | u16::from(run.line_height != 0.0) << 4
            | u16::from(run.oblique) << 5
            | u16::from(run.letter_spacing != 0.0) << 6
            | u16::from(run.word_spacing != 0.0) << 7
            | u16::from(!run.kerning) << 9
            | u16::from(translucent) << 15
            | u16::from(run.decoration_color.is_some()) << 8
            | u16::from(run.background.is_some()) << 10
            | u16::from(run.rise != 0.0) << 11
            | u16::from(matches!(
                run.white_space,
                WhiteSpace::Pre | WhiteSpace::Nowrap
            )) << 12
            | u16::from(run.marker) << 13;
        b.u16(f64::from(flags));
        if let Some(link) = run.link.as_deref() {
            b.u16(links.iter().position(|own| *own == link).unwrap() as f64);
        }
        if run.line_height != 0.0 {
            b.f32(run.line_height);
        }
        if run.letter_spacing != 0.0 {
            b.f32(run.letter_spacing);
        }
        if run.word_spacing != 0.0 {
            b.f32(run.word_spacing);
        }
        if run.rise != 0.0 {
            b.f32(run.rise);
        }
        for channel in run.decoration_color.into_iter().flatten() {
            b.u8(channel);
        }
        for channel in run.background.into_iter().flatten() {
            b.u8(channel);
        }
        if translucent {
            b.u8(alpha(run.opacity));
        }
        b.text(&run.text);
    }
}

fn box_start(b: &mut Bytes, value: &Box) {
    for edge in value
        .margin
        .iter()
        .chain(&value.padding)
        .chain(&value.border)
    {
        b.f32(*edge);
    }
    match value.background {
        Some(background) => {
            b.u8(1);
            for channel in background {
                b.u8(channel);
            }
        }
        None => b.u8(0),
    }
    for channel in value.border_color.iter().flatten() {
        b.u8(*channel);
    }
    b.u8(alpha(value.opacity));
    // Two bits per side, top first.
    let styles = value
        .border_style
        .iter()
        .enumerate()
        .fold(0, |bits, (side, style)| bits | style << (side * 2));
    b.u8(styles);
    match value.radius {
        Some(radius) => {
            b.u8(1);
            b.u8(radius.percent);
            for corner in radius.values {
                b.f32(corner);
            }
        }
        None => b.u8(0),
    }
    let size = value.size;
    let position = size.and_then(|size| size.position);
    b.u8(size.map_or(0, |size| {
        u8::from(size.border_box) | size.auto_margin << 1 | u8::from(position.is_some()) << 5
    }));
    b.size(size.and_then(|size| size.width));
    b.size(size.and_then(|size| size.min_width));
    b.size(size.and_then(|size| size.max_width));
    b.height(size.and_then(|size| size.height));
    b.height(size.and_then(|size| size.min_height));
    b.height(size.and_then(|size| size.max_height));
    if let Some(position) = position {
        b.u8(position.kind);
        match position.z {
            Some(z) => {
                b.u8(1);
                b.0.extend_from_slice(&z.to_le_bytes());
            }
            None => b.u8(0),
        }
        for side in position.inset {
            b.size(side);
        }
    }
}

pub struct Writer<S> {
    pub sink: S,
    /// The render option whose header or footer block is being written.
    repeating: Option<String>,
    /// Slot per drawing, by encoded content, so an icon on every row embeds once.
    drawings: HashMap<Vec<u8>, u16>,
    /// Slot per staged raster, registered on first paint so slots follow compile order.
    rasters: HashMap<u16, u16>,
    pages: u32,
    held: f64,
    /// Box and stack starts waiting for content, so a held margin lands outside them.
    starts: Vec<Record>,
    payload: Vec<u8>,
    /// Records of the `position: fixed` box being written.
    fixed: Option<Vec<Record>>,
    /// Records of every finished `position: fixed` box, which `end` writes.
    deferred: Vec<Record>,
}

impl<S: Sink> Writer<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            repeating: None,
            drawings: HashMap::new(),
            rasters: HashMap::new(),
            pages: 1,
            held: 0.0,
            starts: Vec::new(),
            payload: Vec::new(),
            fixed: None,
            deferred: Vec::new(),
        }
    }

    fn record(&mut self, record: Record) -> Result<(), Error> {
        if let Some(fixed) = &mut self.fixed {
            fixed.push(record);
            return Ok(());
        }
        self.payload.clear();
        record.encode(&mut self.payload);
        if self.payload.len() > MAX_RECORD {
            return Err(Error::new(if matches!(record, Record::Paragraph { .. }) {
                "paragraph exceeds 64 KiB; split it into several paragraphs"
            } else {
                "record too large"
            }));
        }
        Ok(self.sink.record(&record, &self.payload)?)
    }

    /// A destination at the top of the next line laid out. `level` 1 to 6 also makes it a bookmark.
    pub fn anchor(&mut self, name: &str, level: u8, title: &str) -> Result<(), Error> {
        self.record(Record::Anchor {
            name: name.into(),
            level,
            title: title.into(),
        })
    }

    pub fn paragraph(
        &mut self,
        runs: Vec<Run>,
        align: u8,
        orphans: f64,
        widows: f64,
    ) -> Result<(), Error> {
        self.flush()?;
        let mut links: Vec<&str> = Vec::new();
        for link in runs.iter().filter_map(|run| run.link.as_deref()) {
            if !links.contains(&link) {
                links.push(link);
            }
        }
        if links.len() > 0xffff {
            return Err(Error::new("paragraph has too many links"));
        }
        self.record(Record::Paragraph {
            runs,
            align,
            orphans,
            widows,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn image(
        &mut self,
        structure: u32,
        slot: u16,
        width: Option<Dimension>,
        height: Option<f64>,
        max_width: Option<Dimension>,
        max_height: Option<f64>,
        alt: &str,
        link: Option<Rc<str>>,
        opacity: f64,
    ) -> Result<(), Error> {
        self.record(Record::Image {
            structure,
            slot,
            width,
            height,
            max_width,
            max_height,
            link,
            alt: alt.into(),
            opacity,
        })
    }

    /// The slot of a drawing, registered once per distinct encoding.
    pub fn drawing(&mut self, bytes: Vec<u8>) -> Result<u16, Error> {
        if let Some(slot) = self.drawings.get(&bytes) {
            return Ok(*slot);
        }
        let slot = self.sink.add_image(bytes.clone())?;
        self.drawings.insert(bytes, slot);
        Ok(slot)
    }

    /// The slot of the raster staged as `handle`, registered once.
    pub fn raster(&mut self, handle: u16) -> Result<u16, Error> {
        if let Some(slot) = self.rasters.get(&handle) {
            return Ok(*slot);
        }
        let slot = self.sink.add_raster(handle)?;
        self.rasters.insert(handle, slot);
        Ok(slot)
    }

    pub fn spacer(&mut self, height: f64) -> Result<(), Error> {
        self.record(Record::Spacer(height))
    }

    /// Holds a vertical margin until content follows, so adjoining margins collapse to the largest (CSS 2.1 8.3.1).
    pub fn margin(&mut self, height: f64) {
        self.held = self.held.max(height);
    }

    /// Writes the held margin and starts, so no margin collapses across this point.
    pub fn flush(&mut self) -> Result<(), Error> {
        if self.held != 0.0 {
            self.record(Record::Spacer(self.held))?;
        }
        self.held = 0.0;
        for start in std::mem::take(&mut self.starts) {
            self.record(start)?;
        }
        Ok(())
    }

    /// With `through`, the start waits for content so a held margin collapses past it to land
    /// outside the box, and the end lets the last child's margin out; an empty one writes nothing.
    pub fn stack_start(&mut self, gap: f64, through: bool) -> Result<(), Error> {
        self.open(through, Record::StackStart(gap))
    }

    pub fn stack_end(&mut self, through: bool) -> Result<(), Error> {
        self.close(through, Record::StackEnd)
    }

    pub fn box_start(&mut self, value: Box, through: bool) -> Result<(), Error> {
        self.open(through, Record::BoxStart(value))
    }

    pub fn box_end(&mut self, through: bool) -> Result<(), Error> {
        self.close(through, Record::BoxEnd)
    }

    /// Without `slots`, each column holds its own top-aligned cell.
    pub fn row_start(&mut self, columns: Vec<Column>, slots: Vec<Slot>) -> Result<(), Error> {
        self.flush()?;
        self.record(Record::RowStart(columns, slots))
    }

    pub fn row_end(&mut self) -> Result<(), Error> {
        self.flush()?;
        self.record(Record::RowEnd)
    }

    pub fn flex_start(&mut self, flex: Flex, items: Vec<FlexItem>) -> Result<(), Error> {
        self.flush()?;
        self.record(Record::FlexStart(flex, items))
    }

    pub fn flex_end(&mut self) -> Result<(), Error> {
        self.flush()?;
        self.record(Record::FlexEnd)
    }

    /// The first `header` children repeat on every page the table continues on.
    pub fn table_start(
        &mut self,
        header: usize,
        footer: usize,
        width: Column,
    ) -> Result<(), Error> {
        if header > 0xffff {
            return Err(Error::new("too many header rows"));
        }
        if footer > 0xffff {
            return Err(Error::new("too many footer rows"));
        }
        self.flush()?;
        self.record(Record::TableStart(header as u16, footer as u16, width))
    }

    pub fn table_end(&mut self) -> Result<(), Error> {
        self.flush()?;
        self.record(Record::TableEnd)
    }

    pub fn page_break(&mut self) -> Result<(), Error> {
        if let Some(option) = &self.repeating {
            return Err(Error::new(format!("{option} cannot contain page breaks")));
        }
        self.held = 0.0;
        self.flush()?;
        self.pages += 1;
        if self.pages > MAX_PAGES {
            return Err(Error::new(format!(
                "document exceeds {MAX_PAGES} pages; split it across several renders"
            )));
        }
        self.record(Record::PageBreak)
    }

    /// Avoids a page break here; a header or footer line has no breaks.
    pub fn avoid(&mut self) -> Result<(), Error> {
        if self.repeating.is_some() {
            return Ok(());
        }
        self.record(Record::Avoid)
    }

    /// The engine takes the next top-level block, this stack, as the header or footer.
    pub fn repeat_start(&mut self, footer: bool, first: bool, option: &str) -> Result<(), Error> {
        self.record(Record::Repeat {
            footer,
            first,
            fixed: false,
        })?;
        self.stack_start(0.0, false)?;
        self.repeating = Some(option.into());
        Ok(())
    }

    pub fn repeat_end(&mut self) -> Result<(), Error> {
        self.repeating = None;
        self.stack_end(false)
    }

    /// Sets aside the held margin and box starts, so an out-of-flow box neither collapses
    /// margins nor ends a collapse, and `attach` restores them after it.
    pub fn detach(&mut self) -> (f64, Vec<Record>) {
        (
            std::mem::take(&mut self.held),
            std::mem::take(&mut self.starts),
        )
    }

    pub fn attach(&mut self, (held, starts): (f64, Vec<Record>)) {
        self.held = held;
        self.starts = starts;
    }

    /// A `position: fixed` box's records wait until the body ends, then follow it as a block
    /// the engine paints on every page.
    pub fn fixed_start(&mut self) -> Result<(), Error> {
        self.fixed = Some(self.fixed.take().unwrap_or_default());
        self.record(Record::Repeat {
            footer: false,
            first: false,
            fixed: true,
        })?;
        self.stack_start(0.0, false)
    }

    pub fn fixed_end(&mut self) -> Result<(), Error> {
        self.stack_end(false)?;
        self.held = 0.0;
        self.deferred.extend(self.fixed.take().unwrap_or_default());
        Ok(())
    }

    pub fn end(&mut self) -> Result<(), Error> {
        for record in std::mem::take(&mut self.deferred) {
            self.record(record)?;
        }
        self.record(Record::End)
    }

    fn open(&mut self, through: bool, start: Record) -> Result<(), Error> {
        if through {
            self.starts.push(start);
            return Ok(());
        }
        self.flush()?;
        self.record(start)
    }

    fn close(&mut self, through: bool, end: Record) -> Result<(), Error> {
        if !through {
            self.flush()?;
        } else if self.starts.pop().is_some() {
            // The start is still held, so the box is empty and its margins collapse through it.
            return Ok(());
        }
        self.record(end)
    }
}

/// A paint of an svg shape: a colour, or the element's text colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paint {
    Rgb(Rgb),
    Current,
}

/// One svg.ts shape with its state reduced to what `encode` reads. `fill_opacity` and
/// `stroke_opacity` already hold the shape's own opacity multiplied in (`fillOpacity * opacity`).
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub matrix: [f64; 6],
    pub fill: Option<Paint>,
    pub even_odd: bool,
    pub fill_opacity: f64,
    pub stroke: Option<Paint>,
    pub stroke_width: f64,
    pub cap: u8,
    pub join: u8,
    pub miter: f64,
    pub dashes: Vec<f64>,
    pub dash_offset: f64,
    pub stroke_opacity: f64,
    /// 0 move, 1 line, 2 cubic, 3 close; `points` holds their coordinates in order.
    pub ops: Vec<u8>,
    pub points: Vec<f64>,
}

/// An svg.ts `Drawing`: intrinsic size in points, the viewBox, and the shapes in user units.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub width: f64,
    pub height: f64,
    pub view_box: [f64; 4],
    /// x and y alignment factors; None is `preserveAspectRatio="none"`.
    pub align: Option<[f64; 2]>,
    pub slice: bool,
    pub shapes: Vec<Shape>,
}

/// The box the viewBox is fitted into; only its aspect ratio reaches the engine.
pub(super) fn frame(
    drawing: &Drawing,
    width: Option<Dimension>,
    height: Option<f64>,
) -> Result<[f64; 2], Error> {
    match (width, height) {
        (Some(width), Some(height)) if width.kind == 1 => Ok([width.value, height]),
        (Some(_), Some(_)) if drawing.align.is_some() => Err(Error::new(
            "a percent width with a height leaves the aspect ratio open; use a pt or px width, or preserveAspectRatio=\"none\"",
        )),
        _ => Ok([drawing.width, drawing.height]),
    }
}

fn multiply(m: &[f64; 6], n: &[f64; 6]) -> [f64; 6] {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// svg.ts `encode`: the drawing fitted to the unit square, y up, as an image fills it.
/// `opacity` is the element's CSS opacity, folded into each shape because a shape's own alpha
/// replaces the page's inside a form XObject.
#[inline(never)]
pub(super) fn encode(
    drawing: &Drawing,
    [width, height]: [f64; 2],
    current: Rgb,
    opacity: f64,
) -> Result<Vec<u8>, Error> {
    let [vx, vy, vw, vh] = drawing.view_box;
    let (mut sx, mut sy) = (width / vw, height / vh);
    if drawing.align.is_some() {
        let scale = if drawing.slice {
            sx.max(sy)
        } else {
            sx.min(sy)
        };
        (sx, sy) = (scale, scale);
    }
    let [ax, ay] = drawing.align.unwrap_or([0.0, 0.0]);
    let fit = [
        sx / width,
        0.0,
        0.0,
        -sy / height,
        ((width - vw * sx) * ax - vx * sx) / width,
        1.0 - ((height - vh * sy) * ay - vy * sy) / height,
    ];
    let mut out = b"FPDV".to_vec();
    let f32 = |out: &mut Vec<u8>, value: f64| {
        if !(value as f32).is_finite() {
            return Err(Error::new("the drawing's coordinates are too large"));
        }
        out.extend_from_slice(&(value as f32).to_le_bytes());
        Ok(())
    };
    let rgba = |out: &mut Vec<u8>, paint: Paint, alpha_value: f64| {
        out.extend_from_slice(&match paint {
            Paint::Rgb(rgb) => rgb,
            Paint::Current => current,
        });
        out.push(alpha(alpha_value));
    };
    f32(&mut out, width)?;
    f32(&mut out, height)?;
    for shape in &drawing.shapes {
        out.push(
            u8::from(shape.fill.is_some())
                | u8::from(shape.even_odd) << 1
                | u8::from(shape.stroke.is_some()) << 2
                | shape.cap << 3
                | shape.join << 5,
        );
        for value in multiply(&fit, &shape.matrix) {
            f32(&mut out, value)?;
        }
        if let Some(fill) = shape.fill {
            rgba(&mut out, fill, shape.fill_opacity * opacity);
        }
        if let Some(stroke) = shape.stroke {
            rgba(&mut out, stroke, shape.stroke_opacity * opacity);
            f32(&mut out, shape.stroke_width)?;
            f32(&mut out, shape.miter)?;
            out.push(shape.dashes.len() as u8);
            for dash in &shape.dashes {
                f32(&mut out, *dash)?;
            }
            let cycle: f64 = shape.dashes.iter().sum();
            let offset = if cycle != 0.0 {
                ((shape.dash_offset % cycle) + cycle) % cycle
            } else {
                0.0
            };
            f32(&mut out, offset)?;
        }
        out.extend_from_slice(&(shape.ops.len() as u32).to_le_bytes());
        let mut point = 0;
        for &op in &shape.ops {
            out.push(op);
            let count = [2, 2, 6, 0].get(usize::from(op)).copied().unwrap_or(0);
            for _ in 0..count {
                f32(
                    &mut out,
                    shape.points.get(point).copied().unwrap_or(f64::NAN),
                )?;
                point += 1;
            }
        }
    }
    Ok(out)
}
