//! A message standing in for content there isn't, such as an empty list.

use crate::ui::{Align, Element, Ui};

/// `text`, dim and centred in the room left in the open box.
#[track_caller]
pub fn notice(ui: &mut Ui, text: &str) {
    let theme = ui.theme();
    let element = Element {
        align: [Align::Center; 2],
        ..Element::column()
    };
    ui.element(element, |ui| {
        ui.text(theme.ui_text(theme.color.dim), text);
    });
}
