//! A short label in a pill, such as a tag in a line of text.

use crate::theme;
use gui::canvas::Color;
use gui::ui::{Align, Direction, Element, Length, TextStyle, Ui};

/// `label` in a pill a little shorter than a row, after a dot of `swatch`
/// when given.
#[track_caller]
pub fn chip(ui: &mut Ui, label: &str, swatch: Option<Color>) {
    let theme = theme::current();
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
    let dot = s.text / 2.;
    let style = TextStyle {
        size: s.text - 2.,
        ..theme.ui_text(c.text)
    };
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
        ui.text(style, label);
    });
}
