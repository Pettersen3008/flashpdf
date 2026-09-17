use pdf_writer::Content;

use crate::command::{CommandParser, OwnedCommand};
use crate::document::{Block, Document, Element};
use crate::font::{EmbeddedFont, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout};
use crate::layout::{LayoutBuffer, LayoutEngine};
use crate::paint::PdfPainter;
use crate::pdf::finish_pdf;
use crate::{Command, Page, RenderError};

pub fn render(page: Page, commands: &[Command<'_>]) -> Result<Vec<u8>, RenderError> {
    let mut renderer = Renderer::new(page);
    renderer.push(commands)?;
    Ok(renderer.finish())
}

pub struct Renderer {
    page: PageLayout,
    contents: Vec<Content>,
    cursor: PageCursor,
    fonts: FontBook,
    layout: LayoutBuffer,
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
            cursor: PageCursor::new(page),
            fonts: FontBook::new(embedded),
            layout: LayoutBuffer::default(),
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
        Ok(())
    }

    pub(crate) fn page_break(&mut self) {
        self.start_page();
    }

    fn render_element(&mut self, element: &Element<'_>) -> Result<(), RenderError> {
        self.layout.clear();
        let height = LayoutEngine::new(&self.fonts)
            .layout(
                element,
                LayoutArea::root(self.page.content_width()),
                &mut self.layout,
            )
            .map_err(RenderError::from)?;
        self.page.validate_block(height)?;
        if !self.cursor.fits(height, self.page) {
            self.start_page();
        }
        for font in self.layout.used_fonts() {
            self.fonts.mark_used(font);
        }
        PdfPainter::new(self.contents.last_mut().unwrap())
            .paint(&self.layout, self.cursor.origin(self.page));
        self.cursor.advance(height);
        Ok(())
    }

    fn start_page(&mut self) {
        self.contents.push(Content::new());
        self.cursor.reset(self.page);
    }

    pub fn finish(self) -> Vec<u8> {
        let (fonts, used) = self.fonts.into_parts();
        finish_pdf(self.page.page(), self.contents, fonts, used)
    }
}
