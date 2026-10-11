//! Pair kerning of an embedded font: GPOS pair adjustment (lookup type 2,
//! formats 1 and 2, also inside extension lookups) under the `kern` feature
//! of the default script, else the `kern` table's format 0 subtables.
//! Parsing records subtable offsets only. A lookup is a few binary searches
//! over the font bytes, and the subtable cap bounds the work per pair.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::Range;

use super::truetype::table_range;

/// Subtables a font may contribute; real fonts use a handful, hostile ones thousands.
const MAX_SUBTABLES: usize = 128;
/// Lookup indices read from `kern` features, which may repeat one huge list.
const MAX_LOOKUPS: usize = 1024;

pub(crate) enum Kerning {
    None,
    /// Format 0 pair lists as (offset, pair count) inside the table.
    Kern(Range<usize>, Vec<(usize, usize)>),
    /// Pair adjustment subtable offsets inside the table in lookup order, each
    /// lookup ended by a 0, which is the header and never a subtable.
    Gpos(Range<usize>, Vec<usize>),
}

impl Kerning {
    pub(crate) fn parse(font: &[u8]) -> Self {
        if let Ok(range) = table_range(font, b"GPOS") {
            if let Some(subtables) = gpos(&font[range.clone()]).filter(|found| !found.is_empty()) {
                return Self::Gpos(range, subtables);
            }
        }
        if let Ok(range) = table_range(font, b"kern") {
            let subtables = kern(&font[range.clone()]);
            if !subtables.is_empty() {
                return Self::Kern(range, subtables);
            }
        }
        Self::None
    }

    /// The adjustment between two glyphs in font units; within a GPOS lookup the
    /// first subtable that covers the pair wins, and lookups add up.
    pub(crate) fn get(&self, font: &[u8], left: u16, right: u16) -> i32 {
        match self {
            Self::None => 0,
            Self::Kern(range, subtables) => {
                let table = &font[range.clone()];
                let key = u32::from(left) << 16 | u32::from(right);
                subtables
                    .iter()
                    .filter_map(|&(start, count)| {
                        let index = search(count, &|index| {
                            Some(u32_at(table, start + index * 6)?.cmp(&key))
                        })?;
                        i16_at(table, start + index * 6 + 4)
                    })
                    .map(i32::from)
                    .sum()
            }
            Self::Gpos(range, subtables) => {
                let table = &font[range.clone()];
                let (mut total, mut applied) = (0, false);
                for &subtable in subtables {
                    if subtable == 0 {
                        applied = false;
                    } else if !applied {
                        if let Some(value) = pair(table, subtable, left, right) {
                            total += i32::from(value);
                            applied = true;
                        }
                    }
                }
                total
            }
        }
    }
}

