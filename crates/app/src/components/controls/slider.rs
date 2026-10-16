//! A knob on a rail that snaps to evenly spaced notches, each marked by a
//! dot. Pressing anywhere on it moves the knob there, and dragging carries
//! it along, so a press is its own small drag.

use crate::theme;
use gui::input::PointerButton;
use gui::ui::{Align, Anchor, Element, Length, Ui};

/// What the user did to a [`slider`] this pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Slid {
    /// The notch nearest the pointer while the knob is held, for trying it
    /// on before letting go.
    pub held: Option<u16>,
    /// The notch it was let go at.
    pub set: Option<u16>,
}

/// Thickness of the rail.
const RAIL: f32 = 3.;
/// Across a notch's dot, wider than the rail so each stands out from it.
const NOTCH: f32 = 7.;

/// Fills its parent across, with `notches` from the left end to the right
/// and the knob on `at`, or on the notch under the pointer while held. The
/// rail is filled up to the knob.
#[track_caller]
pub fn slider(ui: &mut Ui, notches: u16, at: u16) -> Slid {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let knob = s.icon;
    let element = Element {
        size: [Length::Grow, Length::Fixed(s.row)],
        // The rail ends at the end notches, where the knob's centre stops.
        padding: [knob / 2., 0., knob / 2., 0.],
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let rail = Element {
        size: [Length::Grow, Length::Fixed(RAIL)],
        background: Some(c.selected),
        radius: [RAIL / 2.; 4],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.element(rail, |_| ());
        // Asked every pass, as the press pass is where the box claims it.
        let held = ui.held(PointerButton::Left);
        let let_go =
            ui.dragged(PointerButton::Left).is_some() && ui.input().released(PointerButton::Left);
        let mut slid = Slid::default();
        let Some(rect) = ui.rect() else {
            return slid;
        };
        // Notches run between the knob's centres at either end.
        let travel = (rect.w - knob).max(0.);
        let span = f32::from(notches.saturating_sub(1).max(1));
        let x = |notch: u16| knob / 2. + travel * f32::from(notch) / span;
        let pointed = ui.input().pointer().map(|[px, _]| {
            let px = px - rect.x;
            (0..notches)
                .min_by(|&a, &b| (x(a) - px).abs().total_cmp(&(x(b) - px).abs()))
                .unwrap_or(0)
        });
        if held || ui.pressed(PointerButton::Left) {
            slid.held = pointed;
        } else if let_go {
            slid.set = pointed;
        }
        let shown = slid
            .held
            .or(slid.set)
            .unwrap_or(at)
            .min(notches.saturating_sub(1));
        let fill = Element {
            size: [Length::Fixed(x(shown) - x(0)), Length::Fixed(RAIL)],
            background: Some(c.accent),
            radius: [RAIL / 2.; 4],
            float: Some(along(x(0), Align::Start)),
            ..Element::DEFAULT
        };
        ui.element(fill, |_| ());
        for notch in 0..notches {
            let mark = Element {
                size: [Length::Fixed(NOTCH); 2],
                background: Some(if notch <= shown { c.accent } else { c.selected }),
                radius: [NOTCH / 2.; 4],
                float: Some(along(x(notch), Align::Center)),
                ..Element::DEFAULT
            };
            ui.element(mark, |_| ());
        }
        let dot = Element {
            size: [Length::Fixed(knob); 2],
            background: Some(c.text),
            radius: [knob / 2.; 4],
            float: Some(along(x(shown), Align::Center)),
            ..Element::DEFAULT
        };
        ui.element(dot, |_| ());
        slid
    })
}

/// Centred on the rail top to bottom, with its `own` side `x` in from the
/// left end.
const fn along(x: f32, own: Align) -> Anchor {
    Anchor::Parent {
        parent: [Align::Start, Align::Center],
        own: [own, Align::Center],
        offset: [x, 0.],
        clipped: true,
    }
}
