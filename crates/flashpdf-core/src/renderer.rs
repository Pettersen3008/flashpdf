use pdf_writer::Content;

use crate::command::{CommandParser, OwnedCommand};
use crate::document::{Block, Document, Element, Table};
use crate::font::{EmbeddedFont, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout};
use crate::image::Image;
use crate::layout::{table_width, LayoutBuffer, LayoutEngine};
use crate::paint::{PageLink, PageTag, PdfPainter};
use crate::pdf::{finish_pdf, Metadata};
use crate::{Command, Page, Pt, RenderError, TextStyle};

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
        self.layout.clear();
        let height = LayoutEngine::new(&self.fonts, &self.images)
            .layout(
                element,
                LayoutArea::root(self.page.content_width()),
                &mut self.layout,
            )
            .map_err(RenderError::from)?;
        let layout = std::mem::take(&mut self.layout);
        self.mark_used(&layout);
        let result = self.place(element, &layout, height);
        self.layout = layout;
        result
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
        let area = LayoutArea::root(table_width(table.width, self.page.content_width())?);
        let split = table.rows.len() - table.footer_rows;
        let (header, header_height) = self.layout_rows(&table.rows[..table.header_rows], area)?;
        let (footer, footer_height) = self.layout_rows(&table.rows[split..], area)?;
        let reserved = header_height + footer_height;
        self.page.validate_block(reserved)?;
        let body = &table.rows[table.header_rows..split];
        if body.is_empty() {
            if !self.cursor.fits(reserved, self.page) {
                self.start_page();
            }
            self.paint_block(&header, header_height)?;
            return self.paint_block(&footer, footer_height);
        }
        let mut header_on_page = false;
        for (index, row) in body.iter().enumerate() {
            self.layout.clear();
            let height =
                LayoutEngine::new(&self.fonts, &self.images).layout(row, area, &mut self.layout)?;
            let layout = std::mem::take(&mut self.layout);
            self.mark_used(&layout);
            if self.page.validate_block(height + reserved).is_err() {
                return Err(RenderError::TableRowOverflow(table.header_rows + index));
            }
            let needed = height
                + if header_on_page {
                    footer_height
                } else {
                    reserved
                };
            if !self.cursor.fits(needed, self.page) {
                if header_on_page {
                    self.paint_block(&footer, footer_height)?;
                }
                self.start_page();
                header_on_page = false;
            }
            if !header_on_page {
                self.paint_block(&header, header_height)?;
                header_on_page = true;
            }
            let painted = self.paint_block(&layout, height);
            self.layout = layout;
            painted?;
        }
        self.paint_block(&footer, footer_height)
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
        )
    }

    fn start_page(&mut self) {
        self.content_bytes += self.contents.last().unwrap().len();
        self.contents.push(Content::new());
        self.links.push(Vec::new());
        self.tags.push(Vec::new());
        self.cursor.reset(self.page);
    }

    pub fn finish(mut self) -> Result<Vec<u8>, RenderError> {
        // The header and footer repeat on every page, so they count against the cap too.
        let mut bytes = self.content_bytes + self.contents.last().unwrap().len();
        let total = self.contents.len() as i32;
        for (index, (content, links)) in self.contents.iter_mut().zip(&mut self.links).enumerate() {
            let before = content.len();
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
                PdfPainter::new(content, links, None).paint_line(
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
        Ok(finish_pdf(
            self.page.page(),
            self.contents,
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
