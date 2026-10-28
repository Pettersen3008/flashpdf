mod backgrounds;
mod command;
mod compile;
mod css;
mod document;
mod effects;
pub use backgrounds::Effects;
mod font;
mod geometry;
mod grid;
mod html;
mod image;
mod layout;
mod model;
mod paint;
mod pdf;
mod renderer;
mod vector;
mod win_ansi;

pub mod protocol;

pub(crate) use command::OwnedCommand;
pub use command::{ColumnWidth, Command, Slot};
pub use effects::BoxEffects;
pub(crate) use font::EmbeddedFont;
pub use grid::{Grid, GridBreadth, GridItem, GridLine, GridTrack};
pub(crate) use image::Image;
pub use model::*;
pub use renderer::{render, Renderer};

pub(crate) const PAGE_NUMBER: char = '\u{1e}';
pub(crate) const TOTAL_PAGES: char = '\u{1f}';

#[cfg(test)]
mod tests;
