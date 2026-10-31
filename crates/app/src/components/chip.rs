//! A short label in a pill, such as a tag in a line of text, or a color
//! tag's swatch in its place.

use super::Atom;
use crate::theme::{self, Theme};
use gui::canvas::Color;
use gui::ui::{Align, Border, Direction, Element, Length, TextStyle, Ui};

/// What a color tag's chip shows in place of its label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swatch {
    /// The color text after it draws in.
    Dot(Color),
    /// Back to the color text drew in before, which isn't known here.
    Ring,
}

/// `label` in a pill a little shorter than a row, or only `swatch` when
/// given.
#[track_caller]
pub fn chip(ui: &mut Ui, label: &str, swatch: Option<Swatch>) {
    chip_on(ui, label, swatch, None);
}

/// A [`chip`] filled with `background`, such as the selection's color, in
/// place of its own.
#[track_caller]
pub fn chip_on(ui: &mut Ui, label: &str, swatch: Option<Swatch>, background: Option<Color>) {
    let theme = theme::current();
    let Some(swatch) = swatch else {
        let element = Element {
            background: background.or(Some(theme.color.selected)),
            ..pill(&theme)
        };
        ui.element(element, |ui| {
            ui.text(label_style(&theme), label);
        });
        return;
    };
    let height = height(&theme);
    let element = Element {
        size: [Length::Fixed(height); 2],
        align: [Align::Center; 2],
        background,
        radius: [height / 2.; 4],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| match swatch {
        Swatch::Dot(color) => dot(ui, color),
        Swatch::Ring => {
            let ring = Border {
                width: theme.size.line,
                color: theme.color.dim,
            };
            spot(ui, None, Some(ring));
        }
    });
}

/// A dot of `color`, as a [`chip`] shows its swatch.
#[track_caller]
pub fn dot(ui: &mut Ui, color: Color) {
    spot(ui, Some(color), None);
}

#[track_caller]
fn spot(ui: &mut Ui, background: Option<Color>, border: Option<Border>) {
    let size = theme::current().size.text / 2.;
    let spot = Element {
        size: [Length::Fixed(size); 2],
        radius: [size / 2.; 4],
        background,
        border,
        ..Element::DEFAULT
    };
    ui.element(spot, |_| {});
}

/// A [`chip`] in wrapped text, with its swatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chip(pub Option<Swatch>);

impl Atom for Chip {
    fn width(self, ui: &mut Ui, label: &str) -> f32 {
        let theme = theme::current();
        if self.0.is_some() {
            return height(&theme);
        }
        let [left, _, right, _] = pill(&theme).padding;
        left + right + ui.measure(label_style(&theme), label)[0]
    }

    fn draw(self, ui: &mut Ui, label: &str) {
        chip(ui, label, self.0);
    }
}

/// A chip's height, and a swatch's width.
fn height(theme: &Theme) -> f32 {
    theme.size.row - theme.size.gap
}

/// The pill a label sits in, shared by [`chip`] and [`Chip::width`].
fn pill(theme: &Theme) -> Element {
    let s = theme.size;
    let height = height(theme);
    Element {
        direction: Direction::LeftToRight,
        size: [Length::Fit, Length::Fixed(height)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        align: [Align::Start, Align::Center],
        border: Some(theme.border()),
        radius: [height / 2.; 4],
        ..Element::DEFAULT
    }
}

fn label_style(theme: &Theme) -> TextStyle {
    TextStyle {
        size: theme.size.text - 2.,
        ..theme.ui_text(theme.color.text)
    }
}
