//! `@page` declarations and render options folded into one page size and margin.

use super::parser::PageDeclaration;
use super::style::{edge_values, point};
use super::{show, split_space, Value};
use crate::css::Message;

/// Width and height in points.
type Size = (f64, f64);

/// Portrait sizes in whole points, as PostScript names them, so A4 keeps its 595 x 842 output.
const FORMATS: [(&str, Size); 5] = [
    ("a3", (842.0, 1191.0)),
    ("a4", (595.0, 842.0)),
    ("a5", (420.0, 595.0)),
    ("letter", (612.0, 792.0)),
    ("legal", (612.0, 1008.0)),
];
const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];
const LONGHANDS: [&str; 4] = ["margin-top", "margin-right", "margin-bottom", "margin-left"];

/// What `@page` sets; margins are top, right, bottom, left.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CssPage {
    pub size: Option<Size>,
    pub margin: [Option<f64>; 4],
}

/// Folds `@page` declarations as the cascade does: normal ones in source order, then `!important` ones.
pub fn css_page(declarations: &[PageDeclaration]) -> Result<CssPage, Message> {
    let mut page = CssPage::default();
    for important in [false, true] {
        for declaration in declarations.iter().filter(|own| own.important == important) {
            let value = declaration.value.as_str();
            let applied = (|| {
                match declaration.name.as_str() {
                    "size" => page.size = Some(css_size(value)?),
                    "margin" => {
                        let sides = edge_values(&value.into(), "margin")?;
                        let mut margin = [None; 4];
                        for (side, value) in margin.iter_mut().zip(&sides) {
                            *side = Some(length(Some(value), "margin")?);
                        }
                        page.margin = margin;
                    }
                    name => {
                        let side = LONGHANDS.iter().position(|own| *own == name);
                        let amount = length(Some(&value.into()), name)?;
                        if let Some(side) = side {
                            page.margin[side] = Some(amount);
                        }
                    }
                }
                Ok::<(), Message>(())
            })();
            applied.map_err(|message| format!("{message}{}", declaration.at))?;
        }
    }
    Ok(page)
}

/// `size: auto | <length>{1,2} | <page-size> || [portrait | landscape]`.
fn css_size(value: &str) -> Result<Size, Message> {
    let tokens = split_space(value);
    let numeric = |token: &&str| token.starts_with(|c: char| c == '.' || c.is_ascii_digit());
    if tokens.len() <= 2 && tokens.iter().all(numeric) {
        let width = side(Some(&tokens[0].into()), "page size")?;
        let height = match tokens.get(1) {
            Some(token) => side(Some(&(*token).into()), "page size")?,
            None => width,
        };
        return Ok((width, height));
    }
    if value.to_lowercase() == "auto" {
        return named(&"A4".into());
    }
    let turns = |token: &&str| {
        ["portrait", "landscape"]
            .iter()
            .any(|own| token.eq_ignore_ascii_case(own))
    };
    let orientation: Vec<&str> = tokens.iter().copied().filter(turns).collect();
    let format: Vec<&str> = tokens
        .iter()
        .copied()
        .filter(|token| !orientation.contains(token))
        .collect();
    if orientation.len() > 1 || format.len() > 1 {
        return Err(format!("invalid page size: {value}").into());
    }
    let size = named(&format.first().copied().unwrap_or("A4").into())?;
    let orientation = orientation
        .first()
        .map(|own| Value::from(own.to_lowercase().as_str()));
    turn(size, orientation.as_ref())
}

/// The `format` render option: a page size name, or a width and height.
#[derive(Clone, Debug, PartialEq)]
pub enum Format {
    Named(Value),
    Size {
        width: Option<Value>,
        height: Option<Value>,
    },
}

/// The `margin` render option: shorthand values, or one value per side, top first.
#[derive(Clone, Debug, PartialEq)]
pub enum Margin {
    All(Value),
    Sides([Option<Value>; 4]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageSetup {
    pub width: f64,
    pub height: f64,
    /// Top, right, bottom, left.
    pub margin: [f64; 4],
}

/// Render options win over `@page`, as an inline style wins over a stylesheet.
pub fn page_setup(
    format: Option<&Format>,
    orientation: Option<&Value>,
    margin: Option<&Margin>,
    pages: &[PageDeclaration],
) -> Result<PageSetup, Message> {
    let css = css_page(pages)?;
    let size = match format {
        None => match css.size {
            Some(size) => size,
            None => named(&"A4".into())?,
        },
        Some(Format::Named(name)) => named(name)?,
        Some(Format::Size { width, height }) => (
            side(width.as_ref(), "page width")?,
            side(height.as_ref(), "page height")?,
        ),
    };
    let (width, height) = turn(size, orientation)?;
    let mut sides = [0.0; 4];
    match margin {
        None => {
            for (side, own) in sides.iter_mut().zip(css.margin) {
                *side = own.unwrap_or(36.0);
            }
        }
        Some(Margin::All(value)) => {
            for (side, value) in sides.iter_mut().zip(&edge_values(value, "margin")?) {
                *side = length(Some(value), "margin")?;
            }
        }
        Some(Margin::Sides(values)) => {
            for ((side, value), name) in sides.iter_mut().zip(values).zip(SIDES) {
                *side = length(value.as_ref(), &format!("margin {name}"))?;
            }
        }
    }
    if sides[1] + sides[3] >= width || sides[0] + sides[2] >= height {
        return Err("invalid margin: the margins leave no room for content".into());
    }
    Ok(PageSetup {
        width,
        height,
        margin: sides,
    })
}

fn named(format: &Value) -> Result<Size, Message> {
    let name = format.text().map(str::to_lowercase);
    FORMATS
        .iter()
        .find(|(own, _)| name.as_deref() == Some(own))
        .map(|(_, size)| *size)
        .ok_or_else(|| {
            format!("unsupported page size: {format} (use A3, A4, A5, Letter, Legal, or a width and height)").into()
        })
}

/// As CSS defines it: landscape puts the longer side across, portrait puts it down.
fn turn(size: Size, orientation: Option<&Value>) -> Result<Size, Message> {
    let Some(orientation) = orientation else {
        return Ok(size);
    };
    let (short, long) = (size.0.min(size.1), size.0.max(size.1));
    match orientation.text() {
        Some("landscape") => Ok((long, short)),
        Some("portrait") => Ok((short, long)),
        _ => Err(format!("invalid orientation: {orientation}").into()),
    }
}

fn length(value: Option<&Value>, name: &str) -> Result<f64, Message> {
    value
        .ok_or(())
        .and_then(|value| point(value, 12.0, false).map_err(drop))
        .map_err(|()| format!("invalid {name}: {}", show(value)).into())
}

/// PDF readers cap each page side at 14,400 units (ISO 32000-1, Annex C).
fn side(value: Option<&Value>, name: &str) -> Result<f64, Message> {
    let result = length(value, name)?;
    if result > 0.0 && result <= 14_400.0 {
        return Ok(result);
    }
    Err(format!(
        "invalid {name}: {} (a page side is above 0 and at most 14400pt)",
        show(value)
    )
    .into())
}
