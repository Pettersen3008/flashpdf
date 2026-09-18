mod command;
mod document;
mod font;
mod geometry;
mod layout;
mod model;
mod paint;
mod pdf;
mod renderer;
mod win_ansi;

pub mod protocol;

pub(crate) use command::OwnedCommand;
pub use command::{ColumnWidth, Command};
pub(crate) use font::EmbeddedFont;
pub use model::*;
pub use renderer::{render, Renderer};

pub(crate) const PAGE_NUMBER: char = '\u{1e}';
pub(crate) const TOTAL_PAGES: char = '\u{1f}';

#[cfg(test)]
mod tests;
