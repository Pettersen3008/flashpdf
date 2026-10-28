//! Sparse CFF subset for a CIDFontType0C program. Glyph ids stay as they are, like the
//! TrueType subset: an unused glyph becomes a bare `endchar` and an unused subroutine a bare
//! `return`, so no charstring needs renumbering. The output is always CID-keyed with an
//! identity charset, so a show string's glyph ids are its CIDs; a name-keyed font gains an
//! `Adobe-Identity` ROS and one Font DICT around its Private DICT.

use std::collections::BTreeSet;
use std::ops::Range;

const CHARSET: u16 = 15;
const ENCODING: u16 = 16;
const CHAR_STRINGS: u16 = 17;
const PRIVATE: u16 = 18;
const SUBRS: u16 = 19;
const ROS: u16 = 0x0c1e;
const CID_COUNT: u16 = 0x0c22;
const FD_ARRAY: u16 = 0x0c24;
const FD_SELECT: u16 = 0x0c25;
const CHARSTRING_TYPE: u16 = 0x0c06;
const UNIQUE_ID: u16 = 13;
const XUID: u16 = 0x0c0e;

/// One DICT entry: its operator, the raw operand bytes, and the operands read as integers.
struct Entry<'a> {
    op: u16,
    raw: &'a [u8],
    args: Vec<i32>,
}

/// A parsed `CFF ` table, checked well enough that subsetting cannot fail.
pub(super) struct Cff<'a> {
    data: &'a [u8],
    names: Range<usize>,
    top: Vec<Entry<'a>>,
    strings: Index<'a>,
    global: Index<'a>,
    glyphs: Index<'a>,
    /// Per Font DICT: its entries other than `Private`, its Private DICT entries other than
    /// `Subrs`, and its local subroutines.
    fonts: Vec<(Vec<Entry<'a>>, Vec<Entry<'a>>, Index<'a>)>,
    /// The Font DICT of each glyph; empty for a name-keyed font, whose glyphs all use the one.
    select: Vec<u8>,
}

#[derive(Clone, Copy)]
struct Index<'a> {
    data: &'a [u8],
    /// Where the offset array starts, the offset size, and the item count.
    offsets: usize,
    size: usize,
    count: usize,
    /// The byte after the INDEX.
    end: usize,
}

impl<'a> Index<'a> {
    fn read(data: &'a [u8], at: usize) -> Option<Self> {
        let count = usize::from(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?));
        if count == 0 {
            return Some(Self {
                data,
                offsets: at + 3,
                size: 1,
                count,
                end: at + 2,
            });
        }
        let size = usize::from(*data.get(at + 2)?);
        if !(1..=4).contains(&size) {
            return None;
        }
        let index = Self {
            data,
            offsets: at + 3,
            size,
            count,
            end: 0,
        };
        let base = at + 2 + (count + 1) * size;
        let end = base.checked_add(index.offset(count)?)?;
        // Offsets start at 1 and must not run backwards, or an item would slice past its end.
        let mut previous = 1;
        for item in 0..=count {
            let offset = index.offset(item)?;
            if offset < previous || base + offset > data.len() {
                return None;
            }
            previous = offset;
        }
        Some(Self { end, ..index })
    }

    fn offset(&self, item: usize) -> Option<usize> {
        let at = self.offsets + item * self.size;
        let bytes = self.data.get(at..at + self.size)?;
        Some(
            bytes
                .iter()
                .fold(0, |sum, byte| sum << 8 | usize::from(*byte)),
        )
    }

    fn get(&self, item: usize) -> Option<&'a [u8]> {
        if item >= self.count {
            return None;
        }
        let base = self.offsets - 1 + (self.count + 1) * self.size;
        self.data
            .get(base + self.offset(item)?..base + self.offset(item + 1)?)
    }
}

/// An INDEX of `items`, with four-byte offsets.
fn index<'b>(items: impl ExactSizeIterator<Item = &'b [u8]>, out: &mut Vec<u8>) {
    let count = items.len();
    out.extend((count as u16).to_be_bytes());
    if count == 0 {
        return;
    }
    out.push(4);
    let start = out.len();
    out.resize(start + (count + 1) * 4, 0);
    let mut offset = 1_u32;
    out[start..start + 4].copy_from_slice(&offset.to_be_bytes());
    for (item, bytes) in items.enumerate() {
        offset += bytes.len() as u32;
        out[start + (item + 1) * 4..start + (item + 2) * 4].copy_from_slice(&offset.to_be_bytes());
        out.extend(bytes);
    }
}

