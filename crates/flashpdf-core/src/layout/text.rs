use std::ops::Range;

use crate::document::Paragraph;
use crate::font::{Font, FontBook};
use crate::geometry::{LayoutArea, Point};
use crate::layout::output::{LayoutBuffer, PositionedLine, Segment};
use crate::layout::LayoutError;
use crate::{FontId, Pt, RenderError, Rgb, TextAlign};

struct RunMetrics<'a> {
    font: &'a Font,
    id: FontId,
    size: Pt,
    color: Rgb,
    ascent: Pt,
    line_height: Pt,
}

impl RunMetrics<'_> {
    fn scaled(&self, advance: u16) -> Pt {
        Pt(f32::from(advance) * self.size.get() / 1000.0)
    }

    /// Appends the character's show-string code and returns its advance.
    fn encode(&self, character: char, out: &mut Vec<u8>) -> Result<Pt, LayoutError> {
        let advance = self.font.encode_into(character, out).map_err(text_error)?;
        Ok(self.scaled(advance))
    }

    fn advance(&self, code: &[u8]) -> Pt {
        self.scaled(self.font.advance(code))
    }
}

/// The part of the pending word that sits in one run.
#[derive(Clone, Copy)]
struct Piece {
    run: usize,
    start: usize,
    end: usize,
    width: Pt,
}

/// The collapsed space before the pending word: its run, encoded bytes and width.
struct Space {
    run: usize,
    text: Range<usize>,
    width: Pt,
}

/// Greedy word wrap over runs. Words are ASCII-whitespace delimited and may
/// span runs; whitespace collapses to one space owned by the run it first
/// appears in, and lines trim it at both ends. CJK characters are words of
/// their own, so unspaced ideographic text breaks between any two of them.
pub(crate) struct ParagraphLayouter<'a> {
    runs: Vec<RunMetrics<'a>>,
    align: TextAlign,
    area: LayoutArea,
    height: Pt,
    segment_start: usize,
    used: Pt,
    ascent: Pt,
    line_height: Pt,
    has_word: bool,
    pieces: Vec<Piece>,
    space: Option<Space>,
}

