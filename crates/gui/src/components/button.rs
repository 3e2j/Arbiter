//! Things to press.

use crate::canvas::IconId;
use crate::input::{Button, Cursor};
use crate::ui::{Align, Border, Element, Size, Ui};

/// A line of text on a raised box. Returns whether it was clicked this pass.
#[track_caller]
pub fn button(ui: &mut Ui, label: &str) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        size: [Size::Fit, Size::Fixed(s.row)],
        padding: [s.gap, 0., s.gap, 0.],
        align: [Align::Center; 2],
        border: Some(Border {
            width: s.line,
            color: c.line,
        }),
        radius: s.radius,
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.style().background = Some(if ui.hovered() { c.selected } else { c.hover });
        ui.text(theme.ui_text(c.text), label);
        ui.clicked(Button::Left)
    })
}

/// An icon in a square box. Returns whether it was clicked this pass.
#[track_caller]
pub fn icon_button(ui: &mut Ui, icon: IconId) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        size: [Size::Fixed(s.row); 2],
        align: [Align::Center; 2],
        radius: s.radius,
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let hovered = ui.hovered();
        ui.style().background = hovered.then_some(c.hover);
        ui.icon(icon, s.icon, if hovered { c.text } else { c.dim });
        ui.clicked(Button::Left)
    })
}
