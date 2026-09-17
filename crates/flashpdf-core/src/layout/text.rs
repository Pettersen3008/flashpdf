use crate::font::{Font, FontBook};
use crate::geometry::{LayoutArea, Point};
use crate::layout::output::{LayoutBuffer, PositionedLine};
use crate::layout::LayoutError;
use crate::win_ansi::encode;
use crate::{Pt, RenderError, TextStyle};

pub(crate) struct TextLayouter<'a> {
    font: &'a Font,
    style: TextStyle,
    ascent: Pt,
    line_height: Pt,
}

impl<'a> TextLayouter<'a> {
    pub(crate) fn new(fonts: &'a FontBook, style: TextStyle) -> Result<Self, LayoutError> {
        if style.size == Pt::ZERO {
            return Err(LayoutError::InvalidFontSize);
        }
        let font = fonts
            .get(style.font)
            .ok_or(LayoutError::UnknownFont(style.font))?;
        let (ascent, descent) = font.metrics();
        let ascent = ascent * style.size.get() / 1000.0;
        let line_height = (ascent - descent * style.size.get() / 1000.0).max(0.0);
        Ok(Self {
            font,
            style,
            ascent: Pt(ascent),
            line_height: Pt(line_height),
        })
    }

    pub(crate) fn layout_into(
        &self,
        text: &str,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let mut height = Pt::ZERO;
        let mut used = Pt::ZERO;
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
                let byte = encode(character).map_err(text_error)?;
                advance += u64::from(self.font.width(byte).map_err(text_error)?);
                output.text.push(byte);
            }
            let word_width = Pt(advance as f32 * self.style.size.get() / 1000.0);
            if word_width > area.width {
                output.text.truncate(previous_end);
                return Err(LayoutError::TextTooWide);
            }
            let space = if has_word {
                Pt(
                    f32::from(self.font.width(b' ').map_err(text_error)?) * self.style.size.get()
                        / 1000.0,
                )
            } else {
                Pt::ZERO
            };
            if has_word && used + space + word_width > area.width {
                self.push_line(output, line_start..previous_end, area, height, used);
                height += self.line_height;
                used = Pt::ZERO;
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
            self.push_line(output, line_start..output.text.len(), area, height, used);
            height += self.line_height;
        }
        Ok(height)
    }

    fn push_line(
        &self,
        output: &mut LayoutBuffer,
        text: std::ops::Range<usize>,
        area: LayoutArea,
        height: Pt,
        text_width: Pt,
    ) {
        output.lines.push(PositionedLine {
            text,
            origin: Point {
                x: area.origin.x,
                y: area.origin.y + height + self.ascent,
            },
            available_width: area.width,
            text_width,
            style: self.style,
        });
    }
}

fn text_error(error: RenderError) -> LayoutError {
    match error {
        RenderError::UnsupportedCharacter(character) => {
            LayoutError::UnsupportedCharacter(character)
        }
        RenderError::MissingGlyph => LayoutError::MissingGlyph,
        _ => unreachable!("text primitives return only character and glyph errors"),
    }
}
