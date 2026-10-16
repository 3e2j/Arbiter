//! A line between what comes before it and after.

use crate::theme;
use gui::ui::{Direction, Element, Length, Ui};

/// A line across the open box, as thick as the theme's lines.
#[track_caller]
pub fn rule(ui: &mut Ui) {
    let theme = theme::current();
    let line = Length::Fixed(theme.size.line);
    let size = match ui.style().direction {
        Direction::LeftToRight => [line, Length::Grow],
        Direction::TopToBottom => [Length::Grow, line],
    };
    let element = Element {
        size,
        background: Some(theme.color.line),
        ..Element::DEFAULT
    };
    ui.element(element, |_| ());
}
