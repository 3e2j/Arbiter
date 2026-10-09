//! The colours, sizes and fonts everything is drawn with, themes carry them.
//!
//! Plain data the app fills in through [`Startup`](crate::host::Startup),
//! read back with [`Ui::theme`](crate::ui::Ui::theme). gui has no values of
//! its own, so [`components`](crate::components) look however the app says.

use crate::canvas::{Color, FontId};
use crate::layout::TextStyle;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Theme {
    pub color: Colors,
    pub size: Sizes,
    pub font: Fonts,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Colors {
    /// Behind everything, the window's clear colour.
    pub page: Color,
    /// Anything raised off the page.
    pub surface: Color,
    /// Borders and dividers.
    pub line: Color,
    pub text: Color,
    /// Icons and secondary text.
    pub dim: Color,
    /// Under the pointer, and buttons at rest.
    pub hover: Color,
    /// The selected row and buttons under the pointer.
    pub selected: Color,
    /// A scroll thumb, drawn over the rows it scrolls, so it should be
    /// translucent and unlike `selected`.
    pub thumb: Color,
    /// A scroll thumb under the pointer, or while it's dragged.
    pub thumb_hover: Color,
    /// Marks where something dragged will land.
    pub accent: Color,
}

/// In logical pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Sizes {
    pub text: f32,
    /// Icons are square.
    pub icon: f32,
    /// Between an icon and its text.
    pub icon_gap: f32,
    /// Holds an icon and a line of text. Rows, buttons and tabs are this tall.
    pub row: f32,
    /// How far a nested row sits right of its parent.
    pub indent: f32,
    /// Between boxes, and inside the edge of a raised one.
    pub gap: f32,
    pub radius: f32,
    /// Width of borders and dividers.
    pub line: f32,
    /// Width of a scroll thumb.
    pub scroll_bar: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Fonts {
    /// Labels, tabs and lists.
    pub ui: FontId,
    /// Code and logs, where text lines up in columns.
    pub buffer: FontId,
}

impl Theme {
    /// Text in the UI font at the text size.
    #[must_use]
    pub const fn ui_text(&self, color: Color) -> TextStyle {
        TextStyle {
            font: self.font.ui,
            size: self.size.text,
            color,
        }
    }
}
