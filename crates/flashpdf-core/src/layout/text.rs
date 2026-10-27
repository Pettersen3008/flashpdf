use std::ops::Range;

use crate::document::Paragraph;
use crate::font::{Font, FontBook, NO_KERNING, NO_LIGATURES, RTL};
use crate::geometry::{LayoutArea, Point};
use crate::layout::output::{Gap, LayoutBuffer, Lift, PositionedLine, Segment, MARKER_SLOT};
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
    /// An embedded font, which harfrust shapes word by word.
    shapes: bool,
    /// `NO_KERNING` and `NO_LIGATURES`.
    features: u8,
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

    /// A fixed gap of `amount` thousandths of an em before the code at byte `at`, with its width.
    fn gap(&self, at: usize, amount: i32) -> (Gap, Pt) {
        let gap = Gap {
            at: at as u32,
            amount: amount as f32,
            stretch: false,
        };
        (gap, Pt(amount as f32 * self.size.get() / 1000.0))
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
    /// Its UAX #9 embedding level; odd is right to left.
    level: u8,
}

/// The collapsed space before the pending word, or the hyphen a soft hyphen
/// shows at a line end: its run, encoded bytes and width.
struct Space {
    run: usize,
    text: Range<usize>,
    width: Pt,
    level: u8,
}

