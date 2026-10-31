//! What's selected in an editor, as a breadcrumb over sections of labelled
//! fields. The format that owns the file draws them, with [`section`] and
//! [`field`], from the open file itself.
//!
//! An unlocked inspector shows each new selection. Locking one keeps what it
//! shows, and the next selection opens a new one. Unlocking one closes the
//! other unlocked one.
//!
//! Hovering a field shows its notes.
//!
//! Editable fields write as they're typed in: [`number`], [`named`] and
//! [`byte_grid`]. A box shows the file's value again whenever nothing has the
//! keyboard, so a value typed past its field's width shows clamped once
//! typing stops.

use std::fmt::Write;

use gui::ui::{Align, Direction, Element, Length, TextStyle, Ui};
use serde::{Deserialize, Serialize};

use super::{Context, Kind, Request, Selection, file_name};
use crate::assets::Icon;
use crate::components::{
    Choice, Dropdown, IconState, Scroll, TextField, Tooltip, dropdown, icon_button, notice, rule,
    scroll, segmented, text_field, tooltip,
};
use crate::documents::Document;
use crate::edit;
use crate::theme::{self, HOVER_THUMB};

/// A label's share of its field's width.
const LABEL_SHARE: f32 = 0.38;
/// The narrowest a label gets, however narrow the panel.
const LABEL_MIN: f32 = 64.;
/// A number box beside a dropdown of names.
const NUMBER_WIDTH: f32 = 68.;
/// Bytes to a row of a [`byte_grid`].
const GRID_COLUMNS: usize = 8;

pub struct Inspector {
    locked: bool,
    /// The selection, or while locked, what was selected as it locked.
    shown: Option<Selection>,
    /// What's shown's name, the tab's title while locked.
    name: String,
    scroll: Scroll,
    /// Keyed by the field's label.
    tooltip: Tooltip<&'static str>,
    fields: Fields,
}

/// What editable fields keep between passes, matched to them in the order
/// they're drawn.
#[derive(Default)]
pub struct Fields {
    numbers: Vec<NumberBox>,
    /// How many number boxes were drawn so far this pass.
    drawn: usize,
    /// The open dropdown, by the draw order of the number box beside it.
    menu: Option<(usize, Dropdown)>,
    /// A [`named`] field's values and their choices, rebuilt for each.
    values: Vec<u16>,
    choices: Vec<Choice<'static>>,
    scratch: String,
    /// Which segment the format's view switch is on.
    pub view: u16,
}

#[derive(Default)]
struct NumberBox {
    text: TextField,
    /// The file's value as last shown. `None` before it's first shown.
    value: Option<u16>,
}

/// An inspector as the session keeps it. What it showed isn't kept.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct InspectorState {
    #[serde(default)]
    locked: bool,
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
            fields: Fields::default(),
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
            fields,
        } = self;
        if !*locked && *shown != *cx.selection {
            shown.clone_from(cx.selection);
            body_scroll.offset = 0.;
            fields.numbers.clear();
            fields.menu = None;
        }
        let format = shown.as_ref().and_then(|shown| {
            let Some(Document::Decoded(format)) = cx.documents.get_mut(&shown.path) else {
                return None;
            };
            Some((shown, format))
        });
        name.clear();
        if let Some((shown, format)) = &format {
            format.item_name(shown.item, name);
        }
        let crumbs = format
            .as_ref()
            .map(|(shown, _)| [file_name(&shown.path), name.as_str()]);
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
            fields.drawn = 0;
            over = format.inspect(ui, shown.item, fields);
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

/// A switch between `labels`, views of the section above, setting
/// [`Fields::view`].
#[track_caller]
pub fn view_switch(ui: &mut Ui, fields: &mut Fields, labels: &[&str]) {
    let s = theme::current().size;
    let element = Element {
        size: [Length::Grow, Length::Fit],
        padding: [s.gap, s.gap / 4., s.gap, s.gap / 2.],
        ..Element::DEFAULT
    };
    if let Some(view) = ui.element(element, |ui| segmented(ui, labels, fields.view)) {
        fields.view = view;
        fields.numbers.clear();
        fields.menu = None;
    }
}

/// `label`, and `value` in a sunken box beside it.
/// Returns whether it's under the pointer, for its tooltip.
#[track_caller]
pub fn field(ui: &mut Ui, label: &str, value: &str) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
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
    labelled(ui, label, |ui| ui.element(well, |ui| ui.text(code, value))).0
}

/// `label`, then what `body` draws beside it. Returns whether it's under the
/// pointer, for its tooltip, and what `body` returned.
#[track_caller]
fn labelled<R>(ui: &mut Ui, label: &str, body: impl FnOnce(&mut Ui) -> R) -> (bool, R) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let indent = s.gap;
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
        (hovered, body(ui))
    })
}

/// `label`, and a box `value` is typed into, up to `max`. Returns whether
/// it's under the pointer, and the value typed.
#[track_caller]
pub fn number(
    ui: &mut Ui,
    fields: &mut Fields,
    label: &str,
    value: u16,
    max: u16,
) -> (bool, Option<u16>) {
    labelled(ui, label, |ui| number_box(ui, fields, value, max, 10))
}

