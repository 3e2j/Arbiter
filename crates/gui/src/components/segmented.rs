//! A row of choices sharing one sunken track, the chosen one raised in it,
//! for picking one of a few fixed values such as a zoom level.

use crate::input::{CursorIcon, PointerButton};
use crate::ui::{Align, Element, Length, Ui};

/// Between the track's edge and its segments, and between segments.
const INSET: f32 = 2.;

/// Fills its parent across, with a segment for each of `labels` split
/// between them, and the one at `at` raised. Returns the segment clicked.
#[track_caller]
pub fn segmented(ui: &mut Ui, labels: &[&str], at: u16) -> Option<u16> {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        size: [Length::Grow, Length::Fixed(s.row)],
        padding: [INSET; 4],
        gap: INSET,
        background: Some(c.sunken),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        ..Element::row()
    };
    let segment = Element {
        size: [Length::Grow; 2],
        align: [Align::Center; 2],
        radius: [(s.radius - INSET).max(0.); 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let mut clicked = None;
        for (n, label) in (0..).zip(labels) {
            ui.element(segment, |ui| {
                let on = n == at;
                ui.style().background = on.then_some(c.selected);
                let color = if on || ui.hovered() { c.text } else { c.dim };
                ui.text(theme.ui_text(color), label);
                if ui.clicked(PointerButton::Left) {
                    clicked = Some(n);
                }
            });
        }
        clicked
    })
}