/// Greedy word wrap over runs. Words end at ASCII whitespace and at the UAX #14
/// break opportunities of the paragraph's text, and may span runs; whitespace
/// collapses to one space owned by the run it first appears in, and lines trim
/// it at both ends.
/// ponytail: a space always breaks, so `a !` may wrap before `!`, which UAX #14 forbids.
pub(crate) struct ParagraphLayouter<'a> {
    fonts: &'a FontBook,
    runs: Vec<RunMetrics<'a>>,
    /// Characters of the pending piece in an embedded font, shaped when the piece ends.
    unshaped: String,
    /// UAX #9 level per byte of the paragraph text, empty when it has no right-to-left text.
    levels: Vec<u8>,
    /// The level of the character being read.
    level: u8,
    /// The level of each segment on the current line.
    line_levels: Vec<u8>,
    /// The next `append` continues a word placed glyph by glyph.
    continues: bool,
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
    /// The hyphen of a soft hyphen that ends the pending word.
    soft_hyphen: Option<Space>,
    /// The hyphen after the line's last word, shown only if the line wraps there.
    hyphen: Option<Space>,
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
                shapes: matches!(font, Font::Embedded(_)),
                features: if run.typography.kerning {
                    0
                } else {
                    NO_KERNING
                } | if run.typography.letter_spacing != 0.0 {
                    NO_LIGATURES
                } else {
                    0
                },
            });
        }
        let mut state = Self {
            fonts,
            runs,
            unshaped: String::new(),
            levels: levels(paragraph.text),
            level: 0,
            line_levels: Vec::new(),
            continues: false,
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
            soft_hyphen: None,
            hyphen: None,
            marker: None,
            widest: Pt::ZERO,
        };
        let mut breaks = unicode_linebreak::linebreaks(paragraph.text)
            .map(|(at, _)| at)
            .peekable();
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
            let base = paragraph.text.len() - rest.len() - text.len();
            state.run(index, base, text, &mut breaks, output)?;
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
        base: usize,
        text: &str,
        breaks: &mut std::iter::Peekable<impl Iterator<Item = usize>>,
        output: &mut LayoutBuffer,
    ) -> Result<(), LayoutError> {
        let nowrap = self.runs[index].typography.nowrap;
        let mut piece: Option<Piece> = None;
        for (offset, character) in text.char_indices() {
            // A level change ends the piece, so each piece shapes in one direction.
            let level = self.levels.get(base + offset).copied().unwrap_or(0);
            if level != self.level {
                self.close(index, &mut piece, output)?;
                self.level = level;
            }
            if character.is_ascii_whitespace() {
                self.close(index, &mut piece, output)?;
                self.word(output)?;
                self.hyphen = None;
                if self.has_word && self.space.is_none() {
                    let start = output.text.len();
                    let metrics = &self.runs[index];
                    let width = metrics.encode(' ', &mut output.text)?;
                    self.space = Some(Space {
                        run: index,
                        text: start..output.text.len(),
                        width: width + Pt(metrics.typography.word_spacing),
                        level,
                    });
                }
                continue;
            }
            while breaks.next_if(|at| *at < base + offset).is_some() {}
            if !nowrap && breaks.next_if_eq(&(base + offset)).is_some() {
                self.close(index, &mut piece, output)?;
                self.word(output)?;
            }
            match character {
                // A soft hyphen shows as a hyphen at a line end and never otherwise.
                '\u{ad}' if !nowrap => {
                    self.close(index, &mut piece, output)?;
                    let start = output.text.len();
                    let width = self.runs[index].encode('-', &mut output.text)?;
                    self.soft_hyphen = Some(Space {
                        run: index,
                        text: start..output.text.len(),
                        width,
                        level,
                    });
                    self.word(output)?;
                    continue;
                }
                // Zero-width break and bidi controls: UAX #14 and #9 read them, no glyph shows.
                '\u{ad}' | '\u{200b}' | '\u{2060}' | '\u{200e}' | '\u{200f}' => continue,
                '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => continue,
                _ => {}
            }
            // Other whitespace keeps its own glyph and gap, so word spacing and justification find it.
            if self.runs[index].shapes && !character.is_whitespace() {
                self.unshaped.push(character);
                continue;
            }
            self.shape(index, &mut piece, output)?;
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
                level,
            });
            if slot.start != start {
                slot.width += lead;
            }
            slot.end = output.text.len();
            slot.width += width;
            // A built-in font's codes stay in logical order, so right to left each is its own piece.
            if level % 2 == 1 {
                self.pieces.extend(piece.take());
            }
        }
        self.close(index, &mut piece, output)
    }

    /// Ends the pending word's piece in this run.
    fn close(
        &mut self,
        run: usize,
        piece: &mut Option<Piece>,
        output: &mut LayoutBuffer,
    ) -> Result<(), LayoutError> {
        self.shape(run, piece, output)?;
        self.pieces.extend(piece.take());
        Ok(())
    }

    /// Shapes the characters waiting in `unshaped` and appends their glyphs to the piece.
    /// Each glyph's advance beyond its `/W` width, as kerning, becomes a gap before the next code.
    fn shape(
        &mut self,
        run: usize,
        piece: &mut Option<Piece>,
        output: &mut LayoutBuffer,
    ) -> Result<(), LayoutError> {
        if self.unshaped.is_empty() {
            return Ok(());
        }
        let metrics = &self.runs[run];
        let shaped = self
            .fonts
            .shape(
                metrics.id,
                &self.unshaped,
                metrics.features | if self.level % 2 == 1 { RTL } else { 0 },
            )
            .map_err(LayoutError::MissingGlyph)?;
        let start = output.text.len();
        let lead = shaped.glyphs.first().map_or(0, |glyph| glyph.gap);
        let (_, lead_width) = metrics.gap(start, lead);
        if lead != 0 {
            push_gap(&mut self.gaps, metrics.gap(start, lead));
        }
        for glyph in &shaped.glyphs {
            let at = output.text.len();
            if at != start && glyph.gap != 0 {
                push_gap(&mut self.gaps, metrics.gap(at, glyph.gap));
            }
            if glyph.lift != 0 {
                output.lifts.push(Lift {
                    at: at as u32,
                    amount: glyph.lift as f32,
                });
            }
            output.text.extend(glyph.gid.to_be_bytes());
        }
        if shaped.trail != 0 {
            push_gap(&mut self.gaps, metrics.gap(output.text.len(), shaped.trail));
        }
        for &(first, last, from, to) in &shaped.actual {
            let codes = start as u32 + first * 2..start as u32 + last * 2;
            output
                .actual
                .push((codes, self.unshaped[from as usize..to as usize].into()));
        }
        let (_, inner) = metrics.gap(0, shaped.advance - lead);
        let width = inner + Pt(metrics.typography.letter_spacing * shaped.glyphs.len() as f32);
        self.unshaped.clear();
        self.previous = None;
        let slot = piece.get_or_insert(Piece {
            run,
            start,
            end: start,
            width: Pt::ZERO,
            lead: lead_width,
            level: self.level,
        });
        if slot.start != start {
            slot.width += lead_width;
        }
        slot.end = output.text.len();
        slot.width += width;
        Ok(())
    }

    /// The kerning before the glyph at `code`, after the word's previous glyph
    /// in the same font and size. A space, a break opportunity, or a hard break
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
        let ending = self.soft_hyphen.take();
        if self.pieces.is_empty() {
            return Ok(());
        }
        let pieces = std::mem::take(&mut self.pieces);
        let mut pending = std::mem::take(&mut self.gaps);
        let width = pieces
            .iter()
            .fold(Pt::ZERO, |total, piece| total + piece.lead + piece.width);
        let space = self.space.take();
        // A word before a soft hyphen keeps room for the hyphen, so a line wrapping there fits.
        // ponytail: it may wrap one word early, when the next word would have fit on this line.
        let reserve = ending.as_ref().map_or(Pt::ZERO, |hyphen| hyphen.width);
        if width + reserve > self.widest {
            self.widest = width + reserve;
        }
        let gap = space.as_ref().map_or(Pt::ZERO, |space| space.width);
        let started = self.has_word || self.used > Pt::ZERO;
        let overflow = self.used + gap + width + reserve > self.area.width;
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
                    self.continues = position != piece.start;
                    self.append(piece.run, code, glyph, piece.level, output);
                    self.continues = false;
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
                self.append(space.run, space.text, space.width, space.level, output);
            }
            for piece in &pieces {
                self.used += piece.lead;
                self.append(
                    piece.run,
                    piece.start..piece.end,
                    piece.width,
                    piece.level,
                    output,
                );
            }
            output.gaps.extend(pending.drain(..).map(|(gap, _)| gap));
        }
        self.pieces = pieces;
        self.pieces.clear();
        self.gaps = pending;
        self.gaps.clear();
        self.has_word = true;
        self.hyphen = ending;
        Ok(())
    }

    fn append(
        &mut self,
        run: usize,
        text: Range<usize>,
        width: Pt,
        level: u8,
        output: &mut LayoutBuffer,
    ) {
        // Right to left, each word and space is its own segment so the line can reverse them,
        // but a word placed glyph by glyph stays one.
        let joins =
            self.line_levels.last() == Some(&level) && (level.is_multiple_of(2) || self.continues);
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
                // An unshown soft hyphen's bytes split the text, so the segment ends there.
                if on_line
                    && last.text.end == text.start
                    && last.font == metrics.id
                    && last.size == metrics.size
                    && last.color == metrics.color
                    && last.letter_spacing == letter_spacing
                    && last.oblique == oblique
                    && last.rise == rise
                    && joins =>
            {
                last.text.end = text.end;
            }
            _ => {
                self.line_levels.push(level);
                output.segments.push(Segment {
                text,
                x: self.used,
                font: metrics.id,
                size: metrics.size,
                color: metrics.color,
                letter_spacing,
                oblique,
                rise,
                shape: 0,
            });
            }
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

    /// Puts the line's segments in visual order by UAX #9 rule L2: from the highest level
    /// down to the lowest odd one, each run of segments at that level or above reverses.
    /// Each keeps its width, so the line keeps its extent.
    fn reorder(&mut self, output: &mut LayoutBuffer) {
        let segments = &mut output.segments[self.segment_start..];
        let start = segments[0].x;
        let widths: Vec<Pt> = (0..segments.len())
            .map(|index| {
                segments.get(index + 1).map_or(self.used, |next| next.x) - segments[index].x
            })
            .collect();
        let mut order: Vec<usize> = (0..segments.len()).collect();
        let levels = &self.line_levels;
        let highest = levels.iter().copied().max().unwrap_or(0);
        let lowest = levels
            .iter()
            .copied()
            .filter(|level| level % 2 == 1)
            .min()
            .unwrap_or(1);
        for level in (lowest..=highest).rev() {
            let mut index = 0;
            while index < order.len() {
                let run = order[index..]
                    .iter()
                    .take_while(|segment| levels[**segment] >= level)
                    .count();
                order[index..index + run].reverse();
                index += run.max(1);
            }
        }
        let mut x = start;
        let mut placed: Vec<Segment> = Vec::with_capacity(segments.len());
        for &index in &order {
            let mut segment = segments[index].clone();
            segment.x = x;
            x += widths[index];
            placed.push(segment);
        }
        for (slot, segment) in segments.iter_mut().zip(placed) {
            *slot = segment;
        }
        self.line_levels = order.iter().map(|index| levels[*index]).collect();
    }

    /// A `wrapped` line is justified; the last line and one ending in a hard break are not.
    fn push_line(&mut self, output: &mut LayoutBuffer, wrapped: bool) {
        if let Some(hyphen) = self.hyphen.take().filter(|_| wrapped) {
            self.append(hyphen.run, hyphen.text, hyphen.width, hyphen.level, output);
        }
        if wrapped && self.align == TextAlign::Justify {
            self.justify(output);
        }
        if self.line_levels.iter().any(|level| level % 2 == 1) {
            self.reorder(output);
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
        self.line_levels.clear();
    }
}

/// UAX #9 levels per byte, with the base direction from the first strong character,
/// or none when nothing in `text` reads right to left.
fn levels(text: &str) -> Vec<u8> {
    use unicode_bidi::BidiClass::{AL, AN, R, RLE, RLI, RLO};
    let rtl = text.chars().any(|character| {
        !character.is_ascii()
            && matches!(
                unicode_bidi::bidi_class(character),
                R | AL | AN | RLE | RLO | RLI
            )
    });
    if !rtl {
        return Vec::new();
    }
    let info = unicode_bidi::BidiInfo::new(text, None);
    info.levels.iter().map(|level| level.number()).collect()
}

/// Adds a fixed gap, merged with one already at the same code.
fn push_gap(gaps: &mut Vec<(Gap, Pt)>, (gap, width): (Gap, Pt)) {
    match gaps.last_mut() {
        Some((last, total)) if last.at == gap.at && !last.stretch => {
            last.amount += gap.amount;
            *total += width;
        }
        _ => gaps.push((gap, width)),
    }
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
