use std::ops::Range;
use std::rc::Rc;

use pdf_writer::Content;

use crate::command::{uniform_rows, CommandParser, OwnedCommand};
use crate::document::{Block, Document, Element, Table};
use crate::font::{EmbeddedFont, Font, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout, Point};
use crate::image::Image;
use crate::layout::{painted, table_width, LayoutBuffer, LayoutEngine, PositionedBox};
use crate::paint::{PageLink, PageTag, PaintState, PdfPainter};
use crate::pdf::{finish_pdf, Anchor, Metadata};
use crate::{BoxStyle, Command, FontId, Page, Pt, RenderError};

/// Total content-stream bytes a document may paint before `finish`.
pub const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;
/// Pages per document; a five-byte page break must not buy an unbounded page list.
pub const MAX_PAGES: usize = 10_000;
/// Destinations and bookmarks per render; each is a PDF object.
const MAX_ANCHORS: usize = 10_000;

pub fn render(page: Page, commands: &[Command<'_>]) -> Result<Vec<u8>, RenderError> {
    let mut renderer = Renderer::new(page);
    renderer.push(commands)?;
    renderer.finish()
}

pub struct Renderer {
    /// The body of the current page.
    page: PageLayout,
    /// The body of every page after the first.
    later: PageLayout,
    contents: Vec<Content>,
    /// Per finished page, its split-box fragments, painted to go before its content.
    underlays: Vec<Vec<u8>>,
    /// The open page's split-box fragments in the order they start, so an outer box
    /// paints beneath an inner one. `origin.y` is minus the fragment's top y.
    fragments: Vec<PositionedBox>,
    /// Bytes in every page except the open last one.
    content_bytes: usize,
    cursor: PageCursor,
    fonts: FontBook,
    images: Vec<Image>,
    used_images: Vec<bool>,
    /// Link annotations per page, parallel to `contents`.
    links: Vec<Vec<PageLink>>,
    tags: Vec<Vec<PageTag>>,
    /// Alphas each page draws with, parallel to `contents`.
    alphas: Vec<Vec<[u8; 2]>>,
    layout: LayoutBuffer,
    /// Index 0 paints on every page; index 1 replaces it on the first page.
    header: [Option<Furniture>; 2],
    footer: [Option<Furniture>; 2],
    metadata: Metadata,
    /// Anchors with no line yet, waiting for the next one painted.
    pending: Vec<u16>,
    table: Option<OpenTable>,
    /// The open page's fill colour and font.
    state: PaintState,
    /// Open boxes, outermost first, as their children arrive: the column to
    /// restore when each closes, and its style.
    boxes: Vec<(LayoutArea, BoxStyle)>,
    /// The painted open boxes' fragments on this page, as indices into `fragments`.
    open: Vec<usize>,
    /// Where children lay out: the page body, or the innermost open box's content area.
    column: LayoutArea,
    /// Content after the page's last allowed break point, unpainted until a later
    /// break point is allowed, so the page can still end before it.
    held: Vec<Held>,
    held_bytes: usize,
    /// Whether the break point after the held content is avoided.
    chained: bool,
    /// Where boxes opened since the last content start in `held`, and whether
    /// the break before them is avoided. A box's top stays with its first child.
    opened: Option<(usize, bool)>,
    /// The table whose painted rows end this page, so its footer goes below them.
    trailer: Option<Rc<TableEdges>>,
    /// Whether the page started at an unforced break, which truncates the margins after it.
    unforced: bool,
    /// Where in `held` a forced break waits for the next content, so boxes that
    /// close right after it end above it, as CSS propagates a break to a parent's edge.
    forced: Option<usize>,
}

/// A top-level table between its start and end: the header and footer rows lay
/// out once, and each body row lays out and places as it arrives.
struct OpenTable {
    area: LayoutArea,
    edges: Rc<TableEdges>,
    columns: Option<usize>,
    /// The next row's index among the table's children, which `TableRowOverflow` reports.
    index: usize,
}

/// The header and footer rows a table repeats on every page it has body rows on.
struct TableEdges {
    header: LayoutBuffer,
    header_height: Pt,
    footer: LayoutBuffer,
    footer_height: Pt,
}

/// Placed content waiting in `Renderer::held`.
enum Held {
    /// A block's lines, painted with block y `offset` at the cursor, and the height they take.
    Lines(LayoutBuffer, Range<usize>, Pt, Pt),
    /// A margin, which a page end truncates.
    Space(Pt),
    /// A box's top edge, where its fragment starts.
    Open(PositionedBox),
    /// A box's bottom edge.
    Close(BoxStyle),
    /// A table body row, or the table's last footer when `None`, and its height;
    /// the header goes above when it is the table's first on the page.
    Row(Option<LayoutBuffer>, Pt, Rc<TableEdges>),
}

impl Held {
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
            + match self {
                Self::Lines(layout, ..) | Self::Row(Some(layout), ..) => layout.capacity(),
                _ => 0,
            }
    }
}

