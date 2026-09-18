use pdf_writer::Content;

use crate::command::{CommandParser, OwnedCommand};
use crate::document::{Block, Document, Element};
use crate::font::{EmbeddedFont, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout};
use crate::layout::{LayoutBuffer, LayoutEngine};
use crate::paint::PdfPainter;
use crate::pdf::finish_pdf;
use crate::{Command, Page, Pt, RenderError, TextStyle};

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
        let mut digit = b'0';
        let mut width = 0;
        for candidate in b'0'..b':' {
            let candidate_width = font.width(candidate)?;
            if candidate_width > width {
                digit = candidate;
                width = candidate_width;
            }
        }
        let placeholder = [digit; 10];
        // SAFETY: every byte is an ASCII digit selected above.
        let placeholder = unsafe { std::str::from_utf8_unchecked(&placeholder) };
        let (_, text_width) = footer_line(&template, placeholder, placeholder, font, style)?;
        if text_width > self.page.content_width() {
            return Err(RenderError::TextTooWide);
        }
        let (ascent, descent) = font.metrics();
        let ascent = Pt(ascent * style.size.get() / 1000.0);
        let height = Pt((ascent.get() - descent * style.size.get() / 1000.0).max(0.0));
        self.page.validate_block(height)?;
        self.fonts.mark_used(style.font);
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
        self.page.validate_block(height + self.footer_height)?;
        if !self.cursor.fits(height + self.footer_height, self.page) {
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
            crate::PAGE_NUMBER => Some(page_number),
            crate::TOTAL_PAGES => Some(total_pages),
            _ => None,
        };
        if let Some(token) = token {
            for byte in token.bytes() {
                width += u32::from(font.width(byte)?);
                bytes.push(byte);
            }
        } else {
            let byte = crate::win_ansi::encode(character)?;
            width += u32::from(font.width(byte)?);
            bytes.push(byte);
        }
    }
    Ok((bytes, Pt(width as f32 * style.size.get() / 1000.0)))
}
