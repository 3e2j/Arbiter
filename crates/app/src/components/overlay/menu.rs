//! A menu opened by a right click, at the pointer: a row per item, and
//! clicking one closes it. A press anywhere else, or Escape, closes it too.
//!
//! The box that's right clicked opens its [`ContextMenu`], then declares
//! [`context_menu`] each pass with what it offers there.

use gui::input::{Key, PointerButton};
use gui::ui::{Anchor, Element, Length, Ui};

/// Where a [`context_menu`] is open, owned by the caller.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ContextMenu {
    /// Its top left, `None` while closed.
    at: Option<[f32; 2]>,
}

/// One line of a [`context_menu`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MenuItem<'a> {
    pub label: &'a str,
    /// Faded and not clickable while false, such as Copy with nothing
    /// selected.
    pub enabled: bool,
}

/// How wide a menu is at least, so short labels don't make it a sliver.
const MIN_WIDTH: f32 = 160.;

impl ContextMenu {
    pub const CLOSED: Self = Self { at: None };

    /// Opens it at the pointer when the right button goes down over the open
    /// box. Returns whether it did.
    pub fn open_on_right_press(&mut self, ui: &Ui) -> bool {
        let at = ui
            .pressed(PointerButton::Right)
            .then(|| ui.input().pointer())
            .flatten();
        self.at = at.or(self.at);
        at.is_some()
    }

    #[must_use]
    pub const fn is_open(self) -> bool {
        self.at.is_some()
    }

    pub const fn close(&mut self) {
        self.at = None;
    }
}

impl<'a> MenuItem<'a> {
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            enabled: true,
        }
    }

    /// Enabled only while `enabled`.
    #[must_use]
    pub const fn enabled(self, enabled: bool) -> Self {
        Self { enabled, ..self }
    }
}

/// `items` in a raised box where `state` opened, over everything else, kept
/// inside the window. Returns the index of the item clicked, which closes
/// it. Declares nothing while it's closed.
#[track_caller]
pub fn context_menu(ui: &mut Ui, state: &mut ContextMenu, items: &[MenuItem]) -> Option<usize> {
    let at = state.at?;
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    // Takes the press that closes it, so it doesn't land on what's under.
    // The pass it opens, the cover isn't under the pointer yet.
    if let Some(window) = ui.window_rect() {
        let cover = Element {
            size: [Length::Fixed(window.w), Length::Fixed(window.h)],
            float: Some(Anchor::At([window.x, window.y])),
            ..Element::DEFAULT
        };
        let pressed = ui.element(cover, |ui| {
            ui.pressed(PointerButton::Left) || ui.pressed(PointerButton::Right)
        });
        if pressed {
            state.close();
        }
    }
    if ui
        .input()
        .keys()
        .iter()
        .any(|press| press.key == Key::Escape)
    {
        state.close();
    }
    let element = Element {
        size: [Length::Fit; 2],
        min: [MIN_WIDTH, 0.],
        padding: [s.gap / 2.; 4],
        background: Some(c.surface),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        float: Some(Anchor::At(at)),
        ..Element::DEFAULT
    };
    let line = Element {
        padding: [s.gap, 0., s.gap, 0.],
        radius: [s.radius; 4],
        ..Element::line(s.row)
    };
    let clicked = ui.element(element, |ui| {
        let mut clicked = None;
        for (index, item) in (0..).zip(items) {
            let picked = if item.enabled {
                let (picked, ()) = crate::components::row(ui, line, false, |ui| {
                    ui.text(theme.ui_text(c.text), item.label);
                });
                picked
            } else {
                ui.element(line, |ui| {
                    ui.text(theme.ui_text(c.dim.alpha(0x60)), item.label);
                });
                false
            };
            if picked {
                clicked = Some(index);
            }
        }
        clicked
    });
    if clicked.is_some() {
        state.close();
    }
    clicked
}
