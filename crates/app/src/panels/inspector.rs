//! What's selected in the active document, as a breadcrumb over cards and
//! sections that fold. The document builds the [`Inspection`], so this knows
//! no format.
//!
//! Only the live inspector follows the selection. Pinning one keeps what it
//! shows, and the next selection opens a new live one. Unpinning, or opening
//! one from the View menu, makes it the live one and closes the old.
//!
//! Hovering shows tooltips.

use gui::input::{CursorIcon, PointerButton};
use gui::ui::{Align, Direction, Element, Length, TextStyle, Ui};

use crate::assets::Icon;
use crate::components::{
    Chip, IconState, Run, Scroll, Tooltip, icon_button, notice, rule, scroll, tooltip, wrapped,
};
use crate::theme::{self, HOVER_THUMB};
use crate::views::{Field, FieldKind, Inspection, Row};

/// A label's share of its field's width.
const LABEL_SHARE: f32 = 0.38;
/// The narrowest a label gets, however narrow the panel.
const LABEL_MIN: f32 = 64.;
const PIN: Field = Field {
    label: "Pin",
    help: Some("Keep showing this as the selection changes."),
};
const UNPIN: Field = Field {
    label: "Unpin",
    help: Some("Follow the selection again, closing the live inspector."),
};

pub struct Inspector {
    mode: Mode,
    /// `None` when nothing is selected.
    shown: Option<Shown>,
    scroll: Scroll,
    /// The titles of the cards folded shut.
    shut_cards: Vec<String>,
    /// The titles of the sections folded shut.
    shut_sections: Vec<String>,
    tooltip: Tooltip<Over>,
}

/// What the pointer is over, for the tooltip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Over {
    Pin,
    /// The row at this position in the [`Inspection`].
    Row(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Live,
    Pinned,
    /// Takes over as the live one, which closes.
    Claiming,
}

struct Shown {
    /// The document it was built from.
    path: String,
    /// What it names there.
    key: usize,
    inspection: Inspection,
}

impl Inspector {
    pub const TITLE: &str = "Inspector";

    pub fn new() -> Self {
        let mut scroll = Scroll::default();
        scroll.thumb = HOVER_THUMB;
        Self {
            mode: Mode::Claiming,
            shown: None,
            scroll,
            shut_cards: Vec::new(),
            shut_sections: Vec::new(),
            tooltip: Tooltip::default(),
        }
    }

    /// Its tab's: the last step of the breadcrumb while pinned.
    pub fn title(&self) -> &str {
        let pinned = self.shown.as_ref().filter(|_| self.mode == Mode::Pinned);
        let crumb = pinned.and_then(|shown| {
            let crumbs = shown.inspection.rows().map_while(|row| match row {
                Row::Crumb(label) => Some(label),
                _ => None,
            });
            crumbs.last()
        });
        crumb.unwrap_or(Self::TITLE)
    }

    pub fn is_live(&self) -> bool {
        self.mode == Mode::Live
    }

    pub fn is_pinned(&self) -> bool {
        self.mode == Mode::Pinned
    }

    /// Whether it's taking over as the live one.
    pub fn is_claiming(&self) -> bool {
        self.mode == Mode::Claiming
    }

    /// Makes it the live one, once the old is closed.
    pub fn go_live(&mut self) {
        self.mode = Mode::Live;
    }

    /// Whether it shows `selection`, a document's path and key.
    pub fn shows(&self, selection: Option<(&str, usize)>) -> bool {
        self.shown
            .as_ref()
            .map(|shown| (shown.path.as_str(), shown.key))
            == selection
    }

    /// Shows `inspection`, built from `key` in the document at `path`, or
    /// nothing.
    pub fn show(&mut self, shown: Option<(&str, usize, Inspection)>) {
        self.shown = shown.map(|(path, key, inspection)| Shown {
            path: path.to_owned(),
            key,
            inspection,
        });
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let Self {
            mode,
            shown,
            scroll: list,
            shut_cards,
            shut_sections,
            tooltip: tip,
        } = self;
        let Some(shown) = shown else {
            return notice(ui, "Nothing selected");
        };
        let s = theme::current().size;
        let mut rows = (0..).zip(shown.inspection.rows()).peekable();
        let pinned = *mode == Mode::Pinned;
        let pin = crumbs(ui, pinned, || {
            rows.next_if(|(_, row)| matches!(row, Row::Crumb(_)))
                .map(|(_, row)| row)
        });
        if pin.clicked {
            *mode = if pinned { Mode::Claiming } else { Mode::Pinned };
        }
        let pin_field = if pinned { &UNPIN } else { &PIN };
        let mut over = pin.hovered.then_some((Over::Pin, pin_field));
        let body = Element {
            padding: [0., 0., 0., s.gap],
            ..Element::column()
        };
        scroll(ui, list, body, |ui| {
            while let Some((_, Row::Card { title, id })) = rows.next() {
                let card_open = !shut_cards.iter().any(|shut| shut == title);
                if card(ui, title, id, card_open) {
                    toggle(shut_cards, title);
                }
                let mut first = true;
                while let Some((_, Row::Section(title))) =
                    rows.next_if(|(_, row)| matches!(row, Row::Section(_)))
                {
                    let open = !shut_sections.iter().any(|shut| shut == title);
                    let group = Element {
                        size: [Length::Grow, Length::Fit],
                        padding: [0., 0., 0., s.gap / 2.],
                        ..Element::DEFAULT
                    };
                    let mut fields = || rows.next_if(|(_, row)| matches!(row, Row::Field { .. }));
                    if !card_open {
                        while fields().is_some() {}
                        continue;
                    }
                    if !first {
                        rule(ui);
                    }
                    first = false;
                    let clicked = ui.element(group, |ui| {
                        let clicked = section(ui, title, open);
                        while let Some((
                            at,
                            Row::Field {
                                field: f,
                                value,
                                kind,
                            },
                        )) = fields()
                        {
                            if open && field(ui, f.label, value, kind) && f.help.is_some() {
                                over = Some((Over::Row(at), f));
                            }
                        }
                        clicked
                    });
                    if clicked {
                        toggle(shut_sections, title);
                    }
                }
            }
        });
        if let Some(at) = tip.rest(ui, over.map(|(key, _)| key))
            && let Some((
                _,
                &Field {
                    label,
                    help: Some(help),
                },
            )) = over
        {
            tooltip(ui, at, label, help);
        }
    }
}

