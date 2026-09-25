//! PNG and JPEG headers are parsed once at registration. Pixel data passes
//! through to the PDF undecoded, except where an alpha channel must be split
//! off into an `/SMask`, which needs the scanlines inflated and unfiltered.

use crate::pdf::deflate;

/// Total bytes of registered images per render.
pub const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
/// Largest pixel buffer a PNG with transparency may inflate to.
const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_DIMENSION: u32 = 65_535;
const PNG_SIGNATURE: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

pub(crate) enum ColorSpace {
    Gray,
    Rgb,
    Cmyk,
    /// RGB palette bytes for `/Indexed /DeviceRGB`.
    Indexed(Vec<u8>),
}

impl ColorSpace {
    pub(crate) fn channels(&self) -> usize {
        match self {
            Self::Gray | Self::Indexed(_) => 1,
            Self::Rgb => 3,
            Self::Cmyk => 4,
        }
    }
}

pub(crate) enum Encoding {
    /// The whole JPEG file; Adobe APP14 CMYK is stored inverted.
    Dct { inverted: bool },
    /// Concatenated IDAT zlib data with PNG filter bytes per row: `/Predictor 15`.
    FlatePredicted,
    /// Re-deflated raw samples.
    Flate,
}

pub(crate) struct Image {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color_space: ColorSpace,
    pub(crate) encoding: Encoding,
    pub(crate) data: Vec<u8>,
    /// Deflated 8-bit alpha plane, emitted as an `/SMask` image.
    pub(crate) alpha: Option<Vec<u8>>,
    /// `/Mask` colour-key ranges from a `tRNS` chunk on a greyscale or RGB PNG.
    pub(crate) color_key: Option<Vec<i32>>,
}

impl Image {
    pub(crate) fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        if bytes.starts_with(PNG_SIGNATURE) {
            png(bytes)
        } else if bytes.starts_with(&[0xff, 0xd8]) {
            jpeg(bytes)
        } else {
            Err("unsupported image format; pass PNG or JPEG bytes".into())
        }
    }
}

fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    bytes
        .get(offset..offset + 2)
        .map(|value| u16::from_be_bytes([value[0], value[1]]))
        .ok_or_else(|| "truncated image".to_owned())
}

fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .map(|value| u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
        .ok_or_else(|| "truncated image".to_owned())
}

fn dimensions(width: u32, height: u32) -> Result<(u32, u32), String> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(format!(
            "image is {width}x{height} pixels; each side must be 1 to {MAX_DIMENSION}"
        ));
    }
    Ok((width, height))
}

/// Walks the marker segments up to the frame header; `/DCTDecode` reads the rest.
fn jpeg(bytes: Vec<u8>) -> Result<Image, String> {
    let mut offset = 2;
    let mut adobe = false;
    loop {
        if bytes.get(offset) != Some(&0xff) {
            return Err("invalid JPEG marker".into());
        }
        let code = *bytes.get(offset + 1).ok_or("truncated JPEG")?;
        if code == 0xff {
            offset += 1;
            continue;
        }
        offset += 2;
        match code {
            0x01 | 0xd0..=0xd8 => continue,
            0xd9 | 0xda => return Err("JPEG has no frame header before its scan".into()),
            _ => {}
        }
        let length = usize::from(be_u16(&bytes, offset)?);
        let segment = bytes
            .get(offset + 2..offset + length.max(2))
            .ok_or("truncated JPEG")?;
        match code {
            0xc0..=0xc2 => {
                if segment.len() < 6 {
                    return Err("truncated JPEG frame header".into());
                }
                if segment[0] != 8 {
                    return Err(format!(
                        "{}-bit JPEG is not supported; save it with 8-bit precision",
                        segment[0]
                    ));
                }
                let height = u16::from_be_bytes([segment[1], segment[2]]);
                let width = u16::from_be_bytes([segment[3], segment[4]]);
                if height == 0 {
                    return Err("JPEG height set by a DNL marker is not supported".into());
                }
                let color_space = match segment[5] {
                    1 => ColorSpace::Gray,
                    3 => ColorSpace::Rgb,
                    4 => ColorSpace::Cmyk,
                    count => {
                        return Err(format!("JPEG with {count} components is not supported"))
                    }
                };
                let (width, height) = dimensions(u32::from(width), u32::from(height))?;
                let inverted = adobe && matches!(color_space, ColorSpace::Cmyk);
                return Ok(Image {
                    width,
                    height,
                    color_space,
                    encoding: Encoding::Dct { inverted },
                    data: bytes,
                    alpha: None,
                    color_key: None,
                });
            }
            0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf => {
                return Err(
                    "lossless, hierarchical, and arithmetic-coded JPEG are not supported; save as baseline or progressive"
                        .into(),
                )
            }
            0xee if segment.starts_with(b"Adobe") => adobe = true,
            _ => {}
        }
        offset += length;
    }
}

