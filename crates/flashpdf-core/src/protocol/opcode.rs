use super::ProtocolError;

#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum Opcode {
    Text = 1,
    Spacer = 2,
    StackStart = 3,
    StackEnd = 4,
    RowStart = 5,
    RowEnd = 6,
    PageBreak = 7,
    StyledText = 16,
    BoxStart = 17,
    BoxEnd = 18,
    End = 255,
}

impl TryFrom<u8> for Opcode {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Text,
            2 => Self::Spacer,
            3 => Self::StackStart,
            4 => Self::StackEnd,
            5 => Self::RowStart,
            6 => Self::RowEnd,
            7 => Self::PageBreak,
            16 => Self::StyledText,
            17 => Self::BoxStart,
            18 => Self::BoxEnd,
            255 => Self::End,
            _ => return Err(ProtocolError::UnknownOpcode),
        })
    }
}