fn dict(data: &[u8]) -> Option<Vec<Entry<'_>>> {
    let mut entries = Vec::new();
    let (mut at, mut start) = (0, 0);
    let mut args = Vec::new();
    while at < data.len() {
        let byte = data[at];
        match byte {
            0..=21 => {
                let op = if byte == 12 {
                    at += 1;
                    0x0c00 | u16::from(*data.get(at)?)
                } else {
                    u16::from(byte)
                };
                let raw = &data[start..at - usize::from(byte == 12)];
                entries.push(Entry {
                    op,
                    raw,
                    args: std::mem::take(&mut args),
                });
                at += 1;
                start = at;
            }
            28 => {
                args.push(i32::from(i16::from_be_bytes(
                    data.get(at + 1..at + 3)?.try_into().ok()?,
                )));
                at += 3;
            }
            29 => {
                args.push(i32::from_be_bytes(
                    data.get(at + 1..at + 5)?.try_into().ok()?,
                ));
                at += 5;
            }
            // A real number: nibbles up to an 0xf one; only its presence matters here.
            30 => {
                at += 1;
                while data.get(at)? & 0x0f != 0x0f && data[at] >> 4 != 0x0f {
                    at += 1;
                }
                at += 1;
                args.push(0);
            }
            32..=246 => {
                args.push(i32::from(byte) - 139);
                at += 1;
            }
            247..=250 => {
                args.push((i32::from(byte) - 247) * 256 + i32::from(*data.get(at + 1)?) + 108);
                at += 2;
            }
            251..=254 => {
                args.push(-(i32::from(byte) - 251) * 256 - i32::from(*data.get(at + 1)?) - 108);
                at += 2;
            }
            _ => return None,
        }
    }
    Some(entries)
}

fn find<'e>(entries: &'e [Entry], op: u16) -> Option<&'e [i32]> {
    entries
        .iter()
        .find(|entry| entry.op == op)
        .map(|entry| entry.args.as_slice())
}