/// A header or footer block, laid out again on every page in `finish` with that page's numbers.
struct Furniture {
    commands: Vec<OwnedCommand>,
    /// Measured with five-digit page numbers and reserved on every page it paints on.
    height: Pt,
}

impl Renderer {
    pub fn new(page: Page) -> Self {
        Self::with_fonts(page, Vec::new())
    }

    pub(crate) fn with_fonts(page: Page, embedded: Vec<EmbeddedFont>) -> Self {
        let page = PageLayout::new(page);
        Self {
            page,
            later: page,
            contents: vec![Content::with_capacity(0)],
            underlays: Vec::new(),
            fragments: Vec::new(),
            content_bytes: 0,
            cursor: PageCursor::new(page),
            fonts: FontBook::new(embedded),
            images: Vec::new(),
            used_images: Vec::new(),
            links: vec![Vec::new()],
            tags: vec![Vec::new()],
            alphas: vec![Vec::new()],
            layout: LayoutBuffer::default(),
            header: Default::default(),
            footer: Default::default(),
            metadata: Metadata::default(),
            pending: Vec::new(),
            table: None,
            state: PaintState::default(),
            boxes: Vec::new(),
            open: Vec::new(),
            column: LayoutArea::root(page.content_width()),
            held: Vec::new(),
            held_bytes: 0,
            chained: false,
            opened: None,
            trailer: None,
            unforced: false,
            forced: None,
        }
    }

    pub(crate) fn font_count(&self) -> usize {
        self.fonts.len()
    }

    /// Images register at any point before the record that paints them.
    pub(crate) fn add_image(&mut self, image: Image) -> u16 {
        self.images.push(image);
        self.used_images.push(false);
        (self.images.len() - 1) as u16
    }

    pub(crate) fn image_count(&self) -> usize {
        self.images.len()
    }

    fn mark_used(&mut self, layout: &LayoutBuffer) {
        self.fonts.mark_layout(layout);
        for image in &layout.images {
            self.used_images[usize::from(image.slot)] = true;
        }
    }

    #[cfg(test)]
    pub(crate) fn scratch_capacity(&self) -> usize {
        self.layout.capacity()
    }

    pub fn push(&mut self, commands: &[Command<'_>]) -> Result<(), RenderError> {
        let document = CommandParser::new(commands)
            .parse_document()
            .map_err(|_| RenderError::InvalidLayout)?;
        self.push_document(&document)
    }

    pub(crate) fn push_owned(&mut self, commands: &[OwnedCommand]) -> Result<(), RenderError> {
        let block = CommandParser::new(commands)
            .parse_single_block()
            .map_err(|_| RenderError::InvalidLayout)?;
        self.push_block(&block)
    }

    /// Starts a streamed table from `TableStart` and its header and footer rows, closed by `TableEnd`.
    pub(crate) fn push_table_head(&mut self, commands: &[OwnedCommand]) -> Result<(), RenderError> {
        match CommandParser::new(commands).parse_single_block() {
            Ok(Block::Element(Element::Table(table))) => self.start_table(&table),
            _ => Err(RenderError::InvalidLayout),
        }
    }

    pub(crate) fn push_table_row(&mut self, commands: &[OwnedCommand]) -> Result<(), RenderError> {
        match CommandParser::new(commands).parse_single_block() {
            Ok(Block::Element(row)) => self.table_row(&row),
            _ => Err(RenderError::InvalidLayout),
        }
    }

    /// Reserves the block's height, measured with five-digit page numbers, on
    /// every page it paints on. Index 1 (`first`) replaces index 0 on the first page.
    pub(crate) fn set_furniture(
        &mut self,
        footer: bool,
        first: bool,
        commands: Vec<OwnedCommand>,
    ) -> Result<(), RenderError> {
        let placeholder = numbered(&commands, &|_, font| {
            let font = self.fonts.get(font).ok_or(RenderError::InvalidLayout)?;
            Ok(widest_digit(font)?.to_string().repeat(5))
        })?;
        let height = self.lay_out_furniture(&placeholder, &mut LayoutBuffer::default())?;
        let slot = &mut if footer {
            &mut self.footer
        } else {
            &mut self.header
        }[usize::from(first)];
        if slot.is_some() {
            return Err(RenderError::InvalidLayout);
        }
        *slot = Some(Furniture { commands, height });
        self.page = self.body(true);
        self.later = self.body(false);
        self.page.validate_furniture()?;
        self.later.validate_furniture()?;
        self.cursor.reset(self.page);
        Ok(())
    }

    /// The body of the first or a later page, between the header and footer it paints.
    fn body(&self, first: bool) -> PageLayout {
        let height = |slots: &[Option<Furniture>; 2]| {
            pick(slots, first).map_or(Pt::ZERO, |furniture| furniture.height)
        };
        let mut body = self.page;
        body.header = height(&self.header);
        body.footer = height(&self.footer);
        body
    }

    /// Furniture never splits, so it lays out whole at the body width.
    fn lay_out_furniture(
        &self,
        commands: &[OwnedCommand],
        output: &mut LayoutBuffer,
    ) -> Result<Pt, RenderError> {
        let Block::Element(element) = CommandParser::new(commands)
            .parse_single_block()
            .map_err(|_| RenderError::InvalidLayout)?
        else {
            return Err(RenderError::InvalidLayout);
        };
        output.clear();
        Ok(LayoutEngine::new(&self.fonts, &self.images).layout(
            &element,
            LayoutArea::root(self.page.content_width()),
            output,
        )?)
    }

    /// A block too tall for the first page's body can still fit a later, taller one.
    fn validate_block(&self, height: Pt) -> Result<(), RenderError> {
        self.page
            .validate_block(height)
            .or_else(|_| self.later.validate_block(height))
    }

    pub(crate) fn set_metadata(&mut self, metadata: Metadata) {
        self.metadata = metadata;
    }

    /// Registers a destination, named for links and `#nameddest=` and bookmarked when
    /// `level` is a heading level. Names the invalid field on failure.
    pub(crate) fn add_anchor(
        &mut self,
        name: &str,
        level: u8,
        title: &str,
    ) -> Result<u16, &'static str> {
        let slot = u16::try_from(self.metadata.anchors.len())
            .ok()
            .filter(|slot| usize::from(*slot) < MAX_ANCHORS)
            .ok_or("anchor count")?;
        if !name.is_empty() {
            let Err(index) = self.metadata.find(name) else {
                return Err("anchor name: already used");
            };
            self.metadata.names.insert(index, (name.to_owned(), slot));
        }
        self.metadata.anchors.push(Anchor {
            level,
            title: title.to_owned(),
            at: None,
        });
        Ok(slot)
    }

