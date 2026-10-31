//! What's selected in an editor, as a breadcrumb over sections of labelled
//! fields. The editor builds the [`Page`], so this knows no format.
//!
//! At most one inspector is unlocked, and it shows each new selection.
//! Locking one keeps what it shows, and the next selection opens a new one.
//! Unlocking one takes over from the unlocked one, which closes.
//!
//! Hovering a field shows its notes.

mod page;

use gui::ui::{Align, Direction, Element, Length, TextStyle, Ui};
use serde::{Deserialize, Serialize};

use super::{Context, Event, Kind, Reply};
use crate::assets::Icon;
use crate::components::{
    Chip, IconState, Run, Scroll, Tooltip, icon_button, notice, rule, scroll, tooltip, wrapped,
};
use crate::theme::{self, HOVER_THUMB};

pub use page::Page;
use page::{FieldKind, Row};

/// A label's share of its field's width.
const LABEL_SHARE: f32 = 0.38;
/// The narrowest a label gets, however narrow the panel.
const LABEL_MIN: f32 = 64.;

pub struct Inspector {
    locked: bool,
    /// Set as it's unlocked, until it's offered the [`Event::Unlocked`] it
    /// posts, which closes the one it takes over from.
    taking_over: bool,
    /// `None` until something is selected.
    shown: Option<Page>,
    scroll: Scroll,
    /// Keyed by the field's row.
    tooltip: Tooltip<u32>,
}

/// An inspector as the session keeps it. What it showed isn't kept.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct InspectorState {
    #[serde(default)]
    locked: bool,
}

impl Inspector {
    pub fn new(shown: Option<Page>) -> Self {
        let mut scroll = Scroll::default();
        scroll.thumb = HOVER_THUMB;
        Self {
            locked: false,
            taking_over: false,
            shown,
            scroll,
            tooltip: Tooltip::default(),
        }
    }

    pub fn from_state(state: &InspectorState) -> Self {
        Self {
            locked: state.locked,
            ..Self::new(None)
        }
    }

    pub const fn state(&self) -> InspectorState {
        InspectorState {
            locked: self.locked,
        }
    }

    fn show(&mut self, page: Page) {
        self.shown = Some(page);
        self.scroll.offset = 0.;
    }
}

impl Inspector {
    /// The last step of the breadcrumb while locked.
    pub(super) fn title(&self) -> &str {
        let locked = self.shown.as_ref().filter(|_| self.locked);
        locked
            .and_then(Page::last_crumb)
            .unwrap_or(Kind::Inspector.title())
    }

    /// While unlocked: shows each selection, stands in for another from the
    /// View menu, and closes once another is unlocked.
    pub(super) fn receive(&mut self, event: &mut Event) -> Reply {
        if self.locked {
            return Reply::Pass;
        }
        match event {
            Event::Inspect(page) => {
                self.show(std::mem::take(page));
                Reply::Show
            }
            Event::Add(Kind::Inspector) => Reply::Show,
            Event::Unlocked if !std::mem::take(&mut self.taking_over) => Reply::Close,
            _ => Reply::Pass,
        }
    }

    pub(super) fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        let Self {
            locked,
            taking_over,
            shown,
            scroll: body_scroll,
            tooltip: tip,
        } = self;
        let s = theme::current().size;
        let crumbs = shown
            .iter()
            .flat_map(Page::rows)
            .map_while(|row| match row {
                Row::Crumb(label) => Some(label),
                _ => None,
            });
        if header(ui, crumbs, *locked) {
            *locked = !*locked;
            *taking_over = !*locked;
            if *taking_over {
                cx.post(Event::Unlocked);
            }
        }
        let Some(shown) = shown else {
            return notice(ui, "Nothing selected");
        };
        let body = Element {
            padding: [0., 0., 0., s.gap],
            ..Element::column()
        };
        let mut over = None;
        scroll(ui, body_scroll, body, |ui| {
            for (at, row) in (0..).zip(shown.rows()) {
                match row {
                    Row::Crumb(_) => {}
                    Row::Section(title) => section(ui, title),
                    Row::Field {
                        label,
                        value,
                        help,
                        kind,
                    } => {
                        if field(ui, label, value, kind) && !help.is_empty() {
                            over = Some((at, label, help));
                        }
                    }
                }
            }
        });
        if let Some(at) = tip.rest(ui, over.map(|(at, ..)| at))
            && let Some((_, label, help)) = over
        {
            tooltip(ui, at, label, help);
        }
    }
}

