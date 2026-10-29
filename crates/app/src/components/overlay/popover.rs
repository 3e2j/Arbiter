//! A panel of settings hanging from the button that opens it.
//! Changes apply at once, so it stays open until a press lands outside it.
//!
//! If picking something should close it, use a menu instead.

use crate::assets::Icon;
use crate::components;
use crate::theme;
use gui::input::PointerButton;
use gui::ui::{Align, Anchor, Element, Length, Ui};

/// A raised box `width` wide under the right end of the open box. Returns
/// what `body` returned, and whether a press landed outside it.
///
/// Toggle it on its button's press, not click, or the press closes it and
/// the click reopens it.
#[track_caller]
pub fn popover<R>(ui: &mut Ui, width: f32, body: impl FnOnce(&mut Ui) -> R) -> (R, bool) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    // Its anchor ignores the parent's padding, so it's offset by it.
    let right = ui.style().padding[2];
    let element = Element {
        size: [Length::Fixed(width), Length::Fit],
        padding: [s.gap / 2.; 4],
        background: Some(c.surface),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        float: Some(Anchor::Parent {
            parent: [Align::End; 2],
            own: [Align::End, Align::Start],
            offset: [-right, 0.],
            clipped: false,
        }),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let out = body(ui);
        // The pass it opens, the press is the one that opened it.
        let outside =
            ui.rect().is_some() && ui.input().pressed(PointerButton::Left) && !ui.hovered();
        (out, outside)
    })
}

/// Of every popover.
pub const WIDTH: f32 = 220.;

/// A dim line naming the checks below it in a popover.
pub fn heading(ui: &mut Ui, text: &str) {
    let theme = theme::current();
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
    let theme = theme::current();
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