    fn push_document(&mut self, document: &Document<'_>) -> Result<(), RenderError> {
        for block in &document.blocks {
            self.push_block(block)?;
        }
        Ok(())
    }

    fn push_block(&mut self, block: &Block<'_>) -> Result<(), RenderError> {
        match block {
            Block::PageBreak => self.page_break()?,
            Block::Avoid => self.avoid(),
            Block::Element(element) => self.render_element(element)?,
        }
        self.check_size()
    }

    fn check_size(&self) -> Result<(), RenderError> {
        if self.content_bytes + self.contents.last().unwrap().len() > MAX_CONTENT_BYTES {
            return Err(RenderError::DocumentTooLarge);
        }
        Ok(())
    }

    /// A forced break, placed before the next content; boxes opened just before
    /// it move along, since a break before a first child is one before its parent.
    pub(crate) fn page_break(&mut self) -> Result<(), RenderError> {
        self.forced = Some(self.opened.map_or(self.held.len(), |(at, _)| at));
        self.chained = false;
        Ok(())
    }

    /// Avoids a break here, or before the boxes opened since the last content.
    pub(crate) fn avoid(&mut self) {
        match &mut self.opened {
            Some((_, avoid)) => *avoid = true,
            None => self.chained = true,
        }
    }

    fn render_element(&mut self, element: &Element<'_>) -> Result<(), RenderError> {
        match element {
            Element::Table(table) => return self.render_table(table),
            Element::Box(node)
                if node.size == crate::BoxSize::AUTO && node.style.radius.is_none() =>
            {
                self.open_box(node.style)?;
                for child in &node.children {
                    self.render_element(child)?;
                }
                return self.close_box();
            }
            Element::Spacer(spacer) => {
                self.page.validate_block(spacer.height)?;
                return self.hold(Held::Space(spacer.height));
            }
            // Without a height to share out, a wrapping row is its rows stacked, so it breaks
            // between them.
            Element::Flex(node)
                if node.style.wrap
                    && node.style.min_height == Pt::ZERO
                    && node.style.max_height.is_none() =>
            {
                let mut rows = LayoutEngine::new(&self.fonts, &self.images)
                    .flex_rows(node, self.column.width)?;
                for index in 0..rows.rows.len() {
                    if index > 0 {
                        self.page.validate_block(node.style.row_gap)?;
                        self.hold(Held::Space(node.style.row_gap))?;
                    }
                    self.begin(false)?;
                    self.layout.clear();
                    let height = LayoutEngine::new(&self.fonts, &self.images).layout_flex_row(
                        node,
                        &mut rows,
                        index,
                        self.column,
                        &mut self.layout,
                    )?;
                    let layout = std::mem::take(&mut self.layout);
                    self.mark_used(&layout);
                    self.place(element, layout, height)?;
                    self.chained = false;
                }
                return Ok(());
            }
            _ => {}
        }
        self.begin(false)?;
        self.layout.clear();
        let height = LayoutEngine::new(&self.fonts, &self.images).layout(
            element,
            self.column,
            &mut self.layout,
        )?;
        let layout = std::mem::take(&mut self.layout);
        self.mark_used(&layout);
        self.place(element, layout, height)?;
        self.chained = false;
        Ok(())
    }

