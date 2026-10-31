//! What only Twilight Princess files need: for now, whether setting a
//! message's id also sets the copy its attributes keep.

use gui::ui::{Ui, anim::Tween};
use serde::{Deserialize, Serialize};

use super::saved::{OnByDefault, lenient};
use super::widgets::{heading, switch_setting};

/// The switch's name, which the inspector points to.
pub const LINK_ID_COPIES: &str = "Link id copies";

/// What's kept of the page in the settings file.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    #[serde(deserialize_with = "lenient")]
    pub link_id_copies: OnByDefault,
}

#[derive(Default)]
pub struct TwilightPrincess {
    link_id_copies: bool,
    /// The switch's knob, on its way to `link_id_copies`.
    slide: Tween,
}

impl TwilightPrincess {
    pub fn new(saved: Saved) -> Self {
        let mut slide = Tween::START;
        slide.jump(saved.link_id_copies.0);
        Self {
            link_id_copies: saved.link_id_copies.0,
            slide,
        }
    }

    pub const fn saved(&self) -> Saved {
        Saved {
            link_id_copies: OnByDefault(self.link_id_copies),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        heading(ui, "Messages");
        switch_setting(
            ui,
            LINK_ID_COPIES,
            Some("Editing an ID also sets its attribute copy."),
            &mut self.link_id_copies,
            OnByDefault::default().0,
            &mut self.slide,
        );
    }
}