/// `label`, a dropdown of `names` and a box `value` is typed into, up to
/// `max`. Either sets it, and a value `names` doesn't hold shows as Custom.
/// Returns whether it's under the pointer, and the value set.
#[track_caller]
pub fn named(
    ui: &mut Ui,
    fields: &mut Fields,
    label: &str,
    value: u16,
    max: u16,
    names: impl Iterator<Item = (u16, &'static str)>,
) -> (bool, Option<u16>) {
    let theme = theme::current();
    let row = Element {
        size: [Length::Grow, Length::Fit],
        gap: theme.size.gap / 2.,
        ..Element::row()
    };
    let number = Element {
        size: [Length::Fixed(NUMBER_WIDTH), Length::Fit],
        ..Element::DEFAULT
    };
    labelled(ui, label, |ui| {
        ui.element(row, |ui| {
            let key = fields.drawn;
            fields.values.clear();
            fields.choices.clear();
            for (value, name) in names {
                fields.values.push(value);
                fields.choices.push(Choice::new(name));
            }
            let selected = fields.values.iter().position(|&named| named == value);
            let selected = selected.unwrap_or_else(|| {
                fields.choices.push(Choice::new("Custom"));
                fields.values.len()
            });
            let mut menu = match fields.menu {
                Some((open, menu)) if open == key => menu,
                _ => Dropdown::CLOSED,
            };
            let chosen = dropdown(ui, &mut menu, &fields.choices, selected);
            if menu.is_open() {
                fields.menu = Some((key, menu));
            } else if fields.menu.is_some_and(|(open, _)| open == key) {
                fields.menu = None;
            }
            let picked = chosen.picked.and_then(|at| fields.values.get(at).copied());
            let typed = ui.element(number, |ui| number_box(ui, fields, value, max, 10));
            picked.filter(|&picked| picked != value).or(typed)
        })
    })
}

/// A box per byte of `bytes`, in hex, eight to a row under their offsets.
/// Returns the byte under the pointer, and the byte typed with where.
#[track_caller]
pub fn byte_grid(
    ui: &mut Ui,
    fields: &mut Fields,
    bytes: &[u8],
) -> (Option<usize>, Option<(usize, u8)>) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let grid = Element {
        size: [Length::Grow, Length::Fit],
        padding: [s.gap, 0., s.gap, s.gap / 2.],
        gap: s.gap / 2.,
        ..Element::column()
    };
    let row = Element {
        size: [Length::Grow, Length::Fit],
        gap: s.gap / 2.,
        ..Element::row()
    };
    let cell = Element {
        size: [Length::Grow, Length::Fit],
        gap: 2.,
        align: [Align::Center, Align::Start],
        ..Element::column()
    };
    let offset_style = TextStyle {
        font: theme.font.buffer,
        size: s.text - 3.,
        ..theme.ui_text(c.dim)
    };
    ui.element(grid, |ui| {
        let (mut over, mut typed) = (None, None);
        let mut offset = String::new();
        for (row_at, chunk) in (0..).step_by(GRID_COLUMNS).zip(bytes.chunks(GRID_COLUMNS)) {
            ui.element(row, |ui| {
                for column in 0..GRID_COLUMNS {
                    ui.element(cell, |ui| {
                        let Some(&byte) = chunk.get(column) else {
                            return;
                        };
                        let at = row_at + column;
                        offset.clear();
                        let _ = write!(offset, "{at:02X}");
                        ui.text(offset_style, &offset);
                        let value = u16::from(byte);
                        let set = number_box(ui, fields, value, 0xFF, 16);
                        if let Some(byte) = set.and_then(|set| u8::try_from(set).ok()) {
                            typed = Some((at, byte));
                        }
                        if ui.hovered() {
                            over = Some(at);
                        }
                    });
                }
            });
        }
        (over, typed)
    })
}

/// A box `value` is typed into in `radix`, up to `max`, filling its parent
/// across. Returns the value typed when it's a new one.
#[track_caller]
fn number_box(ui: &mut Ui, fields: &mut Fields, value: u16, max: u16, radix: u32) -> Option<u16> {
    let Fields {
        numbers,
        drawn,
        scratch,
        ..
    } = fields;
    if numbers.len() <= *drawn {
        numbers.resize_with(*drawn + 1, NumberBox::default);
    }
    let shown = numbers.get_mut(*drawn)?;
    *drawn += 1;
    scratch.clear();
    let _ = if radix == 16 {
        write!(scratch, "{value:02X}")
    } else {
        write!(scratch, "{value}")
    };
    if (!ui.typing() || shown.value != Some(value)) && shown.text.text != *scratch {
        shown.text.text.clone_from(scratch);
        shown.text.selection = edit::Selection::at(scratch.len());
    }
    shown.value = Some(value);
    if !text_field(ui, &mut shown.text, None, "") {
        return None;
    }
    let typed = u32::from_str_radix(shown.text.text.trim(), radix).ok()?;
    let typed = u16::try_from(typed.min(u32::from(max))).ok()?;
    shown.value = Some(typed);
    (typed != value).then_some(typed)
}
