use super::ProtocolError;

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Opcode {
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
    BoxStart = 17,
    BoxEnd = 18,
    End = 255,
}

impl TryFrom<u8> for Opcode {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
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
            17 => Self::BoxStart,
            18 => Self::BoxEnd,
            255 => Self::End,
            _ => return Err(ProtocolError::UnknownOpcode),
        })
    }
}