    /// Starts a box whose children place in its content area until `close_box`,
    /// so it breaks across pages like CSS `box-decoration-break: slice`.
    pub(crate) fn open_box(&mut self, style: BoxStyle) -> Result<(), RenderError> {
        let column = self.column;
        let width = column.width - style.margin.left - style.margin.right;
        let inset = style.border.left + style.padding.left;
        let inner = width - inset - style.padding.right - style.border.right;
        if inner <= Pt::ZERO {
            return Err(RenderError::InvalidLayout);
        }
        let top = style.margin.top + style.border.top + style.padding.top;
        self.validate_block(top + style.padding.bottom + style.border.bottom)?;
        self.boxes.push((column, style));
        self.column = column
            .translated(style.margin.left + inset, Pt::ZERO)
            .with_width(inner);
        self.opened.get_or_insert((self.held.len(), self.chained));
        self.chained = false;
        self.hold(Held::Open(PositionedBox {
            origin: Point {
                x: column.origin.x + style.margin.left,
                y: Pt::ZERO,
            },
            width,
            height: Pt::ZERO,
            style,
        }))
    }

    /// Ends the innermost box. Its bottom border and padding stay with its last
    /// content, and only its last fragment has them, as `slice` does.
    pub(crate) fn close_box(&mut self) -> Result<(), RenderError> {
        let (column, style) = self.boxes.pop().ok_or(RenderError::InvalidLayout)?;
        self.column = column;
        self.begin(true)?;
        self.room(style.padding.bottom + style.border.bottom, None)?;
        let after = self.forced == Some(self.held.len());
        self.hold(Held::Close(style))?;
        self.hold(Held::Space(style.margin.bottom))?;
        if after {
            self.forced = Some(self.held.len());
        }
        Ok(())
    }

    /// Settles the break point before new content: what precedes the page's
    /// last allowed break point paints, because no later break can move it.
    fn begin(&mut self, sticky: bool) -> Result<(), RenderError> {
        if let Some(at) = self.forced.filter(|_| !sticky) {
            self.forced = None;
            self.commit(at)?;
            // At a page top a forced break has nothing to move.
            if !self.cursor.at_top(self.page) {
                self.start_page()?;
                self.unforced = false;
            }
        }
        let at = match self.opened.take() {
            Some((at, avoid)) => (!avoid).then_some(at),
            None => (!sticky && !self.chained).then_some(self.held.len()),
        };
        at.map_or(Ok(()), |at| self.commit(at))
    }

    fn hold(&mut self, item: Held) -> Result<(), RenderError> {
        self.held_bytes += item.size();
        self.held.push(item);
        // Past this, avoid rules give way rather than buffer more, as CSS lets them.
        if self.held_bytes > MAX_CONTENT_BYTES {
            self.commit(self.held.len())?;
        }
        Ok(())
    }

    /// Paints the first `count` held items at the cursor.
    fn commit(&mut self, count: usize) -> Result<(), RenderError> {
        self.opened = None;
        self.forced = self.forced.map(|at| at.saturating_sub(count));
        let rest = self.held.split_off(count);
        for item in std::mem::replace(&mut self.held, rest) {
            self.held_bytes -= item.size();
            match item {
                Held::Lines(layout, lines, offset, height) => {
                    self.paint(&layout, lines, offset, height)?;
                    self.layout = layout;
                }
                Held::Space(height) if !self.truncates() => {
                    let room = self.cursor.remaining(self.page).get().max(0.0);
                    self.cursor.advance(Pt(height.get().min(room)));
                }
                Held::Space(_) => {}
                Held::Open(mut fragment) => {
                    let style = fragment.style;
                    if !self.truncates() {
                        self.cursor.advance(style.margin.top);
                    }
                    fragment.origin.y = Pt::ZERO - self.cursor.origin(self.page).y;
                    self.open.push(self.fragments.len());
                    self.fragments.push(fragment);
                    self.cursor.advance(style.border.top + style.padding.top);
                }
                Held::Close(style) => {
                    self.cursor
                        .advance(style.padding.bottom + style.border.bottom);
                    let y = self.cursor.origin(self.page).y;
                    if let Some(fragment) = self
                        .open
                        .pop()
                        .and_then(|slot| self.fragments.get_mut(slot))
                    {
                        fragment.height = Pt::ZERO - fragment.origin.y - y;
                    }
                }
                Held::Row(row, height, edges) => {
                    self.table_top(&edges)?;
                    let rows = row.as_ref().unwrap_or(&edges.footer);
                    self.paint(rows, 0..rows.lines.len(), Pt::ZERO, height)?;
                    self.trailer = row.is_some().then(|| edges.clone());
                    if let Some(row) = row {
                        self.layout = row;
                    }
                }
            }
        }
        Ok(())
    }

