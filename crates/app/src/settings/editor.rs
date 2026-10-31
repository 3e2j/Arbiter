//! How editing behaves: for now, what a middle click does.

use crate::components::notice;
use gui::{clipboard::Clipboard, ui::Ui, ui::anim::Tween};

use serde::{Deserialize, Serialize};

use super::saved::{OnByDefault, lenient};
use super::widgets::switch_setting;

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
    pub fn sync(&self, ui: &mut Ui) {
        ui.clipboard().middle_click_paste = self.middle_click_paste;
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        if !Clipboard::HAS_PRIMARY {
            notice(ui, "Nothing to set here yet.");
            return;
        }
        switch_setting(
            ui,
            "Middle-click paste",
            Some("Pastes the primary selection."),
            &mut self.middle_click_paste,
            OnByDefault::default().0,
            &mut self.slide,
        );
    }
}