/// Subtables of the `kern` feature's pair lookups for DFLT, else dflt or latn, as HarfBuzz falls back.
fn gpos(table: &[u8]) -> Option<Vec<usize>> {
    let scripts = usize::from(u16_at(table, 4)?);
    let features = usize::from(u16_at(table, 6)?);
    let lookups = usize::from(u16_at(table, 8)?);
    let count = usize::from(u16_at(table, scripts)?);
    let script = [b"DFLT", b"dflt", b"latn"].iter().find_map(|tag| {
        (0..count).find_map(|index| {
            let record = scripts + 2 + index * 6;
            (table.get(record..record + 4)? == *tag).then_some(())?;
            Some(scripts + usize::from(u16_at(table, record + 4)?))
        })
    })?;
    let language = usize::from(u16_at(table, script)?);
    if language == 0 {
        return None;
    }
    let language = script + language;
    let required = u16_at(table, language + 2)?;
    let feature_count = u16_at(table, features)?;
    // A set keeps lookup order and drops repeats.
    let mut indices = BTreeSet::new();
    let mut reads = 0;
    let listed = usize::from(u16_at(table, language + 4)?);
    // The slot past the list is the required feature, 0xffff when there is none.
    for slot in 0..=listed {
        let feature = if slot < listed {
            u16_at(table, language + 6 + slot * 2)?
        } else {
            required
        };
        let record = features + 2 + usize::from(feature) * 6;
        if feature >= feature_count || table.get(record..record + 4)? != b"kern" {
            continue;
        }
        let feature = features + usize::from(u16_at(table, record + 4)?);
        for index in 0..usize::from(u16_at(table, feature + 2)?) {
            if reads == MAX_LOOKUPS {
                break;
            }
            reads += 1;
            indices.insert(u16_at(table, feature + 4 + index * 2)?);
        }
    }
    let lookup_count = u16_at(table, lookups)?;
    let mut budget = MAX_SUBTABLES;
    let mut result = Vec::new();
    for &index in indices.iter().filter(|index| **index < lookup_count) {
        let lookup = lookups + usize::from(u16_at(table, lookups + 2 + usize::from(index) * 2)?);
        let kind = u16_at(table, lookup)?;
        let start = result.len();
        for slot in 0..usize::from(u16_at(table, lookup + 4)?) {
            if budget == 0 {
                break;
            }
            budget -= 1;
            let subtable = lookup + usize::from(u16_at(table, lookup + 6 + slot * 2)?);
            match kind {
                2 => result.push(subtable),
                // An extension subtable holds a 32-bit offset to the real one.
                9 if u16_at(table, subtable + 2)? == 2 => result.push(
                    subtable.checked_add(usize::try_from(u32_at(table, subtable + 4)?).ok()?)?,
                ),
                _ => {}
            }
        }
        if result.len() > start {
            result.push(0);
        }
    }
    Some(result)
}

/// Format 0 subtables that kern horizontally, without minimum or cross-stream values.
fn kern(table: &[u8]) -> Vec<(usize, usize)> {
    let mut result = Vec::new();
    if u16_at(table, 0) != Some(0) {
        return result;
    }
    let mut offset = 4;
    for _ in 0..u16_at(table, 2).map_or(0, usize::from).min(MAX_SUBTABLES) {
        let (Some(length), Some(coverage), Some(pairs)) = (
            u16_at(table, offset + 2),
            u16_at(table, offset + 4),
            u16_at(table, offset + 6),
        ) else {
            break;
        };
        if coverage & 0xff07 == 1 {
            let start = offset + 14;
            // A long table overflows the 16-bit length, so trust the bytes that are there.
            let available = table.len().saturating_sub(start) / 6;
            result.push((start, usize::from(pairs).min(available)));
        }
        if length < 14 {
            break;
        }
        offset += usize::from(length);
    }
    result
}

/// The first glyph's x-advance adjustment, if the subtable covers the pair.
fn pair(table: &[u8], subtable: usize, left: u16, right: u16) -> Option<i16> {
    let covered = coverage(
        table,
        subtable + usize::from(u16_at(table, subtable + 2)?),
        left,
    )?;
    let (first, second) = (u16_at(table, subtable + 4)?, u16_at(table, subtable + 6)?);
    let size = |format: u16| 2 * (format & 0xff).count_ones() as usize;
    let record = match u16_at(table, subtable)? {
        1 => {
            if covered >= u16_at(table, subtable + 8)? {
                return None;
            }
            let set =
                subtable + usize::from(u16_at(table, subtable + 10 + usize::from(covered) * 2)?);
            let stride = 2 + size(first) + size(second);
            let index = search(usize::from(u16_at(table, set)?), &|index| {
                Some(u16_at(table, set + 2 + index * stride)?.cmp(&right))
            })?;
            set + 4 + index * stride
        }
        2 => {
            let class1 = class(
                table,
                subtable + usize::from(u16_at(table, subtable + 8)?),
                left,
            )?;
            let class2 = class(
                table,
                subtable + usize::from(u16_at(table, subtable + 10)?),
                right,
            )?;
            let (count1, count2) = (u16_at(table, subtable + 12)?, u16_at(table, subtable + 14)?);
            if class1 >= count1 || class2 >= count2 {
                return None;
            }
            let index = usize::from(class1) * usize::from(count2) + usize::from(class2);
            subtable + 16 + index * (size(first) + size(second))
        }
        _ => return None,
    };
    // X advance follows the x and y placements that are present.
    if first & 4 == 0 {
        return Some(0);
    }
    i16_at(table, record + size(first & 3))
}

