use std::ops::Range;

use crate::document::Paragraph;
use crate::font::{Font, FontBook};
use crate::geometry::{LayoutArea, Point};
use crate::layout::output::{Gap, LayoutBuffer, PositionedLine, Segment, MARKER_SLOT};
use crate::layout::LayoutError;
use crate::{FontId, Marker, Pt, RenderError, Rgb, TextAlign, Typography};

struct RunMetrics<'a> {
    font: &'a Font,
    id: FontId,
    size: Pt,
    color: Rgb,
    ascent: Pt,
    /// The run's line box below the baseline.
    depth: Pt,
    typography: Typography,
    /// Spaces carry a `Gap` for word spacing or justification.
    gapped: bool,
}

impl RunMetrics<'_> {
    fn scaled(&self, advance: u16) -> Pt {
        Pt(f32::from(advance) * self.size.get() / 1000.0)
    }

    /// Appends the character's show-string code and returns its advance with letter spacing.
    fn encode(&self, character: char, out: &mut Vec<u8>) -> Result<Pt, LayoutError> {
        let advance = self.font.encode_into(character, out).map_err(text_error)?;
        Ok(self.scaled(advance) + Pt(self.typography.letter_spacing))
    }

    fn advance(&self, code: &[u8]) -> Pt {
        self.scaled(self.font.advance(code)) + Pt(self.typography.letter_spacing)
    }

    /// The word-spacing gap after a space that ends at byte `at`, with its width.
    fn word_gap(&self, at: usize) -> Option<(Gap, Pt)> {
        let spacing = self.typography.word_spacing;
        self.gapped.then(|| {
            let gap = Gap {
                at: at as u32,
                amount: spacing * 1000.0 / self.size.get(),
                stretch: true,
            };
            (gap, Pt(spacing))
        })
    }
}

/// The part of the pending word that sits in one run.
#[derive(Clone, Copy)]
struct Piece {
    run: usize,
    start: usize,
    end: usize,
    width: Pt,
    /// Kerning against the previous piece's last glyph, which places this piece.
    lead: Pt,
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
    gap_start: usize,
    used: Pt,
    ascent: Pt,
    depth: Pt,
    has_word: bool,
    pieces: Vec<Piece>,
    /// Gaps inside the pending word, with their widths.
    gaps: Vec<(Gap, Pt)>,
    /// The pending word's last glyph: its run and code, for kerning.
    previous: Option<(usize, Range<usize>)>,
    space: Option<Space>,
    /// The list marker's segment, slot width, and whether it sits inside the line, until the first line takes it.
    marker: Option<(usize, Pt, bool)>,
    /// The widest word so far: the paragraph's min-content width.
    widest: Pt,
}

