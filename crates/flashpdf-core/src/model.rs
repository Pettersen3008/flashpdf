#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Pt(pub(crate) f32);

impl Pt {
    pub fn new(value: f32) -> Result<Self, RenderError> {
        if value.is_finite() && value >= 0.0 {
            Ok(Self(value))
        } else {
            Err(RenderError::InvalidPoint)
        }
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
/// Font slots 0 and 1 preserve the regular and bold Helvetica fallback.
pub const HELVETICA: u8 = 0;
pub const HELVETICA_BOLD: u8 = 1;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxStyle {
    pub margin: [Pt; 4],
    pub padding: [Pt; 4],
    pub border: [Pt; 4],
    pub background: Option<[u8; 3]>,
    pub border_color: [[u8; 3]; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command<'a> {
    Text {
        text: &'a str,
        size: Pt,
    },
    StyledText {
        text: &'a str,
        size: Pt,
        align: TextAlign,
        color: [u8; 3],
        font: u8,
    },
    BoxStart {
        style: BoxStyle,
    },
    BoxEnd,
    Spacer(Pt),
    StackStart {
        gap: Pt,
    },
    StackEnd,
    RowStart {
        columns: &'a [ColumnWidth],
    },
    RowEnd,
    PageBreak,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    Fixed(Pt),
    Fraction(Pt),
    Percent(Pt),
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
}
