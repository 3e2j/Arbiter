//! How editing behaves: for now, what a middle click does.

use crate::components::{notice, switch};
use gui::{
    anim::{Tween, ease_in_out},
    clipboard::Clipboard,
    ui::{Align, Direction, Element, Length, Ui},
};

use serde::{Deserialize, Serialize};

use super::file::{OnByDefault, lenient};
use super::widgets::{beside, row, sized};
use crate::theme;

/// How long the switch's knob takes to cross.
const SLIDE_SECONDS: f32 = 0.16;

/// What's kept of the page in the settings file.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    #[serde(deserialize_with = "lenient")]
    pub middle_click_paste: OnByDefault,
}

#[derive(Default)]
pub struct Editor {
    middle_click_paste: bool,
    /// The switch's knob, on its way to `middle_click_paste`.
    slide: Tween,
}

impl Editor {
    pub fn new(saved: Saved) -> Self {
        let mut slide = Tween::START;
        slide.jump(saved.middle_click_paste.0);
        Self {
            middle_click_paste: saved.middle_click_paste.0,
            slide,
        }
    }

    pub const fn saved(&self) -> Saved {
        Saved {
            middle_click_paste: OnByDefault(self.middle_click_paste),
        }
    }

    /// Hands what gui keeps to it. Before anything that pastes reads it.
    pub fn apply(&self, ui: &mut Ui) {
        ui.clipboard().middle_click_paste = self.middle_click_paste;
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        if !Clipboard::HAS_PRIMARY {
            notice(ui, "Nothing to set here yet.");
            return;
        }
        let changed = self.middle_click_paste != OnByDefault::default().0;
        self.slide.go(self.middle_click_paste);
        let on = ease_in_out(self.slide.step(ui, SLIDE_SECONDS));
        let state = if self.middle_click_paste { "On" } else { "Off" };
        let end = Element {
            direction: Direction::LeftToRight,
            size: [Length::Grow, Length::Fit],
            gap: 10.,
            align: [Align::End, Align::Center],
            ..Element::DEFAULT
        };
        row(ui, |ui| {
            let desc = Some("Pastes the primary selection.");
            let (reset, flipped) = beside(ui, "Middle-click paste", desc, changed, |ui| {
                ui.element(end, |ui| {
                    let theme = theme::current();
                    ui.text(sized(&theme, -1., theme.color.dim), state);
                    switch(ui, on)
                })
            });
            if reset {
                self.middle_click_paste = OnByDefault::default().0;
            } else if flipped {
                self.middle_click_paste = !self.middle_click_paste;
            }
        });
    }
}
