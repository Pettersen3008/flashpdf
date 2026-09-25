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
    Ok(renderer.finish())
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
    footer: Option<Footer>,
    footer_height: crate::Pt,
    header: Option<Footer>,
    header_height: crate::Pt,
    metadata: Metadata,
}

struct Footer {
    template: String,
    style: TextStyle,
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
            footer: None,
            footer_height: crate::Pt::ZERO,
            header: None,
            header_height: crate::Pt::ZERO,
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
        let (_, text_width) = footer_line(&template, &placeholder, &placeholder, font, style)?;
        if text_width > self.page.content_width() {
            return Err(RenderError::TextTooWide);
        }
        let (ascent, descent, gap) = font.metrics();
        let scale = style.size.get() / 1000.0;
        let ascent = Pt(ascent * scale);
        let height = Pt((ascent.get() - descent * scale + gap * scale).max(0.0));
        self.page.validate_block(height + self.header_height)?;
        self.fonts.mark_used(style.font, &[]);
        self.footer_height = height;
        self.footer = Some(Footer {
            template,
            style,
            baseline: height - ascent,
        });
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
        let (_, text_width) = footer_line(&template, &placeholder, &placeholder, font, style)?;
        if text_width > self.page.content_width() {
            return Err(RenderError::TextTooWide);
        }
        let (ascent, descent, gap) = font.metrics();
        let scale = style.size.get() / 1000.0;
        let height = Pt(((ascent - descent + gap) * scale).max(0.0));
        self.page.validate_block(height + self.footer_height)?;
        self.fonts.mark_used(style.font, &[]);
        self.header_height = height;
        self.cursor.advance(height);
        self.header = Some(Footer {
            template,
            style,
            baseline: Pt(ascent * scale),
        });
        Ok(())
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
            self.page.validate_block(height + self.footer_height)?;
            if !self.cursor.fits(height + self.footer_height, self.page) {
                self.start_page();
            }
            let origin = self.cursor.origin(self.page);
            self.painter().paint(layout, 0..lines, origin);
            self.cursor.advance(height);
            return Ok(());
        }
        // Stream positioned lines page by page; `offset` is the block y where
        // the current page starts, so gaps straddling a break collapse.
        let mut offset = Pt::ZERO;
        let mut index = 0;
        loop {
            let available = self.cursor.remaining(self.page) - self.footer_height;
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

    /// Rows place one at a time; the header lays out once into its own buffer
    /// and repaints at the top of every page a body row opens.
    fn render_table(&mut self, table: &Table<'_>) -> Result<(), RenderError> {
        let area = LayoutArea::root(table_width(table.width, self.page.content_width())?);
        let mut header = LayoutBuffer::default();
        let mut header_height = Pt::ZERO;
        for row in &table.rows[..table.header_rows] {
            header_height += LayoutEngine::new(&self.fonts, &self.images).layout(
                row,
                area.translated(Pt::ZERO, header_height),
                &mut header,
            )?;
        }
        self.mark_used(&header);
        let reserved = header_height + self.footer_height;
        self.page.validate_block(reserved)?;
        let body = &table.rows[table.header_rows..];
        if body.is_empty() {
            if !self.cursor.fits(reserved, self.page) {
                self.start_page();
            }
            self.paint_block(&header, header_height);
            return Ok(());
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
                    self.footer_height
                } else {
                    reserved
                };
            if !self.cursor.fits(needed, self.page) {
                self.start_page();
                header_on_page = false;
            }
            if !header_on_page {
                self.paint_block(&header, header_height);
                header_on_page = true;
            }
            self.paint_block(&layout, height);
            self.layout = layout;
        }
        Ok(())
    }

    fn paint_block(&mut self, layout: &LayoutBuffer, height: Pt) {
        let origin = self.cursor.origin(self.page);
        self.painter().paint(layout, 0..layout.lines.len(), origin);
        self.cursor.advance(height);
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
        self.cursor.advance(self.header_height);
    }

    pub fn finish(mut self) -> Vec<u8> {
        if let Some(header) = self.header.take() {
            let total = self.contents.len() as i32;
            for (index, (content, links)) in
                self.contents.iter_mut().zip(&mut self.links).enumerate()
            {
                let mut page_buffer = itoa::Buffer::new();
                let mut total_buffer = itoa::Buffer::new();
                let font = self.fonts.get(header.style.font).unwrap();
                let (text, width) = footer_line(
                    &header.template,
                    page_buffer.format(index as i32 + 1),
                    total_buffer.format(total),
                    font,
                    header.style,
                )
                .unwrap();
                self.fonts.mark_used(header.style.font, &text);
                if self.metadata.tagged {
                    content.begin_marked_content(pdf_writer::Name(b"Artifact"));
                }
                PdfPainter::new(content, links, None).paint_line(
                    &text,
                    header.style,
                    crate::geometry::Point {
                        x: self.page.margin(),
                        y: self.page.top() - header.baseline,
                    },
                    self.page.content_width(),
                    width,
                );
                if self.metadata.tagged {
                    content.end_marked_content();
                }
            }
        }
        if let Some(footer) = self.footer.take() {
            let total = self.contents.len() as i32;
            for (index, (content, links)) in
                self.contents.iter_mut().zip(&mut self.links).enumerate()
            {
                let mut page_buffer = itoa::Buffer::new();
                let mut total_buffer = itoa::Buffer::new();
                let page_number = page_buffer.format(index as i32 + 1);
                let total_pages = total_buffer.format(total);
                let font = self.fonts.get(footer.style.font).unwrap();
                let (text, text_width) = footer_line(
                    &footer.template,
                    page_number,
                    total_pages,
                    font,
                    footer.style,
                )
                .unwrap();
                self.fonts.mark_used(footer.style.font, &text);
                if self.metadata.tagged {
                    content.begin_marked_content(pdf_writer::Name(b"Artifact"));
                }
                PdfPainter::new(content, links, None).paint_line(
                    &text,
                    footer.style,
                    crate::geometry::Point {
                        x: self.page.margin(),
                        y: self.page.margin() + footer.baseline,
                    },
                    self.page.content_width(),
                    text_width,
                );
                if self.metadata.tagged {
                    content.end_marked_content();
                }
            }
        }
        let (fonts, used) = self.fonts.into_parts();
        finish_pdf(
            self.page.page(),
            self.contents,
            fonts,
            used,
            self.images,
            self.used_images,
            self.links,
            self.tags,
            self.metadata,
        )
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