fn offset(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

impl<'a> Cff<'a> {
    pub(super) fn parse(data: &'a [u8]) -> Option<Self> {
        if data.first() != Some(&1) {
            return None;
        }
        let names = Index::read(data, usize::from(*data.get(2)?))?;
        let tops = Index::read(data, names.end)?;
        let strings = Index::read(data, tops.end)?;
        let global = Index::read(data, strings.end)?;
        let top = dict(tops.get(0)?)?;
        if find(&top, CHARSTRING_TYPE).is_some_and(|kind| kind != [2]) {
            return None;
        }
        let glyphs = Index::read(data, offset(*find(&top, CHAR_STRINGS)?.first()?)?)?;
        let private = |args: &[i32]| -> Option<(Vec<Entry<'a>>, Index<'a>)> {
            let [size, at] = args else { return None };
            let (size, at) = (offset(*size)?, offset(*at)?);
            let entries = dict(data.get(at..at.checked_add(size)?)?)?;
            let subrs = match find(&entries, SUBRS) {
                Some([own]) => Index::read(data, at.checked_add(offset(*own)?)?)?,
                Some(_) => return None,
                None => Index::read(&[0, 0], 0)?,
            };
            let entries = entries
                .into_iter()
                .filter(|entry| entry.op != SUBRS)
                .collect();
            Some((entries, subrs))
        };
        let cid = find(&top, ROS).is_some();
        let (fonts, select) = if cid {
            let array = Index::read(data, offset(*find(&top, FD_ARRAY)?.first()?)?)?;
            let mut fonts = Vec::with_capacity(array.count);
            for item in 0..array.count {
                let font = dict(array.get(item)?)?;
                let (entries, subrs) = private(find(&font, PRIVATE)?)?;
                let font = font
                    .into_iter()
                    .filter(|entry| entry.op != PRIVATE)
                    .collect();
                fonts.push((font, entries, subrs));
            }
            let at = offset(*find(&top, FD_SELECT)?.first()?)?;
            let select = fd_select(data, at, glyphs.count)?;
            if select.iter().any(|fd| usize::from(*fd) >= fonts.len()) {
                return None;
            }
            (fonts, select)
        } else {
            let (entries, subrs) = private(find(&top, PRIVATE)?)?;
            (vec![(Vec::new(), entries, subrs)], Vec::new())
        };
        Some(Self {
            data,
            names: usize::from(data[2])..names.end,
            top,
            strings,
            global,
            glyphs,
            fonts,
            select,
        })
    }

    fn fd(&self, gid: usize) -> usize {
        self.select.get(gid).map_or(0, |fd| usize::from(*fd))
    }

    /// The subset showing `used` plus glyph 0; every glyph under `None`.
    pub(super) fn subset(&self, used: Option<&BTreeSet<u16>>) -> Vec<u8> {
        let count = self.glyphs.count;
        let keep: Vec<bool> = (0..count)
            .map(|gid| used.is_none_or(|used| gid == 0 || used.contains(&(gid as u16))))
            .collect();
        // Subroutines the kept glyphs call, per Font DICT and global; all of them when a
        // charstring cannot be followed.
        let mut global = vec![used.is_none(); self.global.count];
        let mut local: Vec<Vec<bool>> = self
            .fonts
            .iter()
            .map(|(_, _, subrs)| vec![used.is_none(); subrs.count])
            .collect();
        if used.is_some() {
            for gid in (0..count).filter(|gid| keep[*gid]) {
                let fd = self.fd(gid);
                let mut walk = Walk {
                    global: (self.global, &mut global),
                    local: (self.fonts[fd].2, &mut local[fd]),
                    stack: Vec::new(),
                    stems: 0,
                    seen_width: false,
                };
                let followed = self
                    .glyphs
                    .get(gid)
                    .is_some_and(|charstring| walk.run(charstring, 0).is_some());
                if !followed {
                    global.fill(true);
                    local.iter_mut().for_each(|marks| marks.fill(true));
                    break;
                }
            }
        }

        let mut strings: Vec<&[u8]> = (0..self.strings.count)
            .filter_map(|item| self.strings.get(item))
            .collect();
        let mut top = Vec::new();
        match self.top.iter().find(|entry| entry.op == ROS) {
            Some(ros) => top.extend(ros.raw),
            None => {
                let sid = 391 + strings.len() as i32;
                strings.extend([&b"Adobe"[..], b"Identity"]);
                for value in [sid, sid + 1, 0] {
                    integer(value, &mut top);
                }
            }
        }
        top.extend(op(ROS));
        for entry in &self.top {
            // Offsets are rewritten below; UniqueID and XUID would name the full font.
            if !matches!(
                entry.op,
                ROS | CHARSET
                    | ENCODING
                    | CHAR_STRINGS
                    | PRIVATE
                    | FD_ARRAY
                    | FD_SELECT
                    | CID_COUNT
                    | UNIQUE_ID
                    | XUID
            ) {
                top.extend(entry.raw);
                top.extend(op(entry.op));
            }
        }
        integer(count as i32, &mut top);
        top.extend(op(CID_COUNT));
        // Five-byte integers, so the Top DICT's length is known before the offsets are.
        let mut patches = Vec::new();
        for operator in [CHARSET, CHAR_STRINGS, FD_ARRAY, FD_SELECT] {
            patches.push(top.len() + 1);
            integer(0, &mut top);
            top.extend(op(operator));
        }

        let mut out = vec![1, 0, 4, 4];
        out.extend(&self.data[self.names.clone()]);
        index([top.as_slice()].into_iter(), &mut out);
        let top_at = out.len() - top.len();
        index(strings.into_iter(), &mut out);
        index(pick(self.global, &global, RETURN), &mut out);

        let charset = out.len();
        out.push(2);
        out.extend(1_u16.to_be_bytes());
        out.extend((count.saturating_sub(2) as u16).to_be_bytes());
        let select = out.len();
        let mut ranges: Vec<(u16, u8)> = Vec::new();
        for gid in 0..count {
            let fd = self.fd(gid) as u8;
            if ranges.last().is_none_or(|(_, last)| *last != fd) {
                ranges.push((gid as u16, fd));
            }
        }
        out.push(3);
        out.extend((ranges.len() as u16).to_be_bytes());
        for (first, fd) in ranges {
            out.extend(first.to_be_bytes());
            out.push(fd);
        }
        out.extend((count as u16).to_be_bytes());
        let glyphs = out.len();
        index(pick(self.glyphs, &keep, ENDCHAR), &mut out);

        // Each Private DICT holds its Subrs offset as a five-byte integer and is followed by
        // its subroutines, so every Private's size is known before the Font DICTs point at it.
        let privates: Vec<(usize, Vec<u8>)> = self
            .fonts
            .iter()
            .zip(&local)
            .map(|((_, entries, subrs), marks)| {
                let mut private = Vec::new();
                for entry in entries {
                    private.extend(entry.raw);
                    private.extend(op(entry.op));
                }
                let length = private.len() + if subrs.count > 0 { 6 } else { 0 };
                if subrs.count > 0 {
                    integer(length as i32, &mut private);
                    private.extend(op(SUBRS));
                    index(pick(*subrs, marks, RETURN), &mut private);
                }
                (length, private)
            })
            .collect();
        let array = out.len();
        let mut fonts: Vec<Vec<u8>> = Vec::with_capacity(self.fonts.len());
        for ((font, _, _), (length, _)) in self.fonts.iter().zip(&privates) {
            let mut dict = Vec::new();
            for entry in font {
                dict.extend(entry.raw);
                dict.extend(op(entry.op));
            }
            integer(*length as i32, &mut dict);
            integer(0, &mut dict);
            dict.extend(op(PRIVATE));
            fonts.push(dict);
        }
        let mut sized = Vec::new();
        index(fonts.iter().map(Vec::as_slice), &mut sized);
        let mut at = array + sized.len();
        for (dict, (_, private)) in fonts.iter_mut().zip(&privates) {
            let end = dict.len();
            dict[end - 5..end - 1].copy_from_slice(&(at as i32).to_be_bytes());
            at += private.len();
        }
        index(fonts.iter().map(Vec::as_slice), &mut out);
        for (_, private) in &privates {
            out.extend(private);
        }

        for (patch, value) in patches.into_iter().zip([charset, glyphs, array, select]) {
            out[top_at + patch..top_at + patch + 4].copy_from_slice(&(value as i32).to_be_bytes());
        }
        out
    }
}

const ENDCHAR: &[u8] = &[14];
const RETURN: &[u8] = &[11];

/// The items of `index`, with `empty` for those `marks` leaves out.
fn pick<'a>(
    index: Index<'a>,
    marks: &'a [bool],
    empty: &'static [u8],
) -> impl ExactSizeIterator<Item = &'a [u8]> {
    (0..index.count).map(move |item| match index.get(item) {
        Some(bytes) if marks[item] => bytes,
        _ => empty,
    })
}

