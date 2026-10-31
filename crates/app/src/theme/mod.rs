//! The colours, sizes and fonts everything is drawn with.
//!
//! The editor [`set`]s the one in use, and components read it with
//! [`current`] to fill in what they hand gui to paint.

mod palette;

use std::cell::Cell;

use gui::{
    canvas::{Color, FontId, IconId},
    ui::{Border, TextStyle},
};

pub use palette::{HOVER_THUMB, Palette, theme};

thread_local! {
    static CURRENT: Cell<Theme> = Cell::new(Theme::default());
}

#[must_use]
pub fn current() -> Theme {
    CURRENT.get()
}

/// Draws with `theme` from now on, in place of the one before.
pub fn set(theme: Theme) {
    CURRENT.set(theme);
}

/// Changes the sizes of the theme in use, for a test's passes to draw with.
#[cfg(test)]
pub fn sizes(edit: impl FnOnce(&mut ThemeSizes)) {
    let mut theme = current();
    edit(&mut theme.size);
    set(theme);
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Theme {
    pub color: ThemeColors,
    pub size: ThemeSizes,
    pub font: ThemeFonts,
    pub icon: ThemeIcons,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ThemeColors {
    /// Behind everything, the window's clear colour.
    pub page: Color,
    /// Anything raised off the page.
    pub surface: Color,
    /// Fields, sunk into what they sit on.
    pub sunken: Color,
    /// Borders and dividers.
    pub line: Color,
    pub text: Color,
    /// Icons and secondary text.
    pub dim: Color,
    /// Raised off a surface: under the pointer, buttons at rest, the chosen
    /// segment, tag chips.
    pub raised: Color,
    /// The selected row and buttons under the pointer.
    pub selected: Color,
    /// Behind selected text. Translucent, so highlights under it show
    /// through.
    pub selection: Color,
    /// A scroll thumb, drawn over the rows it scrolls, so it should be
    /// translucent and unlike `selected`.
    pub thumb: Color,
    /// A scroll thumb under the pointer, or while it's dragged.
    pub thumb_hover: Color,
    /// Marks where something dragged will land.
    pub accent: Color,
    /// Success, and what's added.
    pub ok: Color,
    /// Warnings, and what's modified.
    pub warn: Color,
    /// Errors, and what's deleted.
    pub err: Color,
}

/// In logical pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ThemeSizes {
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
    /// The window's corners, while the app draws its own frame and the
    /// window isn't maximized.
    pub window_radius: f32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ThemeFonts {
    /// Labels, tabs and lists.
    pub ui: FontId,
    /// Code and logs, where text lines up in columns.
    pub buffer: FontId,
}

/// The icons components draw, from the app's own set.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ThemeIcons {
    /// By the picked choice in a list.
    pub check: IconId,
    /// On a button that opens a list under it.
    pub down: IconId,
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

    /// The border of anything raised, in the line colour and width.
    #[must_use]
    pub const fn border(&self) -> Border {
        Border {
            width: self.size.line,
            color: self.color.line,
        }
    }
}
