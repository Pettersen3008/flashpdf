//! Variable TrueType instancing. read-fonts parses `fvar`, `avar`, `gvar`, `HVAR` and `MVAR`;
//! the deltas add up in 16.16 fixed point and round once, so every runtime draws the same
//! outline, and the instance embeds as a static `glyf` subset without hinting.

use read_fonts::tables::glyf::{PointFlags, PointMarker, SimpleGlyph};
use read_fonts::tables::gvar::{GlyphVariationData, Gvar};
use read_fonts::tables::hvar::Hvar;
use read_fonts::types::{F2Dot14, Fixed, GlyphId, Point, Tag};
use read_fonts::{FontData, FontRead, FontRef, TableProvider};

/// A variation axis in user units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Axis {
    pub(crate) tag: [u8; 4],
    pub(crate) min: f32,
    pub(crate) default: f32,
    pub(crate) max: f32,
}

/// The font's `fvar` axes; none for a static font.
pub(super) fn axes(bytes: &[u8]) -> Result<Vec<Axis>, ()> {
    let font = FontRef::new(bytes).map_err(|_| ())?;
    if font.table_data(Tag::new(b"fvar")).is_none() {
        return Ok(Vec::new());
    }
    let fvar = font.fvar().map_err(|_| ())?;
    Ok(fvar
        .axes()
        .map_err(|_| ())?
        .iter()
        .map(|axis| Axis {
            tag: axis.axis_tag().to_be_bytes(),
            min: axis.min_value().to_f32(),
            default: axis.default_value().to_f32(),
            max: axis.max_value().to_f32(),
        })
        .collect())
}

/// Normalized coordinates, one F2Dot14 per axis, for user values clamped to each axis and
/// mapped through `avar`; axes `values` leaves out stay at their default.
pub(super) fn normalize(bytes: &[u8], count: usize, values: &[([u8; 4], f32)]) -> Box<[i16]> {
    let mut coords = vec![F2Dot14::ZERO; count];
    if let Some(fvar) = FontRef::new(bytes).ok().and_then(|font| {
        let avar = font.avar().ok();
        Some((font.fvar().ok()?, avar))
    }) {
        let user = values
            .iter()
            .map(|(tag, value)| (Tag::new(tag), Fixed::from_f64(f64::from(*value))));
        fvar.0
            .user_to_normalized(fvar.1.as_ref(), user, &mut coords);
    }
    coords.iter().map(|coord| coord.to_bits()).collect()
}

/// A font at one set of normalized coordinates.
pub(super) struct Instance<'a> {
    font: FontRef<'a>,
    coords: Vec<F2Dot14>,
    gvar: Option<Gvar<'a>>,
    hvar: Option<Hvar<'a>>,
}

impl<'a> Instance<'a> {
    pub(super) fn new(bytes: &'a [u8], coords: &[i16]) -> Option<Self> {
        let font = FontRef::new(bytes).ok()?;
        Some(Self {
            gvar: font.gvar().ok(),
            hvar: font.hvar().ok(),
            coords: coords
                .iter()
                .map(|coord| F2Dot14::from_bits(*coord))
                .collect(),
            font,
        })
    }

    /// How far the advance of `gid` moves, in font units: `HVAR` where the font has it, as
    /// the spec prefers, else the `gvar` phantom points.
    pub(super) fn advance_delta(&self, gid: u16) -> i32 {
        let gid = GlyphId::new(u32::from(gid));
        let delta = match &self.hvar {
            Some(hvar) => hvar.advance_width_delta(gid, &self.coords).ok(),
            None => self.gvar.as_ref().and_then(|gvar| {
                let glyf = self.font.glyf().ok()?;
                let loca = self.font.loca(None).ok()?;
                let [left, right, ..] = gvar
                    .phantom_point_deltas(&glyf, &loca, &self.coords, gid)
                    .ok()??;
                Some(right.x - left.x)
            }),
        };
        delta.map_or(0, |delta| delta.round().to_i32())
    }

    /// How far an `MVAR` metric moves, in font units.
    pub(super) fn metric(&self, tag: &[u8; 4]) -> f32 {
        self.font
            .mvar()
            .ok()
            .and_then(|mvar| mvar.metric_delta(Tag::new(tag), &self.coords).ok())
            .map_or(0.0, Fixed::to_f32)
    }

