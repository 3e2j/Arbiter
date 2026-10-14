//! The user's settings, under the gear in the title bar. Only the theme
//! for now.

use gui::{
    components::{IconState, icon_button, popover},
    ui::{Element, Ui},
};

use crate::assets::Icon;
use crate::popover::{WIDTH, check, heading};
use crate::theme::Palette;

// TODO: saved to the user's settings file, once there is one.
#[derive(Default)]
pub struct Settings {
    open: bool,
    palette: Palette,
    /// Since the last [`Self::take_picked`].
    picked: bool,
}

impl Settings {
    pub const fn palette(&self) -> Palette {
        self.palette
    }

    /// The gear, with the settings hanging under it while open.
    pub fn ui(&mut self, ui: &mut Ui) {
        // Its own box, so the popover hangs from the gear and not the bar.
        ui.element(Element::DEFAULT, |ui| {
            let state = if self.open {
                IconState::Lit
            } else {
                IconState::Normal
            };
            if icon_button(ui, Icon::Settings, state).pressed {
                self.open = !self.open;
            }
            if !self.open {
                return;
            }
            let ((), outside) = popover(ui, WIDTH, |ui| {
                heading(ui, "Theme");
                for palette in Palette::ALL {
                    if check(ui, palette.name(), palette == self.palette) && palette != self.palette
                    {
                        self.palette = palette;
                        self.picked = true;
                    }
                }
            });
            if outside {
                self.open = false;
            }
        });
    }

    /// The palette, if it was picked since the last call.
    pub fn take_picked(&mut self) -> Option<Palette> {
        std::mem::take(&mut self.picked).then_some(self.palette)
    }
}
