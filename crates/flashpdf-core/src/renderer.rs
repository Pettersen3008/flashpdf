use pdf_writer::Content;

use crate::command::{CommandParser, OwnedCommand};
use crate::document::{Block, Document, Element, Table};
use crate::font::{EmbeddedFont, Font, FontBook};
use crate::geometry::{LayoutArea, PageCursor, PageLayout, Point};
use crate::image::Image;
use crate::layout::{table_width, LayoutBuffer, LayoutEngine};
use crate::paint::{PageLink, PageTag, PdfPainter};
use crate::pdf::{finish_pdf, Metadata};
use crate::{Command, FontId, Page, Pt, RenderError};

/// Total content-stream bytes a document may paint before `finish`.
pub const MAX_CONTENT_BYTES: usize = 16 * 1024 * 1024;

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
    /// Index 0 paints on every page; index 1 replaces it on the first page.
    header: [Option<Furniture>; 2],
    footer: [Option<Furniture>; 2],
    metadata: Metadata,
}

/// A header or footer block, laid out again on every page in `finish` with that page's numbers.
struct Furniture {
    commands: Vec<OwnedCommand>,
    /// Measured with ten-digit page numbers and reserved on every page it paints on.
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
            contents: vec![Content::new()],
            content_bytes: 0,
            cursor: PageCursor::new(page),
            fonts: FontBook::new(embedded),
            images: Vec::new(),
            used_images: Vec::new(),
            links: vec![Vec::new()],
            tags: vec![Vec::new()],
            layout: LayoutBuffer::default(),
            header: Default::default(),
            footer: Default::default(),
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

    /// Reserves the block's height, measured with ten-digit page numbers, on
    /// every page it paints on. Index 1 (`first`) replaces index 0 on the first page.
    pub(crate) fn set_furniture(
        &mut self,
        footer: bool,
        first: bool,
        commands: Vec<OwnedCommand>,
    ) -> Result<(), RenderError> {
        let placeholder = numbered(&commands, &|_, font| {
            let font = self.fonts.get(font).ok_or(RenderError::InvalidLayout)?;
            Ok(widest_digit(font)?.to_string().repeat(10))
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
            self.validate_block(height)?;
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
        self.validate_block(reserved)?;
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
            if self.validate_block(height + reserved).is_err() {
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
        self.page = self.later;
        self.cursor.reset(self.page);
    }

    pub fn finish(mut self) -> Result<Vec<u8>, RenderError> {
        // The header and footer repeat on every page, so they count against the cap too.
        let mut bytes = self.content_bytes + self.contents.last().unwrap().len();
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
                PdfPainter::new(content, &mut self.links[index], None).paint(
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
        let (fonts, used) = self.fonts.into_parts();
        Ok(finish_pdf(
            page,
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
        let OwnedCommand::Paragraph(_, text, runs, _) = command else {
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