/// The breadcrumb, each step of `crumbs`, the last brightest, then the lock,
/// lit while `locked`. Returns whether the lock was clicked.
#[track_caller]
fn header<'a>(ui: &mut Ui, crumbs: impl Iterator<Item = &'a str>, locked: bool) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let bar = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        padding: [s.gap, s.gap / 2., s.gap / 2., s.gap / 2.],
        gap: s.gap / 2.,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    // Clipping, so steps that don't fit are cut off before the lock.
    let steps = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        gap: s.gap / 2.,
        clip: true,
        ..Element::DEFAULT
    };
    ui.element(bar, |ui| {
        ui.element(steps, |ui| {
            let mut crumbs = crumbs.peekable();
            let mut first = true;
            while let Some(label) = crumbs.next() {
                if !first {
                    ui.text(theme.ui_text(c.dim), "›");
                }
                first = false;
                let last = crumbs.peek().is_none();
                ui.text(theme.ui_text(if last { c.text } else { c.dim }), label);
            }
        });
        let state = if locked {
            IconState::Lit
        } else {
            IconState::Normal
        };
        icon_button(ui, Icon::Pin, state).clicked
    })
}

/// A section's heading in small capitals, under a rule.
#[track_caller]
fn section(ui: &mut Ui, title: &str) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let style = TextStyle {
        size: s.text - 2.,
        ..theme.ui_text(c.dim)
    };
    let element = Element {
        padding: [s.gap, 0., s.gap, 0.],
        ..Element::line(s.row + s.gap / 2.)
    };
    rule(ui);
    ui.element(element, |ui| ui.text(style, &title.to_uppercase()));
}

/// `label`, and `value` in a sunken box, beside it or, for bytes, under it.
/// Returns whether it's under the pointer, for its tooltip.
#[track_caller]
fn field(ui: &mut Ui, label: &str, value: &str, kind: FieldKind) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let indent = s.gap;
    let code = TextStyle {
        font: theme.font.buffer,
        size: s.text - 1.,
        ..theme.ui_text(c.text)
    };
    let well = Element {
        size: [Length::Grow, Length::Fixed(s.row - 2.)],
        padding: [s.gap * 0.75, 0., s.gap * 0.75, 0.],
        background: Some(c.sunken),
        radius: [s.radius - 1.; 4],
        clip: true,
        ..Element::line(0.)
    };
    if kind == FieldKind::Bytes {
        let element = Element {
            size: [Length::Grow, Length::Fit],
            padding: [indent, 0., s.gap, s.gap / 4.],
            ..Element::DEFAULT
        };
        let block = Element {
            size: [Length::Grow, Length::Fit],
            padding: [s.gap * 0.75, s.gap / 4., s.gap * 0.75, s.gap / 4.],
            ..well
        };
        return ui.element(element, |ui| {
            let hovered = ui.hovered();
            let dim = if hovered { c.text } else { c.dim };
            ui.element(Element::line(s.row), |ui| {
                ui.text(theme.ui_text(dim), label);
            });
            ui.element(block, |ui| {
                wrapped(ui, code, s.row - s.gap / 2., [Run::<Chip>::Text(value)]);
            });
            hovered
        });
    }
    let element = Element {
        padding: [indent, 0., s.gap, 0.],
        gap: s.gap,
        clip: true,
        ..Element::line(s.row + 2.)
    };
    ui.element(element, |ui| {
        let width = ui.rect().map_or(0., |rect| rect.w - indent - s.gap);
        let label_box = Element {
            size: [
                Length::Fixed((width * LABEL_SHARE).max(LABEL_MIN)),
                Length::Fit,
            ],
            clip: true,
            ..Element::DEFAULT
        };
        let hovered = ui.hovered();
        let dim = if hovered { c.text } else { c.dim };
        ui.element(label_box, |ui| ui.text(theme.ui_text(dim), label));
        ui.element(well, |ui| ui.text(code, value));
        hovered
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_takeover_replaces_only_the_other_unlocked_one() {
        let mut taking_over = Inspector::new(None);
        taking_over.taking_over = true;
        let mut unlocked = Inspector::new(None);
        let mut locked = Inspector::from_state(&InspectorState { locked: true });
        let closes = |inspector: &mut Inspector| {
            matches!(inspector.receive(&mut Event::Unlocked), Reply::Close)
        };
        assert!(!closes(&mut taking_over));
        assert!(closes(&mut unlocked));
        assert!(!closes(&mut locked));
        assert!(!taking_over.taking_over);
    }
}
