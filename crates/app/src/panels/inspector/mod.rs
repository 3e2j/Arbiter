//! What's selected in an editor, as a breadcrumb over sections of labelled
//! fields. The format that owns the file draws them, with [`section`] and
//! [`field`], from the open file itself.
//!
//! An unlocked inspector shows each new selection. Locking one keeps what it
//! shows, and the next selection opens a new one. Unlocking one closes the
//! other unlocked one.
//!
//! Hovering a field shows its notes.

use gui::ui::{Align, Direction, Element, Length, TextStyle, Ui};
use serde::{Deserialize, Serialize};

use super::{Context, Kind, Request, Selection, file_name};
use crate::assets::Icon;
use crate::components::{
    Chip, IconState, Run, Scroll, Tooltip, icon_button, notice, rule, scroll, tooltip, wrapped,
};
use crate::documents::Document;
use crate::theme::{self, HOVER_THUMB};

/// A label's share of its field's width.
const LABEL_SHARE: f32 = 0.38;
/// The narrowest a label gets, however narrow the panel.
const LABEL_MIN: f32 = 64.;

pub struct Inspector {
    locked: bool,
    /// The selection, or while locked, what was selected as it locked.
    shown: Option<Selection>,
    /// What's shown's name, the tab's title while locked.
    name: String,
    scroll: Scroll,
    /// Keyed by the field's label.
    tooltip: Tooltip<&'static str>,
}

/// An inspector as the session keeps it. What it showed isn't kept.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct InspectorState {
    #[serde(default)]
    locked: bool,
}

/// How a field's value is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Beside its label.
    Value,
    /// Hex pairs wrapped under its label, as they're too long to sit beside
    /// it.
    Bytes,
}

impl Inspector {
    pub fn new() -> Self {
        let mut scroll = Scroll::default();
        scroll.thumb = HOVER_THUMB;
        Self {
            locked: false,
            shown: None,
            name: String::new(),
            scroll,
            tooltip: Tooltip::default(),
        }
    }

    pub fn from_state(state: &InspectorState) -> Self {
        Self {
            locked: state.locked,
            ..Self::new()
        }
    }

    pub const fn state(&self) -> InspectorState {
        InspectorState {
            locked: self.locked,
        }
    }

    pub(super) const fn locked(&self) -> bool {
        self.locked
    }

    /// The file it shows, relative to the project's root.
    pub(super) fn path(&self) -> Option<&str> {
        self.shown.as_ref().map(|shown| shown.path.as_str())
    }

    /// What it shows while locked.
    pub(super) fn title(&self) -> &str {
        if self.locked && !self.name.is_empty() {
            &self.name
        } else {
            Kind::Inspector.title()
        }
    }

    pub(super) fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        let Self {
            locked,
            shown,
            name,
            scroll: body_scroll,
            tooltip: tip,
        } = self;
        if !*locked && *shown != *cx.selection {
            shown.clone_from(cx.selection);
            body_scroll.offset = 0.;
        }
        let format = shown.as_ref().and_then(|shown| {
            let Some(Document::Decoded(format)) = cx.documents.get(&shown.path) else {
                return None;
            };
            Some((shown, format))
        });
        name.clear();
        if let Some((shown, format)) = format {
            format.item_name(shown.item, name);
        }
        let crumbs = format.map(|(shown, _)| [file_name(&shown.path), name.as_str()]);
        if header(ui, crumbs.iter().flatten().copied(), *locked) {
            *locked = !*locked;
            if !*locked {
                cx.requests.push(Request::Unlocked);
            }
        }
        let Some((shown, format)) = format else {
            return notice(ui, "Nothing selected");
        };
        let body = Element {
            padding: [0., 0., 0., theme::current().size.gap],
            ..Element::column()
        };
        let mut over = None;
        scroll(ui, body_scroll, body, |ui| {
            over = format.inspect(ui, shown.item);
        });
        if let Some(at) = tip.rest(ui, over.as_ref().map(|&(label, _)| label))
            && let Some((label, help)) = &over
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
pub fn section(ui: &mut Ui, title: &str) {
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
pub fn field(ui: &mut Ui, label: &str, value: &str, kind: FieldKind) -> bool {
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
