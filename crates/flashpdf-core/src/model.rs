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
    pub const HELVETICA_OBLIQUE: Self = Self(2);
    pub const HELVETICA_BOLD_OBLIQUE: Self = Self(3);
    pub const TIMES: Self = Self(4);
    pub const TIMES_BOLD: Self = Self(5);
    pub const TIMES_ITALIC: Self = Self(6);
    pub const TIMES_BOLD_ITALIC: Self = Self(7);
    pub const COURIER: Self = Self(8);
    pub const COURIER_BOLD: Self = Self(9);
    pub const COURIER_OBLIQUE: Self = Self(10);
    pub const COURIER_BOLD_OBLIQUE: Self = Self(11);

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

/// Font slots 0 to 11 are the built-in Helvetica, Times, and Courier faces; embedded fonts follow.
pub const HELVETICA: FontId = FontId::HELVETICA;
pub const HELVETICA_BOLD: FontId = FontId::HELVETICA_BOLD;
pub const HELVETICA_OBLIQUE: FontId = FontId::HELVETICA_OBLIQUE;
pub const HELVETICA_BOLD_OBLIQUE: FontId = FontId::HELVETICA_BOLD_OBLIQUE;
pub const TIMES: FontId = FontId::TIMES;
pub const TIMES_BOLD: FontId = FontId::TIMES_BOLD;
pub const TIMES_ITALIC: FontId = FontId::TIMES_ITALIC;
pub const TIMES_BOLD_ITALIC: FontId = FontId::TIMES_BOLD_ITALIC;
pub const COURIER: FontId = FontId::COURIER;
pub const COURIER_BOLD: FontId = FontId::COURIER_BOLD;
pub const COURIER_OBLIQUE: FontId = FontId::COURIER_OBLIQUE;
pub const COURIER_BOLD_OBLIQUE: FontId = FontId::COURIER_BOLD_OBLIQUE;
pub(crate) const BUILT_IN: usize = 12;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    /// Stretches the spaces of every wrapped line; the last line and a line
    /// ending in a hard break stay left-aligned.
    Justify,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size: Pt,
    pub align: TextAlign,
    pub color: Rgb,
    pub font: FontId,
    pub typography: Typography,
}

impl TextStyle {
    pub const fn plain(size: Pt) -> Self {
        Self {
            size,
            align: TextAlign::Left,
            color: Rgb::BLACK,
            font: FontId::HELVETICA,
            typography: Typography::NORMAL,
        }
    }
}

/// Spacing and slant of a run; `NORMAL` changes nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typography {
    /// CSS `letter-spacing` in points, added after every character.
    pub letter_spacing: f32,
    /// CSS `word-spacing` in points, added after every space and no-break space.
    pub word_spacing: f32,
    /// Slants the upright face, for italic text in a family without an italic face.
    pub oblique: bool,
    /// CSS `font-kerning`: `auto` and `normal` kern pairs within a word.
    pub kerning: bool,
}

impl Typography {
    pub const NORMAL: Self = Self {
        letter_spacing: 0.0,
        word_spacing: 0.0,
        oblique: false,
        kerning: true,
    };
}

/// One styled slice of a paragraph's text; `len` bytes follow the previous run's.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextRun {
    pub len: usize,
    pub font: FontId,
    pub size: Pt,
    /// CSS `line-height`; `None` is `normal`, the font's ascent, descent, and line gap.
    pub line_height: Option<Pt>,
    pub color: Rgb,
    /// Ends the line after this run without adding a space.
    pub hard_break: bool,
    pub decoration: RunDecoration,
    pub typography: Typography,
}

/// Underline, strike-through, and link of a run; painted per line segment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunDecoration {
    pub underline: bool,
    pub line_through: bool,
    /// CSS `text-decoration-color`; `None` is the run's colour.
    pub color: Option<Rgb>,
    /// Index into the paragraph's URI table.
    pub link: Option<u16>,
}

/// An image's requested width; `Auto` is the intrinsic size at 96 px per inch, capped to the container.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageSize {
    Auto,
    Fixed(Pt),
    Percent(Percent),
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
    /// A table row and its repeated header exceed an empty page; the index is the table child.
    TableRowOverflow(usize),
    InvalidLayout,
    /// The embedded font has no glyph for the character.
    MissingGlyph(char),
    DocumentTooLarge,
    ImageTooWide,
}