fn op(op: u16) -> Vec<u8> {
    if op >= 0x0c00 {
        vec![12, op as u8]
    } else {
        vec![op as u8]
    }
}

fn integer(value: i32, out: &mut Vec<u8>) {
    out.push(29);
    out.extend(value.to_be_bytes());
}

/// The Font DICT of each of `count` glyphs from an FDSelect of format 0 or 3.
fn fd_select(data: &[u8], at: usize, count: usize) -> Option<Vec<u8>> {
    match *data.get(at)? {
        0 => Some(data.get(at + 1..at + 1 + count)?.to_vec()),
        3 => {
            let ranges = usize::from(u16::from_be_bytes(
                data.get(at + 1..at + 3)?.try_into().ok()?,
            ));
            let mut select = vec![0; count];
            for range in 0..ranges {
                let record = at + 3 + range * 3;
                let first = usize::from(u16::from_be_bytes(
                    data.get(record..record + 2)?.try_into().ok()?,
                ));
                let fd = *data.get(record + 2)?;
                let next = usize::from(u16::from_be_bytes(
                    data.get(record + 3..record + 5)?.try_into().ok()?,
                ));
                if first > next || next > count {
                    return None;
                }
                select[first..next].fill(fd);
            }
            Some(select)
        }
        _ => None,
    }
}