    /// The height the held content takes on this page, or on an empty one, plus
    /// `table`'s header when its next row would be its first on the page.
    // Inlined into both callers, it costs about 1 KB of WASM.
    #[inline(never)]
    fn ahead(&self, fresh: bool, table: Option<&Rc<TableEdges>>) -> Pt {
        let header = |trailer: Option<&Rc<TableEdges>>, edges: &Rc<TableEdges>| {
            if trailer.is_some_and(|trailer| Rc::ptr_eq(trailer, edges)) {
                Pt::ZERO
            } else {
                edges.header_height
            }
        };
        let mut trailer = self.trailer.as_ref().filter(|_| !fresh);
        let mut height = Pt::ZERO;
        for item in &self.held {
            height += match item {
                Held::Lines(_, _, _, height) | Held::Space(height) => *height,
                Held::Open(fragment) => {
                    let style = fragment.style;
                    style.margin.top + style.border.top + style.padding.top
                }
                Held::Close(style) => style.padding.bottom + style.border.bottom,
                Held::Row(row, height, edges) => {
                    let top = header(trailer, edges);
                    trailer = row.as_ref().map(|_| edges);
                    top + *height
                }
            };
        }
        height + table.map_or(Pt::ZERO, |edges| header(trailer, edges))
    }

    fn truncates(&self) -> bool {
        self.unforced && self.cursor.at_top(self.page)
    }

    fn fits(&self, need: Pt, table: Option<&Rc<TableEdges>>) -> bool {
        self.ahead(false, table) + need <= self.cursor.remaining(self.page)
    }

    /// Breaks before the held content, the page's last allowed break point,
    /// unless that is the page top or the content would not fit an empty page.
    fn rewind(&mut self, need: Pt, table: Option<&Rc<TableEdges>>) -> Result<bool, RenderError> {
        let moves = (!self.cursor.at_top(self.page)
            || self.later.content_height() > self.page.content_height())
            && self.ahead(true, table) + need <= self.later.content_height();
        if moves {
            self.start_page()?;
        }
        Ok(moves)
    }

    /// Makes room for `need` after the held content. When no allowed break point
    /// helps, the avoided break before the new content gives way, as CSS lets it.
    fn room(&mut self, need: Pt, table: Option<&Rc<TableEdges>>) -> Result<(), RenderError> {
        if !self.fits(need, table) && !self.rewind(need, table)? {
            self.commit(self.held.len())?;
            if !self.fits(need, table) && !self.rewind(need, table)? {
                return Err(RenderError::PageOverflow);
            }
        }
        Ok(())
    }

    /// Paints the open page's fragments for `finish` to put before its content.
    /// Open boxes end theirs at the body bottom without a bottom border and
    /// continue on the next page without a top border.
    fn end_page(&mut self) {
        let mut next = Vec::with_capacity(self.open.len());
        for slot in &mut self.open {
            let fragment = &mut self.fragments[*slot];
            let mut continued = *fragment;
            continued.origin.y = Pt::ZERO - self.later.top();
            continued.style.border.top = Pt::ZERO;
            fragment.height = Pt::ZERO - fragment.origin.y - self.page.bottom();
            fragment.style.border.bottom = Pt::ZERO;
            *slot = next.len();
            next.push(continued);
        }
        let mut fragments = std::mem::replace(&mut self.fragments, next);
        fragments.retain(|fragment| fragment.height > Pt::ZERO && painted(&fragment.style));
        let mut underlay = Vec::new();
        if !fragments.is_empty() {
            let mut content = Content::new();
            PdfPainter::new(
                &mut content,
                self.links.last_mut().unwrap(),
                self.metadata.tagged.then(|| self.tags.last_mut().unwrap()),
                self.alphas.last_mut().unwrap(),
                &mut PaintState::default(),
            )
            .paint_boxes(
                &fragments,
                Point {
                    x: self.page.page().margin.left,
                    y: Pt::ZERO,
                },
            );
            underlay = content.finish().into_vec();
        }
        self.content_bytes += self.contents.last().unwrap().len() + underlay.len();
        self.underlays.push(underlay);
    }

