//! A knob in a rounded track that flips a setting on or off when clicked.
//! On, the knob sits at the right end and the track takes the accent.

use crate::theme;
use gui::input::{CursorIcon, PointerButton};
use gui::ui::{Align, Anchor, Element, Length, Ui};

/// Between the knob and the track's edge.
const INSET: f32 = 3.;

/// A track as tall as the knob plus its inset and a knob longer, with the
/// knob `on` of the way from the left end, 0 off and 1 on. Between them while
/// the caller slides it, which tints the track with the accent as far.
/// Returns whether it was clicked.
#[track_caller]
pub fn switch(ui: &mut Ui, on: f32) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let knob = s.icon;
    let tall = knob + 2. * INSET;
    let pill = |float| Element {
        size: [Length::Fixed(tall + knob), Length::Fixed(tall)],
        radius: [tall / 2.; 4],
        float,
        ..Element::DEFAULT
    };
    let element = Element {
        background: Some(c.selected),
        cursor: Some(CursorIcon::Pointer),
        ..pill(None)
    };
    let tint = Element {
        background: Some(c.accent.faded(on)),
        ..pill(Some(Anchor::Parent {
            parent: [Align::Start; 2],
            own: [Align::Start; 2],
            offset: [0.; 2],
            clipped: true,
        }))
    };
    let dot = Element {
        size: [Length::Fixed(knob); 2],
        background: Some(c.text),
        radius: [knob / 2.; 4],
        float: Some(Anchor::Parent {
            parent: [Align::Start; 2],
            own: [Align::Start; 2],
            offset: [INSET + knob * on.clamp(0., 1.), INSET],
            clipped: true,
        }),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.element(tint, |_| ());
        ui.element(dot, |_| ());
        ui.clicked(PointerButton::Left)
    })
}
