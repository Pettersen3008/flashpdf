//! Vector drawings. TypeScript flattens an SVG into filled and stroked paths
//! of absolute segments in the unit square an image fills; this validates
//! them and writes the content stream of a form XObject clipped to that square.

use pdf_writer::{Content, Name};

use crate::image::{ColorSpace, Encoding, Image};
use crate::pdf::{deflate, Stream};

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
        data: deflate(&content.finish(), Stream::Content),
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

// Rotations already need these kernels; sharing them avoids a separate tangent kernel in WASM.
#[inline(never)]
pub(crate) fn tangent(angle: f64) -> f64 {
    libm::sin(angle) / libm::cos(angle)
}

/// Absolute SVG coordinates; callers choose their coordinate system and fill rule.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PathOp {
    Move([f32; 2]),
    Line([f32; 2]),
    Cubic([f32; 6]),
    Close,
}

const MAX_PATH_BYTES: usize = 64 * 1024;
const MAX_PATH_COORDINATE: f64 = 1_000_000.0;

#[inline(never)]
fn invalid_path() -> crate::css::Message {
    "invalid or over-limit SVG path".into()
}

struct PathData<'a> {
    text: &'a str,
    at: usize,
    comma_allowed: bool,
}

impl PathData<'_> {
    fn space(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .is_some_and(|b| crate::css::whitespace(*b))
        {
            self.at += 1;
        }
    }

    #[inline(never)]
    fn argument(&mut self) -> Result<(), crate::css::Message> {
        self.space();
        if self.text.as_bytes().get(self.at) == Some(&b',') {
            if !self.comma_allowed {
                return Err(invalid_path());
            }
            self.at += 1;
            self.space();
        }
        self.comma_allowed = true;
        Ok(())
    }

    #[inline(never)]
    fn number(&mut self) -> Result<f64, crate::css::Message> {
        self.argument()?;
        let rest = &self.text[self.at..];
        let sign = usize::from(matches!(rest.as_bytes().first(), Some(b'+' | b'-')));
        let end = sign + crate::css::number_end(&rest.as_bytes()[sign..], true);
        if end == sign {
            return Err(invalid_path());
        }
        let value = rest[..end].parse::<f64>().map_err(|_| invalid_path())?;
        if !value.is_finite() || value.abs() > MAX_PATH_COORDINATE {
            return Err(invalid_path());
        }
        self.at += end;
        Ok(value)
    }

    #[inline(never)]
    fn flag(&mut self) -> Result<bool, crate::css::Message> {
        self.argument()?;
        let flag = *self.text.as_bytes().get(self.at).ok_or_else(invalid_path)?;
        if !matches!(flag, b'0' | b'1') {
            return Err(invalid_path());
        }
        self.at += 1;
        Ok(flag == b'1')
    }

    #[inline(never)]
    fn pair(&mut self, offset: [f64; 2]) -> Result<[f64; 2], crate::css::Message> {
        Ok([self.number()? + offset[0], self.number()? + offset[1]])
    }
}

struct SvgPath {
    ops: Vec<PathOp>,
    point: [f64; 2],
    start: [f64; 2],
    open: bool,
}

impl SvgPath {
    #[inline(never)]
    fn push<const N: usize>(
        &mut self,
        coordinates: [f64; N],
    ) -> Result<[f32; N], crate::css::Message> {
        if self.ops.len() >= MAX_SEGMENTS
            || coordinates
                .iter()
                .any(|v| !v.is_finite() || v.abs() > MAX_PATH_COORDINATE)
        {
            return Err(invalid_path());
        }
        Ok(coordinates.map(|v| v as f32))
    }

    fn move_to(&mut self, point: [f64; 2]) -> Result<(), crate::css::Message> {
        let values = self.push(point)?;
        self.ops.push(PathOp::Move(values));
        self.point = point;
        self.start = point;
        self.open = true;
        Ok(())
    }

    fn reopen(&mut self) -> Result<(), crate::css::Message> {
        if !self.open {
            self.move_to(self.point)?;
        }
        Ok(())
    }

    #[inline(never)]
    fn line(&mut self, point: [f64; 2]) -> Result<(), crate::css::Message> {
        self.reopen()?;
        let values = self.push(point)?;
        self.ops.push(PathOp::Line(values));
        self.point = point;
        Ok(())
    }

    #[inline(never)]
    fn cubic(
        &mut self,
        a: [f64; 2],
        b: [f64; 2],
        end: [f64; 2],
    ) -> Result<(), crate::css::Message> {
        self.reopen()?;
        let values = self.push([a[0], a[1], b[0], b[1], end[0], end[1]])?;
        self.ops.push(PathOp::Cubic(values));
        self.point = end;
        Ok(())
    }

    fn close(&mut self) -> Result<(), crate::css::Message> {
        if self.open {
            self.push::<0>([])?;
            self.ops.push(PathOp::Close);
            self.point = self.start;
            self.open = false;
        }
        Ok(())
    }