/// Follows a Type 2 charstring far enough to mark the subroutines it calls: the operand
/// stack for their numbers, and the stem count, which sizes each hint mask.
struct Walk<'w, 'a> {
    global: (Index<'a>, &'w mut [bool]),
    local: (Index<'a>, &'w mut [bool]),
    stack: Vec<i32>,
    stems: usize,
    seen_width: bool,
}

fn bias(count: usize) -> i32 {
    match count {
        0..1240 => 107,
        1240..33900 => 1131,
        _ => 32768,
    }
}

impl Walk<'_, '_> {
    /// `Some(true)` at `endchar`, `Some(false)` at `return`; `None` when the charstring
    /// cannot be followed, such as a `seac`-style accent the CID-keyed output cannot hold.
    fn run(&mut self, code: &[u8], depth: usize) -> Option<bool> {
        if depth > 10 {
            return None;
        }
        let mut at = 0;
        while at < code.len() {
            let byte = code[at];
            at += 1;
            match byte {
                28 => {
                    self.stack.push(i32::from(i16::from_be_bytes(
                        code.get(at..at + 2)?.try_into().ok()?,
                    )));
                    at += 2;
                }
                32..=246 => self.stack.push(i32::from(byte) - 139),
                247..=250 => {
                    self.stack
                        .push((i32::from(byte) - 247) * 256 + i32::from(*code.get(at)?) + 108);
                    at += 1;
                }
                251..=254 => {
                    self.stack
                        .push(-(i32::from(byte) - 251) * 256 - i32::from(*code.get(at)?) - 108);
                    at += 1;
                }
                255 => {
                    self.stack
                        .push(i32::from_be_bytes(code.get(at..at + 4)?.try_into().ok()?) >> 16);
                    at += 4;
                }
                // hstem, vstem, hstemhm, vstemhm
                1 | 3 | 18 | 23 => {
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                    self.seen_width = true;
                }
                // hintmask, cntrmask: a trailing vstem's arguments may come first.
                19 | 20 => {
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                    self.seen_width = true;
                    at += self.stems.div_ceil(8);
                }
                10 | 29 => {
                    let (index, marks) = if byte == 10 {
                        (self.local.0, &mut *self.local.1)
                    } else {
                        (self.global.0, &mut *self.global.1)
                    };
                    let number = self.stack.pop()? + bias(index.count);
                    let number = usize::try_from(number).ok()?;
                    let subr = index.get(number)?;
                    *marks.get_mut(number)? = true;
                    if self.run(subr, depth + 1)? {
                        return Some(true);
                    }
                }
                11 => return Some(false),
                14 => {
                    // endchar with four accent arguments, after an optional width, is seac.
                    let args = self.stack.len()
                        - usize::from(!self.seen_width && self.stack.len() % 2 == 1);
                    return (args < 4).then_some(true);
                }
                12 => {
                    at += 1;
                    self.stack.clear();
                }
                _ => {
                    self.stack.clear();
                    self.seen_width = true;
                }
            }
        }
        Some(false)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{find, Cff, ROS};
    use crate::font::truetype::ttf_table;

    #[test]
    fn given_a_name_keyed_cff_when_subsetting_then_it_turns_cid_keyed_keeps_glyph_ids_and_empties_the_rest(
    ) {
        let font =
            include_bytes!("../../../../packages/flashpdf/test/fixtures/TestSansCFF-subset.otf");
        let table = ttf_table(font, b"CFF ").unwrap();
        let cff = Cff::parse(table).unwrap();
        assert!(find(&cff.top, ROS).is_none());
        let program = cff.subset(Some(&BTreeSet::from([2, 9])));
        let subset = Cff::parse(&program).unwrap();
        assert!(find(&subset.top, ROS).is_some());
        assert_eq!(subset.glyphs.count, cff.glyphs.count);
        for gid in [0, 2, 9] {
            assert_eq!(subset.glyphs.get(gid), cff.glyphs.get(gid));
        }
        assert_eq!(subset.glyphs.get(3), Some(&[14][..]));
        // Subroutines no kept glyph calls shrink to a bare return.
        let returns = |cff: &Cff| {
            (0..cff.global.count)
                .filter(|item| cff.global.get(*item) == Some(&[11][..]))
                .count()
        };
        assert!(returns(&subset) > returns(&cff));
        assert!(program.len() < table.len() / 2, "{}", program.len());
    }
}