/// The glyph's coverage index.
fn coverage(table: &[u8], at: usize, glyph: u16) -> Option<u16> {
    let count = usize::from(u16_at(table, at + 2)?);
    match u16_at(table, at)? {
        1 => search(count, &|index| {
            Some(u16_at(table, at + 4 + index * 2)?.cmp(&glyph))
        })
        .and_then(|index| u16::try_from(index).ok()),
        2 => {
            let index = search(count, &|index| range(table, at + 4 + index * 6, glyph))?;
            let record = at + 4 + index * 6;
            u16_at(table, record + 4)?.checked_add(glyph - u16_at(table, record)?)
        }
        _ => None,
    }
}

/// The glyph's class; a glyph the definition does not list is class 0.
fn class(table: &[u8], at: usize, glyph: u16) -> Option<u16> {
    match u16_at(table, at)? {
        1 => {
            let start = u16_at(table, at + 2)?;
            let offset = glyph.wrapping_sub(start);
            if glyph < start || offset >= u16_at(table, at + 4)? {
                return Some(0);
            }
            u16_at(table, at + 6 + usize::from(offset) * 2)
        }
        2 => {
            let count = usize::from(u16_at(table, at + 2)?);
            match search(count, &|index| range(table, at + 4 + index * 6, glyph)) {
                Some(index) => u16_at(table, at + 4 + index * 6 + 4),
                None => Some(0),
            }
        }
        _ => None,
    }
}

/// Where a `start, end` glyph range record lies relative to `glyph`.
fn range(table: &[u8], record: usize, glyph: u16) -> Option<Ordering> {
    let (start, end) = (u16_at(table, record)?, u16_at(table, record + 2)?);
    Some(if end < glyph {
        Ordering::Less
    } else if start > glyph {
        Ordering::Greater
    } else {
        Ordering::Equal
    })
}

/// Binary search over `count` sorted records; `compare` orders a record against the target.
/// Unsorted or truncated data finds nothing rather than looping.
fn search(count: usize, compare: &dyn Fn(usize) -> Option<Ordering>) -> Option<usize> {
    let (mut low, mut high) = (0, count);
    while low < high {
        let middle = low + (high - low) / 2;
        match compare(middle)? {
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
            Ordering::Equal => return Some(middle),
        }
    }
    None
}

