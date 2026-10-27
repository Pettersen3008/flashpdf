//! Shaping of embedded fonts with harfrust, one word at a time. The layouter
//! breaks lines only between words, so a word is shaped once and never
//! reshaped. A shaped word depends only on the font bytes, its text and the
//! feature switches, so fonts registered again with the same bytes share one
//! word cache across renders, as a browser shares one per font face.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use harfrust::{
    BufferFlags, Direction, Feature, FontRef, GlyphBuffer, Script, ShapeOptions, ShapePlan,
    ShaperData, Tag, UnicodeBuffer,
};

use super::EmbeddedFont;

/// Words kept per font; the cache starts over past this, so memory stays bounded.
const CACHE_WORDS: usize = 4096;
/// Fonts whose bytes and caches outlive a render, oldest dropped first.
const FONTS_KEPT: usize = 8;
/// Their bytes together, past which older fonts drop even under `FONTS_KEPT`.
const FONT_BYTES_KEPT: usize = 32 * 1024 * 1024;

/// `font-kerning: none` turns off `kern`.
pub(crate) const NO_KERNING: u8 = 1;
/// Letter spacing turns off the optional ligatures, as CSS Text asks.
pub(crate) const NO_LIGATURES: u8 = 2;
/// Right to left: harfrust returns the glyphs in visual order and mirrors brackets.
pub(crate) const RTL: u8 = 4;

static FONTS: Mutex<Vec<Arc<Shaping>>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Glyph {
    pub(crate) gid: u16,
    /// Advance before this glyph beyond the previous glyph's `/W` width, in 1/1000 em.
    /// On the first glyph it is the glyph's own horizontal offset.
    pub(crate) gap: i32,
    /// This glyph's own baseline shift in 1/1000 em, up positive, for a mark.
    pub(crate) lift: i32,
}

#[derive(Debug, PartialEq)]
pub(crate) struct Shaped {
    pub(crate) glyphs: Vec<Glyph>,
    /// Advance after the last glyph beyond its `/W` width.
    pub(crate) trail: i32,
    /// The sum of the shaped advances.
    pub(crate) advance: i32,
    /// Clusters whose glyphs' `/ToUnicode` text is not their source text, as a glyph
    /// range and a byte range of the word, for `/ActualText`.
    pub(crate) actual: Vec<(u32, u32, u32, u32)>,
    /// Text for glyphs no code point of their own spells, such as a ligature, for `/ToUnicode`.
    pub(crate) names: Vec<(u16, Box<str>)>,
}

/// What a font's bytes keep across renders: harfrust's lookup caches and the shaped words.
pub(crate) struct Shaping {
    pub(crate) bytes: Arc<Vec<u8>>,
    data: ShaperData,
    words: Mutex<Words>,
}

