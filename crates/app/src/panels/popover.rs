//! Lines that go in a [`popover`](crate::components::popover): headings, and
//! checks under them.

use crate::components;
use gui::ui::{Align, Element, Length, Ui};

use crate::assets::Icon;

/// Of every popover.
pub const WIDTH: f32 = 220.;

/// A dim line naming the checks below it in a popover.
pub fn heading(ui: &mut Ui, text: &str) {
    let theme = ui.theme();
    let s = theme.size;
    let element = Element {
        size: [Length::Grow, Length::Fixed(s.row)],
        padding: [s.icon_gap, 0., s.icon_gap, 0.],
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.text(theme.ui_text(theme.color.dim), text);
    });
}

/// A line in a popover: a tick while `on`, then `label`. Returns whether it
/// was clicked this pass.
pub fn check(ui: &mut Ui, label: &str, on: bool) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        padding: [s.icon_gap, 0., s.icon_gap, 0.],
        gap: s.icon_gap,
        radius: [s.radius; 4],
        ..Element::line(s.row)
    };
    let tick = Element {
        size: [Length::Fixed(s.icon); 2],
        ..Element::DEFAULT
    };
    let (clicked, ()) = components::row(ui, element, false, |ui| {
        ui.element(tick, |ui| {
            if on {
                ui.icon(Icon::Check, s.icon, c.accent);
            }
        });
        ui.text(theme.ui_text(c.text), label);
    });
    clicked
}
