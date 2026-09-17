use pdf_writer::{Content, Name, Str};
use std::ops::Range;

use crate::font::{encode_win_ansi, font_name, Font};
use crate::pdf::{finish_pdf, start_page};
use crate::{
    BoxStyle, ColumnWidth, Command, EmbeddedFont, Page, Pt, RenderError, TextAlign, HELVETICA,
};

pub fn render(page: Page, commands: &[Command<'_>]) -> Result<Vec<u8>, RenderError> {
    let mut renderer = Renderer::new(page);
    renderer.push(commands)?;
    Ok(renderer.finish())
}

pub struct Renderer {
    page: Page,
    contents: Vec<Content>,
    cursor: f32,
    scratch: Scratch,
    fonts: Vec<Font>,
    used_fonts: Vec<bool>,
}

impl Renderer {
    pub fn new(page: Page) -> Self {
        Self::with_fonts(page, Vec::new())
    }

    pub(crate) fn with_fonts(page: Page, embedded: Vec<EmbeddedFont>) -> Self {
        let mut fonts = vec![
            Font::Helvetica { bold: false },
            Font::Helvetica { bold: true },
        ];
        fonts.extend(
            embedded
                .into_iter()
                .map(|font| Font::Embedded(Box::new(font))),
        );
        let used_fonts = vec![false; fonts.len()];
        Self {
            page,
            contents: vec![Content::new()],
            cursor: page.height.0 - page.margin.0,
            scratch: Scratch::default(),
            fonts,
            used_fonts,
        }
    }

    pub(crate) fn font_count(&self) -> usize {
        self.fonts.len()
    }

    #[cfg(test)]
    pub(crate) fn scratch_capacity(&self) -> usize {
        self.scratch.capacity()
    }

    pub fn push(&mut self, commands: &[Command<'_>]) -> Result<(), RenderError> {
        let mut index = 0;
        while index < commands.len() {
            if commands[index] == Command::PageBreak {
                self.cursor = start_page(self.page, &mut self.contents);
                index += 1;
                continue;
            }
            let Self {
                scratch,
                fonts,
                used_fonts,
                ..
            } = self;
            scratch.clear();
            let height = measure(
                commands,
                &mut index,
                self.page.width.0 - self.page.margin.0 * 2.0,
                0.0,
                0.0,
                0,
                fonts,
                scratch,
            )?;
            // Only a font a line actually uses is written into the PDF.
            for line in &scratch.lines {
                used_fonts[usize::from(line.font)] = true;
            }
            if !height.is_finite() || height > self.page.height.0 - self.page.margin.0 * 2.0 {
                return Err(RenderError::PageOverflow);
            }
            if height > self.cursor - self.page.margin.0 {
                self.cursor = start_page(self.page, &mut self.contents);
            }
            write_lines(
                self.contents.last_mut().unwrap(),
                &self.scratch,
                self.page.margin.0,
                self.cursor,
            );
            self.cursor -= height;
        }
        Ok(())
    }

    pub fn finish(self) -> Vec<u8> {
        finish_pdf(self.page, self.contents, self.fonts, self.used_fonts)
    }
}

/// The protocol's decoded form, owned so a block outlives the input chunk that
/// carried it.
pub(crate) enum OwnedCommand {
    Text(String, Pt),
    StyledText(String, Pt, TextAlign, [u8; 3], u8),
    BoxStart(BoxStyle),
    BoxEnd,
    Spacer(Pt),
    StackStart(Pt),
    StackEnd,
    RowStart(Vec<ColumnWidth>),
    RowEnd,
}

impl OwnedCommand {
    pub(crate) fn borrow(&self) -> Command<'_> {
        match self {
            Self::Text(text, size) => Command::Text { text, size: *size },
            Self::StyledText(text, size, align, color, font) => Command::StyledText {
                text,
                size: *size,
                align: *align,
                color: *color,
                font: *font,
            },
            Self::BoxStart(style) => Command::BoxStart { style: *style },
            Self::BoxEnd => Command::BoxEnd,
            Self::Spacer(value) => Command::Spacer(*value),
            Self::StackStart(gap) => Command::StackStart { gap: *gap },
            Self::StackEnd => Command::StackEnd,
            Self::RowStart(columns) => Command::RowStart { columns },
            Self::RowEnd => Command::RowEnd,
        }
    }
}
struct Line {
    text: Range<usize>,
    size: f32,
    x: f32,
    y: f32,
    width: f32,
    text_width: f32,
    align: TextAlign,
    color: [u8; 3],
    font: u8,
}

struct BoxPaint {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    style: BoxStyle,
}

#[derive(Default)]
struct Scratch {
    lines: Vec<Line>,
    text: Vec<u8>,
    boxes: Vec<BoxPaint>,
}

impl Scratch {
    fn clear(&mut self) {
        self.lines.clear();
        self.text.clear();
        self.boxes.clear();
    }

    #[cfg(test)]
    fn capacity(&self) -> usize {
        self.lines.capacity() * std::mem::size_of::<Line>() + self.text.capacity()
    }
}

fn write_lines(content: &mut Content, scratch: &Scratch, left: f32, top: f32) {
    for paint in &scratch.boxes {
        let x = left + paint.x;
        let y = top - paint.y - paint.height;
        if let Some(color) = paint.style.background {
            content.set_fill_rgb(
                f32::from(color[0]) / 255.0,
                f32::from(color[1]) / 255.0,
                f32::from(color[2]) / 255.0,
            );
            content.rect(x, y, paint.width, paint.height).fill_nonzero();
        }
        let [top, right, bottom, left] = paint.style.border.map(|edge| edge.0);
        for (edge, (x, y, width, height)) in paint.style.border_color.iter().zip([
            (x, y + paint.height - top, paint.width, top),
            (x + paint.width - right, y, right, paint.height),
            (x, y, paint.width, bottom),
            (x, y, left, paint.height),
        ]) {
            if width > 0.0 && height > 0.0 {
                content.set_fill_rgb(
                    f32::from(edge[0]) / 255.0,
                    f32::from(edge[1]) / 255.0,
                    f32::from(edge[2]) / 255.0,
                );
                content.rect(x, y, width, height).fill_nonzero();
            }
        }
    }
    for line in &scratch.lines {
        content.begin_text();
        content.set_fill_rgb(
            f32::from(line.color[0]) / 255.0,
            f32::from(line.color[1]) / 255.0,
            f32::from(line.color[2]) / 255.0,
        );
        let mut buffer = [0_u8; 4];
        content.set_font(Name(font_name(line.font, &mut buffer)), line.size);
        let x = match line.align {
            TextAlign::Left => line.x,
            TextAlign::Center => line.x + (line.width - line.text_width) / 2.0,
            TextAlign::Right => line.x + line.width - line.text_width,
        };
        content.next_line(left + x, top - line.y);
        content.show(Str(&scratch.text[line.text.clone()]));
        content.end_text();
    }
}

fn measure_text(
    text: &str,
    size: Pt,
    width: f32,
    x: f32,
    y: f32,
    fonts: &[Font],
    scratch: &mut Scratch,
) -> Result<f32, RenderError> {
    measure_text_styled(
        text,
        size,
        width,
        x,
        y,
        TextAlign::Left,
        [0, 0, 0],
        HELVETICA,
        fonts,
        scratch,
    )
}

#[allow(clippy::too_many_arguments)] // Text layout takes its explicit paint inputs from the protocol.
fn measure_text_styled(
    text: &str,
    size: Pt,
    width: f32,
    x: f32,
    y: f32,
    align: TextAlign,
    color: [u8; 3],
    font: u8,
    fonts: &[Font],
    scratch: &mut Scratch,
) -> Result<f32, RenderError> {
    if size.0 == 0.0 {
        return Err(RenderError::InvalidFontSize);
    }
    let selected_font = fonts
        .get(usize::from(font))
        .ok_or(RenderError::InvalidLayout)?;
    let (ascent, descent) = selected_font.metrics();
    let ascent = ascent * size.0 / 1000.0;
    let line_height = (ascent - descent * size.0 / 1000.0).max(0.0);
    let mut height = 0.0;
    let mut used = 0.0;
    let mut line_start = scratch.text.len();
    let mut has_word = false;
    for word in text.split_ascii_whitespace() {
        let word_width = word.chars().try_fold(0_u64, |width, character| {
            encode_win_ansi(character)
                .and_then(|byte| selected_font.width(byte))
                .map(|advance| width + u64::from(advance))
        })? as f32
            * size.0
            / 1000.0;
        if word_width > width {
            return Err(RenderError::TextTooWide);
        }
        let space = if has_word {
            f32::from(selected_font.width(b' ')?) * size.0 / 1000.0
        } else {
            0.0
        };
        if has_word && used + space + word_width > width {
            scratch.lines.push(Line {
                text: line_start..scratch.text.len(),
                size: size.0,
                x,
                y: y + height + ascent,
                width,
                text_width: used,
                align,
                color,
                font,
            });
            height += line_height;
            used = 0.0;
            line_start = scratch.text.len();
            has_word = false;
        }
        if has_word {
            scratch.text.push(b' ');
            used += space;
        }
        for character in word.chars() {
            scratch.text.push(encode_win_ansi(character)?);
        }
        used += word_width;
        has_word = true;
    }
    if has_word {
        scratch.lines.push(Line {
            text: line_start..scratch.text.len(),
            size: size.0,
            x,
            y: y + height + ascent,
            width,
            text_width: used,
            align,
            color,
            font,
        });
        height += line_height;
    }
    Ok(height)
}

// One top-level block owns all measured lines; nested containers share this buffer.
#[allow(clippy::too_many_arguments)] // Layout state is explicit rather than boxed into a context struct.
fn measure(
    commands: &[Command<'_>],
    index: &mut usize,
    width: f32,
    x: f32,
    y: f32,
    depth: usize,
    fonts: &[Font],
    scratch: &mut Scratch,
) -> Result<f32, RenderError> {
    if !width.is_finite() || width <= 0.0 {
        return Err(RenderError::InvalidLayout);
    }
    let command = *commands.get(*index).ok_or(RenderError::InvalidLayout)?;
    *index += 1;
    if depth >= 64
        && matches!(
            command,
            Command::StackStart { .. } | Command::BoxStart { .. } | Command::RowStart { .. }
        )
    {
        return Err(RenderError::InvalidLayout);
    }
    match command {
        Command::Text { text, size } => measure_text(text, size, width, x, y, fonts, scratch),
        Command::StyledText {
            text,
            size,
            align,
            color,
            font,
        } => measure_text_styled(text, size, width, x, y, align, color, font, fonts, scratch),
        Command::BoxStart { style } => {
            let horizontal = style.margin[1].0
                + style.margin[3].0
                + style.padding[1].0
                + style.padding[3].0
                + style.border[1].0
                + style.border[3].0;
            if horizontal >= width
                || style
                    .margin
                    .iter()
                    .chain(style.padding.iter())
                    .any(|value| value.0 < 0.0)
            {
                return Err(RenderError::InvalidLayout);
            }
            let inset_x = style.margin[3].0 + style.padding[3].0 + style.border[3].0;
            let inset_y = style.margin[0].0 + style.padding[0].0 + style.border[0].0;
            let inner_width = width - horizontal;
            let mut inner_height = 0.0;
            let paint = (style.background.is_some()
                || style.border.iter().any(|edge| edge.0 > 0.0))
            .then(|| {
                let index = scratch.boxes.len();
                scratch.boxes.push(BoxPaint {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                    style,
                });
                index
            });
            while commands.get(*index) != Some(&Command::BoxEnd) {
                inner_height += measure(
                    commands,
                    index,
                    inner_width,
                    x + inset_x,
                    y + inset_y + inner_height,
                    depth + 1,
                    fonts,
                    scratch,
                )?;
            }
            *index += 1;
            let box_height = inner_height
                + style.padding[0].0
                + style.padding[2].0
                + style.border[0].0
                + style.border[2].0;
            if let Some(index) = paint {
                scratch.boxes[index] = BoxPaint {
                    x: x + style.margin[3].0,
                    y: y + style.margin[0].0,
                    width: width - style.margin[1].0 - style.margin[3].0,
                    height: box_height,
                    style,
                };
            }
            Ok(style.margin[0].0 + box_height + style.margin[2].0)
        }
        Command::Spacer(space) => Ok(space.0),
        Command::StackStart { gap } => {
            let mut height = 0.0;
            let mut first = true;
            while commands.get(*index) != Some(&Command::StackEnd) {
                if !first {
                    height += gap.0;
                }
                height += measure(
                    commands,
                    index,
                    width,
                    x,
                    y + height,
                    depth + 1,
                    fonts,
                    scratch,
                )?;
                first = false;
            }
            *index += 1;
            Ok(height)
        }
        Command::RowStart { columns } => {
            if columns.is_empty() || columns.len() > 256 {
                return Err(RenderError::InvalidLayout);
            }
            let mut fixed = 0.0_f64;
            let mut fractions = 0.0_f64;
            for column in columns {
                match *column {
                    ColumnWidth::Fixed(value) if value.0 > 0.0 => fixed += f64::from(value.0),
                    ColumnWidth::Percent(value) if value.0 > 0.0 => {
                        fixed += f64::from(width) * f64::from(value.0) / 100.0
                    }
                    ColumnWidth::Fraction(value) if value.0 > 0.0 => {
                        fractions += f64::from(value.0)
                    }
                    _ => return Err(RenderError::InvalidLayout),
                }
            }
            let tolerance = f64::from(width) * f64::from(f32::EPSILON) * columns.len() as f64;
            let remaining = f64::from(width) - fixed;
            if remaining < -tolerance || (fractions > 0.0 && remaining <= 0.0) {
                return Err(RenderError::InvalidLayout);
            }
            let remaining = remaining.max(0.0);
            let mut offset = 0.0;
            let mut height = 0.0_f32;
            for column in columns {
                let cell_width = match *column {
                    ColumnWidth::Fixed(value) => value.0,
                    ColumnWidth::Percent(value) => width * value.0 / 100.0,
                    ColumnWidth::Fraction(value) => {
                        (remaining * f64::from(value.0) / fractions) as f32
                    }
                };
                height = height.max(measure(
                    commands,
                    index,
                    cell_width,
                    x + offset,
                    y,
                    depth + 1,
                    fonts,
                    scratch,
                )?);
                offset += cell_width;
            }
            if commands.get(*index) != Some(&Command::RowEnd) {
                return Err(RenderError::InvalidLayout);
            }
            *index += 1;
            Ok(height)
        }
        _ => Err(RenderError::InvalidLayout),
    }
}
