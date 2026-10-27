use super::ProtocolError;

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Opcode {
    Page = 1,
    Spacer = 2,
    StackStart = 3,
    StackEnd = 4,
    RowStart = 5,
    RowEnd = 6,
    PageBreak = 7,
    Footer = 8,
    Paragraph = 9,
    TableStart = 10,
    TableEnd = 11,
    Image = 12,
    Header = 13,
    Metadata = 14,
    BoxStart = 17,
    BoxEnd = 18,
    PdfA = 20,
    Attachment = 21,
    Anchor = 22,
    Avoid = 23,
    FlexStart = 24,
    FlexEnd = 25,
    Family = 26,
    Stylesheet = 27,
    Setup = 28,
    Block = 29,
    Open = 30,
    Text = 31,
    Close = 32,
    Drawing = 33,
    End = 255,
}

impl TryFrom<u8> for Opcode {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Page,
            2 => Self::Spacer,
            3 => Self::StackStart,
            4 => Self::StackEnd,
            5 => Self::RowStart,
            6 => Self::RowEnd,
            7 => Self::PageBreak,
            8 => Self::Footer,
            9 => Self::Paragraph,
            10 => Self::TableStart,
            11 => Self::TableEnd,
            12 => Self::Image,
            13 => Self::Header,
            14 => Self::Metadata,
            17 => Self::BoxStart,
            18 => Self::BoxEnd,
            20 => Self::PdfA,
            21 => Self::Attachment,
            22 => Self::Anchor,
            23 => Self::Avoid,
            24 => Self::FlexStart,
            25 => Self::FlexEnd,
            26 => Self::Family,
            27 => Self::Stylesheet,
            28 => Self::Setup,
            29 => Self::Block,
            30 => Self::Open,
            31 => Self::Text,
            32 => Self::Close,
            33 => Self::Drawing,
            255 => Self::End,
            _ => return Err(ProtocolError::UnknownOpcode),
        })
    }
}
