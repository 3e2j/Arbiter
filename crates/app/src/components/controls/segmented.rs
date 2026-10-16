//! A row of choices sharing one sunken track, the chosen one raised in it,
//! for picking one of a few fixed values such as a zoom level.

use crate::theme;
use gui::input::{CursorIcon, PointerButton};
use gui::ui::{Align, Element, Length, TextStyle, Ui};

/// Between the track's edge and its segments, and between segments.
const INSET: f32 = 2.;
/// How much taller a segment is than a row, so the track stands as tall as
/// a field.
const TALLER: f32 = 2.;

/// Fills its parent across, with a segment for each of `labels` split
/// between them, and the one at `at` raised. Returns the segment clicked.
#[track_caller]
pub fn segmented(ui: &mut Ui, labels: &[&str], at: u16) -> Option<u16> {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    // Its corners follow its segments', a gap further out.
    let element = Element {
        size: [Length::Grow, Length::Fit],
        padding: [INSET; 4],
        gap: INSET,
        background: Some(c.sunken),
        border: Some(theme.border()),
        radius: [s.radius + INSET; 4],
        ..Element::row()
    };
    let segment = Element {
        size: [Length::Grow, Length::Fixed(s.row + TALLER)],
        align: [Align::Center; 2],
        radius: [s.radius; 4],
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
                let style = TextStyle {
                    size: s.text - 1.,
                    ..theme.ui_text(color)
                };
                ui.text(style, label);
                if ui.clicked(PointerButton::Left) {
                    clicked = Some(n);
                }
            });
        }
        clicked
    })
}
