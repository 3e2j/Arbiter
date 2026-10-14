//! What the pages build their rows from.

use gui::{
    canvas::Color,
    components::{IconState, icon_button},
    ui::{Align, Direction, Element, Length, TextStyle, Theme, Ui},
};

use crate::assets::Icon;

/// Text in the UI font, `by` pixels off the text size.
pub const fn sized(theme: &Theme, by: f32, color: Color) -> TextStyle {
    TextStyle {
        size: theme.size.text + by,
        ..theme.ui_text(color)
    }
}

/// A setting: its label, then its control under it, lit under the pointer.
pub fn row(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    let c = ui.theme().color;
    let element = Element {
        size: [Length::Grow, Length::Fit],
        padding: [12., 12., 8., 12.],
        radius: [8.; 4],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.style().background = ui.hovered().then(|| c.hover.faded(0.55));
        body(ui);
    });
}

/// A setting's name and what it does, with a dot after the name and a reset
/// button at the end while it's `changed`. Returns whether reset was clicked.
pub fn label(ui: &mut Ui, name: &str, desc: &str, changed: bool) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let line = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        gap: 16.,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let words = Element {
        size: [Length::Grow, Length::Fit],
        gap: 1.,
        ..Element::DEFAULT
    };
    let named = Element {
        direction: Direction::LeftToRight,
        gap: 6.,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let dot = Element {
        size: [Length::Fixed(6.); 2],
        background: Some(c.accent),
        radius: [3.; 4],
        ..Element::DEFAULT
    };
    let hole = Element {
        size: [Length::Fixed(s.row); 2],
        ..Element::DEFAULT
    };
    ui.element(line, |ui| {
        ui.element(words, |ui| {
            ui.element(named, |ui| {
                ui.text(theme.ui_text(c.text), name);
                if changed {
                    ui.element(dot, |_| ());
                }
            });
            ui.text(sized(theme, -1., c.dim), desc);
        });
        if changed {
            icon_button(ui, Icon::Reset, IconState::Normal).clicked
        } else {
            ui.element(hole, |_| false)
        }
    })
}