fn png(bytes: Vec<u8>) -> Result<Image, String> {
    let mut offset = PNG_SIGNATURE.len();
    let mut header = None;
    let mut palette = None;
    let mut transparency = None;
    let mut idat = Vec::new();
    while offset < bytes.len() {
        let length = usize::try_from(be_u32(&bytes, offset)?).map_err(|_| "truncated PNG")?;
        let kind = bytes.get(offset + 4..offset + 8).ok_or("truncated PNG")?;
        let data = offset
            .checked_add(8 + length)
            .and_then(|end| bytes.get(offset + 8..end))
            .ok_or("truncated PNG")?;
        match kind {
            b"IHDR" => {
                if data.len() != 13 {
                    return Err("invalid PNG header".into());
                }
                if data[12] != 0 {
                    return Err(
                        "interlaced (Adam7) PNG is not supported; save it non-interlaced".into(),
                    );
                }
                header = Some((be_u32(data, 0)?, be_u32(data, 4)?, data[8], data[9]));
            }
            b"PLTE" => palette = Some(data.to_vec()),
            b"tRNS" => transparency = Some(data.to_vec()),
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        offset += 12 + length;
    }
    let (width, height, depth, color_type) = header.ok_or("PNG has no IHDR chunk")?;
    if depth != 8 {
        return Err(format!(
            "{depth}-bit PNG is not supported; save it with 8 bits per channel"
        ));
    }
    if idat.is_empty() {
        return Err("PNG has no IDAT chunk".into());
    }
    let (width, height) = dimensions(width, height)?;
    let passthrough = |color_space: ColorSpace, color_key| Image {
        width,
        height,
        color_space,
        encoding: Encoding::FlatePredicted,
        data: idat.clone(),
        alpha: None,
        color_key,
    };
    match color_type {
        0 => Ok(passthrough(ColorSpace::Gray, color_key(transparency, 1)?)),
        2 => Ok(passthrough(ColorSpace::Rgb, color_key(transparency, 3)?)),
        3 => {
            let palette = palette.ok_or("palette PNG has no PLTE chunk")?;
            if palette.is_empty() || palette.len() % 3 != 0 || palette.len() > 768 {
                return Err("invalid PNG palette".into());
            }
            match transparency {
                None => Ok(passthrough(ColorSpace::Indexed(palette), None)),
                Some(alpha) => split_alpha(&idat, width, height, 1, 1, |pixel| {
                    alpha.get(usize::from(pixel[0])).copied().unwrap_or(255)
                })
                .map(|(data, alpha)| Image {
                    width,
                    height,
                    color_space: ColorSpace::Indexed(palette),
                    encoding: Encoding::Flate,
                    data,
                    alpha: Some(alpha),
                    color_key: None,
                }),
            }
        }
        4 | 6 => {
            let channels = if color_type == 4 { 2 } else { 4 };
            let (data, alpha) =
                split_alpha(&idat, width, height, channels, channels - 1, |pixel| {
                    pixel[channels - 1]
                })?;
            Ok(Image {
                width,
                height,
                color_space: if color_type == 4 {
                    ColorSpace::Gray
                } else {
                    ColorSpace::Rgb
                },
                encoding: Encoding::Flate,
                data,
                alpha: Some(alpha),
                color_key: None,
            })
        }
        _ => Err("unknown PNG colour type".into()),
    }
}

/// A `tRNS` chunk on a greyscale or RGB PNG holds one 16-bit sample per channel;
/// PDF colour-key masking takes `[min max]` per channel instead.
fn color_key(transparency: Option<Vec<u8>>, channels: usize) -> Result<Option<Vec<i32>>, String> {
    let Some(values) = transparency else {
        return Ok(None);
    };
    if values.len() != channels * 2 {
        return Err("invalid PNG tRNS chunk".into());
    }
    let mut ranges = Vec::with_capacity(channels * 2);
    for pair in values.chunks_exact(2) {
        let sample = i32::from(u16::from_be_bytes([pair[0], pair[1]]));
        ranges.extend([sample, sample]);
    }
    Ok(Some(ranges))
}

/// Inflates and unfilters the scanlines, then splits every pixel into its first
/// `color_len` bytes and the alpha `alpha` reads from it; both planes are re-deflated.
fn split_alpha(
    idat: &[u8],
    width: u32,
    height: u32,
    channels: usize,
    color_len: usize,
    alpha: impl Fn(&[u8]) -> u8,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let too_large =
        || "PNG with transparency decodes to more than 64 MiB; flatten it or use JPEG".to_owned();
    let stride = (width as usize)
        .checked_mul(channels)
        .ok_or_else(too_large)?;
    let expected = (height as usize)
        .checked_mul(stride + 1)
        .filter(|total| *total <= MAX_DECODED_BYTES)
        .ok_or_else(too_large)?;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(idat)
        .map_err(|_| "PNG image data is not a valid zlib stream")?;
    if raw.len() != expected {
        return Err("PNG image data does not match its dimensions".into());
    }
    let pixels = unfilter(&raw, stride, height as usize, channels)?;
    let mut color = Vec::with_capacity(pixels.len() / channels * color_len);
    let mut mask = Vec::with_capacity(pixels.len() / channels);
    for pixel in pixels.chunks_exact(channels) {
        color.extend_from_slice(&pixel[..color_len]);
        mask.push(alpha(pixel));
    }
    Ok((deflate(&color), deflate(&mask)))
}

/// Reverses the five PNG scanline filters; each row starts with its filter type.
fn unfilter(raw: &[u8], stride: usize, rows: usize, bpp: usize) -> Result<Vec<u8>, String> {
    let mut out = vec![0_u8; rows * stride];
    for row in 0..rows {
        let filter = raw[row * (stride + 1)];
        let source = &raw[row * (stride + 1) + 1..(row + 1) * (stride + 1)];
        let (done, rest) = out.split_at_mut(row * stride);
        let current = &mut rest[..stride];
        let previous = (row > 0).then(|| &done[(row - 1) * stride..]);
        for index in 0..stride {
            let left = if index >= bpp {
                current[index - bpp]
            } else {
                0
            };
            let up = previous.map_or(0, |line| line[index]);
            let up_left = previous
                .filter(|_| index >= bpp)
                .map_or(0, |line| line[index - bpp]);
            let predicted = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, up_left),
                _ => return Err("PNG has an unknown scanline filter".into()),
            };
            current[index] = source[index].wrapping_add(predicted);
        }
    }
    Ok(out)
}

fn paeth(left: u8, up: u8, up_left: u8) -> u8 {
    let estimate = i16::from(left) + i16::from(up) - i16::from(up_left);
    let (pa, pb, pc) = (
        (estimate - i16::from(left)).abs(),
        (estimate - i16::from(up)).abs(),
        (estimate - i16::from(up_left)).abs(),
    );
    if pa <= pb && pa <= pc {
        left
    } else if pb <= pc {
        up
    } else {
        up_left
    }
}
