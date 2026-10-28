use super::*;
use crate::backgrounds::{Gradient, Shape};

#[derive(Default)]
pub(crate) struct Shadings {
    pub items: Vec<Shading>,
    pub bytes: usize,
    pub invalid: bool,
}

#[derive(Clone)]
pub(crate) struct Shading {
    pub radial: bool,
    pub tile: [f32; 4],
    pub matrix: [f32; 6],
    pub pattern: [f32; 6],
    pub coords: Vec<f32>,
    pub stops: Vec<(f32, [f32; 3])>,
}

impl PdfPainter<'_> {
    pub(super) fn paint_shadows(&mut self, style: &BoxStyle, rect: [f32; 4], inset: bool) {
        let Some(effects) = &style.effects else {
            return;
        };
        let [x, y, w, h] = rect;
        let corners = corners(style.radius, w, h);
        for shadow in effects.shadows.iter().rev().filter(|s| s.inset == inset) {
            let (base, radii) = if inset {
                let [t, r, b, l] = style.border.into_array().map(Pt::get);
                (
                    [x + l, y + b, (w - l - r).max(0.0), (h - t - b).max(0.0)],
                    [
                        [(corners[0][0] - l).max(0.0), (corners[0][1] - t).max(0.0)],
                        [(corners[1][0] - r).max(0.0), (corners[1][1] - t).max(0.0)],
                        [(corners[2][0] - r).max(0.0), (corners[2][1] - b).max(0.0)],
                        [(corners[3][0] - l).max(0.0), (corners[3][1] - b).max(0.0)],
                    ],
                )
            } else {
                (rect, corners)
            };
            if base[2] <= 0.0 || base[3] <= 0.0 {
                continue;
            }
            let spread = if inset { -shadow.spread } else { shadow.spread };
            let cast = [
                base[0] + shadow.offset[0] - spread,
                base[1] - shadow.offset[1] - spread,
                (base[2] + 2.0 * spread).max(0.0),
                (base[3] + 2.0 * spread).max(0.0),
            ];
            let mut cast_radii = radii.map(|pair| {
                pair.map(|r| {
                    let delta = if spread > 0.0 && r < spread {
                        let q = r / spread - 1.0;
                        spread * (1.0 + q * q * q)
                    } else {
                        spread
                    };
                    (r + delta).max(0.0)
                })
            });
            normalize_corners(&mut cast_radii, cast[2], cast[3]);
            self.set_alpha(((u16::from(style.alpha) * u16::from(shadow.alpha) + 127) / 255) as u8);
            self.set_fill(Rgb {
                r: shadow.color[0],
                g: shadow.color[1],
                b: shadow.color[2],
            });
            self.content.save_state();
            if inset {
                outline(self.content, base, radii);
                op(self.content, "W", &[]);
                op(self.content, "n", &[]);
                // The enclosing path contains both the clip and the displaced hole.
                let left = base[0].min(cast[0]) - 1.0;
                let bottom = base[1].min(cast[1]) - 1.0;
                op(
                    self.content,
                    "re",
                    &[
                        left,
                        bottom,
                        (base[0] + base[2]).max(cast[0] + cast[2]) - left + 1.0,
                        (base[1] + base[3]).max(cast[1] + cast[3]) - bottom + 1.0,
                    ],
                );
                if cast[2] > 0.0 && cast[3] > 0.0 {
                    outline(self.content, cast, cast_radii);
                }
                op(self.content, "f*", &[]);
            } else if cast[2] > 0.0 && cast[3] > 0.0 {
                op(
                    self.content,
                    "re",
                    &[
                        x.min(cast[0]) - 1.0,
                        y.min(cast[1]) - 1.0,
                        (x + w).max(cast[0] + cast[2]) - x.min(cast[0]) + 2.0,
                        (y + h).max(cast[1] + cast[3]) - y.min(cast[1]) + 2.0,
                    ],
                );
                outline(self.content, rect, corners);
                op(self.content, "W*", &[]);
                op(self.content, "n", &[]);
                outline(self.content, cast, cast_radii);
                op(self.content, "f", &[]);
            }
            self.content.restore_state();
        }
        self.set_alpha(style.alpha);
    }

    pub(super) fn paint_gradient(&mut self, style: &BoxStyle, rect: [f32; 4]) {
        let Some(gradient) = style.effects.as_ref().and_then(|e| e.gradient.as_ref()) else {
            return;
        };
        let [_, _, width, height] = rect;
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        self.content.save_state();
        outline(self.content, rect, corners(style.radius, width, height));
        op(self.content, "W", &[]);
        op(self.content, "n", &[]);
        let [top, right, bottom, left] = style.border.into_array().map(Pt::get);
        let tile = [
            rect[0] + left,
            rect[1] + bottom,
            width - left - right,
            height - top - bottom,
        ];
        if tile[2] > 0.0 && tile[3] > 0.0 {
            let (mut shading, matrix) = resolve_gradient(gradient, [0.0, 0.0, tile[2], tile[3]]);
            if shading.coords.iter().chain(&matrix).any(|v| !v.is_finite())
                || shading.stops.iter().any(|s| !s.0.is_finite())
                || shading.stops.windows(2).all(|p| p[0].0 >= p[1].0)
            {
                self.shadings.invalid = true;
                self.content.restore_state();
                return;
            }
            shading.tile = tile;
            shading.pattern = [1.0, 0.0, 0.0, 1.0, tile[0], tile[1]];
            shading.matrix = matrix;
            let slot = self.shadings.items.len();
            self.shadings.bytes += 512 + shading.stops.len() * 256;
            if self.shadings.bytes > crate::renderer::MAX_CONTENT_BYTES {
                self.content.restore_state();
                return;
            }
            self.shadings.items.push(shading);
            self.content
                .set_fill_color_space(pdf_writer::types::ColorSpaceOperand::Pattern);
            self.content
                .set_fill_pattern([], Name(format!("Gr{slot}").as_bytes()));
            self.content
                .rect(rect[0], rect[1], width, height)
                .fill_nonzero();
        }
        self.content.restore_state();
    }
}