    /// Holds a block's lines after its last allowed break point on the page and
    /// paints those before it. Only a paragraph breaks inside, between lines,
    /// leaving at least `orphans` lines before a break and `widows` after it.
    fn place(
        &mut self,
        element: &Element<'_>,
        layout: LayoutBuffer,
        height: Pt,
    ) -> Result<(), RenderError> {
        let lines = layout.lines.len();
        let Element::Paragraph(paragraph) = element else {
            self.validate_block(height)?;
            self.room(height, None)?;
            return self.hold(Held::Lines(layout, 0..lines, Pt::ZERO, height));
        };
        let allowed = |start: usize, split: usize| {
            split - start >= usize::from(paragraph.orphans)
                && lines - split >= usize::from(paragraph.widows)
        };
        let head = (1..lines)
            .find(|split| allowed(0, *split))
            .map_or(height, |split| layout.lines[split - 1].bottom);
        // `offset` is the block y where this page starts, so gaps straddling a break collapse.
        let (mut start, mut offset) = (0, Pt::ZERO);
        loop {
            let available = self.cursor.remaining(self.page) - self.ahead(false, None);
            let mut end = start;
            while layout
                .lines
                .get(end)
                .is_some_and(|line| line.bottom - offset <= available)
            {
                end += 1;
            }
            if end == lines {
                if let Some(split) = (start + 1..lines)
                    .rev()
                    .find(|split| allowed(start, *split))
                {
                    self.commit(self.held.len())?;
                    let top = layout.lines[split].top;
                    self.paint(&layout, start..split, offset, top - offset)?;
                    (start, offset) = (split, top);
                }
                return self.hold(Held::Lines(layout, start..lines, offset, height - offset));
            }
            let split = match (start + 1..=end).rev().find(|split| allowed(start, *split)) {
                Some(split) => split,
                None if start == 0 && self.rewind(head, None)? => continue,
                // CSS drops orphans and widows first, then breaks before the paragraph.
                None if end > start => end,
                None if self.cursor.at_top(self.page) && self.held.is_empty() => {
                    return Err(RenderError::PageOverflow)
                }
                None => {
                    self.commit(self.held.len())?;
                    self.start_page()?;
                    continue;
                }
            };
            self.commit(self.held.len())?;
            self.paint(&layout, start..split, offset, Pt::ZERO)?;
            self.start_page()?;
            (start, offset) = (split, layout.lines[split].top);
        }
    }

    /// Rows place one at a time; the header and footer rows lay out once into
    /// their own buffers. The header repaints at the top of every page a body
    /// row opens, the footer after the last body row of every page.
    fn render_table(&mut self, table: &Table<'_>) -> Result<(), RenderError> {
        self.start_table(table)?;
        for row in &table.rows[table.header_rows + table.footer_rows..] {
            self.table_row(row)?;
        }
        self.end_table()
    }

    /// Lays out the header and footer, the first `header_rows + footer_rows` rows.
    fn start_table(&mut self, table: &Table<'_>) -> Result<(), RenderError> {
        let area = self
            .column
            .with_width(table_width(table.width, self.column.width)?);
        let head = table.header_rows + table.footer_rows;
        let (header, header_height) = self.layout_rows(&table.rows[..table.header_rows], area)?;
        let (footer, footer_height) =
            self.layout_rows(&table.rows[table.header_rows..head], area)?;
        self.validate_block(header_height + footer_height)?;
        let mut columns = None;
        for row in &table.rows[..head] {
            uniform_rows(row, &mut columns);
        }
        self.table = Some(OpenTable {
            area,
            edges: Rc::new(TableEdges {
                header,
                header_height,
                footer,
                footer_height,
            }),
            columns,
            index: head,
        });
        Ok(())
    }

    /// A body row moves whole, and stays with the next one under `break-after: avoid`.
    fn table_row(&mut self, row: &Element<'_>) -> Result<(), RenderError> {
        let table = self.table.as_mut().ok_or(RenderError::InvalidLayout)?;
        if !uniform_rows(row, &mut table.columns) {
            return Err(RenderError::InvalidLayout);
        }
        let (area, index, edges) = (table.area, table.index, table.edges.clone());
        table.index += 1;
        self.begin(false)?;
        self.layout.clear();
        let height =
            LayoutEngine::new(&self.fonts, &self.images).layout(row, area, &mut self.layout)?;
        let layout = std::mem::take(&mut self.layout);
        self.mark_used(&layout);
        let reserved = edges.header_height + edges.footer_height;
        if self.validate_block(height + reserved).is_err() {
            return Err(RenderError::TableRowOverflow(index));
        }
        self.room(height + edges.footer_height, Some(&edges))?;
        self.hold(Held::Row(Some(layout), height, edges))?;
        self.chained = false;
        Ok(())
    }

