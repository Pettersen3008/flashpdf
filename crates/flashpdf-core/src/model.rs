#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Pt(pub(crate) f32);

impl Pt {
    pub const ZERO: Self = Self(0.0);

    pub fn new(value: f32) -> Result<Self, RenderError> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(value))
        } else {
            Err(RenderError::InvalidPoint)
        }
    }

    pub(crate) const fn get(self) -> f32 {
        self.0
    }
}

impl std::ops::Add for Pt {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::AddAssign for Pt {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl std::ops::Sub for Pt {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::SubAssign for Pt {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Page {
    pub(crate) width: Pt,
    pub(crate) height: Pt,
    pub(crate) margin: Pt,
}

impl Page {
    pub const A4: Self = Self {
        width: Pt(595.0),
        height: Pt(842.0),
        margin: Pt(36.0),
    };

    pub const LETTER: Self = Self {
        width: Pt(612.0),
        height: Pt(792.0),
        margin: Pt(36.0),
    };

    pub fn new(width: Pt, height: Pt, margin: Pt) -> Result<Self, RenderError> {
        if width.0 > margin.0 * 2.0 && height.0 > margin.0 * 2.0 {
            Ok(Self {
                width,
                height,
                margin,
            })
        } else {
            Err(RenderError::InvalidPage)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges<T> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> Edges<T> {
    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    pub const fn into_array(self) -> [T; 4] {
        [self.top, self.right, self.bottom, self.left]
    }
}

impl<T> Edges<T> {
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        [&self.top, &self.right, &self.bottom, &self.left].into_iter()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };

    pub(crate) fn normalized(self) -> [f32; 3] {
        [
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
        ]
    }
}

impl From<[u8; 3]> for Rgb {
    fn from([r, g, b]: [u8; 3]) -> Self {
        Self { r, g, b }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct FontId(u8);

impl FontId {
    pub const HELVETICA: Self = Self(0);
    pub const HELVETICA_BOLD: Self = Self(1);

    pub(crate) const fn new(value: u8) -> Self {
        Self(value)
    }

    pub(crate) const fn slot(self) -> u8 {
        self.0
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Font slots 0 and 1 preserve the regular and bold Helvetica fallback.
pub const HELVETICA: FontId = FontId::HELVETICA;
pub const HELVETICA_BOLD: FontId = FontId::HELVETICA_BOLD;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: Pt,
    pub align: TextAlign,
    pub color: Rgb,
    pub font: FontId,
}

impl TextStyle {
    pub const fn plain(size: Pt) -> Self {
        Self {
            size,
            align: TextAlign::Left,
            color: Rgb::BLACK,
            font: FontId::HELVETICA,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxStyle {
    pub margin: Edges<Pt>,
    pub padding: Edges<Pt>,
    pub border: Edges<Pt>,
    pub background: Option<Rgb>,
    pub border_color: Edges<Rgb>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Fraction(f32);

impl Fraction {
    pub fn new(value: f32) -> Result<Self, RenderError> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value))
        } else {
            Err(RenderError::InvalidLayout)
        }
    }

    pub(crate) const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Percent(f32);

impl Percent {
    pub fn new(value: f32) -> Result<Self, RenderError> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value))
        } else {
            Err(RenderError::InvalidLayout)
        }
    }

    pub(crate) const fn get(self) -> f32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderError {
    InvalidPoint,
    InvalidPage,
    InvalidFontSize,
    UnsupportedCharacter(char),
    TextTooWide,
    PageOverflow,
    InvalidLayout,
    MissingGlyph,
    DocumentTooLarge,
}