impl<'a> ParagraphLayouter<'a> {
    /// Returns the height and the widest word.
    pub(crate) fn layout(
        fonts: &'a FontBook,
        paragraph: &Paragraph<'_>,
        area: LayoutArea,
        output: &mut LayoutBuffer,
    ) -> Result<(Pt, Pt), LayoutError> {
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
            let rise = run.typography.rise;
            let content = ascent - descent * scale;
            // CSS half-leading: the difference to the content height splits evenly above and below.
            let (ascent, line_height) = match run.line_height {
                Some(height) => (ascent + (height.get() - content) / 2.0, height.get()),
                None => (ascent, (content + gap * scale).max(0.0)),
            };
            runs.push(RunMetrics {
                font,
                id: run.font,
                size: run.size,
                color: run.color,
                ascent: Pt(ascent + rise),
                depth: Pt(line_height - ascent - rise),
                typography: run.typography,
                gapped: run.typography.word_spacing != 0.0 || paragraph.align == TextAlign::Justify,
            });
        }
        let mut state = Self {
            runs,
            align: paragraph.align,
            area,
            height: Pt::ZERO,
            segment_start: output.segments.len(),
            gap_start: output.gaps.len(),
            used: Pt::ZERO,
            ascent: Pt::ZERO,
            depth: Pt::ZERO,
            has_word: false,
            pieces: Vec::new(),
            gaps: Vec::new(),
            previous: None,
            space: None,
            marker: None,
            widest: Pt::ZERO,
        };
        let mut rest = paragraph.text;
        for (index, run) in paragraph.runs.iter().enumerate() {
            let (text, remaining) = rest
                .split_at_checked(run.len)
                .ok_or(LayoutError::InvalidRuns)?;
            rest = remaining;
            if run.typography.marker != Marker::None {
                if index != 0 {
                    return Err(LayoutError::InvalidRuns);
                }
                state.marker(text, output)?;
                continue;
            }
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
            state.push_line(output, false);
        }
        Ok((state.height, state.widest))
    }

    /// Measures the list marker and queues it for the first line, which `push_line` pairs it
    /// with. An inside marker starts that line, so the first word wraps below it if it must.
    fn marker(&mut self, text: &str, output: &mut LayoutBuffer) -> Result<(), LayoutError> {
        let metrics = &self.runs[0];
        let shape = match text {
            "\u{2022}" => 1,
            "\u{25e6}" => 2,
            "\u{25aa}" => 3,
            _ => 0,
        };
        let start = output.text.len();
        let width = if shape == 0 {
            let mut width = Pt::ZERO;
            for character in text.chars() {
                width += metrics.encode(character, &mut output.text)?;
            }
            width
        } else {
            Pt(metrics.size.get() * MARKER_SLOT)
        };
        output.segments.push(Segment {
            text: start..output.text.len(),
            x: Pt::ZERO,
            font: metrics.id,
            size: metrics.size,
            color: metrics.color,
            letter_spacing: metrics.typography.letter_spacing,
            oblique: metrics.typography.oblique,
            rise: 0.0,
            shape,
        });
        self.segment_start = output.segments.len();
        let inside = metrics.typography.marker == Marker::Inside;
        if inside {
            (self.used, self.ascent, self.depth) = (width, metrics.ascent, metrics.depth);
        }
        self.marker = Some((self.segment_start - 1, width, inside));
        Ok(())
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
                    let metrics = &self.runs[index];
                    let width = metrics.encode(' ', &mut output.text)?;
                    self.space = Some(Space {
                        run: index,
                        text: start..output.text.len(),
                        width: width + Pt(metrics.typography.word_spacing),
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
            let mut width = self.runs[index].encode(character, &mut output.text)?;
            let lead = if character.is_whitespace() {
                self.previous = None;
                Pt::ZERO
            } else {
                self.kern(index, start..output.text.len(), output)
            };
            // CSS word spacing widens a no-break space too.
            if character == '\u{a0}' {
                if let Some((gap, extra)) = self.runs[index].word_gap(output.text.len()) {
                    self.gaps.push((gap, extra));
                    width += extra;
                }
            }
            let slot = piece.get_or_insert(Piece {
                run: index,
                start,
                end: start,
                width: Pt::ZERO,
                lead,
            });
            if slot.start != start {
                slot.width += lead;
            }
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

    /// The kerning before the glyph at `code`, after the word's previous glyph
    /// in the same font and size. A space, a CJK character, or a hard break
    /// ends the word, so a line break never falls inside a kerned pair.
    fn kern(&mut self, run: usize, code: Range<usize>, output: &LayoutBuffer) -> Pt {
        let Some((last, previous)) = self.previous.replace((run, code.clone())) else {
            return Pt::ZERO;
        };
        let (before, metrics) = (&self.runs[last], &self.runs[run]);
        if before.id != metrics.id
            || before.size != metrics.size
            || !(before.typography.kerning && metrics.typography.kerning)
        {
            return Pt::ZERO;
        }
        let amount = f32::from(
            metrics
                .font
                .kern(&output.text[previous], &output.text[code.clone()]),
        );
        if amount == 0.0 {
            return Pt::ZERO;
        }
        let width = Pt(amount * metrics.size.get() / 1000.0);
        let gap = Gap {
            at: code.start as u32,
            amount,
            stretch: false,
        };
        self.gaps.push((gap, width));
        width
    }

    /// Places the pending word: after its space when that fits, else on a new
    /// line, else glyph by glyph when it is wider than the column.
    fn word(&mut self, output: &mut LayoutBuffer) -> Result<(), LayoutError> {
        self.previous = None;
        if self.pieces.is_empty() {
            return Ok(());
        }
        let pieces = std::mem::take(&mut self.pieces);
        let mut pending = std::mem::take(&mut self.gaps);
        let width = pieces
            .iter()
            .fold(Pt::ZERO, |total, piece| total + piece.lead + piece.width);
        let space = self.space.take();
        if width > self.widest {
            self.widest = width;
        }
        let gap = space.as_ref().map_or(Pt::ZERO, |space| space.width);
        let started = self.has_word || self.used > Pt::ZERO;
        let overflow = self.used + gap + width > self.area.width;
        // A word after a space may still move to the next line; a word that never could, may not.
        if pieces
            .iter()
            .any(|piece| self.runs[piece.run].typography.nowrap)
            && (width > self.area.width || started && space.is_none() && overflow)
        {
            return Err(LayoutError::LineTooWide);
        }
        if width > self.area.width {
            if started {
                self.push_line(output, true);
            }
            let mut gaps = pending.drain(..).peekable();
            for piece in &pieces {
                let stride = self.runs[piece.run].font.stride();
                let mut position = piece.start;
                while position < piece.end {
                    let code = position..position + stride;
                    let glyph = self.runs[piece.run].advance(&output.text[code.clone()]);
                    if glyph > self.area.width {
                        return Err(LayoutError::TextTooWide);
                    }
                    // A gap before this glyph applies only when the line goes on.
                    let gap = gaps.next_if(|(gap, _)| gap.at as usize == position);
                    let lead = gap.map_or(Pt::ZERO, |(_, width)| width);
                    if self.used + lead + glyph > self.area.width {
                        self.push_line(output, true);
                    } else if let Some((gap, width)) = gap {
                        output.gaps.push(gap);
                        self.used += width;
                    }
                    self.append(piece.run, code, glyph, output);
                    position += stride;
                }
            }
        } else {
            if started && overflow {
                self.push_line(output, true);
            } else if let Some(space) = space {
                if let Some((gap, _)) = self.runs[space.run].word_gap(space.text.end) {
                    output.gaps.push(gap);
                }
                self.append(space.run, space.text, space.width, output);
            }
            for piece in &pieces {
                self.used += piece.lead;
                self.append(piece.run, piece.start..piece.end, piece.width, output);
            }
            output.gaps.extend(pending.drain(..).map(|(gap, _)| gap));
        }
        self.pieces = pieces;
        self.pieces.clear();
        self.gaps = pending;
        self.gaps.clear();
        self.has_word = true;
        Ok(())
    }

    fn append(&mut self, run: usize, text: Range<usize>, width: Pt, output: &mut LayoutBuffer) {
        let metrics = &self.runs[run];
        let on_line = output.segments.len() > self.segment_start;
        // The line box spans the tallest ascent and the deepest depth of its runs.
        if !on_line || metrics.ascent > self.ascent {
            self.ascent = metrics.ascent;
        }
        if !on_line || metrics.depth > self.depth {
            self.depth = metrics.depth;
        }
        let Typography {
            letter_spacing,
            oblique,
            rise,
            ..
        } = metrics.typography;
        match output.segments.last_mut() {
            Some(last)
                if on_line
                    && last.font == metrics.id
                    && last.size == metrics.size
                    && last.color == metrics.color
                    && last.letter_spacing == letter_spacing
                    && last.oblique == oblique
                    && last.rise == rise =>
            {
                last.text.end = text.end;
            }
            _ => output.segments.push(Segment {
                text,
                x: self.used,
                font: metrics.id,
                size: metrics.size,
                color: metrics.color,
                letter_spacing,
                oblique,
                rise,
                shape: 0,
            }),
        }
        self.used += width;
    }

    fn hard_break(&mut self, run: usize, output: &mut LayoutBuffer) {
        if !self.has_word {
            self.ascent = self.runs[run].ascent;
            self.depth = self.runs[run].depth;
        }
        self.push_line(output, false);
    }

    /// Spreads the line's slack over its spaces and moves later segments right.
    fn justify(&mut self, output: &mut LayoutBuffer) {
        let segments = &mut output.segments[self.segment_start..];
        let Some(end) = segments.last().map(|segment| segment.text.end) else {
            return;
        };
        // A no-break space that ends the line has nothing after it to push right.
        let inside = |gap: &Gap| gap.stretch && (gap.at as usize) < end;
        let gaps = &mut output.gaps[self.gap_start..];
        let count = gaps.iter().filter(|gap| inside(gap)).count();
        let slack = (self.area.width - self.used).get();
        if count == 0 || slack <= 0.0 {
            return;
        }
        let extra = slack / count as f32;
        let mut gaps = gaps.iter_mut().filter(|gap| inside(gap)).peekable();
        let mut shift = 0.0;
        for segment in segments {
            segment.x += Pt(shift);
            while let Some(gap) = gaps.next_if(|gap| gap.at as usize <= segment.text.end) {
                gap.amount += extra * 1000.0 / segment.size.get();
                shift += extra;
            }
        }
        self.used = self.area.width;
    }

    /// A `wrapped` line is justified; the last line and one ending in a hard break are not.
    fn push_line(&mut self, output: &mut LayoutBuffer, wrapped: bool) {
        if wrapped && self.align == TextAlign::Justify {
            self.justify(output);
        }
        let top = self.area.origin.y + self.height;
        let height = self.ascent + self.depth;
        let origin = Point {
            x: self.area.origin.x,
            y: top + self.ascent,
        };
        output.lines.push(PositionedLine {
            segments: self.segment_start..output.segments.len(),
            origin,
            top,
            bottom: top + height,
            available_width: self.area.width,
            text_width: self.used,
            align: self.align,
        });
        // The marker follows the first line with the same extent, so the page splitter never parts them.
        if let Some((segment, width, inside)) = self.marker.take() {
            let (x, align, available_width, text_width) = if inside {
                (origin.x, self.align, self.area.width, self.used)
            } else {
                (origin.x - width, TextAlign::Left, width, width)
            };
            output.lines.push(PositionedLine {
                segments: segment..segment + 1,
                origin: Point { x, y: origin.y },
                top,
                bottom: top + height,
                available_width,
                text_width,
                align,
            });
        }
        self.height += height;
        self.segment_start = output.segments.len();
        self.gap_start = output.gaps.len();
        self.used = Pt::ZERO;
        self.ascent = Pt::ZERO;
        self.depth = Pt::ZERO;
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