    // SVG 1.1 F.6.5, ported from svg.ts: endpoint arcs become cubic quarter turns.
    fn arc(
        &mut self,
        radii: [f64; 2],
        rotation: f64,
        large: bool,
        sweep: bool,
        end: [f64; 2],
    ) -> Result<(), crate::css::Message> {
        if self.point == end {
            return Ok(());
        }
        let [mut rx, mut ry] = radii.map(f64::abs);
        if rx == 0.0 || ry == 0.0 {
            return self.line(end);
        }
        let angle = rotation.to_radians();
        let (cos, sin) = (libm::cos(angle), libm::sin(angle));
        let dx = (self.point[0] - end[0]) / 2.0;
        let dy = (self.point[1] - end[1]) / 2.0;
        let (px, py) = (cos * dx + sin * dy, -sin * dx + cos * dy);
        let (ux, uy) = (px / rx, py / ry);
        let scale = libm::sqrt(ux * ux + uy * uy).max(1.0);
        rx *= scale;
        ry *= scale;
        let numerator = rx * rx * ry * ry - rx * rx * py * py - ry * ry * px * px;
        let root = (if large == sweep { -1.0 } else { 1.0 })
            * libm::sqrt((numerator / (rx * rx * py * py + ry * ry * px * px)).max(0.0));
        let (cx1, cy1) = (root * rx * py / ry, -root * ry * px / rx);
        let cx = cos * cx1 - sin * cy1 + (self.point[0] + end[0]) / 2.0;
        let cy = sin * cx1 + cos * cy1 + (self.point[1] + end[1]) / 2.0;
        let angle = |u: [f64; 2], v: [f64; 2]| {
            libm::atan2(u[0] * v[1] - u[1] * v[0], u[0] * v[0] + u[1] * v[1])
        };
        let u = [(px - cx1) / rx, (py - cy1) / ry];
        let start = angle([1.0, 0.0], u);
        let mut delta = angle(u, [(-px - cx1) / rx, (-py - cy1) / ry]);
        let tau = 2.0 * std::f64::consts::PI;
        if !sweep && delta > 0.0 {
            delta -= tau;
        } else if sweep && delta < 0.0 {
            delta += tau;
        }
        let count = libm::ceil(delta.abs() / std::f64::consts::FRAC_PI_2 - 1e-9).max(1.0) as usize;
        let step = delta / count as f64;
        let k = (4.0 / 3.0) * tangent(step / 4.0);
        let point = |ux, uy| {
            [
                cx + rx * ux * cos - ry * uy * sin,
                cy + rx * ux * sin + ry * uy * cos,
            ]
        };
        for index in 0..count {
            let a = start + index as f64 * step;
            let b = a + step;
            let first = point(
                libm::cos(a) - k * libm::sin(a),
                libm::sin(a) + k * libm::cos(a),
            );
            let second = point(
                libm::cos(b) + k * libm::sin(b),
                libm::sin(b) - k * libm::cos(b),
            );
            let last = if index + 1 == count {
                end
            } else {
                point(libm::cos(b), libm::sin(b))
            };
            self.cubic(first, second, last)?;
        }
        Ok(())
    }
}

/// The complete SVG d command set, normalized to absolute moves, lines and cubics.
pub(crate) fn parse_path(text: &str) -> Result<Vec<PathOp>, crate::css::Message> {
    if text.len() > MAX_PATH_BYTES {
        return Err(invalid_path());
    }
    let mut data = PathData {
        text,
        at: 0,
        comma_allowed: false,
    };
    let mut path = SvgPath {
        ops: Vec::new(),
        point: [0.0; 2],
        start: [0.0; 2],
        open: false,
    };
    let (mut command, mut previous) = (b'\0', b'\0');
    let mut control = [0.0; 2];
    loop {
        data.space();
        let Some(&next) = text.as_bytes().get(data.at) else {
            break;
        };
        if next.is_ascii_alphabetic() {
            if !b"mMlLhHvVcCsSqQtTaAzZ".contains(&next)
                || (path.ops.is_empty() && !next.eq_ignore_ascii_case(&b'm'))
            {
                return Err(invalid_path());
            }
            command = next;
            data.at += 1;
            data.comma_allowed = false;
        } else if command == 0 || command.eq_ignore_ascii_case(&b'z') {
            return Err(invalid_path());
        }
        let lower = command.to_ascii_lowercase();
        let start = path.point;
        let offset = if command.is_ascii_lowercase() {
            start
        } else {
            [0.0; 2]
        };
        let reflect = |kinds: &[u8]| {
            if kinds.contains(&previous) {
                [2.0 * start[0] - control[0], 2.0 * start[1] - control[1]]
            } else {
                start
            }
        };
        match lower {
            b'm' => {
                path.move_to(data.pair(offset)?)?;
                command = if command.is_ascii_lowercase() {
                    b'l'
                } else {
                    b'L'
                };
            }
            b'l' => path.line(data.pair(offset)?)?,
            b'h' => path.line([data.number()? + offset[0], start[1]])?,
            b'v' => path.line([start[0], data.number()? + offset[1]])?,
            b'c' | b's' => {
                let first = if lower == b'c' {
                    data.pair(offset)?
                } else {
                    reflect(b"cs")
                };
                control = data.pair(offset)?;
                path.cubic(first, control, data.pair(offset)?)?;
            }
            b'q' | b't' => {
                control = if lower == b'q' {
                    data.pair(offset)?
                } else {
                    reflect(b"qt")
                };
                let end = data.pair(offset)?;
                path.cubic(
                    [
                        start[0] + 2.0 / 3.0 * (control[0] - start[0]),
                        start[1] + 2.0 / 3.0 * (control[1] - start[1]),
                    ],
                    [
                        end[0] + 2.0 / 3.0 * (control[0] - end[0]),
                        end[1] + 2.0 / 3.0 * (control[1] - end[1]),
                    ],
                    end,
                )?;
            }
            b'a' => {
                let radii = data.pair([0.0; 2])?;
                if radii.iter().any(|v| *v < 0.0) {
                    return Err(invalid_path());
                }
                let rotation = data.number()?;
                let large = data.flag()?;
                let sweep = data.flag()?;
                path.arc(radii, rotation, large, sweep, data.pair(offset)?)?;
            }
            b'z' => path.close()?,
            _ => return Err(invalid_path()),
        }
        previous = lower;
    }
    Ok(path.ops)
}
