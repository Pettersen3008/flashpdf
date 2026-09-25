use pdf_writer::Content;

use crate::command::{CommandParser, OwnedCommand};
use crate::document::{Block, Document, Element, Table};
use crate::font::{EmbeddedFont, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout};
use crate::layout::{table_width, LayoutBuffer, LayoutEngine};
use crate::paint::PdfPainter;
use crate::pdf::finish_pdf;
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
    layout: LayoutBuffer,
    footer: Option<Footer>,
    footer_height: crate::Pt,
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
            layout: LayoutBuffer::default(),
            footer: None,
            footer_height: crate::Pt::ZERO,
        }
    }

    pub(crate) fn font_count(&self) -> usize {
        self.fonts.len()
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
        self.page.validate_block(height)?;
        self.fonts.mark_used(style.font, &[]);
        self.footer_height = height;
        self.footer = Some(Footer {
            template,
            style,
            baseline: height - ascent,
        });
        Ok(())
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
        let height = LayoutEngine::new(&self.fonts)
            .layout(
                element,
                LayoutArea::root(self.page.content_width()),
                &mut self.layout,
            )
            .map_err(RenderError::from)?;
        self.fonts.mark_layout(&self.layout);
        let lines = self.layout.lines.len();
        if !splittable(element) {
            self.page.validate_block(height + self.footer_height)?;
            if !self.cursor.fits(height + self.footer_height, self.page) {
                self.start_page();
            }
            PdfPainter::new(self.contents.last_mut().unwrap()).paint(
                &self.layout,
                0..lines,
                self.cursor.origin(self.page),
            );
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
            while self
                .layout
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
            PdfPainter::new(self.contents.last_mut().unwrap()).paint(
                &self.layout,
                start..index,
                self.cursor.origin(self.page).translated(Pt::ZERO, offset),
            );
            if index == lines {
                let rest = (height - offset).get().min(available.get());
                self.cursor.advance(Pt(rest.max(0.0)));
                return Ok(());
            }
            offset = self.layout.lines[index].top;
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
            header_height += LayoutEngine::new(&self.fonts).layout(
                row,
                area.translated(Pt::ZERO, header_height),
                &mut header,
            )?;
        }
        self.fonts.mark_layout(&header);
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
            let height = LayoutEngine::new(&self.fonts).layout(row, area, &mut self.layout)?;
            self.fonts.mark_layout(&self.layout);
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
            let Self {
                contents,
                cursor,
                layout,
                page,
                ..
            } = self;
            paint_block(contents, cursor, *page, layout, height);
        }
        Ok(())
    }

    fn paint_block(&mut self, layout: &LayoutBuffer, height: Pt) {
        paint_block(
            &mut self.contents,
            &mut self.cursor,
            self.page,
            layout,
            height,
        );
    }

    fn start_page(&mut self) {
        self.content_bytes += self.contents.last().unwrap().len();
        self.contents.push(Content::new());
        self.cursor.reset(self.page);
    }

    pub fn finish(mut self) -> Vec<u8> {
        if let Some(footer) = self.footer.take() {
            let total = self.contents.len() as i32;
            for (index, content) in self.contents.iter_mut().enumerate() {
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
                PdfPainter::new(content).paint_line(
                    &text,
                    footer.style,
                    crate::geometry::Point {
                        x: self.page.margin(),
                        y: self.page.margin() + footer.baseline,
                    },
                    self.page.content_width(),
                    text_width,
                );
            }
        }
        let (fonts, used) = self.fonts.into_parts();
        finish_pdf(self.page.page(), self.contents, fonts, used)
    }
}

fn paint_block(
    contents: &mut [Content],
    cursor: &mut PageCursor,
    page: PageLayout,
    layout: &LayoutBuffer,
    height: Pt,
) {
    PdfPainter::new(contents.last_mut().unwrap()).paint(
        layout,
        0..layout.lines.len(),
        cursor.origin(page),
    );
    cursor.advance(height);
}

/// Text and unpainted boxes split across pages. Stack stays atomic: the
/// protocol has no break-inside flag, so the compiler emits Stack for `avoid`.
fn splittable(element: &Element<'_>) -> bool {
    match element {
        Element::Paragraph(_) => true,
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