fn u16_at(table: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        table.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn i16_at(table: &[u8], at: usize) -> Option<i16> {
    u16_at(table, at).map(|value| value as i16)
}

fn u32_at(table: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        table.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::{Kerning, MAX_SUBTABLES};
    use crate::font::EmbeddedFont;

    const ABEL: &[u8] =
        include_bytes!("../../../../packages/flashpdf/test/fixtures/Abel-Regular.ttf");

    /// Abel with one more table appended after a directory grown by one record.
    fn with_table(tag: &[u8; 4], table: &[u16]) -> Vec<u8> {
        let count = usize::from(u16::from_be_bytes([ABEL[4], ABEL[5]]));
        let mut font = ABEL[..12].to_vec();
        font[4..6].copy_from_slice(&(count as u16 + 1).to_be_bytes());
        for record in ABEL[12..12 + count * 16].chunks(16) {
            let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) + 16;
            font.extend(&record[..8]);
            font.extend(offset.to_be_bytes());
            font.extend(&record[12..]);
        }
        let data: Vec<u8> = table.iter().flat_map(|word| word.to_be_bytes()).collect();
        font.extend(tag);
        font.extend([0; 4]);
        font.extend((ABEL.len() as u32 + 16).to_be_bytes());
        font.extend((data.len() as u32).to_be_bytes());
        font.extend(&ABEL[12 + count * 16..]);
        font.extend(data);
        font
    }

    fn tag(name: &[u8; 2]) -> u16 {
        u16::from_be_bytes(*name)
    }

    /// DFLT's `kern` feature: lookup 0 holds `count` copies of a format 1 pair, A V -200;
    /// lookup 1 is an extension to a format 2 pair whose class 1 (T) before class 1 (o) is -100.
    fn gpos([a, v, t, o]: [u16; 4], count: u16) -> Vec<u16> {
        let mut words = vec![1, 0, 10, 30, 46];
        words.extend([1, tag(b"DF"), tag(b"LT"), 8, 4, 0, 0, 0xffff, 1, 0]);
        words.extend([1, tag(b"ke"), tag(b"rn"), 8, 0, 2, 0, 1]);
        words.extend([2, 6, 36 + 2 * count, 2, 0, count]);
        words.extend(std::iter::repeat_n(6 + 2 * count, usize::from(count)));
        words.extend([1, 12, 4, 0, 1, 18, 1, 1, a, 1, v, (-200_i16) as u16]);
        words.extend([9, 0, 1, 8, 1, 2, 0, 8]);
        words.extend([2, 24, 4, 0, 34, 44, 2, 2, 0, 0, 0, (-100_i16) as u16]);
        words.extend([2, 1, t, t, 0, 2, 1, t, t, 1, 1, o, 1, 1]);
        words
    }

    fn glyphs<const N: usize>(characters: [char; N]) -> [u16; N] {
        let font = EmbeddedFont::parse(ABEL.to_vec()).unwrap();
        characters.map(|character| font.glyph(character).unwrap())
    }

    #[test]
    fn given_abel_when_kerning_then_reads_its_format_0_kern_table_in_thousandths() {
        let font = EmbeddedFont::parse(ABEL.to_vec()).unwrap();
        let [a, v, t, o] = glyphs(['A', 'V', 'T', 'o']);
        // -41 and -82 of 2048 units.
        assert_eq!(font.kern(a, v), -20);
        assert_eq!(font.kern(t, o), -40);
        assert_eq!(font.kern(o, t), 0);
    }

    #[test]
    fn given_a_gpos_kern_feature_when_kerning_then_formats_1_and_2_apply_through_an_extension_instead_of_the_kern_table(
    ) {
        let [a, v, t, o, l, small_a] = glyphs(['A', 'V', 'T', 'o', 'L', 'a']);
        let font = EmbeddedFont::parse(with_table(b"GPOS", &gpos([a, v, t, o], 1))).unwrap();
        // -200 and -100 of 2048 units; class 0 kerns by zero, and the kern table's L T no longer counts.
        assert_eq!(font.kern(a, v), -98);
        assert_eq!(font.kern(t, o), -49);
        assert_eq!(font.kern(t, small_a), 0);
        assert_eq!(font.kern(l, t), 0);
    }

    #[test]
    fn given_a_gpos_lookup_with_thirty_thousand_subtables_when_parsing_then_keeps_the_first_128() {
        let pair = glyphs(['A', 'V', 'T', 'o']);
        let bytes = with_table(b"GPOS", &gpos(pair, 30_000));
        let kerning = Kerning::parse(&bytes);
        let Kerning::Gpos(_, subtables) = &kerning else {
            panic!("expected GPOS kerning");
        };
        assert_eq!(
            subtables.iter().filter(|offset| **offset != 0).count(),
            MAX_SUBTABLES
        );
        assert_eq!(kerning.get(&bytes, pair[0], pair[1]), -200);
        // The cap leaves the extension lookup out.
        assert_eq!(kerning.get(&bytes, pair[2], pair[3]), 0);
    }
}
