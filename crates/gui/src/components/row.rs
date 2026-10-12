//! A line of a list or menu that can be clicked, lit while selected or under
//! the pointer.

use crate::input::{Button, Cursor};
use crate::ui::{Element, Ui};

/// A clickable box holding whatever `body` declares. Its background shows
/// when `selected` or under the pointer.
/// Returns whether it was clicked this pass, and what `body` returned.
#[track_caller]
pub fn row<R>(
    ui: &mut Ui,
    element: Element,
    selected: bool,
    body: impl FnOnce(&mut Ui) -> R,
) -> (bool, R) {
    let c = ui.theme().color;
    let element = Element {
        cursor: Some(Cursor::Pointer),
        ..element
    };
    ui.element(element, |ui| {
        ui.style().background = if selected {
            Some(c.selected)
        } else {
            ui.hovered().then_some(c.hover)
        };
        let out = body(ui);
        (ui.clicked(Button::Left), out)
    })
}