    /// `outline`, the `glyf` entry of `gid`, at these coordinates, without instructions,
    /// and its new `xMin` for a simple glyph. `None` when the variation data is malformed.
    pub(super) fn glyph(&self, gid: u16, outline: &[u8]) -> Option<(Vec<u8>, Option<i16>)> {
        if outline.len() < 10 {
            return Some((outline.to_vec(), None));
        }
        let data = match &self.gvar {
            Some(gvar) => gvar
                .glyph_variation_data(GlyphId::new(u32::from(gid)))
                .ok()?,
            None => None,
        };
        if i16::from_be_bytes([outline[0], outline[1]]) < 0 {
            composite(outline, data, &self.coords).map(|glyph| (glyph, None))
        } else {
            simple(outline, data, &self.coords).map(|(glyph, x_min)| (glyph, Some(x_min)))
        }
    }
}

/// Original points and contour ends.
type Contours<'a> = (&'a [(i16, i16)], &'a [usize]);

/// The summed deltas of `count` points plus the four phantom points. `contours` gives the
/// original points and contour ends, which infer the deltas a sparse tuple leaves out;
/// a composite has none, so its untouched components stay put.
fn deltas<'a>(
    data: Option<GlyphVariationData<'a>>,
    coords: &'a [F2Dot14],
    count: usize,
    contours: Option<Contours>,
) -> Option<Vec<Point<Fixed>>> {
    let mut total = vec![Point::<Fixed>::default(); count + 4];
    let Some(data) = data else {
        return Some(total);
    };
    let mut sparse = Vec::new();
    let mut flags = Vec::new();
    for (tuple, scalar) in data.active_tuples_at(coords) {
        if tuple.has_deltas_for_all_points() {
            tuple.accumulate_dense_deltas(&mut total, scalar).ok()?;
            continue;
        }
        sparse.clear();
        sparse.resize(count + 4, Point::<Fixed>::default());
        flags.clear();
        flags.resize(count + 4, PointFlags::default());
        tuple
            .accumulate_sparse_deltas(&mut sparse, &mut flags, scalar)
            .ok()?;
        if let Some((points, ends)) = contours {
            infer(points, ends, &mut sparse, &flags)?;
        }
        for (sum, delta) in total.iter_mut().zip(&sparse) {
            *sum += *delta;
        }
    }
    Some(total)
}

/// Interpolates the deltas of untouched points from the nearest touched points of the same
/// contour on either side, per axis, as the gvar spec's IUP describes.
fn infer(
    points: &[(i16, i16)],
    ends: &[usize],
    deltas: &mut [Point<Fixed>],
    flags: &[PointFlags],
) -> Option<()> {
    let touched = |index: usize| flags[index].has_marker(PointMarker::HAS_DELTA);
    let mut start = 0;
    for &end in ends {
        if end < start || end >= points.len() {
            return None;
        }
        let length = end - start + 1;
        let next = |index: usize| start + (index - start + 1) % length;
        let Some(first) = (start..=end).find(|index| touched(*index)) else {
            start = end + 1;
            continue;
        };
        let mut previous = first;
        let mut index = next(first);
        for _ in 0..length {
            if touched(index) {
                let mut between = next(previous);
                while between != index {
                    let (a, b, own) = (points[previous], points[index], points[between]);
                    let (da, db) = (deltas[previous], deltas[index]);
                    deltas[between] = Point::new(
                        interpolate(own.0, (a.0, da.x), (b.0, db.x)),
                        interpolate(own.1, (a.1, da.y), (b.1, db.y)),
                    );
                    between = next(between);
                }
                previous = index;
            }
            index = next(index);
        }
        start = end + 1;
    }
    Some(())
}

fn interpolate(at: i16, (c1, d1): (i16, Fixed), (c2, d2): (i16, Fixed)) -> Fixed {
    if c1 == c2 {
        return if d1 == d2 { d1 } else { Fixed::ZERO };
    }
    let ((c1, d1), (c2, d2)) = if c1 < c2 {
        ((c1, d1), (c2, d2))
    } else {
        ((c2, d2), (c1, d1))
    };
    if at <= c1 {
        return d1;
    }
    if at >= c2 {
        return d2;
    }
    let span = i64::from(d2.to_bits()) - i64::from(d1.to_bits());
    let step = span * i64::from(at - c1) / i64::from(c2 - c1);
    Fixed::from_bits(d1.to_bits().wrapping_add(step as i32))
}

fn round(value: i16, delta: Fixed) -> i16 {
    (i32::from(value) + delta.round().to_i32()).clamp(-32768, 32767) as i16
}