/// Shuts `title` if it's open in `shut`, or opens it.
fn toggle(shut: &mut Vec<String>, title: &str) {
    match shut.iter().position(|shut| shut == title) {
        Some(at) => _ = shut.swap_remove(at),
        None => shut.push(title.to_owned()),
    }
}

/// What the pointer did to the pin.
struct Pin {
    clicked: bool,
    hovered: bool,
}

/// The breadcrumb, each step `next` gives, the last brightest, then the pin,
/// lit while `pinned`.
#[track_caller]
fn crumbs<'a>(ui: &mut Ui, pinned: bool, mut next: impl FnMut() -> Option<Row<'a>>) -> Pin {
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
    // Clipping, it doesn't fit its steps, so they're cut off before the pin.
    let steps = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        gap: s.gap / 2.,
        clip: true,
        ..Element::DEFAULT
    };
    ui.element(bar, |ui| {
        ui.element(steps, |ui| {
            let mut step = next();
            let mut first = true;
            while let Some(Row::Crumb(label)) = step {
                step = next();
                if !first {
                    ui.text(theme.ui_text(c.dim), "›");
                }
                first = false;
                ui.text(
                    theme.ui_text(if step.is_none() { c.text } else { c.dim }),
                    label,
                );
            }
        });
        let state = if pinned {
            IconState::Lit
        } else {
            IconState::Normal
        };
        ui.element(Element::DEFAULT, |ui| Pin {
            clicked: icon_button(ui, Icon::Pin, state).clicked,
            hovered: ui.hovered(),
        })
    })
}

/// A bar across the panel with a chevron saying whether the card is `open`,
/// then `title` and `id`. Returns whether it was clicked, to fold or unfold
/// the card.
#[track_caller]
fn card(ui: &mut Ui, title: &str, id: &str, open: bool) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let bar = Element {
        background: Some(c.page),
        padding: [s.gap, 0., s.gap, 0.],
        gap: s.icon_gap,
        cursor: Some(CursorIcon::Pointer),
        ..Element::line(s.row + s.gap)
    };
    let id_style = TextStyle {
        font: theme.font.buffer,
        size: s.text - 1.,
        ..theme.ui_text(c.dim)
    };
    rule(ui);
    let clicked = ui.element(bar, |ui| {
        ui.icon(
            chevron(open),
            s.icon,
            if ui.hovered() { c.text } else { c.dim },
        );
        ui.text(theme.ui_text(c.accent), title);
        if !id.is_empty() {
            ui.text(id_style, id);
        }
        ui.clicked(PointerButton::Left)
    });
    rule(ui);
    clicked
}

/// A section's heading in small capitals, with a chevron saying whether it's
/// `open`. Returns whether it was clicked, to fold or unfold it.
#[track_caller]
fn section(ui: &mut Ui, title: &str, open: bool) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        padding: [s.gap, 0., s.gap, 0.],
        gap: s.icon_gap,
        cursor: Some(CursorIcon::Pointer),
        ..Element::line(s.row + s.gap / 2.)
    };
    ui.element(element, |ui| {
        let color = if ui.hovered() { c.text } else { c.dim };
        let style = TextStyle {
            size: s.text - 2.,
            ..theme.ui_text(color)
        };
        ui.icon(chevron(open), s.icon - 4., color);
        ui.text(style, &title.to_uppercase());
        ui.clicked(PointerButton::Left)
    })
}

const fn chevron(open: bool) -> Icon {
    if open {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    }
}

/// `label`, in line with its section's chevron, and `value` in a sunken
/// box, beside it or, for bytes, under it. Returns whether it's under the
/// pointer, for its tooltip.
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
                ui.text(theme.ui_text(dim), label)
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
