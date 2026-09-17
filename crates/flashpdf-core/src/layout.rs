use pdf_writer::{Content, Name, Str};
use std::ops::Range;

use crate::font::{encode_win_ansi, font_name, Font};
use crate::pdf::{finish_pdf, start_page};
use crate::{
    BoxStyle, ColumnWidth, Command, EmbeddedFont, FontId, Page, Pt, RenderError, Rgb, TextAlign,
    TextStyle,
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
    output: LayoutOutput,
    fonts: Vec<Font>,
    used_fonts: Vec<bool>,
}

enum Commands<'a, 'b> {
    Borrowed(&'a [Command<'b>]),
    Owned(&'a [OwnedCommand]),
}

impl Commands<'_, '_> {
    fn get(&self, index: usize) -> Option<Command<'_>> {
        match self {
            Self::Borrowed(commands) => commands.get(index).copied(),
            Self::Owned(commands) => commands.get(index).map(OwnedCommand::borrow),
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Borrowed(commands) => commands.len(),
            Self::Owned(commands) => commands.len(),
        }
    }
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
            output: LayoutOutput::default(),
            fonts,
            used_fonts,
        }
    }

    pub(crate) fn font_count(&self) -> usize {
        self.fonts.len()
    }

    #[cfg(test)]
    pub(crate) fn scratch_capacity(&self) -> usize {
        self.output.capacity()
    }

    pub fn push(&mut self, commands: &[Command<'_>]) -> Result<(), RenderError> {
        self.push_commands(Commands::Borrowed(commands))
    }

    pub(crate) fn push_owned(&mut self, commands: &[OwnedCommand]) -> Result<(), RenderError> {
        self.push_commands(Commands::Owned(commands))
    }

    fn push_commands(&mut self, commands: Commands<'_, '_>) -> Result<(), RenderError> {
        let mut index = 0;
        while index < commands.len() {
            if commands.get(index) == Some(Command::PageBreak) {
                self.cursor = start_page(self.page, &mut self.contents);
                index += 1;
                continue;
            }
            let Self {
                output,
                fonts,
                used_fonts,
                ..
            } = self;
            output.clear();
            let height = LayoutEngine {
                commands: &commands,
                fonts,
                output,
            }
            .measure(
                &mut index,
                Area {
                    x: 0.0,
                    y: 0.0,
                    width: self.page.width.0 - self.page.margin.0 * 2.0,
                },
                0,
            )?;
            // Only a font a line actually uses is written into the PDF.
            for line in &output.lines {
                used_fonts[line.font.index()] = true;
            }
            if !height.is_finite() || height > self.page.height.0 - self.page.margin.0 * 2.0 {
                return Err(RenderError::PageOverflow);
            }
            if height > self.cursor - self.page.margin.0 {
                self.cursor = start_page(self.page, &mut self.contents);
            }
            write_lines(
                self.contents.last_mut().unwrap(),
                &self.output,
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
    Text(String, TextStyle),
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
            Self::Text(text, style) => Command::Text {
                text,
                style: *style,
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
    color: Rgb,
    font: FontId,
}

struct BoxPaint {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    style: BoxStyle,
}

#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    width: f32,
}

#[derive(Default)]
struct LayoutOutput {
    lines: Vec<Line>,
    text: Vec<u8>,
    boxes: Vec<BoxPaint>,
}

impl LayoutOutput {
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

fn write_lines(content: &mut Content, output: &LayoutOutput, left: f32, top: f32) {
    for paint in &output.boxes {
        let x = left + paint.x;
        let y = top - paint.y - paint.height;
        if let Some(color) = paint.style.background {
            content.set_fill_rgb(
                f32::from(color.r) / 255.0,
                f32::from(color.g) / 255.0,
                f32::from(color.b) / 255.0,
            );
            content.rect(x, y, paint.width, paint.height).fill_nonzero();
        }
        let [top, right, bottom, left] = paint.style.border.into_array().map(|edge| edge.0);
        for (edge, (x, y, width, height)) in paint.style.border_color.into_array().iter().zip([
            (x, y + paint.height - top, paint.width, top),
            (x + paint.width - right, y, right, paint.height),
            (x, y, paint.width, bottom),
            (x, y, left, paint.height),
        ]) {
            if width > 0.0 && height > 0.0 {
                content.set_fill_rgb(
                    f32::from(edge.r) / 255.0,
                    f32::from(edge.g) / 255.0,
                    f32::from(edge.b) / 255.0,
                );
                content.rect(x, y, width, height).fill_nonzero();
            }
        }
    }
    for line in &output.lines {
        content.begin_text();
        content.set_fill_rgb(
            f32::from(line.color.r) / 255.0,
            f32::from(line.color.g) / 255.0,
            f32::from(line.color.b) / 255.0,
        );
        let mut buffer = [0_u8; 4];
        content.set_font(Name(font_name(line.font, &mut buffer)), line.size);
        let x = match line.align {
            TextAlign::Left => line.x,
            TextAlign::Center => line.x + (line.width - line.text_width) / 2.0,
            TextAlign::Right => line.x + line.width - line.text_width,
        };
        content.next_line(left + x, top - line.y);
        content.show(Str(&output.text[line.text.clone()]));
        content.end_text();
    }
}

fn measure_text(
    text: &str,
    style: TextStyle,
    width: f32,
    x: f32,
    y: f32,
    fonts: &[Font],
    output: &mut LayoutOutput,
) -> Result<f32, RenderError> {
    let TextStyle {
        size,
        align,
        color,
        font,
    } = style;
    if size.0 == 0.0 {
        return Err(RenderError::InvalidFontSize);
    }
    let selected_font = fonts.get(font.index()).ok_or(RenderError::InvalidLayout)?;
    let (ascent, descent) = selected_font.metrics();
    let ascent = ascent * size.0 / 1000.0;
    let line_height = (ascent - descent * size.0 / 1000.0).max(0.0);
    let mut height = 0.0;
    let mut used = 0.0;
    let mut line_start = output.text.len();
    let mut has_word = false;
    for word in text.split_ascii_whitespace() {
        let previous_end = output.text.len();
        if has_word {
            output.text.push(b' ');
        }
        let word_start = output.text.len();
        let mut advance = 0_u64;
        for character in word.chars() {
            let byte = encode_win_ansi(character)?;
            advance += u64::from(selected_font.width(byte)?);
            output.text.push(byte);
        }
        let word_width = advance as f32 * size.0 / 1000.0;
        if word_width > width {
            output.text.truncate(previous_end);
            return Err(RenderError::TextTooWide);
        }
        let space = if has_word {
            f32::from(selected_font.width(b' ')?) * size.0 / 1000.0
        } else {
            0.0
        };
        if has_word && used + space + word_width > width {
            output.lines.push(Line {
                text: line_start..previous_end,
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
            // The separator before the wrapped word stays outside both ranges.
            line_start = word_start;
            has_word = false;
        }
        if has_word {
            used += space;
        }
        used += word_width;
        has_word = true;
    }
    if has_word {
        output.lines.push(Line {
            text: line_start..output.text.len(),
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
struct LayoutEngine<'a, 'b, 'c> {
    commands: &'a Commands<'b, 'c>,
    fonts: &'a [Font],
    output: &'a mut LayoutOutput,
}

impl LayoutEngine<'_, '_, '_> {
    fn measure(&mut self, index: &mut usize, area: Area, depth: usize) -> Result<f32, RenderError> {
        let Area { x, y, width } = area;
        if !width.is_finite() || width <= 0.0 {
            return Err(RenderError::InvalidLayout);
        }
        let command = self
            .commands
            .get(*index)
            .ok_or(RenderError::InvalidLayout)?;
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
            Command::Text { text, style } => {
                measure_text(text, style, width, x, y, self.fonts, self.output)
            }
            Command::BoxStart { style } => {
                let horizontal = style.margin.right.0
                    + style.margin.left.0
                    + style.padding.right.0
                    + style.padding.left.0
                    + style.border.right.0
                    + style.border.left.0;
                if horizontal >= width
                    || style
                        .margin
                        .iter()
                        .chain(style.padding.iter())
                        .any(|value| value.0 < 0.0)
                {
                    return Err(RenderError::InvalidLayout);
                }
                let inset_x = style.margin.left.0 + style.padding.left.0 + style.border.left.0;
                let inset_y = style.margin.top.0 + style.padding.top.0 + style.border.top.0;
                let inner_width = width - horizontal;
                let mut inner_height = 0.0;
                let paint = (style.background.is_some()
                    || style.border.iter().any(|edge| edge.0 > 0.0))
                .then(|| {
                    let index = self.output.boxes.len();
                    self.output.boxes.push(BoxPaint {
                        x: 0.0,
                        y: 0.0,
                        width: 0.0,
                        height: 0.0,
                        style,
                    });
                    index
                });
                while self.commands.get(*index) != Some(Command::BoxEnd) {
                    inner_height += self.measure(
                        index,
                        Area {
                            x: x + inset_x,
                            y: y + inset_y + inner_height,
                            width: inner_width,
                        },
                        depth + 1,
                    )?;
                }
                *index += 1;
                let box_height = inner_height
                    + style.padding.top.0
                    + style.padding.bottom.0
                    + style.border.top.0
                    + style.border.bottom.0;
                if let Some(index) = paint {
                    self.output.boxes[index] = BoxPaint {
                        x: x + style.margin.left.0,
                        y: y + style.margin.top.0,
                        width: width - style.margin.right.0 - style.margin.left.0,
                        height: box_height,
                        style,
                    };
                }
                Ok(style.margin.top.0 + box_height + style.margin.bottom.0)
            }
            Command::Spacer(space) => Ok(space.0),
            Command::StackStart { gap } => {
                let mut height = 0.0;
                let mut first = true;
                while self.commands.get(*index) != Some(Command::StackEnd) {
                    if !first {
                        height += gap.0;
                    }
                    height += self.measure(
                        index,
                        Area {
                            x,
                            y: y + height,
                            width,
                        },
                        depth + 1,
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
                        ColumnWidth::Percent(value) => {
                            fixed += f64::from(width) * f64::from(value.get()) / 100.0
                        }
                        ColumnWidth::Fraction(value) => fractions += f64::from(value.get()),
                        ColumnWidth::Fixed(_) => return Err(RenderError::InvalidLayout),
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
                        ColumnWidth::Percent(value) => width * value.get() / 100.0,
                        ColumnWidth::Fraction(value) => {
                            (remaining * f64::from(value.get()) / fractions) as f32
                        }
                    };
                    height = height.max(self.measure(
                        index,
                        Area {
                            x: x + offset,
                            y,
                            width: cell_width,
                        },
                        depth + 1,
                    )?);
                    offset += cell_width;
                }
                if self.commands.get(*index) != Some(Command::RowEnd) {
                    return Err(RenderError::InvalidLayout);
                }
                *index += 1;
                Ok(height)
            }
            _ => Err(RenderError::InvalidLayout),
        }
    }
}
