use crate::layout::LayoutError;
use crate::{ColumnWidth, Pt};

pub(crate) fn resolve_column_widths(
    columns: impl Iterator<Item = ColumnWidth> + Clone,
    available: Pt,
) -> Result<Vec<Pt>, LayoutError> {
    let available = f64::from(available.get());
    let mut fixed = 0.0;
    let mut fractions = 0.0;
    let mut count = 0;
    for column in columns.clone() {
        count += 1;
        match column {
            ColumnWidth::Fixed(width) if width > Pt::ZERO => fixed += f64::from(width.get()),
            ColumnWidth::Fixed(_) => return Err(LayoutError::InvalidColumnWidth),
            ColumnWidth::Percent(percent) => {
                fixed += available * f64::from(percent.get()) / 100.0;
            }
            ColumnWidth::Fraction(fraction) => fractions += f64::from(fraction.get()),
        }
    }
    let tolerance = available * f64::from(f32::EPSILON) * f64::from(count);
    let remaining = available - fixed;
    if remaining < -tolerance || (fractions > 0.0 && remaining <= 0.0) {
        return Err(LayoutError::ColumnsOverflow);
    }
    let remaining = remaining.max(0.0);
    Ok(columns
        .map(|column| {
            Pt(match column {
                ColumnWidth::Fixed(width) => width.get(),
                ColumnWidth::Percent(percent) => available as f32 * percent.get() / 100.0,
                ColumnWidth::Fraction(fraction) => {
                    (remaining * f64::from(fraction.get()) / fractions) as f32
                }
            })
        })
        .collect())
}

/// A table's width resolves like one track against its container.
pub(crate) fn table_width(width: ColumnWidth, available: Pt) -> Result<Pt, LayoutError> {
    Ok(resolve_column_widths(std::iter::once(width), available)?[0])
}
