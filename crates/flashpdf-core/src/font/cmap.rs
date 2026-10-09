//! TrueType `cmap` formats 4 and 12, flattened to sorted non-overlapping
//! ranges so a lookup is one binary search and the reverse map is one pass.

use std::collections::{BTreeMap, BTreeSet};

use super::truetype::{be_u16, be_u32, INVALID};

/// Bounds the flattened table; Unicode has fewer than 2^18 assigned characters.
const MAX_RANGES: usize = 1 << 18;

/// `start..=end` maps to consecutive glyphs from `gid`.
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct Range {
    start: u32,
    end: u32,
    gid: u16,
}

/// The one best Unicode subtable (platform 0, or 3 with encoding 1 or 10): format 12
/// before format 4, then platform 3 before 0, then the earlier record.
pub(super) fn parse(cmap: &[u8], glyphs: u16) -> Result<Vec<Range>, String> {
    let count = usize::from(be_u16(cmap, 2)?);
    let mut best: Option<((bool, bool), u16, &[u8])> = None;
    for index in 0..count {
        let record = 4 + index * 8;
        let platform = be_u16(cmap, record)?;
        let encoding = be_u16(cmap, record + 2)?;
        if platform != 0 && !(platform == 3 && matches!(encoding, 1 | 10)) {
            continue;
        }
        let offset = usize::try_from(be_u32(cmap, record + 4)?).map_err(|_| INVALID)?;
        let subtable = cmap.get(offset..).ok_or(INVALID)?;
        let format = be_u16(subtable, 0)?;
        let rank = (format != 12, platform != 3);
        if matches!(format, 4 | 12) && best.is_none_or(|(best, ..)| rank < best) {
            best = Some((rank, format, subtable));
        }
    }
    let mut ranges = Vec::new();
    match best {
        Some((_, 4, subtable)) => format4(subtable, glyphs, &mut ranges)?,
        Some((_, _, subtable)) => format12(subtable, glyphs, &mut ranges)?,
        None => {}
    }
    if ranges.is_empty() {
        return Err("unsupported TrueType cmap".into());
    }
    Ok(ranges)
}

fn follows(last: &Range, start: u32, gid: u16) -> bool {
    start == last.end + 1 && u32::from(last.gid) + (last.end - last.start) + 1 == u32::from(gid)
}

/// Keeps `ranges` sorted and disjoint: codes an earlier range maps keep that glyph.
fn push(ranges: &mut Vec<Range>, start: u32, end: u32, gid: u16) {
    let next = ranges.last().map_or(0, |last| last.end + 1);
    if end < next {
        return;
    }
    let skipped = next.saturating_sub(start);
    let (start, gid) = (start + skipped, gid + skipped as u16);
    match ranges.last_mut() {
        Some(last) if follows(last, start, gid) => last.end = end,
        _ => ranges.push(Range { start, end, gid }),
    }
}

fn format4(subtable: &[u8], glyphs: u16, ranges: &mut Vec<Range>) -> Result<(), String> {
    let length = usize::from(be_u16(subtable, 2)?);
    let subtable = subtable.get(..length).ok_or(INVALID)?;
    let segments = usize::from(be_u16(subtable, 6)? / 2);
    let ends = 14;
    let starts = ends + segments * 2 + 2;
    let deltas = starts + segments * 2;
    let offsets = deltas + segments * 2;
    // Visit each code once; overlapping segments could otherwise cost 8K segments x 64K codes.
    let mut next = 0_u16;
    for index in 0..segments {
        let end = be_u16(subtable, ends + index * 2)?;
        let start = be_u16(subtable, starts + index * 2)?;
        let delta = be_u16(subtable, deltas + index * 2)?;
        let range_offset = usize::from(be_u16(subtable, offsets + index * 2)?);
        for code in start.max(next)..=end {
            let gid = if range_offset == 0 {
                code.wrapping_add(delta)
            } else {
                let at = offsets + index * 2 + range_offset + usize::from(code - start) * 2;
                match be_u16(subtable, at)? {
                    0 => 0,
                    gid => gid.wrapping_add(delta),
                }
            };
            if gid >= glyphs {
                return Err(INVALID.into());
            }
            if gid != 0 {
                push(ranges, u32::from(code), u32::from(code), gid);
            }
        }
        next = next.max(end.saturating_add(1));
    }
    Ok(())
}

fn format12(subtable: &[u8], glyphs: u16, ranges: &mut Vec<Range>) -> Result<(), String> {
    let length = usize::try_from(be_u32(subtable, 4)?).map_err(|_| INVALID)?;
    let subtable = subtable.get(..length).ok_or(INVALID)?;
    let groups = usize::try_from(be_u32(subtable, 12)?).map_err(|_| INVALID)?;
    if groups > MAX_RANGES {
        return Err(INVALID.into());
    }
    for group in 0..groups {
        let at = 16 + group * 12;
        let mut start = be_u32(subtable, at)?;
        let end = be_u32(subtable, at + 4)?;
        let mut gid = be_u32(subtable, at + 8)?;
        if start > end || end > 0x10ffff {
            return Err(INVALID.into());
        }
        if gid == 0 {
            if start == end {
                continue;
            }
            start += 1;
            gid = 1;
        }
        let last = gid.checked_add(end - start).ok_or(INVALID)?;
        if last >= u32::from(glyphs) {
            return Err(INVALID.into());
        }
        push(ranges, start, end, gid as u16);
    }
    Ok(())
}

