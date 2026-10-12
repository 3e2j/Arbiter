//! A panel of settings hanging from the button that opens it.
//! Changes apply at once, so it stays open until a press lands outside it.
//!
//! If picking something should close it, use a menu instead.

use crate::input::PointerButton;
use crate::ui::{Align, Anchor, Element, Length, Ui};

/// A raised box `width` wide under the right end of the open box. Returns
/// what `body` returned, and whether a press landed outside it.
///
/// Toggle it on its button's press, not click, or the press closes it and
/// the click reopens it.
#[track_caller]
pub fn popover<R>(ui: &mut Ui, width: f32, body: impl FnOnce(&mut Ui) -> R) -> (R, bool) {
    let theme = ui.theme();
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
