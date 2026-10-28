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
    pub(crate) margin: Edges<Pt>,
}

impl Page {
    pub const A4: Self = Self {
        width: Pt(595.0),
        height: Pt(842.0),
        margin: Edges::all(Pt(36.0)),
    };

    pub const LETTER: Self = Self {
        width: Pt(612.0),
        height: Pt(792.0),
        margin: Edges::all(Pt(36.0)),
    };

    pub fn new(width: Pt, height: Pt, margin: Edges<Pt>) -> Result<Self, RenderError> {
        if width.0 > margin.left.0 + margin.right.0 && height.0 > margin.top.0 + margin.bottom.0 {
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

    /// Three decimals, which still round back to the same 8-bit channel.
    pub(crate) fn normalized(self) -> [f32; 3] {
        [self.r, self.g, self.b]
            .map(|channel| (f32::from(channel) * 1000.0 / 255.0).round() / 1000.0)
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
    /// ending in a hard break align to the start.
    Justify,
    /// Left in a left-to-right paragraph, right in a right-to-left one.
    Start,
    End,
}

/// A paragraph's base direction: CSS `direction`, or the first strong character for `dir="auto"`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
    Auto,
}

/// Where a table cell's content sits when its row is taller than the content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Middle,
    Bottom,
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

/// A list marker: hung before the paragraph's first line, or filling its start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    None,
    Outside,
    Inside,
}

/// Spacing, slant, and placement of a run; `NORMAL` changes nothing.
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
    /// Baseline shift in points, up positive: `vertical-align: sub | super`.
    pub rise: f32,
    /// A word wider than its line is an error, not a break: `white-space: pre | nowrap`.
    pub nowrap: bool,
    /// Only the first run of a paragraph may be a marker. The shapes U+2022, U+25E6, and
    /// U+25AA draw a disc, circle, or square; any other text is shown in the run's font.
    pub marker: Marker,
}

impl Typography {
    pub const NORMAL: Self = Self {
        letter_spacing: 0.0,
        word_spacing: 0.0,
        oblique: false,
        kerning: true,
        rise: 0.0,
        nowrap: false,
        marker: Marker::None,
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

/// Underline, strike-through, link, and opacity of a run; painted per line segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunDecoration {
    pub underline: bool,
    pub line_through: bool,
    /// CSS `text-decoration-color`; `None` is the run's colour.
    pub color: Option<Rgb>,
    /// Index into the paragraph's URI table.
    pub link: Option<u16>,
    pub alpha: u8,
    /// CSS `background-color` of an inline element, painted behind each line segment.
    pub background: Option<Rgb>,
}

impl Default for RunDecoration {
    fn default() -> Self {
        Self {
            underline: false,
            line_through: false,
            link: None,
            color: None,
            background: None,
            alpha: OPAQUE,
        }
    }
}

/// An alpha of 255 paints without a graphics state.
pub const OPAQUE: u8 = 255;

/// An image's requested width; `Auto` is the intrinsic size at 96 px per inch, capped to the container.
/// A width in points or percent of the container's width. An image's `Auto` is its
/// intrinsic size at 96 px per inch, capped to the container.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageSize {
    Auto,
    Fixed(Pt),
    Percent(Percent),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

/// Elliptical corner radii, clockwise from the top left, each a horizontal then a vertical value.
/// Bit `i` of `percent` makes `values[i]` a percent of the box width (even `i`) or height (odd `i`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    pub values: [f32; 8],
    pub percent: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxStyle {
    pub margin: Edges<Pt>,
    pub padding: Edges<Pt>,
    pub border: Edges<Pt>,
    pub background: Option<Rgb>,
    pub border_color: Edges<Rgb>,
    pub border_style: Edges<LineStyle>,
    /// Rounds the background, and the border when all four sides match.
    pub radius: Option<Radii>,
    pub alpha: u8,
}

impl BoxStyle {
    /// Whether the four borders share one width, colour, and style, which one stroked outline can draw.
    /// Borders with no width need no more.
    pub(crate) fn uniform_border(&self) -> bool {
        self.border.iter().all(|edge| *edge == self.border.top)
            && (self.border.top == Pt::ZERO
                || (self
                    .border_color
                    .iter()
                    .all(|color| *color == self.border_color.top)
                    && self
                        .border_style
                        .iter()
                        .all(|line| *line == self.border_style.top)))
    }
}

/// CSS sizing of a box. `Auto` widths and `None` heights follow the content, and each
/// value sizes the content box unless `border_box` is set.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxSize {
    pub width: ImageSize,
    /// `Auto` is zero in block flow and the min-content width for a flex item.
    pub min_width: ImageSize,
    /// `Auto` is `none`.
    pub max_width: ImageSize,
    pub height: Option<Pt>,
    pub min_height: Pt,
    pub max_height: Option<Pt>,
    pub border_box: bool,
    /// One bit per `auto` margin in `Edges` order: top 1, right 2, bottom 4, left 8.
    pub auto_margin: u8,
}

impl BoxSize {
    pub const AUTO: Self = Self {
        width: ImageSize::Auto,
        min_width: ImageSize::Auto,
        max_width: ImageSize::Auto,
        height: None,
        min_height: Pt::ZERO,
        max_height: None,
        border_box: false,
        auto_margin: 0,
    };
}

/// `justify-content`; `start` and `end` are `flex-start` and `flex-end`, as nothing reverses.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Justify {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// `align-items` and `align-self`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Align {
    Stretch,
    Start,
    End,
    Center,
}

/// A CSS flex container. Only a row wraps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flex {
    pub column: bool,
    pub wrap: bool,
    pub justify: Justify,
    pub align: Align,
    /// `align-content`; `None` is `stretch` and `normal`.
    pub align_content: Option<Justify>,
    /// Between items along the main axis.
    pub gap: Pt,
    /// Between the rows of a wrapping row.
    pub row_gap: Pt,
    /// Bounds of the container's inner height; equal when the height is definite.
    pub min_height: Pt,
    pub max_height: Option<Pt>,
}

/// `align` `None` follows the container's `align-items`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlexItem {
    pub grow: f32,
    pub shrink: f32,
    pub basis: ImageSize,
    pub align: Option<Align>,
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
    /// A `white-space: pre` or `nowrap` line is wider than its container.
    LineTooWide,
    PageOverflow,
    /// A table row and its repeated header exceed an empty page; the index is the table child.
    TableRowOverflow(usize),
    InvalidLayout,
    /// The embedded font has no glyph for the character.
    MissingGlyph(char),
    DocumentTooLarge,
    ImageTooWide,
    TooManyPages,
    /// A link names a destination no element registered.
    UnknownDestination,
    /// Content is taller than a box's fixed or maximum height, or than its flex container.
    ContentOverflow,
}
