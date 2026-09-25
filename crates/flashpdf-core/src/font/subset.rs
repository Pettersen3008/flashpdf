//! Sparse TrueType subset: glyph ids stay as they are, so `/W`, the show
//! strings and `/ToUnicode` need no remapping; unused glyphs become
//! zero-length `loca` entries and `numGlyphs` shrinks to the last used one.

use std::collections::BTreeSet;

use super::truetype::{be_i16, be_u16, be_u32, ttf_table};

/// `None` on any malformed table; the caller then embeds the whole file.
pub(super) fn subset(font: &[u8], used: &BTreeSet<u16>) -> Option<Vec<u8>> {
    let table = |tag: &[u8; 4]| ttf_table(font, tag).ok();
    let (head, hhea, hmtx, maxp, loca, glyf) = (
        table(b"head")?,
        table(b"hhea")?,
        table(b"hmtx")?,
        table(b"maxp")?,
        table(b"loca")?,
        table(b"glyf")?,
    );
    let glyphs = be_u16(maxp, 4).ok()?;
    let metrics = usize::from(be_u16(hhea, 34).ok()?);
    let long = be_i16(head, 50).ok()? != 0;
    let outline = |gid: usize| -> Option<&[u8]> {
        let offset = |index: usize| {
            if long {
                be_u32(loca, index * 4).ok().map(|value| value as usize)
            } else {
                be_u16(loca, index * 2)
                    .ok()
                    .map(|value| usize::from(value) * 2)
            }
        };
        glyf.get(offset(gid)?..offset(gid + 1)?)
    };

    let mut keep = used.clone();
    keep.insert(0);
    let mut pending: Vec<u16> = keep.iter().copied().collect();
    while let Some(gid) = pending.pop() {
        if gid >= glyphs {
            return None;
        }
        for component in components(outline(usize::from(gid))?)? {
            if keep.insert(component) {
                pending.push(component);
            }
        }
    }
    let count = usize::from(*keep.last()?) + 1;

    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity((count + 1) * 4);
    let mut new_hmtx = Vec::with_capacity(count * 4);
    for gid in 0..count {
        new_loca.extend((new_glyf.len() as u32).to_be_bytes());
        if !keep.contains(&(gid as u16)) {
            new_hmtx.extend([0; 4]);
            continue;
        }
        new_glyf.extend(outline(gid)?);
        new_glyf.resize(new_glyf.len().next_multiple_of(4), 0);
        let metric = gid.min(metrics.checked_sub(1)?) * 4;
        new_hmtx.extend(hmtx.get(metric..metric + 2)?);
        let lsb = if gid < metrics {
            metric + 2
        } else {
            metrics * 4 + (gid - metrics) * 2
        };
        new_hmtx.extend(hmtx.get(lsb..lsb + 2).unwrap_or(&[0, 0]));
    }
    new_loca.extend((new_glyf.len() as u32).to_be_bytes());

    let mut new_head = head.get(..54)?.to_vec();
    new_head[8..12].fill(0);
    new_head[50..52].copy_from_slice(&1_i16.to_be_bytes());
    let mut new_hhea = hhea.get(..36)?.to_vec();
    new_hhea[34..36].copy_from_slice(&(count as u16).to_be_bytes());
    let mut new_maxp = maxp.get(..6)?.to_vec();
    new_maxp.extend(&maxp[6..]);
    new_maxp[4..6].copy_from_slice(&(count as u16).to_be_bytes());
    let mut post = vec![0, 3, 0, 0];
    post.extend(
        table(b"post")
            .and_then(|post| post.get(4..32))
            .unwrap_or(&[0; 28][..]),
    );

    let mut tables = vec![
        (b"glyf", new_glyf),
        (b"head", new_head),
        (b"hhea", new_hhea),
        (b"hmtx", new_hmtx),
        (b"loca", new_loca),
        (b"maxp", new_maxp),
        (b"post", post),
    ];
    tables.extend(
        [b"cvt ", b"fpgm", b"prep"]
            .into_iter()
            .filter_map(|tag| Some((tag, table(tag)?.to_vec()))),
    );
    tables.sort_by_key(|(tag, _)| *tag);
    let selector = tables.len().ilog2() as u16;
    let search_range = 16_u16 << selector;
    let mut out = vec![0, 1, 0, 0];
    out.extend((tables.len() as u16).to_be_bytes());
    out.extend(search_range.to_be_bytes());
    out.extend(selector.to_be_bytes());
    out.extend((tables.len() as u16 * 16 - search_range).to_be_bytes());
    let mut offset = out.len() + tables.len() * 16;
    for (tag, data) in &tables {
        out.extend(*tag);
        out.extend(checksum(data).to_be_bytes());
        out.extend((offset as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }
    for (_, data) in &tables {
        out.extend(data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    Some(out)
}

/// Glyph ids a composite glyph references; empty for simple glyphs.
fn components(outline: &[u8]) -> Option<Vec<u16>> {
    let mut found = Vec::new();
    if outline.len() < 10 || be_i16(outline, 0).ok()? >= 0 {
        return Some(found);
    }
    let mut at = 10;
    loop {
        let flags = be_u16(outline, at).ok()?;
        found.push(be_u16(outline, at + 2).ok()?);
        at += 4 + if flags & 1 != 0 { 4 } else { 2 };
        at += match flags & 0xc8 {
            0x08 => 2,
            0x40 => 4,
            0x80 => 8,
            _ => 0,
        };
        if flags & 0x20 == 0 {
            return Some(found);
        }
    }
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0_u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// Six-letter subset prefix from an FNV-1a hash of the used glyph set.
pub(super) fn tag(used: &BTreeSet<u16>) -> String {
    let mut hash = 0x811c_9dc5_u32;
    for byte in used.iter().flat_map(|gid| gid.to_be_bytes()) {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    (0..6)
        .map(|_| {
            let letter = char::from(b'A' + (hash % 26) as u8);
            hash /= 26;
            letter
        })
        .collect()
}