fn normalize_corners(corners: &mut Corners, w: f32, h: f32) {
    let [tl, tr, br, bl] = *corners;
    let scale = [
        (w, tl[0] + tr[0]),
        (w, bl[0] + br[0]),
        (h, tl[1] + bl[1]),
        (h, tr[1] + br[1]),
    ]
    .into_iter()
    .fold(
        1.0f32,
        |s, (side, sum)| if sum > side { s.min(side / sum) } else { s },
    );
    for r in corners.iter_mut().flatten() {
        *r *= scale;
    }
}

fn resolve_gradient(gradient: &Gradient, rect: [f32; 4]) -> (Shading, [f32; 6]) {
    let [x, y, w, h] = rect;
    let (radial, coords, reference, unit, matrix, mirror, solid) = match &gradient.shape {
        Shape::Linear(_) | Shape::Corner(_) => {
            let (dx, dy) = match gradient.shape {
                Shape::Linear(angle) => {
                    let a = f64::from(angle) * std::f64::consts::PI / 180.0;
                    (libm::sin(a) as f32, libm::cos(a) as f32)
                }
                Shape::Corner([dx, dy]) if dx != 0.0 && dy != 0.0 => {
                    let len = libm::sqrt(f64::from(w) * f64::from(w) + f64::from(h) * f64::from(h))
                        as f32;
                    (dx * h / len, dy * w / len)
                }
                Shape::Corner([dx, dy]) => (dx, dy),
                _ => unreachable!(),
            };
            let length = w * dx.abs() + h * dy.abs();
            (
                false,
                vec![
                    x + w / 2.0 - dx * length / 2.0,
                    y + h / 2.0 - dy * length / 2.0,
                    x + w / 2.0 + dx * length / 2.0,
                    y + h / 2.0 + dy * length / 2.0,
                ],
                length,
                length.max(f32::MIN_POSITIVE),
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                false,
                false,
            )
        }
        Shape::Radial {
            circle,
            size,
            radii,
            at,
        } => {
            let cx = at[0].resolve(w);
            let cy = at[1].resolve(h);
            let near = size.starts_with("closest");
            let select = |a: f32, b: f32| if near { a.min(b) } else { a.max(b) };
            let sx = select(cx.abs(), (w - cx).abs());
            let sy = select(cy.abs(), (h - cy).abs());
            let (mut rx, mut ry) = if radii.is_empty() {
                if *circle {
                    let r = if size.ends_with("corner") {
                        libm::sqrt(f64::from(sx) * f64::from(sx) + f64::from(sy) * f64::from(sy))
                            as f32
                    } else {
                        select(sx, sy)
                    };
                    (r, r)
                } else if size.ends_with("corner") {
                    (sx * std::f32::consts::SQRT_2, sy * std::f32::consts::SQRT_2)
                } else {
                    (sx, sy)
                }
            } else {
                (
                    radii[0].resolve(w),
                    radii.get(1).unwrap_or(&radii[0]).resolve(h),
                )
            };
            let center = [x + cx, y + h - cy];
            if !circle && rx == 0.0 {
                (
                    false,
                    vec![center[0], center[1], center[0] + 1.0, center[1]],
                    0.0,
                    1.0,
                    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    true,
                    false,
                )
            } else if !circle && ry == 0.0 {
                (
                    false,
                    vec![x, y, x + 1.0, y],
                    1.0,
                    1.0,
                    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    false,
                    true,
                )
            } else {
                let reference = rx;
                if rx == 0.0 {
                    rx = 1.0;
                    ry = 1.0;
                }
                (
                    true,
                    vec![0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
                    reference,
                    rx,
                    [rx, 0.0, 0.0, ry, center[0], center[1]],
                    false,
                    false,
                )
            }
        }
    };
    let mut positions: Vec<Option<f64>> = gradient
        .stops
        .iter()
        .map(|(_, p)| {
            p.map(|p| {
                (if p.1 {
                    f64::from(p.0) * f64::from(reference) / 100.0
                } else {
                    f64::from(p.0)
                }) / f64::from(unit)
            })
        })
        .collect();
    let last = positions.len() - 1;
    positions[0] = Some(positions[0].unwrap_or(0.0));
    positions[last] = Some(positions[last].unwrap_or(f64::from(reference) / f64::from(unit)));
    let mut previous = positions[0].unwrap();
    for p in positions.iter_mut().flatten() {
        *p = p.max(previous);
        previous = *p;
    }
    let mut start = 0;
    while start < last {
        let end = (start + 1..=last)
            .find(|&i| positions[i].is_some())
            .unwrap();
        let a = positions[start].unwrap();
        let b = positions[end].unwrap();
        for (i, p) in positions.iter_mut().enumerate().take(end).skip(start + 1) {
            *p = Some(a + (b - a) * (i - start) as f64 / (end - start) as f64);
        }
        start = end;
    }
    let mut stops: Vec<_> = gradient
        .stops
        .iter()
        .zip(positions)
        .map(|((c, _), p)| (p.unwrap(), c.map(|c| f32::from(c) / 255.0)))
        .collect();
    // Extra constant intervals preserve coincident stops as hard edges, with strict PDF bounds.
    let mut low = if radial || mirror {
        0.0
    } else if stops[0].0 == stops[1].0 {
        stops[0].0.min(0.0) - 1.0
    } else {
        stops[0].0.min(0.0)
    };
    let mut high = stops[last].0.max(1.0);
    if stops[last].0 == stops[last - 1].0 && stops[last].0 == high {
        high += 1.0;
    }
    if radial || mirror {
        let mut at_zero = stops[0].1;
        for pair in stops.windows(2) {
            if pair[0].0 <= 0.0 {
                at_zero = pair[0].1;
            }
            if pair[0].0 <= 0.0 && pair[1].0 > 0.0 {
                let t = -pair[0].0 / (pair[1].0 - pair[0].0);
                at_zero = std::array::from_fn(|i| {
                    pair[0].1[i] + (pair[1].1[i] - pair[0].1[i]) * t as f32
                });
                break;
            }
        }
        if stops[last].0 <= 0.0 {
            at_zero = stops[last].1;
        }
        stops.retain(|(p, _)| *p > 0.0);
        stops.insert(0, (0.0, at_zero));
    }
    if stops[0].0 > low {
        stops.insert(0, (low, stops[0].1));
    }
    if stops.last().unwrap().0 < high {
        stops.push((high, stops.last().unwrap().1));
    }
    if stops.len() == 1 {
        stops.push((high, stops[0].1));
    }
    if solid {
        for (_, color) in &mut stops {
            *color = gradient
                .stops
                .last()
                .unwrap()
                .0
                .map(|c| f32::from(c) / 255.0);
        }
    }
    if mirror {
        let reflected: Vec<_> = stops.iter().rev().map(|(p, c)| (-*p, *c)).collect();
        stops.splice(0..0, reflected);
        low = -high;
    }
    for (position, _) in &mut stops {
        *position = (*position - low) / (high - low);
    }
    let mut matrix = matrix;
    let coords = if radial {
        matrix[0] = (f64::from(matrix[0]) * high) as f32;
        matrix[3] = (f64::from(matrix[3]) * high) as f32;
        vec![0.0, 0.0, (low / high) as f32, 0.0, 0.0, 1.0]
    } else {
        let dx = f64::from(coords[2]) - f64::from(coords[0]);
        let dy = f64::from(coords[3]) - f64::from(coords[1]);
        vec![
            (f64::from(coords[0]) + dx * low) as f32,
            (f64::from(coords[1]) + dy * low) as f32,
            (f64::from(coords[0]) + dx * high) as f32,
            (f64::from(coords[1]) + dy * high) as f32,
        ]
    };
    (
        Shading {
            radial,
            coords,
            stops: stops.into_iter().map(|(p, c)| (p as f32, c)).collect(),
            tile: rect,
            pattern: [1.0, 0.0, 0.0, 1.0, rect[0], rect[1]],
            matrix,
        },
        matrix,
    )
}

pub(crate) fn write_shadings(
    pdf: &mut pdf_writer::Pdf,
    shadings: &[Shading],
    refs: &[pdf_writer::Ref],
) {
    use pdf_writer::types::{PaintType, TilingType};
    use pdf_writer::Rect;
    for (shading, reference) in shadings.iter().zip(refs) {
        let mut content = Content::new();
        content.transform(shading.matrix).shading(Name(b"S"));
        let bytes = content.finish();
        let [_, _, w, h] = shading.tile;
        let mut pattern = pdf.tiling_pattern(*reference, &bytes);
        pattern
            .paint_type(PaintType::Colored)
            .tiling_type(TilingType::NoDistortion)
            .bbox(Rect::new(0.0, 0.0, w, h))
            .x_step(w)
            .y_step(h)
            .matrix(shading.pattern);
        let mut resources = pattern.resources();
        let mut dict = resources.shadings();
        let mut s = dict.insert(Name(b"S")).dict();
        s.pair(Name(b"ShadingType"), if shading.radial { 3 } else { 2 });
        s.pair(Name(b"ColorSpace"), Name(b"DeviceRGB"));
        s.insert(Name(b"Coords"))
            .array()
            .items(shading.coords.iter().copied());
        let domain = [0.0, 1.0];
        s.insert(Name(b"Domain")).array().items(domain);
        s.insert(Name(b"Extend")).array().items([true, true]);
        let intervals: Vec<_> = shading
            .stops
            .windows(2)
            .filter(|p| p[0].0 < p[1].0)
            .collect();
        let mut function = s.insert(Name(b"Function")).dict();
        if intervals.len() == 1 {
            exponential(&mut function, intervals[0][0].1, intervals[0][1].1);
        } else {
            function.pair(Name(b"FunctionType"), 3);
            function.insert(Name(b"Domain")).array().items(domain);
            {
                let mut funcs = function.insert(Name(b"Functions")).array();
                for pair in &intervals {
                    exponential(&mut funcs.push().dict(), pair[0].1, pair[1].1);
                }
            }
            function
                .insert(Name(b"Bounds"))
                .array()
                .items(intervals.iter().take(intervals.len() - 1).map(|p| p[1].0));
            function
                .insert(Name(b"Encode"))
                .array()
                .items(intervals.iter().flat_map(|_| [0.0, 1.0]));
        }
    }
}
#[inline(never)]
fn exponential(dict: &mut pdf_writer::Dict, c0: [f32; 3], c1: [f32; 3]) {
    dict.pair(Name(b"FunctionType"), 2);
    dict.insert(Name(b"Domain")).array().items([0.0, 1.0]);
    dict.insert(Name(b"C0")).array().items(c0);
    dict.insert(Name(b"C1")).array().items(c1);
    dict.pair(Name(b"N"), 1);
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resolved(source: &str) -> Shading {
        let effect = crate::backgrounds::parse("none", source, 12.0, [0; 3]).unwrap();
        resolve_gradient(effect.gradient.as_ref().unwrap(), [0.0, 0.0, 200.0, 100.0]).0
    }
    #[test]
    fn given_gradient_geometry_when_resolved_then_uses_magic_corners_radial_extents_and_css_stop_fixup(
    ) {
        let shade = resolved("linear-gradient(to right,red 20%,green,blue 80%)");
        assert_eq!(shade.coords, vec![0.0, 50.0, 200.0, 50.0]);
        assert_eq!(
            shade.stops.iter().map(|s| s.0).collect::<Vec<_>>(),
            [0.0, 0.2, 0.5, 0.8, 1.0]
        );
        let shade = resolved("linear-gradient(to top right,red,blue)");
        assert!((shade.coords[0] - 60.0).abs() < 0.0001);
        assert!((shade.coords[1] + 30.0).abs() < 0.0001);
        let shade = resolved("radial-gradient(ellipse closest-corner at 25% 50%,red,blue)");
        assert!(shade.radial);
        assert!((shade.matrix[0] - 50.0 * std::f32::consts::SQRT_2).abs() < 0.0001);
        assert_eq!(shade.matrix[4..], [50.0, 50.0]);
        let shade = resolved("radial-gradient(circle 100pt,red -50pt,blue 50pt)");
        assert_eq!(shade.stops[0].1, [0.5, 0.0, 0.5]);
        let tiny = resolved("radial-gradient(circle 1e-40pt,red 0pt,blue 1pt)");
        assert!(tiny
            .coords
            .iter()
            .chain(&tiny.matrix)
            .all(|v| v.is_finite()));
        assert!(tiny.stops.iter().all(|s| s.0.is_finite()));
        let shade = resolved("radial-gradient(ellipse 0 10pt,red 0pt,blue 10pt)");
        assert!(!shade.radial);
        assert_eq!(shade.stops.first().unwrap().1, [0.0, 0.0, 1.0]);
        assert_eq!(shade.stops.last().unwrap().1, [0.0, 0.0, 1.0]);
    }
}
