use crate::win_ansi::valid;
use crate::{
    Align, Direction, Fraction, ImageSize, Marker, Percent, Pt, Rgb, TextAlign, Typography,
    VerticalAlign,
};

/// Letter and word spacing in points, either sign; the largest page side.
pub(super) const MAX_SPACING: f32 = 14_400.0;

use super::ProtocolError;

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ProtocolError::InvalidLength)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ProtocolError::TruncatedRecord)?;
        self.offset = end;
        Ok(value)
    }

    pub(super) fn u16(&mut self) -> Result<u16, ProtocolError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    pub(super) fn u32(&mut self) -> Result<u32, ProtocolError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    pub(super) fn positive_f32(&mut self, name: &'static str) -> Result<f32, ProtocolError> {
        positive_f32(self.take(4)?, name)
    }

    pub(super) fn nonnegative_f32(&mut self, name: &'static str) -> Result<f32, ProtocolError> {
        nonnegative_f32(self.take(4)?, name)
    }

    /// Bits 0 to 2 are the alignment and bits 3 and 4 the base direction.
    pub(super) fn align(&mut self) -> Result<(TextAlign, Direction), ProtocolError> {
        let byte = self.take(1)?[0];
        let align = match byte & 7 {
            0 => TextAlign::Left,
            1 => TextAlign::Center,
            2 => TextAlign::Right,
            3 => TextAlign::Justify,
            4 => TextAlign::Start,
            5 => TextAlign::End,
            _ => return Err(ProtocolError::InvalidTextAlignment),
        };
        let direction = match byte >> 3 {
            0 => Direction::Ltr,
            1 => Direction::Rtl,
            2 => Direction::Auto,
            _ => return Err(ProtocolError::InvalidTextAlignment),
        };
        Ok((align, direction))
    }

    /// Run flag bit 5 slants the face, 6, 7, and 11 mean an f32 letter spacing, word spacing, and
    /// baseline shift follow, 9 turns kerning off, 12 forbids wrapping, and bits 13 and 14 make a
    /// marker: 1 outside, 2 inside.
    pub(super) fn typography(&mut self, flags: u16) -> Result<Typography, ProtocolError> {
        let mut spacing = |bit: u16, name: &'static str| {
            if flags & bit == 0 {
                return Ok(0.0);
            }
            let value = f32::from_le_bytes(self.take(4)?.try_into().unwrap());
            // CSS allows negative spacing; the bound keeps every line width finite.
            if value.is_finite() && value.abs() <= MAX_SPACING {
                Ok(value)
            } else {
                Err(ProtocolError::InvalidValue(name))
            }
        };
        Ok(Typography {
            letter_spacing: spacing(64, "letter spacing")?,
            word_spacing: spacing(128, "word spacing")?,
            rise: spacing(2048, "baseline shift")?,
            oblique: flags & 32 != 0,
            kerning: flags & 512 == 0,
            nowrap: flags & 4096 != 0,
            marker: match (flags >> 13) & 3 {
                0 => Marker::None,
                1 => Marker::Outside,
                2 => Marker::Inside,
                _ => return Err(ProtocolError::InvalidValue("run flags")),
            },
        })
    }

    pub(super) fn rgb(&mut self) -> Result<Rgb, ProtocolError> {
        Ok(Rgb::from(<[u8; 3]>::try_from(self.take(3)?).unwrap()))
    }

    pub(super) fn text(&mut self, furniture: bool) -> Result<&'a str, ProtocolError> {
        let text = self.utf8_text()?;
        if !valid(text, furniture) {
            return Err(ProtocolError::ControlCharacter);
        }
        Ok(text)
    }

    pub(super) fn f64(&mut self) -> Result<f64, ProtocolError> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    /// Any UTF-8 text; the caller checks its content.
    pub(super) fn utf8(&mut self) -> Result<&'a str, ProtocolError> {
        self.utf8_text()
    }

    fn utf8_text(&mut self) -> Result<&'a str, ProtocolError> {
        let length =
            usize::try_from(self.u32()?).map_err(|_| ProtocolError::InvalidStringLength)?;
        std::str::from_utf8(self.take(length)?).map_err(|_| ProtocolError::InvalidUtf8)
    }

    pub(super) fn columns(
        &mut self,
    ) -> Result<Vec<(crate::ColumnWidth, crate::Slot)>, ProtocolError> {
        let count = usize::from(self.u16()?);
        if count == 0 || count > crate::command::MAX_COLUMNS {
            return Err(ProtocolError::InvalidColumnCount);
        }
        (0..count)
            .map(|_| Ok((self.column()?, self.slot()?)))
            .collect()
    }

    /// Bits 0-1 align a cell top, middle, or bottom, and bit 2 means a u16 row
    /// span of 2 or more follows; 8 joins the cell to the left, and 16 marks a
    /// column a cell from an earlier row covers.
    fn slot(&mut self) -> Result<crate::Slot, ProtocolError> {
        let flags = self.take(1)?[0];
        let align = [
            VerticalAlign::Top,
            VerticalAlign::Middle,
            VerticalAlign::Bottom,
        ];
        Ok(match flags {
            0..=2 => crate::Slot::Cell {
                rows: 1,
                align: align[usize::from(flags)],
            },
            4..=6 => crate::Slot::Cell {
                rows: Some(self.u16()?)
                    .filter(|rows| *rows > 1)
                    .ok_or(ProtocolError::InvalidValue("row span"))?,
                align: align[usize::from(flags & 3)],
            },
            8 => crate::Slot::Join,
            16 => crate::Slot::Covered,
            _ => return Err(ProtocolError::InvalidValue("cell slot")),
        })
    }

    pub(super) fn column(&mut self) -> Result<crate::ColumnWidth, ProtocolError> {
        let kind = self.take(1)?[0];
        let value = self.positive_f32("column width")?;
        Ok(match kind {
            0 => crate::ColumnWidth::Fixed(pt(value)?),
            1 => crate::ColumnWidth::Fraction(Fraction::new(value)?),
            2 => crate::ColumnWidth::Percent(Percent::new(value)?),
            _ => return Err(ProtocolError::InvalidColumnKind),
        })
    }

    /// Hosts percent-encode non-ASCII before sending, so a URI is printable ASCII without spaces.
    pub(super) fn uri(&mut self) -> Result<&'a str, ProtocolError> {
        let uri = self.utf8_text()?;
        if uri.is_empty() || !uri.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
            return Err(ProtocolError::InvalidLink);
        }
        Ok(uri)
    }

    /// Alternative text is stored for accessibility tagging, so it accepts any
    /// Unicode except control characters.
    pub(super) fn alt(&mut self) -> Result<&'a str, ProtocolError> {
        let alt = self.utf8_text()?;
        if alt.chars().any(char::is_control) {
            return Err(ProtocolError::InvalidValue("alt text"));
        }
        Ok(alt)
    }

    pub(super) fn image_size(&mut self) -> Result<ImageSize, ProtocolError> {
        Ok(match self.take(1)?[0] {
            0 => ImageSize::Auto,
            1 => ImageSize::Fixed(pt(self.positive_f32("image width")?)?),
            2 => ImageSize::Percent(Percent::new(self.positive_f32("image width")?)?),
            _ => return Err(ProtocolError::InvalidValue("image width kind")),
        })
    }

    /// Kind 0 is `auto`, 1 points (zero allowed), 2 a positive percent.
    pub(super) fn size(&mut self, name: &'static str) -> Result<ImageSize, ProtocolError> {
        Ok(match self.take(1)?[0] {
            0 => ImageSize::Auto,
            1 => ImageSize::Fixed(pt(self.nonnegative_f32(name)?)?),
            2 => ImageSize::Percent(Percent::new(self.positive_f32(name)?)?),
            _ => return Err(ProtocolError::InvalidValue(name)),
        })
    }

    /// A height in points, read as a `size` that may not be a percent.
    pub(super) fn height(&mut self) -> Result<Option<Pt>, ProtocolError> {
        match self.size("height")? {
            ImageSize::Auto => Ok(None),
            ImageSize::Fixed(height) => Ok(Some(height)),
            ImageSize::Percent(_) => Err(ProtocolError::InvalidValue("height")),
        }
    }

    /// 0 is `auto`, then stretch, start, end, and center.
    pub(super) fn alignment(&mut self) -> Result<Option<Align>, ProtocolError> {
        Ok(Some(match self.take(1)?[0] {
            0 => return Ok(None),
            1 => Align::Stretch,
            2 => Align::Start,
            3 => Align::End,
            4 => Align::Center,
            _ => return Err(ProtocolError::InvalidValue("flex alignment")),
        }))
    }

    pub(super) fn done(&self) -> Result<(), ProtocolError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidRecordLength)
        }
    }
}

pub(super) fn pt(value: f32) -> Result<crate::Pt, ProtocolError> {
    crate::Pt::new(value).map_err(ProtocolError::from)
}

pub(super) fn positive_f32(bytes: &[u8], name: &'static str) -> Result<f32, ProtocolError> {
    let value = f32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ProtocolError::TruncatedRecord)?,
    );
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        Err(ProtocolError::InvalidValue(name))
    }
}

pub(super) fn nonnegative_f32(bytes: &[u8], name: &'static str) -> Result<f32, ProtocolError> {
    let value = f32::from_le_bytes(
        bytes
            .try_into()
            .map_err(|_| ProtocolError::TruncatedRecord)?,
    );
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(ProtocolError::InvalidValue(name))
    }
}