fn simple<'a>(
    outline: &[u8],
    data: Option<GlyphVariationData<'a>>,
    coords: &'a [F2Dot14],
) -> Option<(Vec<u8>, i16)> {
    let glyph = SimpleGlyph::read(FontData::new(outline)).ok()?;
    let ends: Vec<usize> = glyph
        .end_pts_of_contours()
        .iter()
        .map(|end| usize::from(end.get()))
        .collect();
    let original: Vec<_> = glyph.points().collect();
    let points: Vec<(i16, i16)> = original.iter().map(|point| (point.x, point.y)).collect();
    let deltas = deltas(data, coords, points.len(), Some((&points, &ends)))?;
    let moved: Vec<(i16, i16)> = points
        .iter()
        .zip(&deltas)
        .map(|(point, delta)| (round(point.0, delta.x), round(point.1, delta.y)))
        .collect();
    let bounds = |pick: fn(&(i16, i16)) -> i16| {
        let values = moved.iter().map(pick);
        (values.clone().min().unwrap_or(0), values.max().unwrap_or(0))
    };
    let ((x_min, x_max), (y_min, y_max)) = (bounds(|point| point.0), bounds(|point| point.1));
    let mut out = Vec::with_capacity(outline.len());
    out.extend((ends.len() as i16).to_be_bytes());
    for value in [x_min, y_min, x_max, y_max] {
        out.extend(value.to_be_bytes());
    }
    for end in &ends {
        out.extend((*end as u16).to_be_bytes());
    }
    out.extend([0, 0]);
    // Flags then coordinates as deltas: a byte with a sign flag where it fits, else a word.
    let overlap = if glyph.has_overlapping_contours() {
        0x40
    } else {
        0
    };
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    let mut last = (0_i16, 0_i16);
    for (index, (point, curve)) in moved.iter().zip(&original).enumerate() {
        let mut flag = u8::from(curve.on_curve) | if index == 0 { overlap } else { 0 };
        for (delta, short, same, out) in [
            (i32::from(point.0) - i32::from(last.0), 0x02, 0x10, &mut xs),
            (i32::from(point.1) - i32::from(last.1), 0x04, 0x20, &mut ys),
        ] {
            if delta == 0 {
                flag |= same;
            } else if delta.abs() < 256 {
                flag |= short | if delta > 0 { same } else { 0 };
                out.push(delta.unsigned_abs() as u8);
            } else {
                out.extend((delta as i16).to_be_bytes());
            }
        }
        out.push(flag);
        last = *point;
    }
    out.extend(xs);
    out.extend(ys);
    Some((out, x_min))
}

/// A composite with each component's x and y offset moved by its delta, written with word
/// arguments so a moved offset always fits, and without instructions.
// ponytail: the header keeps the default bounding box; viewers read the components' own.
fn composite<'a>(
    outline: &[u8],
    data: Option<GlyphVariationData<'a>>,
    coords: &'a [F2Dot14],
) -> Option<Vec<u8>> {
    let mut count = 0;
    let mut at = 10;
    loop {
        let flags = u16::from_be_bytes(outline.get(at..at + 2)?.try_into().ok()?);
        at += 4 + arguments(flags) + transform(flags);
        count += 1;
        if flags & 0x20 == 0 {
            break;
        }
    }
    let deltas = deltas(data, coords, count, None)?;
    let mut out = outline[..10].to_vec();
    let mut at = 10;
    for delta in &deltas[..count] {
        let flags = u16::from_be_bytes(outline.get(at..at + 2)?.try_into().ok()?);
        out.extend(((flags | 1) & !0x100).to_be_bytes());
        out.extend(outline.get(at + 2..at + 4)?);
        let args = outline.get(at + 4..at + 4 + arguments(flags))?;
        let (first, second) = match (flags & 1 != 0, flags & 2 != 0) {
            (true, true) => (
                i32::from(i16::from_be_bytes([args[0], args[1]])),
                i32::from(i16::from_be_bytes([args[2], args[3]])),
            ),
            (true, false) => (
                i32::from(u16::from_be_bytes([args[0], args[1]])),
                i32::from(u16::from_be_bytes([args[2], args[3]])),
            ),
            (false, true) => (i32::from(args[0] as i8), i32::from(args[1] as i8)),
            (false, false) => (i32::from(args[0]), i32::from(args[1])),
        };
        // Point-matched components follow their anchor points, which already moved.
        let (first, second) = if flags & 2 != 0 {
            (
                (first + delta.x.round().to_i32()).clamp(-32768, 32767),
                (second + delta.y.round().to_i32()).clamp(-32768, 32767),
            )
        } else {
            (first, second)
        };
        out.extend((first as u16).to_be_bytes());
        out.extend((second as u16).to_be_bytes());
        let start = at + 4 + arguments(flags);
        out.extend(outline.get(start..start + transform(flags))?);
        at = start + transform(flags);
    }
    Some(out)
}

fn arguments(flags: u16) -> usize {
    if flags & 1 != 0 {
        4
    } else {
        2
    }
}

fn transform(flags: u16) -> usize {
    match flags & 0xc8 {
        0x08 => 2,
        0x40 => 4,
        0x80 => 8,
        _ => 0,
    }
}
