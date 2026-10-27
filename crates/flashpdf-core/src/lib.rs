mod command;
// Not wired yet: the next change decodes element records and compiles them through it.
#[allow(dead_code)]
mod css;
mod document;
mod font;
mod geometry;
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
pub(crate) use font::EmbeddedFont;
pub(crate) use image::Image;
pub use model::*;
pub use renderer::{render, Renderer};

pub(crate) const PAGE_NUMBER: char = '\u{1e}';
pub(crate) const TOTAL_PAGES: char = '\u{1f}';

#[cfg(test)]
mod tests;
