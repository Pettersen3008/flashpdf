use crate::win_ansi::valid;
use crate::{Fraction, Percent, Rgb, TextAlign};

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

    pub(super) fn align(&mut self) -> Result<TextAlign, ProtocolError> {
        Ok(match self.take(1)?[0] {
            0 => TextAlign::Left,
            1 => TextAlign::Center,
            2 => TextAlign::Right,
            _ => return Err(ProtocolError::InvalidTextAlignment),
        })
    }

    pub(super) fn rgb(&mut self) -> Result<Rgb, ProtocolError> {
        Ok(Rgb::from(<[u8; 3]>::try_from(self.take(3)?).unwrap()))
    }

    pub(super) fn text(&mut self, footer: bool) -> Result<&'a str, ProtocolError> {
        let text = self.utf8_text()?;
        if !valid(text, footer) {
            return Err(ProtocolError::ControlCharacter);
        }
        Ok(text)
    }

    fn utf8_text(&mut self) -> Result<&'a str, ProtocolError> {
        let length =
            usize::try_from(self.u32()?).map_err(|_| ProtocolError::InvalidStringLength)?;
        std::str::from_utf8(self.take(length)?).map_err(|_| ProtocolError::InvalidUtf8)
    }

    pub(super) fn columns(&mut self) -> Result<Vec<crate::ColumnWidth>, ProtocolError> {
        let count = usize::from(self.u16()?);
        if count == 0 || count > crate::command::MAX_COLUMNS {
            return Err(ProtocolError::InvalidColumnCount);
        }
        (0..count).map(|_| self.column()).collect()
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
