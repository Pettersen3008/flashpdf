use super::{Cursor, ProtocolError};
use crate::command::MAX_COLUMNS;
use crate::{Grid, GridBreadth, GridItem, GridLine, GridTrack};

impl Grid {
    pub(crate) fn validate(&self) -> Result<(), ProtocolError> {
        for tracks in [
            &self.columns,
            &self.rows,
            &self.auto_columns,
            &self.auto_rows,
        ] {
            if tracks.len() > MAX_COLUMNS {
                return Err(ProtocolError::InvalidValue("grid track count"));
            }
            for track in tracks {
                if matches!(track.min, GridBreadth::Fr(_)) {
                    return Err(ProtocolError::InvalidValue("grid minimum track"));
                }
                for breadth in [track.min, track.max] {
                    match breadth {
                        GridBreadth::Length(value)
                        | GridBreadth::Percent(value)
                        | GridBreadth::Fr(value) => nonnegative(value, "grid track")?,
                        _ => {}
                    }
                }
            }
        }
        if self.justify_items > 5 || self.align_items > 5 {
            return Err(ProtocolError::InvalidValue("grid item alignment"));
        }
        if self.justify_content > 6 || self.align_content > 6 {
            return Err(ProtocolError::InvalidValue("grid content alignment"));
        }
        nonnegative(self.column_gap, "grid column gap")?;
        nonnegative(self.row_gap, "grid row gap")?;
        nonnegative(self.min_height, "grid min height")?;
        if let Some(max) = self.max_height {
            nonnegative(max, "grid max height")?;
            if max < self.min_height {
                return Err(ProtocolError::InvalidValue("grid max height"));
            }
        }
        Ok(())
    }
}

pub(crate) fn validate_items(items: &[GridItem]) -> Result<(), ProtocolError> {
    if items.len() > MAX_COLUMNS {
        return Err(ProtocolError::InvalidValue("grid item count"));
    }
    for item in items {
        if item.justify > 5 || item.align > 5 {
            return Err(ProtocolError::InvalidValue("grid item alignment"));
        }
        for line in item.column.into_iter().chain(item.row) {
            let valid = match line {
                GridLine::Auto => true,
                GridLine::Line(value) => value != 0 && value.unsigned_abs() <= 256,
                GridLine::Span(value) => (1..=256).contains(&value),
            };
            if !valid {
                return Err(ProtocolError::InvalidValue("grid line"));
            }
        }
    }
    Ok(())
}

fn nonnegative(value: f32, name: &'static str) -> Result<(), ProtocolError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(ProtocolError::InvalidValue(name))
    }
}

// Track lists: u16 count then tagged min/max breadths; numeric tags carry f32 ratios or points.
pub(crate) fn encode(grid: &Grid, items: &[GridItem], out: &mut Vec<u8>) {
    for tracks in [
        &grid.columns,
        &grid.rows,
        &grid.auto_columns,
        &grid.auto_rows,
    ] {
        out.extend_from_slice(&(tracks.len() as u16).to_le_bytes());
        for track in tracks {
            for breadth in [track.min, track.max] {
                let (tag, value) = match breadth {
                    GridBreadth::Auto => (0, None),
                    GridBreadth::Length(value) => (1, Some(value)),
                    GridBreadth::Percent(value) => (2, Some(value)),
                    GridBreadth::Fr(value) => (3, Some(value)),
                    GridBreadth::MinContent => (4, None),
                    GridBreadth::MaxContent => (5, None),
                };
                out.push(tag);
                if let Some(value) = value {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
    }
    out.extend_from_slice(&[
        u8::from(grid.column_flow) | (u8::from(grid.boxed) << 1),
        grid.justify_items,
        grid.align_items,
        grid.justify_content,
        grid.align_content,
    ]);
    for value in [grid.column_gap, grid.row_gap, grid.min_height] {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.push(u8::from(grid.max_height.is_some()));
    if let Some(value) = grid.max_height {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out.extend_from_slice(&(items.len() as u16).to_le_bytes());
    for item in items {
        for line in item.column.into_iter().chain(item.row) {
            match line {
                GridLine::Auto => out.push(0),
                GridLine::Line(value) => {
                    out.push(1);
                    out.extend_from_slice(&value.to_le_bytes());
                }
                GridLine::Span(value) => {
                    out.push(2);
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        out.extend_from_slice(&[item.justify, item.align]);
    }
}

pub(super) fn decode(cursor: &mut Cursor<'_>) -> Result<(Grid, Vec<GridItem>), ProtocolError> {
    let columns = tracks(cursor)?;
    let rows = tracks(cursor)?;
    let auto_columns = tracks(cursor)?;
    let auto_rows = tracks(cursor)?;
    let flags = cursor.take(1)?[0];
    if flags > 3 {
        return Err(ProtocolError::InvalidValue("grid flow"));
    }
    let grid = Grid {
        columns,
        rows,
        auto_columns,
        auto_rows,
        column_flow: flags & 1 != 0,
        boxed: flags & 2 != 0,
        justify_items: cursor.take(1)?[0],
        align_items: cursor.take(1)?[0],
        justify_content: cursor.take(1)?[0],
        align_content: cursor.take(1)?[0],
        column_gap: cursor.nonnegative_f32("grid column gap")?,
        row_gap: cursor.nonnegative_f32("grid row gap")?,
        min_height: cursor.nonnegative_f32("grid min height")?,
        max_height: match cursor.take(1)?[0] {
            0 => None,
            1 => Some(cursor.nonnegative_f32("grid max height")?),
            _ => return Err(ProtocolError::InvalidValue("grid max height")),
        },
    };
    grid.validate()?;
    let count = usize::from(cursor.u16()?);
    if count > MAX_COLUMNS {
        return Err(ProtocolError::InvalidValue("grid item count"));
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(GridItem {
            column: [line(cursor)?, line(cursor)?],
            row: [line(cursor)?, line(cursor)?],
            justify: cursor.take(1)?[0],
            align: cursor.take(1)?[0],
        });
    }
    validate_items(&items)?;
    Ok((grid, items))
}

fn tracks(cursor: &mut Cursor<'_>) -> Result<Vec<GridTrack>, ProtocolError> {
    let count = usize::from(cursor.u16()?);
    if count > MAX_COLUMNS {
        return Err(ProtocolError::InvalidValue("grid track count"));
    }
    let mut tracks = Vec::with_capacity(count);
    for _ in 0..count {
        tracks.push(GridTrack {
            min: breadth(cursor)?,
            max: breadth(cursor)?,
        });
    }
    Ok(tracks)
}

fn breadth(cursor: &mut Cursor<'_>) -> Result<GridBreadth, ProtocolError> {
    Ok(match cursor.take(1)?[0] {
        0 => GridBreadth::Auto,
        1 => GridBreadth::Length(cursor.nonnegative_f32("grid track")?),
        2 => GridBreadth::Percent(cursor.nonnegative_f32("grid track")?),
        3 => GridBreadth::Fr(cursor.nonnegative_f32("grid track")?),
        4 => GridBreadth::MinContent,
        5 => GridBreadth::MaxContent,
        _ => return Err(ProtocolError::InvalidValue("grid breadth")),
    })
}

fn line(cursor: &mut Cursor<'_>) -> Result<GridLine, ProtocolError> {
    Ok(match cursor.take(1)?[0] {
        0 => GridLine::Auto,
        1 => GridLine::Line(cursor.u16()? as i16),
        2 => GridLine::Span(cursor.u16()?),
        _ => return Err(ProtocolError::InvalidValue("grid line")),
    })
}
