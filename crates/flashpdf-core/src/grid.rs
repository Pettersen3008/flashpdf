// Item alignment: 0 auto, 1 stretch, 2 start, 3 end, 4 center, 5 normal.
// Content alignment: 0 stretch, 1 start, 2 end, 3 center, 4 between, 5 around, 6 evenly.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub columns: Vec<GridTrack>,
    pub rows: Vec<GridTrack>,
    pub auto_columns: Vec<GridTrack>,
    pub auto_rows: Vec<GridTrack>,
    pub column_flow: bool,
    /// The immediately wrapping Box belongs to this grid.
    pub boxed: bool,
    pub justify_items: u8,
    pub align_items: u8,
    pub justify_content: u8,
    pub align_content: u8,
    pub column_gap: f32,
    pub row_gap: f32,
    pub min_height: f32,
    pub max_height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridTrack {
    pub min: GridBreadth,
    pub max: GridBreadth,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridBreadth {
    Length(f32),
    Percent(f32),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GridLine {
    Auto,
    Line(i16),
    Span(u16),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridItem {
    pub column: [GridLine; 2],
    pub row: [GridLine; 2],
    pub justify: u8,
    pub align: u8,
}
