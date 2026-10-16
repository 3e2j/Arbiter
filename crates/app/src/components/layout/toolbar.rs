//! A strip of controls along the top of a panel.

use crate::theme;
use gui::ui::{Align, Direction, Element, Length, Ui};

/// Fills the open box across, as tall as what `body` declares, which sits
/// left to right with half a gap around and between.
#[track_caller]
pub fn toolbar<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let s = theme::current().size;
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        padding: [s.gap / 2.; 4],
        gap: s.gap / 2.,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    ui.element(element, body)
}

/// Takes the room left along the open box, pushing what follows to its end.
#[track_caller]
pub fn spacer(ui: &mut Ui) {
    let size = match ui.style().direction {
        Direction::LeftToRight => [Length::Grow, Length::Fixed(0.)],
        Direction::TopToBottom => [Length::Fixed(0.), Length::Grow],
    };
    let element = Element {
        size,
        ..Element::DEFAULT
    };
    ui.element(element, |_| ());
}
