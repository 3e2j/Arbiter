//! What the pages build their rows from.

use gui::{
    canvas::Color,
    components::{IconState, icon_button},
    ui::{Align, Direction, Element, Length, TextStyle, Theme, Ui},
};

use crate::assets::Icon;

/// Text in the UI font, `by` pixels off the text size.
pub const fn sized(theme: &Theme, by: f32, color: Color) -> TextStyle {
    TextStyle {
        size: theme.size.text + by,
        ..theme.ui_text(color)
    }
}

/// A setting: its label, then its control under it or beside. Its words
/// line up with the [`rule`](gui::components::rule)s a page puts between
/// rows.
pub fn row(ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
    let element = Element {
        size: [Length::Grow, Length::Fit],
        padding: [0., 12., 0., 12.],
        ..Element::DEFAULT
    };
    ui.element(element, body);
}

/// How wide a control beside its label is, so they line up down a page.
const CONTROL: f32 = 250.;
/// Between a line's words, control and reset button.
const GAP: f32 = 16.;
/// The changed dot after a setting's name, and the gap before it.
const DOT: f32 = 6.;
const DOT_GAP: f32 = 6.;

/// A setting's name and what it does if it needs saying, with a dot after
/// the name and a reset button at the end while it's `changed`. Returns
/// whether reset was clicked.
pub fn label(ui: &mut Ui, name: &str, desc: Option<&str>, changed: bool) -> bool {
    ui.element(line(), |ui| {
        words(ui, name, desc, changed);
        reset(ui, changed)
    })
}

/// [`label`] with its control beside it rather than under, for a control
/// that fits a line. Under it after all when the row is too narrow for
/// both, since text doesn't wrap. Returns whether reset was clicked, and
/// what `control` did.
pub fn beside<R>(
    ui: &mut Ui,
    name: &str,
    desc: Option<&str>,
    changed: bool,
    control: impl FnOnce(&mut Ui) -> R,
) -> (bool, R) {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let wrap = Element {
        size: [Length::Grow, Length::Fit],
        ..Element::DEFAULT
    };
    let column = Element {
        size: [Length::Fixed(CONTROL), Length::Fit],
        ..Element::DEFAULT
    };
    let under = Element {
        padding: [0., 8., 0., 0.],
        ..column
    };
    // The name's dot is counted whether it's shown or not, so setting one
    // doesn't move the control.
    let [named, _] = ui.measure(theme.ui_text(c.text), name);
    let said = desc.map_or(0., |desc| ui.measure(sized(theme, -1., c.dim), desc)[0]);
    let need = (named + DOT_GAP + DOT).max(said) + 2. * GAP + CONTROL + s.row;
    ui.element(wrap, |ui| {
        if ui.rect().is_none_or(|rect| rect.w >= need) {
            return ui.element(line(), |ui| {
                words(ui, name, desc, changed);
                let did = ui.element(column, control);
                (reset(ui, changed), did)
            });
        }
        let reset = label(ui, name, desc, changed);
        (reset, ui.element(under, control))
    })
}

const fn line() -> Element {
    Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        gap: GAP,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    }
}

fn words(ui: &mut Ui, name: &str, desc: Option<&str>, changed: bool) {
    let theme = ui.theme();
    let c = theme.color;
    let words = Element {
        size: [Length::Grow, Length::Fit],
        gap: 1.,
        ..Element::DEFAULT
    };
    let named = Element {
        direction: Direction::LeftToRight,
        gap: DOT_GAP,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let dot = Element {
        size: [Length::Fixed(DOT); 2],
        background: Some(c.accent),
        radius: [DOT / 2.; 4],
        ..Element::DEFAULT
    };
    ui.element(words, |ui| {
        ui.element(named, |ui| {
            ui.text(theme.ui_text(c.text), name);
            if changed {
                ui.element(dot, |_| ());
            }
        });
        if let Some(desc) = desc {
            ui.text(sized(theme, -1., c.dim), desc);
        }
    });
}

/// The reset button while `changed`, else a hole its size. Returns whether
/// it was clicked.
fn reset(ui: &mut Ui, changed: bool) -> bool {
    let hole = Element {
        size: [Length::Fixed(ui.theme().size.row); 2],
        ..Element::DEFAULT
    };
    if changed {
        icon_button(ui, Icon::Reset, IconState::Normal).clicked
    } else {
        ui.element(hole, |_| false)
    }
}