impl<'a> ParagraphLayouter<'a> {
    pub(crate) fn layout(
        fonts: &'a FontBook,
        paragraph: &Paragraph<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<Pt, LayoutError> {
        let mut runs = Vec::with_capacity(paragraph.runs.len());
        for run in &paragraph.runs {
            if run.size == Pt::ZERO {
                return Err(LayoutError::InvalidFontSize);
            }
            let font = fonts
                .get(run.font)
                .ok_or(LayoutError::UnknownFont(run.font))?;
            let (ascent, descent, gap) = font.metrics();
            let scale = run.size.get() / 1000.0;
            let ascent = ascent * scale;
            runs.push(RunMetrics {
                font,
                id: run.font,
                size: run.size,
                color: run.color,
                ascent: Pt(ascent),
                line_height: Pt((ascent - descent * scale + gap * scale).max(0.0)),
            });
        }
        let mut state = Self {
            runs,
            align: paragraph.align,
            area,
            height: Pt::ZERO,
            segment_start: output.segments.len(),
            used: Pt::ZERO,
            ascent: Pt::ZERO,
            line_height: Pt::ZERO,
            has_word: false,
            pieces: Vec::new(),
            space: None,
        };
        let mut rest = paragraph.text;
        for (index, run) in paragraph.runs.iter().enumerate() {
            let (text, remaining) = rest
                .split_at_checked(run.len)
                .ok_or(LayoutError::InvalidRuns)?;
            rest = remaining;
            state.run(index, text, output)?;
            if run.hard_break {
                state.word(output)?;
                state.hard_break(index, output);
            }
        }
        if !rest.is_empty() {
            return Err(LayoutError::InvalidRuns);
        }
        state.word(output)?;
        if state.has_word {
            state.push_line(output);
        }
        Ok(state.height)
    }

    fn run(
        &mut self,
        index: usize,
        text: &str,
        output: &mut LayoutBuffer,
    ) -> Result<(), LayoutError> {
        let mut piece: Option<Piece> = None;
        for character in text.chars() {
            if character.is_ascii_whitespace() {
                self.pieces.extend(piece.take());
                self.word(output)?;
                if self.has_word && self.space.is_none() {
                    let start = output.text.len();
                    let width = self.runs[index].encode(' ', &mut output.text)?;
                    self.space = Some(Space {
                        run: index,
                        text: start..output.text.len(),
                        width,
                    });
                }
                continue;
            }
            let ideograph = ideograph(character);
            if ideograph {
                self.pieces.extend(piece.take());
                self.word(output)?;
            }
            let start = output.text.len();
            let width = self.runs[index].encode(character, &mut output.text)?;
            let slot = piece.get_or_insert(Piece {
                run: index,
                start,
                end: start,
                width: Pt::ZERO,
            });
            slot.end = output.text.len();
            slot.width += width;
            if ideograph {
                self.pieces.extend(piece.take());
                self.word(output)?;
            }
        }
        self.pieces.extend(piece);
        Ok(())
    }

    /// Places the pending word: after its space when that fits, else on a new
    /// line, else glyph by glyph when it is wider than the column.
    fn word(&mut self, output: &mut LayoutBuffer) -> Result<(), LayoutError> {
        if self.pieces.is_empty() {
            return Ok(());
        }
        let pieces = std::mem::take(&mut self.pieces);
        let width = pieces
            .iter()
            .fold(Pt::ZERO, |total, piece| total + piece.width);
        let space = self.space.take();
        if width > self.area.width {
            if self.has_word {
                self.push_line(output);
            }
            for piece in &pieces {
                let stride = self.runs[piece.run].font.stride();
                let mut position = piece.start;
                while position < piece.end {
                    let code = position..position + stride;
                    let glyph = self.runs[piece.run].advance(&output.text[code.clone()]);
                    if glyph > self.area.width {
                        return Err(LayoutError::TextTooWide);
                    }
                    if self.used + glyph > self.area.width {
                        self.push_line(output);
                    }
                    self.append(piece.run, code, glyph, output);
                    position += stride;
                }
            }
        } else {
            let gap = space.as_ref().map_or(Pt::ZERO, |space| space.width);
            if self.has_word && self.used + gap + width > self.area.width {
                self.push_line(output);
            } else if let Some(space) = space {
                self.append(space.run, space.text, space.width, output);
            }
            for piece in &pieces {
                self.append(piece.run, piece.start..piece.end, piece.width, output);
            }
        }
        self.pieces = pieces;
        self.pieces.clear();
        self.has_word = true;
        Ok(())
    }

    fn append(&mut self, run: usize, text: Range<usize>, width: Pt, output: &mut LayoutBuffer) {
        let metrics = &self.runs[run];
        if metrics.ascent > self.ascent {
            self.ascent = metrics.ascent;
        }
        if metrics.line_height > self.line_height {
            self.line_height = metrics.line_height;
        }
        let on_line = output.segments.len() > self.segment_start;
        match output.segments.last_mut() {
            Some(last)
                if on_line
                    && last.font == metrics.id
                    && last.size == metrics.size
                    && last.color == metrics.color =>
            {
                last.text.end = text.end;
            }
            _ => output.segments.push(Segment {
                text,
                x: self.used,
                font: metrics.id,
                size: metrics.size,
                color: metrics.color,
            }),
        }
        self.used += width;
    }

    fn hard_break(&mut self, run: usize, output: &mut LayoutBuffer) {
        if !self.has_word {
            self.ascent = self.runs[run].ascent;
            self.line_height = self.runs[run].line_height;
        }
        self.push_line(output);
    }

    fn push_line(&mut self, output: &mut LayoutBuffer) {
        let top = self.area.origin.y + self.height;
        output.lines.push(PositionedLine {
            segments: self.segment_start..output.segments.len(),
            origin: Point {
                x: self.area.origin.x,
                y: top + self.ascent,
            },
            top,
            bottom: top + self.line_height,
            available_width: self.area.width,
            text_width: self.used,
            align: self.align,
        });
        self.height += self.line_height;
        self.segment_start = output.segments.len();
        self.used = Pt::ZERO;
        self.ascent = Pt::ZERO;
        self.line_height = Pt::ZERO;
        self.has_word = false;
        self.space = None;
    }
}

/// A line may break before and after these: CJK scripts do not separate words
/// with spaces. Covers the ideographic space and CJK punctuation, kana, Hangul,
/// the unified ideographs with their extensions, and the full-width forms.
/// ponytail: no kinsoku, so a line may start with a closing bracket or full stop.
fn ideograph(character: char) -> bool {
    matches!(
        character as u32,
        0x3000..=0x30ff
            | 0x3400..=0x4dbf
            | 0x4e00..=0x9fff
            | 0xac00..=0xd7af
            | 0xf900..=0xfaff
            | 0xff00..=0xffef
            | 0x20000..=0x3134f
    )
}

fn text_error(error: RenderError) -> LayoutError {
    match error {
        RenderError::UnsupportedCharacter(character) => {
            LayoutError::UnsupportedCharacter(character)
        }
        RenderError::MissingGlyph(character) => LayoutError::MissingGlyph(character),
        _ => unreachable!("text primitives return only character and glyph errors"),
    }
}

#[cfg(test)]
mod tests {
    use super::ideograph;

    #[test]
    fn given_cjk_and_latin_code_points_when_asked_for_break_opportunities_then_only_cjk_break_anywhere(
    ) {
        for character in ['中', 'あ', 'カ', '한', '\u{3000}', '。', '\u{20000}'] {
            assert!(ideograph(character), "{character:?}");
        }
        for character in ['a', 'Æ', 'α', 'Д', '\u{a0}', '-', '\u{2013}'] {
            assert!(!ideograph(character), "{character:?}");
        }
    }
}
