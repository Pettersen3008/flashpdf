#[derive(Debug)]
pub enum ProtocolError {
    FontsAfterDocument,
    TooManyFonts,
    InvalidFont(String),
    DataAfterEnd,
    RecordTooLarge,
    BlockTooLarge,
    InvalidMagic,
    UnknownVersion,
    InvalidNesting,
    InvalidTextAlignment,
    UnknownFont,
    InvalidBackground,
    EmptyDocument,
    UnknownOpcode,
    MissingHeader,
    NestingTooDeep,
    RowCellCountMismatch,
    TruncatedProtocol,
    InvalidLength,
    TruncatedRecord,
    InvalidStringLength,
    InvalidUtf8,
    UnsupportedWinAnsi,
    InvalidColumnCount,
    InvalidColumnKind,
    InvalidRecordLength,
    InvalidValue(&'static str),
    Render(crate::RenderError),
    InvalidImage(String),
    ImagesTooLarge,
    TooManyImages,
    UnknownImage,
    InvalidLink,
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::FontsAfterDocument => "fonts must be registered before document",
            Self::TooManyFonts => "too many fonts",
            Self::InvalidFont(message) => return formatter.write_str(message),
            Self::DataAfterEnd => "data after end",
            Self::RecordTooLarge => "record too large",
            Self::BlockTooLarge => "block exceeds 16 MiB",
            Self::InvalidMagic => "invalid magic",
            Self::UnknownVersion => "unknown version",
            Self::InvalidNesting => "invalid nesting",
            Self::InvalidTextAlignment => "invalid text alignment",
            Self::UnknownFont => "unknown font",
            Self::InvalidBackground => "invalid background",
            Self::EmptyDocument => "empty document",
            Self::UnknownOpcode => "unknown opcode",
            Self::MissingHeader => "missing header",
            Self::NestingTooDeep => "nesting exceeds 64",
            Self::RowCellCountMismatch => "row cell count mismatch",
            Self::TruncatedProtocol => "truncated protocol",
            Self::InvalidLength => "invalid length",
            Self::TruncatedRecord => "truncated record",
            Self::InvalidStringLength => "invalid string length",
            Self::InvalidUtf8 => "invalid UTF-8",
            Self::UnsupportedWinAnsi => "unsupported WinAnsi character",
            Self::InvalidColumnCount => "invalid column count",
            Self::InvalidColumnKind => "invalid column kind",
            Self::InvalidRecordLength => "invalid record length",
            Self::InvalidValue(name) => return write!(formatter, "invalid {name}"),
            Self::Render(error) => return write!(formatter, "{error:?}"),
            Self::InvalidImage(message) => return formatter.write_str(message),
            Self::ImagesTooLarge => "images exceed 64 MiB per render",
            Self::TooManyImages => "too many images",
            Self::UnknownImage => "unknown image",
            Self::InvalidLink => "invalid link: URIs must be printable ASCII",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ProtocolError {}

impl From<crate::RenderError> for ProtocolError {
    fn from(error: crate::RenderError) -> Self {
        Self::Render(error)
    }
}