pub(crate) fn lookup(ranges: &[Range], character: char) -> Option<u16> {
    let code = character as u32;
    let index = ranges.partition_point(|range| range.start <= code);
    let range = ranges.get(index.checked_sub(1)?)?;
    (code <= range.end).then(|| (u32::from(range.gid) + (code - range.start)) as u16)
}

/// Lowest code point per used glyph, for `/ToUnicode`.
pub(crate) fn reverse(ranges: &[Range], used: &BTreeSet<u16>) -> BTreeMap<u16, char> {
    let mut map = BTreeMap::new();
    for range in ranges {
        let last = (u32::from(range.gid) + (range.end - range.start)) as u16;
        for gid in used.range(range.gid..=last) {
            let code = range.start + u32::from(gid - range.gid);
            if let Some(character) = char::from_u32(code) {
                map.entry(*gid).or_insert(character);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::{lookup, parse};

    /// One format 4 segment mapping `A` through `delta`, then the 0xffff terminator.
    fn cmap(delta: u16) -> Vec<u8> {
        let mut bytes = vec![0, 0, 0, 1, 0, 3, 0, 1, 0, 0, 0, 12];
        bytes.extend([0, 4, 0, 32, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0]);
        bytes.extend([0, 65, 255, 255, 0, 0, 0, 65, 255, 255]);
        bytes.extend(delta.to_be_bytes());
        bytes.extend([0, 1, 0, 0, 0, 0]);
        bytes
    }

    /// `(platform, encoding, offset into subtables)` records, then the subtables.
    fn table(records: &[(u16, u16, usize)], subtables: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0, 0];
        bytes.extend((records.len() as u16).to_be_bytes());
        for (platform, encoding, offset) in records {
            bytes.extend(platform.to_be_bytes());
            bytes.extend(encoding.to_be_bytes());
            bytes.extend(((4 + records.len() * 8 + offset) as u32).to_be_bytes());
        }
        bytes.extend(subtables);
        bytes
    }

    fn format12(groups: &[(u32, u32, u32)]) -> Vec<u8> {
        let mut bytes = vec![0, 12, 0, 0];
        bytes.extend(((16 + groups.len() * 12) as u32).to_be_bytes());
        bytes.extend([0; 4]);
        bytes.extend((groups.len() as u32).to_be_bytes());
        for (start, end, gid) in groups {
            for value in [start, end, gid] {
                bytes.extend(value.to_be_bytes());
            }
        }
        bytes
    }

    /// Format 4 with `segments` copies of one segment mapping codes `0..codes` through a zero glyph array.
    fn overlapping(segments: usize, codes: usize) -> Vec<u8> {
        let array = 16 + segments * 8;
        let mut bytes = vec![0; array + codes * 2];
        let length = bytes.len() as u16;
        bytes[..2].copy_from_slice(&4_u16.to_be_bytes());
        bytes[2..4].copy_from_slice(&length.to_be_bytes());
        bytes[6..8].copy_from_slice(&(segments as u16 * 2).to_be_bytes());
        for index in 0..segments {
            let end = 14 + index * 2;
            bytes[end..end + 2].copy_from_slice(&(codes as u16 - 1).to_be_bytes());
            let offset = 16 + segments * 6 + index * 2;
            bytes[offset..offset + 2].copy_from_slice(&((array - offset) as u16).to_be_bytes());
        }
        bytes
    }

    #[test]
    fn given_a_zero_or_out_of_range_cmap_glyph_when_parsing_then_rejects_it() {
        assert_eq!(
            parse(&cmap(0xffbf), 100).unwrap_err(),
            "unsupported TrueType cmap"
        );
        assert_eq!(parse(&cmap(35), 100), Err("invalid TrueType font".into()));
        let ranges = parse(&cmap(3), 100).unwrap();
        assert_eq!(lookup(&ranges, 'A'), Some(68));
        assert_eq!(lookup(&ranges, 'B'), None);
    }

    #[test]
    fn given_records_repeating_a_subtable_of_overlapping_segments_when_parsing_then_visits_each_code_once(
    ) {
        let records = vec![(3, 1, 0); usize::from(u16::MAX)];
        let cmap = table(&records, &overlapping(4_000, 16_000));
        assert_eq!(parse(&cmap, 1).unwrap_err(), "unsupported TrueType cmap");
    }

    #[test]
    fn given_bmp_and_full_unicode_subtables_when_parsing_then_uses_only_the_full_one() {
        let bmp = cmap(3)[12..].to_vec();
        let full = format12(&[(65, 65, 7)]);
        let cmap = table(&[(3, 1, 0), (3, 10, bmp.len())], &[bmp, full].concat());
        assert_eq!(lookup(&parse(&cmap, 100).unwrap(), 'A'), Some(7));
    }

    #[test]
    fn given_a_group_starting_inside_the_previous_one_when_parsing_then_keeps_its_tail() {
        let cmap = table(&[(3, 10, 0)], &format12(&[(65, 70, 10), (68, 75, 20)]));
        let ranges = parse(&cmap, 100).unwrap();
        assert_eq!(lookup(&ranges, 'D'), Some(13));
        assert_eq!(lookup(&ranges, 'H'), Some(24));
    }
}
