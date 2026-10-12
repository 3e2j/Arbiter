//! Things to press.

use crate::canvas::IconId;
use crate::input::{Button, Cursor};
use crate::ui::{Align, Element, Size, Ui};

/// A line of text on a raised box. Returns whether it was clicked this pass.
#[track_caller]
pub fn button(ui: &mut Ui, label: &str) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        size: [Size::Fit, Size::Fixed(s.row)],
        padding: [s.gap, 0., s.gap, 0.],
        align: [Align::Center; 2],
        border: Some(theme.border()),
        radius: [s.radius; 4],
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.style().background = Some(if ui.hovered() { c.selected } else { c.hover });
        ui.text(theme.ui_text(c.text), label);
        ui.clicked(Button::Left)
    })
}

/// How an [`icon_button`] shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IconState {
    /// Dim, lit up under the pointer.
    Normal,
    /// In the accent, such as while what it opens is open.
    Lit,
    /// Faded, and never pressed or clicked.
    Disabled,
}

/// What the pointer did to a button this pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Clicks {
    /// The left button went down over it.
    pub pressed: bool,
    /// The left button came up over it after going down over it.
    pub clicked: bool,
}

/// An icon in a square box.
#[track_caller]
pub fn icon_button(ui: &mut Ui, icon: impl Into<IconId>, state: IconState) -> Clicks {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let enabled = state != IconState::Disabled;
    let element = Element {
        size: [Size::Fixed(s.row); 2],
        align: [Align::Center; 2],
        radius: [s.radius; 4],
        cursor: enabled.then_some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let hovered = enabled && ui.hovered();
        ui.style().background = hovered.then_some(c.hover);
        let color = match state {
            IconState::Lit => c.accent,
            IconState::Normal if hovered => c.text,
            IconState::Normal => c.dim,
            IconState::Disabled => c.dim.alpha(0x60),
        };
        ui.icon(icon, s.icon, color);
        Clicks {
            pressed: enabled && ui.pressed(Button::Left),
            clicked: enabled && ui.clicked(Button::Left),
        }
    })
}
