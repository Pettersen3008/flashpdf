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

/// Every Unicode subtable (platform 0, or 3 with encoding 1 or 10); on overlap the earlier record wins.
pub(super) fn parse(cmap: &[u8], glyphs: u16) -> Result<Vec<Range>, String> {
    let count = usize::from(be_u16(cmap, 2)?);
    let mut ranges = Vec::new();
    for index in 0..count {
        let record = 4 + index * 8;
        let platform = be_u16(cmap, record)?;
        let encoding = be_u16(cmap, record + 2)?;
        if platform != 0 && !(platform == 3 && matches!(encoding, 1 | 10)) {
            continue;
        }
        let offset = usize::try_from(be_u32(cmap, record + 4)?).map_err(|_| INVALID)?;
        let subtable = cmap.get(offset..).ok_or(INVALID)?;
        match be_u16(subtable, 0)? {
            4 => format4(subtable, glyphs, &mut ranges)?,
            12 => format12(subtable, glyphs, &mut ranges)?,
            _ => {}
        }
        if ranges.len() > MAX_RANGES {
            return Err(INVALID.into());
        }
    }
    if ranges.is_empty() {
        return Err("unsupported TrueType cmap".into());
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<Range> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => {}
            Some(last) if follows(last, range.start, range.gid) => last.end = range.end,
            _ => merged.push(range),
        }
    }
    Ok(merged)
}

fn follows(last: &Range, start: u32, gid: u16) -> bool {
    start == last.end + 1 && u32::from(last.gid) + (last.end - last.start) + 1 == u32::from(gid)
}

fn push(ranges: &mut Vec<Range>, start: u32, end: u32, gid: u16) {
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
    for index in 0..segments {
        let end = be_u16(subtable, ends + index * 2)?;
        let start = be_u16(subtable, starts + index * 2)?;
        let delta = be_u16(subtable, deltas + index * 2)?;
        let range_offset = usize::from(be_u16(subtable, offsets + index * 2)?);
        for code in start..=end {
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
    }
    Ok(())
}

fn format12(subtable: &[u8], glyphs: u16, ranges: &mut Vec<Range>) -> Result<(), String> {
    let length = usize::try_from(be_u32(subtable, 4)?).map_err(|_| INVALID)?;
    let subtable = subtable.get(..length).ok_or(INVALID)?;
    let groups = usize::try_from(be_u32(subtable, 12)?).map_err(|_| INVALID)?;
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
}