    /// Holds the last footer, which stays with the last body row; a table
    /// without body rows paints its header above it.
    pub(crate) fn end_table(&mut self) -> Result<(), RenderError> {
        let edges = self.table.take().ok_or(RenderError::InvalidLayout)?.edges;
        self.begin(true)?;
        self.room(edges.footer_height, Some(&edges))?;
        self.hold(Held::Row(None, edges.footer_height, edges))
    }

    /// Paints a table's header above its first row on a page.
    fn table_top(&mut self, edges: &Rc<TableEdges>) -> Result<(), RenderError> {
        if self
            .trailer
            .as_ref()
            .is_some_and(|trailer| Rc::ptr_eq(trailer, edges))
        {
            return Ok(());
        }
        let header = &edges.header;
        self.paint(header, 0..header.lines.len(), Pt::ZERO, edges.header_height)
    }

    fn layout_rows(
        &mut self,
        rows: &[Element<'_>],
        area: LayoutArea,
    ) -> Result<(LayoutBuffer, Pt), RenderError> {
        let mut buffer = LayoutBuffer::default();
        let mut height = Pt::ZERO;
        for row in rows {
            height += LayoutEngine::new(&self.fonts, &self.images).layout(
                row,
                area.translated(Pt::ZERO, height),
                &mut buffer,
            )?;
        }
        self.mark_used(&buffer);
        Ok((buffer, height))
    }

    /// Checks the cap per paint: a repeated table header multiplies one block's bytes.
    fn paint(
        &mut self,
        layout: &LayoutBuffer,
        lines: Range<usize>,
        offset: Pt,
        height: Pt,
    ) -> Result<(), RenderError> {
        let origin = self.cursor.origin(self.page).translated(Pt::ZERO, offset);
        self.painter().paint(layout, lines.clone(), origin);
        self.place_anchors(layout, lines, origin);
        self.cursor.advance(height);
        self.check_size()
    }

    /// Paints `lines`, then places each anchor on the first line at or after it, with the
    /// first fragment winning. An anchor with no line left in `layout` waits for the next one.
    fn place_anchors(&mut self, layout: &LayoutBuffer, lines: Range<usize>, origin: Point) {
        let page = self.contents.len() - 1;
        let at = |line: usize| {
            let line = &layout.lines[line];
            (
                page,
                (origin.x + line.origin.x).get(),
                (origin.y - line.top).get(),
            )
        };
        let anchors = &mut self.metadata.anchors;
        if !lines.is_empty() {
            for slot in self.pending.drain(..) {
                if let Some(anchor) = anchors.get_mut(usize::from(slot)) {
                    anchor.at.get_or_insert_with(|| at(lines.start));
                }
            }
        }
        for &(slot, line) in &layout.anchors {
            let Some(anchor) = anchors.get_mut(usize::from(slot)) else {
                continue;
            };
            if lines.contains(&line) {
                anchor.at.get_or_insert_with(|| at(line));
            } else if line == lines.end && line == layout.lines.len() && anchor.at.is_none() {
                self.pending.push(slot);
            }
        }
    }

    fn painter(&mut self) -> PdfPainter<'_> {
        PdfPainter::new(
            self.contents.last_mut().unwrap(),
            self.links.last_mut().unwrap(),
            self.metadata.tagged.then(|| self.tags.last_mut().unwrap()),
            self.alphas.last_mut().unwrap(),
            &mut self.state,
        )
    }

    /// Ends the page below the painted content, under a table's footer if its rows end it.
    fn start_page(&mut self) -> Result<(), RenderError> {
        if self.contents.len() >= MAX_PAGES {
            return Err(RenderError::TooManyPages);
        }
        if let Some(edges) = self.trailer.take() {
            let origin = self.cursor.origin(self.page);
            let footer = &edges.footer;
            self.painter().paint(footer, 0..footer.lines.len(), origin);
            self.place_anchors(footer, 0..footer.lines.len(), origin);
        }
        self.end_page();
        self.contents.push(Content::with_capacity(0));
        self.alphas.push(Vec::new());
        self.links.push(Vec::new());
        self.tags.push(Vec::new());
        self.page = self.later;
        self.state = PaintState::default();
        self.cursor.reset(self.page);
        self.unforced = true;
        Ok(())
    }

