//! Vector drawings. TypeScript flattens an SVG into filled and stroked paths
//! of absolute segments in the unit square an image fills; this validates
//! them and writes the content stream of a form XObject clipped to that square.

use pdf_writer::{Content, Name};

use crate::image::{ColorSpace, Encoding, Image};
use crate::pdf::deflate;

pub(crate) const MAGIC: &[u8; 4] = b"FPDV";
/// Bounds one drawing's content stream; a version 40 QR code needs under 80,000.
const MAX_SEGMENTS: usize = 100_000;
const MAX_DASHES: usize = 32;

fn invalid() -> String {
    "invalid vector drawing".to_owned()
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], String> {
        let (head, rest) = self.0.split_first_chunk().ok_or_else(invalid)?;
        self.0 = rest;
        Ok(*head)
    }

    fn real(&mut self) -> Result<f32, String> {
        Some(f32::from_le_bytes(self.take()?))
            .filter(|value| value.is_finite())
            .ok_or_else(invalid)
    }

    fn reals<const N: usize>(&mut self) -> Result<[f32; N], String> {
        let mut values = [0.0; N];
        for value in &mut values {
            *value = self.real()?;
        }
        Ok(values)
    }
}

/// One call site for every numeric operator, because pdf-writer inlines its
/// float formatting into each caller and that would grow the WASM.
#[inline(never)]
pub(crate) fn op(content: &mut Content, operator: &str, operands: &[f32]) {
    content.op(operator).operands(operands.iter().copied());
}

#[inline(never)]
pub(crate) fn dash(content: &mut Content, dashes: &[f32], phase: f32) {
    content.set_dash_pattern(dashes.iter().copied(), phase);
}

/// `MAGIC`, the intrinsic size in points, then per path: flags (bit 0 fill, 1 even-odd,
/// 2 stroke, 3-4 cap, 5-6 join), a matrix, the fill RGBA, the stroke RGBA, width,
/// miter limit, dashes and phase, and the segments (0 move, 1 line, 2 cubic, 3 close).
pub(crate) fn parse(bytes: &[u8]) -> Result<Image, String> {
    let mut input = Reader(&bytes[MAGIC.len()..]);
    let (width, height) = (input.real()?, input.real()?);
    if width <= 0.0 || height <= 0.0 {
        return Err(invalid());
    }
    let mut content = Content::new();
    // One flag per fill and stroke alpha pair, which also sorts the resource names.
    let mut alphas = vec![false; 1 << 16];
    let mut budget = MAX_SEGMENTS;
    while !input.0.is_empty() {
        let [flags] = input.take()?;
        let (cap, join) = (flags >> 3 & 3, flags >> 5 & 3);
        if flags & 5 == 0 || flags & 0x80 != 0 || cap > 2 || join > 2 {
            return Err(invalid());
        }
        op(&mut content, "q", &[]);
        op(&mut content, "cm", &input.reals::<6>()?);
        let mut alpha = [255; 2];
        for (index, operator) in [(0, "rg"), (1, "RG")] {
            if flags & [1, 4][index] != 0 {
                let [r, g, b, a] = input.take()?;
                op(
                    &mut content,
                    operator,
                    &[r, g, b].map(|c| f32::from(c) / 255.0),
                );
                alpha[index] = a;
            }
        }
        if flags & 4 != 0 {
            let [line_width, miter] = input.reals()?;
            let [count] = input.take()?;
            let mut dashes = [0.0; MAX_DASHES];
            let dashes = dashes.get_mut(..usize::from(count)).ok_or_else(invalid)?;
            for dash in dashes.iter_mut() {
                *dash = input.real()?;
            }
            let phase = input.real()?;
            if line_width <= 0.0
                || miter < 1.0
                || dashes.iter().any(|dash| *dash < 0.0)
                || (count > 0 && dashes.iter().sum::<f32>() <= 0.0)
            {
                return Err(invalid());
            }
            op(&mut content, "w", &[line_width]);
            op(&mut content, "J", &[f32::from(cap)]);
            op(&mut content, "j", &[f32::from(join)]);
            op(&mut content, "M", &[miter]);
            if count > 0 {
                dash(&mut content, dashes, phase);
            }
        }
        if alpha != [255; 2] {
            content.set_parameters(Name(&alpha_name(alpha)));
            alphas[usize::from(u16::from_be_bytes(alpha))] = true;
        }
        let count = u32::from_le_bytes(input.take()?) as usize;
        budget = budget
            .checked_sub(count)
            .filter(|_| count > 0)
            .ok_or_else(invalid)?;
        // A line or curve needs a current point, which a close ends.
        let mut open = false;
        for _ in 0..count {
            let [segment] = input.take()?;
            match (segment, open) {
                (0, _) => op(&mut content, "m", &input.reals::<2>()?),
                (1, true) => op(&mut content, "l", &input.reals::<2>()?),
                (2, true) => op(&mut content, "c", &input.reals::<6>()?),
                (3, true) => op(&mut content, "h", &[]),
                _ => return Err(invalid()),
            }
            open = segment != 3;
        }
        let paint = ["", "f", "", "f*", "S", "B", "S", "B*"][usize::from(flags & 7)];
        op(&mut content, paint, &[]);
        op(&mut content, "Q", &[]);
    }
    let alphas = (0..=u16::MAX)
        .filter(|pair| alphas[usize::from(*pair)])
        .map(u16::to_be_bytes);
    Ok(Image {
        width: 0,
        height: 0,
        color_space: ColorSpace::Rgb,
        encoding: Encoding::Form {
            width,
            height,
            alphas: alphas.collect(),
        },
        data: deflate(&content.finish()),
        alpha: None,
        color_key: None,
    })
}

/// `/G80FF` sets fill alpha 0x80 and stroke alpha 0xFF.
pub(crate) fn alpha_name(alpha: [u8; 2]) -> [u8; 5] {
    let hex = |value: u8| b"0123456789ABCDEF"[usize::from(value & 15)];
    [
        b'G',
        hex(alpha[0] >> 4),
        hex(alpha[0]),
        hex(alpha[1] >> 4),
        hex(alpha[1]),
    ]
}
