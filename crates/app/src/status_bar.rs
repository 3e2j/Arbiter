//! The strip along the window's bottom, with a toggle at its right end for
//! each [`Side`]'s bands.

use gui::{
    components::{IconState, icon_button, spacer, toolbar},
    ui::Ui,
};

use crate::assets::Icon;
use crate::workspace::{Side, Workspace};

pub fn ui(ui: &mut Ui, workspace: &mut Workspace) {
    toolbar(ui, |ui| {
        spacer(ui);
        for side in Side::ALL {
            let icon = match side {
                Side::Left => Icon::LayoutLeft,
                Side::Bottom => Icon::LayoutBottom,
                Side::Right => Icon::LayoutRight,
            };
            let state = if !workspace.side_has_tabs(side) {
                IconState::Disabled
            } else if workspace.side_shown(side) {
                IconState::Lit
            } else {
                IconState::Normal
            };
            if icon_button(ui, icon, state).clicked {
                workspace.toggle_side(side);
            }
        }
    });
}