    pub fn finish(mut self) -> Result<Vec<u8>, RenderError> {
        self.commit(self.held.len())?;
        self.end_page();
        // The header and footer repeat on every page, so they count against the cap too.
        let mut bytes = self.content_bytes;
        let page = self.page.page();
        let furniture = [
            (std::mem::take(&mut self.header), false),
            (std::mem::take(&mut self.footer), true),
        ];
        let mut total = itoa::Buffer::new();
        let total = total.format(self.contents.len());
        let mut layout = LayoutBuffer::default();
        for index in 0..self.contents.len() {
            let mut number = itoa::Buffer::new();
            let number = number.format(index + 1);
            let before = self.contents[index].len();
            for (slots, footer) in &furniture {
                let Some(item) = pick(slots, index == 0) else {
                    continue;
                };
                let commands = numbered(&item.commands, &|token, _| {
                    Ok(if token == crate::PAGE_NUMBER {
                        number
                    } else {
                        total
                    }
                    .to_owned())
                })?;
                // A shorter number can pull a taller run up a line, so a page can outgrow the measure.
                if self.lay_out_furniture(&commands, &mut layout)? > item.height {
                    return Err(RenderError::PageOverflow);
                }
                self.mark_used(&layout);
                let y = if *footer {
                    page.margin.bottom + item.height
                } else {
                    page.height - page.margin.top
                };
                let content = &mut self.contents[index];
                if self.metadata.tagged {
                    content.begin_marked_content(pdf_writer::Name(b"Artifact"));
                }
                PdfPainter::new(
                    content,
                    &mut self.links[index],
                    None,
                    &mut self.alphas[index],
                    &mut PaintState::default(),
                )
                .paint(
                    &layout,
                    0..layout.lines.len(),
                    Point {
                        x: page.margin.left,
                        y,
                    },
                );
                if self.metadata.tagged {
                    content.end_marked_content();
                }
            }
            bytes += self.contents[index].len() - before;
            if bytes > MAX_CONTENT_BYTES {
                return Err(RenderError::DocumentTooLarge);
            }
        }
        // What no line claimed, such as an empty last element, sits where the content ends.
        let end = (
            self.contents.len() - 1,
            self.page.page().margin.left.get(),
            self.cursor.origin(self.page).y.get(),
        );
        for anchor in &mut self.metadata.anchors {
            anchor.at.get_or_insert(end);
        }
        let metadata = &self.metadata;
        if (self.links.iter().flatten()).any(|link| {
            (link.uri.strip_prefix('#')).is_some_and(|name| metadata.find(name).is_err())
        }) {
            return Err(RenderError::UnknownDestination);
        }
        let (fonts, used) = self.fonts.into_parts();
        let pages = self
            .contents
            .into_iter()
            .zip(self.underlays)
            .map(|(content, mut page)| {
                // pdf-writer separates an operator from the one before it, not after.
                if !page.is_empty() {
                    page.push(b'\n');
                }
                page.extend_from_slice(&content.finish());
                page
            })
            .collect();
        Ok(finish_pdf(
            self.page.page(),
            pages,
            fonts,
            used,
            self.images,
            self.used_images,
            self.links,
            self.tags,
            self.alphas,
            self.metadata,
        ))
    }
}

/// The first-page block where there is one, else the every-page block.
fn pick(slots: &[Option<Furniture>; 2], first: bool) -> Option<&Furniture> {
    slots[1].as_ref().filter(|_| first).or(slots[0].as_ref())
}

/// Copies a furniture block with each page token spelled by `spell(token, run font)`.
fn numbered(
    commands: &[OwnedCommand],
    spell: &dyn Fn(char, FontId) -> Result<String, RenderError>,
) -> Result<Vec<OwnedCommand>, RenderError> {
    let mut copy = commands.to_vec();
    for command in &mut copy {
        let OwnedCommand::Paragraph(_, _, text, runs, _) = command else {
            continue;
        };
        let mut spelled = String::new();
        let mut rest = text.as_str();
        for run in runs {
            let (part, tail) = rest
                .split_at_checked(run.len)
                .ok_or(RenderError::InvalidLayout)?;
            rest = tail;
            let start = spelled.len();
            for character in part.chars() {
                match character {
                    crate::PAGE_NUMBER | crate::TOTAL_PAGES => {
                        spelled.push_str(&spell(character, run.font)?)
                    }
                    _ => spelled.push(character),
                }
            }
            run.len = spelled.len() - start;
        }
        *text = spelled;
    }
    Ok(copy)
}

/// Placeholder numbers use the widest digit, so a real number is never wider.
fn widest_digit(font: &Font) -> Result<char, RenderError> {
    let mut scratch = Vec::new();
    let mut widest = ('0', 0);
    for digit in '0'..='9' {
        let width = font.encode_into(digit, &mut scratch)?;
        if width > widest.1 {
            widest = (digit, width);
        }
    }
    Ok(widest.0)
}

#[cfg(test)]
mod tests;