#[derive(Default)]
struct Words {
    shaped: HashMap<Box<str>, Arc<Shaped>>,
    key: String,
    plans: Vec<((Script, u8), ShapePlan)>,
    buffer: Option<UnicodeBuffer>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shaping {
    /// The state shared by every font registered with exactly these bytes.
    pub(crate) fn of(bytes: Vec<u8>) -> Result<Arc<Self>, ()> {
        Self::kept(&mut lock(&FONTS), bytes)
    }

    /// Finds `bytes` among `fonts`, or adds them and drops the oldest past the bounds.
    fn kept(fonts: &mut Vec<Arc<Self>>, bytes: Vec<u8>) -> Result<Arc<Self>, ()> {
        if let Some(found) = fonts.iter().find(|font| **font.bytes == bytes) {
            return Ok(found.clone());
        }
        let data = ShaperData::new(&FontRef::new(&bytes).map_err(|_| ())?);
        let shaping = Arc::new(Self {
            bytes: Arc::new(bytes),
            data,
            words: Mutex::default(),
        });
        fonts.push(shaping.clone());
        while fonts.len() > FONTS_KEPT
            || fonts.len() > 1
                && fonts.iter().map(|font| font.bytes.len()).sum::<usize>() > FONT_BYTES_KEPT
        {
            fonts.remove(0);
        }
        Ok(shaping)
    }

    /// Shapes `text` in the script of its first letter, right to left under `RTL`, or names a
    /// character the font has no glyph for.
    pub(crate) fn shape(
        &self,
        font: &EmbeddedFont,
        text: &str,
        flags: u8,
    ) -> Result<Arc<Shaped>, char> {
        let mut words = lock(&self.words);
        let words = &mut *words;
        words.key.clear();
        words.key.push(char::from(flags));
        words.key.push_str(text);
        if let Some(shaped) = words.shaped.get(words.key.as_str()) {
            return Ok(shaped.clone());
        }
        let shaped = Arc::new(self.shape_uncached(words, font, text, flags)?);
        if words.shaped.len() >= CACHE_WORDS {
            words.shaped.clear();
        }
        words
            .shaped
            .insert(words.key.as_str().into(), shaped.clone());
        Ok(shaped)
    }

    fn shape_uncached(
        &self,
        words: &mut Words,
        font: &EmbeddedFont,
        text: &str,
        flags: u8,
    ) -> Result<Shaped, char> {
        let mut features = Vec::new();
        if flags & NO_KERNING != 0 {
            features.push(Feature::new(Tag::new(b"kern"), 0, ..));
        }
        if flags & NO_LIGATURES != 0 {
            features.push(Feature::new(Tag::new(b"liga"), 0, ..));
            features.push(Feature::new(Tag::new(b"clig"), 0, ..));
        }
        let mut buffer = words.buffer.take().unwrap_or_default();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let direction = if flags & RTL != 0 {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        };
        buffer.set_direction(direction);
        buffer.set_flags(BufferFlags::REMOVE_DEFAULT_IGNORABLES);
        let script = buffer.script();
        // `of` read these bytes as a font already.
        let face = FontRef::new(&self.bytes).expect("a parsed font");
        let shaper = self.data.shaper(&face).build();
        let plan = match words
            .plans
            .iter()
            .position(|(key, _)| *key == (script, flags))
        {
            Some(index) => index,
            None => {
                let plan = ShapePlan::new(&shaper, direction, Some(script), None, &features);
                words.plans.push(((script, flags), plan));
                words.plans.len() - 1
            }
        };
        let options = ShapeOptions::new()
            .plan(Some(&words.plans[plan].1))
            .features(&features);
        let glyphs = shaper.shape(buffer, options);
        let shaped = collect(font, text, &glyphs);
        words.buffer = Some(glyphs.clear());
        shaped
    }
}

/// Converts harfrust's glyphs to `/W` widths plus gaps in 1/1000 em, and names
/// the glyphs no code point of their own spells.
fn collect(font: &EmbeddedFont, text: &str, buffer: &GlyphBuffer) -> Result<Shaped, char> {
    let infos = buffer.glyph_infos();
    let positions = buffer.glyph_positions();
    let scale = font.scale;
    let units = |value: i32| (value as f32 * scale).round() as i32;
    let mut glyphs = Vec::with_capacity(infos.len());
    let mut previous: Option<(i32, i32, i32)> = None;
    let mut advance = 0;
    for (info, position) in infos.iter().zip(positions) {
        let gid = u16::try_from(info.glyph_id).unwrap_or(0);
        if gid == 0 {
            let cluster = &text[info.cluster as usize..];
            let first = cluster.chars().next().unwrap_or(' ');
            return Err(cluster
                .chars()
                .find(|character| font.glyph(*character).is_none())
                .unwrap_or(first));
        }
        let (step, offset, width) = (
            units(position.x_advance),
            units(position.x_offset),
            i32::from(font.advance(gid)),
        );
        let gap = previous.map_or(offset, |(step, before, width)| {
            step + offset - before - width
        });
        glyphs.push(Glyph {
            gid,
            gap,
            lift: units(position.y_offset),
        });
        advance += step;
        previous = Some((step, offset, width));
    }
    let trail = previous.map_or(0, |(step, offset, width)| step - offset - width);
    let mut starts: Vec<u32> = infos.iter().map(|info| info.cluster).collect();
    starts.sort_unstable();
    starts.dedup();
    let (mut actual, mut names) = (Vec::new(), Vec::<(u16, Box<str>)>::new());
    let mut first = 0;
    while first < infos.len() {
        let cluster = infos[first].cluster;
        let last = first
            + infos[first..]
                .iter()
                .take_while(|info| info.cluster == cluster)
                .count();
        let end = starts
            .get(starts.partition_point(|start| *start <= cluster))
            .map_or(text.len(), |end| *end as usize);
        let source = &text[cluster as usize..end];
        let mut characters = source.chars();
        let single = characters.next().filter(|_| characters.next().is_none());
        let gid = glyphs[first].gid;
        let alone = last - first == 1;
        let named = |names: &[(u16, Box<str>)], gid: u16| {
            names
                .iter()
                .find(|(named, _)| *named == gid)
                .map(|(_, name)| name.clone())
        };
        // A glyph shown for its own character needs nothing.
        if single.is_some_and(|character| alone && font.glyph(character) == Some(gid)) {
            first = last;
            continue;
        }
        // A ligature reads as all its characters, even where a code point such as
        // U+FB01 maps to it; an alternate or a cluster's first glyph only where none does.
        if (alone && single.is_none() || font.character(gid).is_none())
            && named(&names, gid).is_none()
        {
            names.push((gid, source.into()));
        }
        let mut shown = String::new();
        for glyph in &glyphs[first..last] {
            match named(&names, glyph.gid) {
                Some(name) => shown.push_str(&name),
                None => shown.extend(font.character(glyph.gid)),
            }
        }
        if shown != source {
            actual.push((first as u32, last as u32, cluster, end as u32));
        }
        first = last;
    }
    Ok(Shaped {
        glyphs,
        trail,
        advance,
        actual,
        names,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{lock, Shaping, CACHE_WORDS, FONTS_KEPT};
    use crate::font::EmbeddedFont;

    const ABEL: &[u8] =
        include_bytes!("../../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");

    #[test]
    fn given_the_same_bytes_registered_again_when_kept_then_both_share_one_shaping_until_eight_newer_fonts_push_it_out(
    ) {
        let mut fonts = Vec::new();
        let first = Shaping::kept(&mut fonts, ABEL.to_vec()).unwrap();
        assert!(Arc::ptr_eq(
            &first,
            &Shaping::kept(&mut fonts, ABEL.to_vec()).unwrap()
        ));
        for index in 0..FONTS_KEPT {
            let mut other = ABEL.to_vec();
            other.extend(index.to_be_bytes());
            Shaping::kept(&mut fonts, other).unwrap();
        }
        assert_eq!(fonts.len(), FONTS_KEPT);
        assert!(!Arc::ptr_eq(
            &first,
            &Shaping::kept(&mut fonts, ABEL.to_vec()).unwrap()
        ));
    }

    #[test]
    fn given_more_distinct_words_than_the_cache_holds_when_shaping_then_it_starts_over() {
        let font = EmbeddedFont::parse(ABEL.to_vec()).unwrap();
        let shaping = Shaping::kept(&mut Vec::new(), ABEL.to_vec()).unwrap();
        let first = shaping.shape(&font, "a", 0).unwrap();
        assert!(Arc::ptr_eq(&first, &shaping.shape(&font, "a", 0).unwrap()));
        for number in 0..CACHE_WORDS {
            shaping.shape(&font, &number.to_string(), 0).unwrap();
        }
        assert!(lock(&shaping.words).shaped.len() <= CACHE_WORDS);
        assert!(!Arc::ptr_eq(&first, &shaping.shape(&font, "a", 0).unwrap()));
    }
}
