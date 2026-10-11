use pdf_writer::Content;

use crate::command::{uniform_rows, CommandParser, OwnedCommand};
use crate::document::{Block, BoxNode, Document, Element, Table};
use crate::font::{EmbeddedFont, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout, Point};
use crate::image::Image;
use crate::layout::{painted, table_width, LayoutBuffer, LayoutEngine, PositionedBox};
use crate::paint::{PageLink, PageTag, PaintState, PdfPainter};
use crate::pdf::{finish_pdf, Metadata};
use crate::{BoxStyle, Command, Page, Pt, RenderError, TextStyle};

/// Total content-stream bytes a document may paint before `finish`.
pub const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;

pub fn render(page: Page, commands: &[Command<'_>]) -> Result<Vec<u8>, RenderError> {
    let mut renderer = Renderer::new(page);
    renderer.push(commands)?;
    renderer.finish()
}

pub struct Renderer {
    page: PageLayout,
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
    layout: LayoutBuffer,
    header: Option<Furniture>,
    footer: Option<Furniture>,
    metadata: Metadata,
    table: Option<OpenTable>,
    /// The open page's fill colour and font.
    state: PaintState,
    /// Boxes too tall to place whole, outermost first, while their children stream:
    /// each one's fragment on this page, as an index into `fragments`, and the
    /// column to restore when it closes.
    boxes: Vec<(usize, LayoutArea)>,
    /// Where children lay out: the page body, or the innermost open box's content area.
    column: LayoutArea,
}

/// A top-level table between its start and end: the header and footer rows lay
/// out once, and each body row lays out and places as it arrives.
struct OpenTable {
    area: LayoutArea,
    header: LayoutBuffer,
    header_height: Pt,
    footer: LayoutBuffer,
    footer_height: Pt,
    columns: Option<usize>,
    /// The next row's index among the table's children, which `TableRowOverflow` reports.
    index: usize,
    header_on_page: bool,
}

/// A header or footer line, painted on every page in `finish`.
struct Furniture {
    template: String,
    style: TextStyle,
    /// Baseline y from the page bottom.
    baseline: Pt,
}

impl Renderer {
    pub fn new(page: Page) -> Self {
        Self::with_fonts(page, Vec::new())
    }

    pub(crate) fn with_fonts(page: Page, embedded: Vec<EmbeddedFont>) -> Self {
        let page = PageLayout::new(page);
        Self {
            page,
            contents: vec![Content::new()],
            underlays: Vec::new(),
            fragments: Vec::new(),
            content_bytes: 0,
            cursor: PageCursor::new(page),
            fonts: FontBook::new(embedded),
            images: Vec::new(),
            used_images: Vec::new(),
            links: vec![Vec::new()],
            tags: vec![Vec::new()],
            layout: LayoutBuffer::default(),
            header: None,
            footer: None,
            metadata: Metadata::default(),
            table: None,
            state: PaintState::default(),
            boxes: Vec::new(),
            column: LayoutArea::root(page.content_width()),
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

    pub(crate) fn set_footer(
        &mut self,
        template: String,
        style: TextStyle,
    ) -> Result<(), RenderError> {
        if self.footer.is_some() {
            return Err(RenderError::InvalidLayout);
        }
        let (ascent, height) = self.measure_furniture(&template, style)?;
        self.footer = Some(Furniture {
            template,
            style,
            baseline: self.page.bottom() + (height - ascent),
        });
        self.page.footer = height;
        Ok(())
    }

    pub(crate) fn set_header(
        &mut self,
        template: String,
        style: TextStyle,
    ) -> Result<(), RenderError> {
        if self.header.is_some() {
            return Err(RenderError::InvalidLayout);
        }
        let (ascent, height) = self.measure_furniture(&template, style)?;
        self.header = Some(Furniture {
            template,
            style,
            baseline: self.page.top() - ascent,
        });
        self.page.header = height;
        self.cursor.reset(self.page);
        Ok(())
    }

    /// Checks that the line fits the page with ten-digit page numbers and
    /// returns its ascent and height.
    fn measure_furniture(
        &mut self,
        template: &str,
        style: TextStyle,
    ) -> Result<(Pt, Pt), RenderError> {
        let font = self
            .fonts
            .get(style.font)
            .ok_or(RenderError::InvalidLayout)?;
        let mut digit = '0';
        let mut width = 0;
        let mut scratch = Vec::new();
        for candidate in '0'..='9' {
            let candidate_width = font.encode_into(candidate, &mut scratch)?;
            if candidate_width > width {
                digit = candidate;
                width = candidate_width;
            }
        }
        let placeholder = digit.to_string().repeat(10);
        let (_, text_width) = footer_line(template, &placeholder, &placeholder, font, style)?;
        if text_width > self.page.content_width() {
            return Err(RenderError::TextTooWide);
        }
        let (ascent, descent, gap) = font.metrics();
        let scale = style.size.get() / 1000.0;
        let ascent = ascent * scale;
        let height = Pt((ascent - descent * scale + gap * scale).max(0.0));
        self.page.validate_block(height)?;
        self.fonts.mark_used(style.font, &[]);
        Ok((Pt(ascent), height))
    }

    pub(crate) fn set_metadata(&mut self, metadata: Metadata) {
        self.metadata = metadata;
    }

    fn push_document(&mut self, document: &Document<'_>) -> Result<(), RenderError> {
        for block in &document.blocks {
            self.push_block(block)?;
        }
        Ok(())
    }

    fn push_block(&mut self, block: &Block<'_>) -> Result<(), RenderError> {
        match block {
            Block::PageBreak => self.start_page(),
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

    pub(crate) fn page_break(&mut self) {
        self.start_page();
    }

    fn render_element(&mut self, element: &Element<'_>) -> Result<(), RenderError> {
        if let Element::Table(table) = element {
            return self.render_table(table);
        }
        let height = self.layout_element(element)?;
        if let Element::Box(node) = element {
            if self.streams(element, height) {
                return self.stream_box(node);
            }
        }
        let layout = std::mem::take(&mut self.layout);
        self.mark_used(&layout);
        let result = self.place(element, &layout, height);
        self.layout = layout;
        result
    }

    fn layout_element(&mut self, element: &Element<'_>) -> Result<Pt, RenderError> {
        self.layout.clear();
        Ok(LayoutEngine::new(&self.fonts, &self.images).layout(
            element,
            self.column,
            &mut self.layout,
        )?)
    }

    /// A box that fits a page still places whole; an atomic one taller than a page streams.
    fn streams(&self, element: &Element<'_>, height: Pt) -> bool {
        matches!(element, Element::Box(_))
            && !splittable(element)
            && self.page.validate_block(height).is_err()
    }

    /// Whether a box the decoder still holds, closed early, can no longer place whole.
    pub(crate) fn overflows(&mut self, commands: &[OwnedCommand]) -> bool {
        let Ok(Block::Element(element)) = CommandParser::new(commands).parse_single_block() else {
            return false;
        };
        self.layout_element(&element)
            .is_ok_and(|height| self.streams(&element, height))
    }

    fn stream_box(&mut self, node: &BoxNode<'_>) -> Result<(), RenderError> {
        self.open_box(node.style)?;
        for child in &node.children {
            self.render_element(child)?;
            self.check_size()?;
        }
        self.close_box()
    }

    /// Lays a split box's top margin, border and padding at the cursor; its
    /// children then place in its content area until `close_box`. The caller has
    /// laid the box out, which checked its widths.
    pub(crate) fn open_box(&mut self, style: BoxStyle) -> Result<(), RenderError> {
        let column = self.column;
        let top = style.margin.top + style.border.top + style.padding.top;
        self.page
            .validate_block(top + style.padding.bottom + style.border.bottom)?;
        if !self.cursor.fits(top, self.page) {
            self.start_page();
        }
        self.cursor.advance(style.margin.top);
        self.boxes.push((self.fragments.len(), column));
        let width = column.width - style.margin.left - style.margin.right;
        self.fragments.push(PositionedBox {
            origin: Point {
                x: column.origin.x + style.margin.left,
                y: Pt::ZERO - self.cursor.origin(self.page).y,
            },
            width,
            height: Pt::ZERO,
            style,
        });
        let inset = style.border.left + style.padding.left;
        self.column = column
            .translated(style.margin.left + inset, Pt::ZERO)
            .with_width(width - inset - style.padding.right - style.border.right);
        self.cursor.advance(style.border.top + style.padding.top);
        Ok(())
    }

    /// Lays the bottom padding, border and margin; a split box's last fragment
    /// is the only one with a bottom border, like CSS `box-decoration-break: slice`.
    pub(crate) fn close_box(&mut self) -> Result<(), RenderError> {
        let style = self.fragments[self.boxes.last().ok_or(RenderError::InvalidLayout)?.0].style;
        let bottom = style.padding.bottom + style.border.bottom;
        if !self.cursor.fits(bottom, self.page) {
            self.start_page();
        }
        self.cursor.advance(bottom);
        let (slot, column) = self.boxes.pop().unwrap();
        self.column = column;
        let fragment = &mut self.fragments[slot];
        fragment.height = Pt::ZERO - fragment.origin.y - self.cursor.origin(self.page).y;
        // Whatever places next checks that it fits, so a margin past the page end is harmless.
        self.cursor.advance(style.margin.bottom);
        self.check_size()
    }

    /// Paints the open page's fragments for `finish` to put before its content.
    /// Open boxes end theirs at the body bottom without a bottom border and
    /// continue on the next page without a top border.
    fn end_page(&mut self) {
        let mut next = Vec::with_capacity(self.boxes.len());
        for (slot, _) in &mut self.boxes {
            let fragment = &mut self.fragments[*slot];
            let mut continued = *fragment;
            continued.origin.y = Pt::ZERO - self.page.top();
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
                &mut PaintState::default(),
            )
            .paint_boxes(
                &fragments,
                Point {
                    x: self.page.margin(),
                    y: Pt::ZERO,
                },
            );
            underlay = content.finish().into_vec();
        }
        self.content_bytes += self.contents.last().unwrap().len() + underlay.len();
        self.underlays.push(underlay);
    }

    fn place(
        &mut self,
        element: &Element<'_>,
        layout: &LayoutBuffer,
        height: Pt,
    ) -> Result<(), RenderError> {
        let lines = layout.lines.len();
        if !splittable(element) {
            self.page.validate_block(height)?;
            if !self.cursor.fits(height, self.page) {
                self.start_page();
            }
            return self.paint_block(layout, height);
        }
        // Stream positioned lines page by page; `offset` is the block y where
        // the current page starts, so gaps straddling a break collapse.
        let mut offset = Pt::ZERO;
        let mut index = 0;
        loop {
            let available = self.cursor.remaining(self.page);
            let start = index;
            while layout
                .lines
                .get(index)
                .is_some_and(|line| line.bottom - offset <= available)
            {
                index += 1;
            }
            if index == start && index < lines {
                if self.cursor.at_top(self.page) {
                    return Err(RenderError::PageOverflow);
                }
                self.start_page();
                continue;
            }
            let origin = self.cursor.origin(self.page).translated(Pt::ZERO, offset);
            self.painter().paint(layout, start..index, origin);
            if index == lines {
                let rest = (height - offset).get().min(available.get());
                self.cursor.advance(Pt(rest.max(0.0)));
                return Ok(());
            }
            offset = layout.lines[index].top;
            self.start_page();
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
        self.page.validate_block(header_height + footer_height)?;
        let mut columns = None;
        for row in &table.rows[..head] {
            uniform_rows(row, &mut columns);
        }
        self.table = Some(OpenTable {
            area,
            header,
            header_height,
            footer,
            footer_height,
            columns,
            index: head,
            header_on_page: false,
        });
        Ok(())
    }

    fn table_row(&mut self, row: &Element<'_>) -> Result<(), RenderError> {
        let mut table = self.table.take().ok_or(RenderError::InvalidLayout)?;
        let result = self.place_row(&mut table, row);
        self.table = Some(table);
        result
    }

    fn place_row(&mut self, table: &mut OpenTable, row: &Element<'_>) -> Result<(), RenderError> {
        if !uniform_rows(row, &mut table.columns) {
            return Err(RenderError::InvalidLayout);
        }
        self.layout.clear();
        let height = LayoutEngine::new(&self.fonts, &self.images).layout(
            row,
            table.area,
            &mut self.layout,
        )?;
        let layout = std::mem::take(&mut self.layout);
        self.mark_used(&layout);
        let reserved = table.header_height + table.footer_height;
        if self.page.validate_block(height + reserved).is_err() {
            return Err(RenderError::TableRowOverflow(table.index));
        }
        table.index += 1;
        let needed = height
            + if table.header_on_page {
                table.footer_height
            } else {
                reserved
            };
        if !self.cursor.fits(needed, self.page) {
            if table.header_on_page {
                self.paint_block(&table.footer, table.footer_height)?;
            }
            self.start_page();
            table.header_on_page = false;
        }
        if !table.header_on_page {
            self.paint_block(&table.header, table.header_height)?;
            table.header_on_page = true;
        }
        let painted = self.paint_block(&layout, height);
        self.layout = layout;
        painted
    }

    /// Paints the last footer; a table without body rows paints its header above it.
    pub(crate) fn end_table(&mut self) -> Result<(), RenderError> {
        let table = self.table.take().ok_or(RenderError::InvalidLayout)?;
        if !table.header_on_page {
            if !self
                .cursor
                .fits(table.header_height + table.footer_height, self.page)
            {
                self.start_page();
            }
            self.paint_block(&table.header, table.header_height)?;
        }
        self.paint_block(&table.footer, table.footer_height)
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
    fn paint_block(&mut self, layout: &LayoutBuffer, height: Pt) -> Result<(), RenderError> {
        let origin = self.cursor.origin(self.page);
        self.painter().paint(layout, 0..layout.lines.len(), origin);
        self.cursor.advance(height);
        self.check_size()
    }

    fn painter(&mut self) -> PdfPainter<'_> {
        PdfPainter::new(
            self.contents.last_mut().unwrap(),
            self.links.last_mut().unwrap(),
            self.metadata.tagged.then(|| self.tags.last_mut().unwrap()),
            &mut self.state,
        )
    }

    fn start_page(&mut self) {
        self.end_page();
        self.contents.push(Content::new());
        self.links.push(Vec::new());
        self.tags.push(Vec::new());
        self.state = PaintState::default();
        self.cursor.reset(self.page);
    }

    pub fn finish(mut self) -> Result<Vec<u8>, RenderError> {
        self.end_page();
        // The header and footer repeat on every page, so they count against the cap too.
        let mut bytes = self.content_bytes;
        let total = self.contents.len() as i32;
        for (index, (content, links)) in self.contents.iter_mut().zip(&mut self.links).enumerate() {
            let before = content.len();
            let mut state = PaintState::default();
            for furniture in self.header.iter().chain(&self.footer) {
                let mut page_buffer = itoa::Buffer::new();
                let mut total_buffer = itoa::Buffer::new();
                let font = self.fonts.get(furniture.style.font).unwrap();
                let (text, width) = footer_line(
                    &furniture.template,
                    page_buffer.format(index as i32 + 1),
                    total_buffer.format(total),
                    font,
                    furniture.style,
                )
                .unwrap();
                self.fonts.mark_used(furniture.style.font, &text);
                if self.metadata.tagged {
                    content.begin_marked_content(pdf_writer::Name(b"Artifact"));
                }
                PdfPainter::new(content, links, None, &mut state).paint_line(
                    &text,
                    furniture.style,
                    crate::geometry::Point {
                        x: self.page.margin(),
                        y: furniture.baseline,
                    },
                    self.page.content_width(),
                    width,
                );
                if self.metadata.tagged {
                    content.end_marked_content();
                }
            }
            bytes += content.len() - before;
            if bytes > MAX_CONTENT_BYTES {
                return Err(RenderError::DocumentTooLarge);
            }
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
            self.metadata,
        ))
    }
}

/// Text, images, and unpainted boxes split across pages. Stack stays atomic: the
/// protocol has no break-inside flag, so the compiler emits Stack for `avoid`.
fn splittable(element: &Element<'_>) -> bool {
    match element {
        Element::Paragraph(_) | Element::Image(_) => true,
        Element::Box(node) => {
            node.style.background.is_none()
                && node
                    .style
                    .padding
                    .iter()
                    .chain(node.style.border.iter())
                    .all(|edge| *edge == Pt::ZERO)
                && node
                    .children
                    .iter()
                    .all(|child| matches!(child, Element::Spacer(_)) || splittable(child))
        }
        Element::Spacer(_) | Element::Stack(_) | Element::Row(_) | Element::Table(_) => false,
    }
}

fn footer_line(
    text: &str,
    page_number: &str,
    total_pages: &str,
    font: &crate::font::Font,
    style: TextStyle,
) -> Result<(Vec<u8>, Pt), RenderError> {
    let mut bytes = Vec::new();
    let mut width = 0_u32;
    for character in text.chars() {
        let token = match character {
            crate::PAGE_NUMBER => page_number,
            crate::TOTAL_PAGES => total_pages,
            _ => {
                width += u32::from(font.encode_into(character, &mut bytes)?);
                continue;
            }
        };
        for digit in token.chars() {
            width += u32::from(font.encode_into(digit, &mut bytes)?);
        }
    }
    Ok((bytes, Pt(width as f32 * style.size.get() / 1000.0)))
}
