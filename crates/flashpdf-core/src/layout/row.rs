use crate::document::Cell;
use crate::layout::LayoutError;
use crate::{ColumnWidth, Pt};

pub(crate) fn resolve_column_widths(
    cells: &[Cell<'_>],
    available: Pt,
) -> Result<Vec<Pt>, LayoutError> {
    let available = f64::from(available.get());
    let mut fixed = 0.0;
    let mut fractions = 0.0;
    for cell in cells {
        match cell.width {
            ColumnWidth::Fixed(width) if width > Pt::ZERO => fixed += f64::from(width.get()),
            ColumnWidth::Fixed(_) => return Err(LayoutError::InvalidColumnWidth),
            ColumnWidth::Percent(percent) => {
                fixed += available * f64::from(percent.get()) / 100.0;
            }
            ColumnWidth::Fraction(fraction) => fractions += f64::from(fraction.get()),
        }
    }
    let tolerance = available * f64::from(f32::EPSILON) * cells.len() as f64;
    let remaining = available - fixed;
    if remaining < -tolerance || (fractions > 0.0 && remaining <= 0.0) {
        return Err(LayoutError::ColumnsOverflow);
    }
    let remaining = remaining.max(0.0);
    Ok(cells
        .iter()
        .map(|cell| {
            Pt(match cell.width {
                ColumnWidth::Fixed(width) => width.get(),
                ColumnWidth::Percent(percent) => available as f32 * percent.get() / 100.0,
                ColumnWidth::Fraction(fraction) => {
                    (remaining * f64::from(fraction.get()) / fractions) as f32
                }
            })
        })
        .collect())
}
