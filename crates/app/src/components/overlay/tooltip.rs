//! A note on what's under the pointer, shown once it has rested there a
//! moment, and gone as soon as it leaves or presses.
//!
//! The caller keeps a [`Tooltip`], tells it each pass what's under the
//! pointer with [`Tooltip::rest`], then declares [`tooltip`] while that says
//! to.

use crate::components::{Chip, Run, wrapped};
use crate::theme;
use gui::input::PointerButton;
use gui::ui::{Anchor, Element, Length, TextStyle, Ui};

/// How long the pointer rests before it shows, in seconds.
const DELAY: f32 = 0.5;
/// How wide it gets before its text wraps.
const MAX_WIDTH: f32 = 280.;
/// Below the pointer, so the pointer doesn't cover its first line.
const DROP: f32 = 20.;

/// What a [`tooltip`] is waiting on, owned by the caller, who picks the key
/// `K` that tells what's under the pointer apart.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Tooltip<K> {
    /// The caller's key for what's under the pointer.
    over: Option<K>,
    /// Seconds the pointer has rested on `over`.
    rested: f32,
    /// Where it shows, fixed when it first does so it doesn't trail the
    /// pointer.
    at: Option<[f32; 2]>,
}

impl<K> Default for Tooltip<K> {
    fn default() -> Self {
        Self {
            over: None,
            rested: 0.,
            at: None,
        }
    }
}

impl<K: PartialEq> Tooltip<K> {
    /// Counts how long the pointer has rested on `over`, the caller's key
    /// for what's under it. Returns where to show it, once it has rested
    /// long enough.
    pub fn rest(&mut self, ui: &mut Ui, over: Option<K>) -> Option<[f32; 2]> {
        let input = ui.input();
        let pressed = [PointerButton::Left, PointerButton::Right]
            .into_iter()
            .any(|button| input.pressed(button));
        if over != self.over || pressed {
            *self = Self {
                over,
                ..Self::default()
            };
        }
        if self.over.is_none() || pressed {
            return None;
        }
        if self.rested < DELAY {
            self.rested += ui.elapsed();
            ui.redraw_in(DELAY - self.rested);
            return None;
        }
        let at = input.pointer().map(|[x, y]| [x, y + DROP]);
        self.at = self.at.or(at);
        self.at
    }
}

/// `title` over `text` in a raised box at `at`, kept inside the window.
#[track_caller]
pub fn tooltip(ui: &mut Ui, at: [f32; 2], title: &str, text: &str) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let style = |color| TextStyle {
        size: s.text - 1.,
        ..theme.ui_text(color)
    };
    let padding = s.gap;
    let line = s.row - s.gap;
    let widest = ui.measure(style(c.text), title)[0].max(ui.measure(style(c.dim), text)[0]);
    let element = Element {
        size: [
            Length::Fixed(widest.min(MAX_WIDTH) + 2. * padding),
            Length::Fit,
        ],
        padding: [padding, padding / 2., padding, padding / 2.],
        background: Some(c.surface),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        float: Some(Anchor::At(at)),
        ..Element::column()
    };
    ui.element(element, |ui| {
        ui.element(Element::line(line), |ui| ui.text(style(c.text), title));
        if !text.is_empty() {
            wrapped(ui, style(c.dim), line, [Run::<Chip>::Text(text)]);
        }
    });
}
