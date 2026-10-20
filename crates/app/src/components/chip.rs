//! A short label in a pill, such as a tag in a line of text.

use super::Atom;
use crate::theme::{self, Theme};
use gui::canvas::Color;
use gui::ui::{Align, Direction, Element, Length, TextStyle, Ui};

/// `label` in a pill a little shorter than a row, after a dot of `swatch`
/// when given.
#[track_caller]
pub fn chip(ui: &mut Ui, label: &str, swatch: Option<Color>) {
    let theme = theme::current();
    let (element, dot) = pill(&theme);
    ui.element(element, |ui| {
        if let Some(color) = swatch {
            let dot = Element {
                size: [Length::Fixed(dot); 2],
                radius: [dot / 2.; 4],
                background: Some(color),
                ..Element::DEFAULT
            };
            ui.element(dot, |_| {});
        }
        ui.text(label_style(&theme), label);
    });
}

/// A [`chip`] in wrapped text, with its swatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chip(pub Option<Color>);

impl Atom for Chip {
    fn width(self, ui: &mut Ui, label: &str) -> f32 {
        let theme = theme::current();
        let (element, dot) = pill(&theme);
        let [left, _, right, _] = element.padding;
        let dot = self.0.map_or(0., |_| dot + element.gap);
        left + right + dot + ui.measure(label_style(&theme), label)[0]
    }

    fn draw(self, ui: &mut Ui, label: &str) {
        chip(ui, label, self.0);
    }
}

/// The pill and its swatch's width, shared by [`chip`] and [`Chip::width`].
fn pill(theme: &Theme) -> (Element, f32) {
    let (c, s) = (theme.color, theme.size);
    let height = s.row - s.gap;
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Length::Fit, Length::Fixed(height)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.gap / 2.,
        align: [Align::Start, Align::Center],
        background: Some(c.selected),
        border: Some(theme.border()),
        radius: [height / 2.; 4],
        ..Element::DEFAULT
    };
    (element, s.text / 2.)
}

fn label_style(theme: &Theme) -> TextStyle {
    TextStyle {
        size: theme.size.text - 2.,
        ..theme.ui_text(theme.color.text)
    }
}
