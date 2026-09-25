mod helvetica;
mod truetype;

pub(crate) use truetype::EmbeddedFont;

use crate::{FontId, RenderError};

pub(crate) enum Font {
    Helvetica { bold: bool },
    Embedded(Box<EmbeddedFont>),
}

pub(crate) struct FontBook {
    fonts: Vec<Font>,
    used: Vec<bool>,
}

impl FontBook {
    pub(crate) fn new(embedded: Vec<EmbeddedFont>) -> Self {
        let mut fonts = vec![
            Font::Helvetica { bold: false },
            Font::Helvetica { bold: true },
        ];
        fonts.extend(
            embedded
                .into_iter()
                .map(|font| Font::Embedded(Box::new(font))),
        );
        let used = vec![false; fonts.len()];
        Self { fonts, used }
    }

    pub(crate) fn get(&self, id: FontId) -> Option<&Font> {
        self.fonts.get(id.index())
    }

    pub(crate) fn len(&self) -> usize {
        self.fonts.len()
    }

    pub(crate) fn mark_used(&mut self, id: FontId) {
        self.used[id.index()] = true;
    }

    pub(crate) fn into_parts(self) -> (Vec<Font>, Vec<bool>) {
        (self.fonts, self.used)
    }
}

impl Font {
    pub(crate) fn width(&self, byte: u8) -> Result<u16, RenderError> {
        match self {
            Self::Helvetica { bold } => Ok(helvetica::width(*bold, byte)),
            Self::Embedded(font) => {
                font.widths[usize::from(byte) - 32].ok_or(RenderError::MissingGlyph)
            }
        }
    }

    /// Ascent, descent and line gap per 1000 units; a line is ascent - descent + gap.
    pub(crate) fn metrics(&self) -> (f32, f32, f32) {
        match self {
            Self::Helvetica { .. } => (718.0, -207.0, 0.0),
            Self::Embedded(font) => (font.ascent, font.descent, font.line_gap),
        }
    }
}

/// `/F1` is slot 0, so a PDF resource name never collides with a slot number.
pub(crate) fn font_name(font: FontId, buffer: &mut [u8; 4]) -> &[u8] {
    let number = u32::from(font.slot()) + 1;
    buffer[0] = b'F';
    let mut length = 1;
    for divisor in [100, 10] {
        if number >= divisor {
            buffer[length] = b'0' + (number / divisor % 10) as u8;
            length += 1;
        }
    }
    buffer[length] = b'0' + (number % 10) as u8;
    &buffer[..length + 1]
}
