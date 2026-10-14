//! The strip along the window's top, drawn in place of the system's. What
//! goes in it besides the window's buttons, such as settings, goes left of
//! them.

use gui::{
    components::{IconState, icon_button, spacer, title_bar},
    input::WindowAction,
    ui::{Element, Ui},
};

use crate::assets::Icon;
use crate::settings::Settings;

/// `title`, then the settings and the window's buttons at the right end.
pub fn ui(ui: &mut Ui, title: &str, settings: &mut Settings) {
    let theme = ui.theme();
    title_bar(ui, |ui| {
        // With the bar's own half gap, two gaps in, about where the panels'
        // contents start.
        let inset = Element {
            padding: [1.5 * theme.size.gap, 0., 0., 0.],
            ..Element::DEFAULT
        };
        ui.element(inset, |ui| ui.text(theme.ui_text(theme.color.dim), title));
        spacer(ui);
        settings.ui(ui);
        let maximize = if ui.input().maximized() {
            Icon::WindowRestore
        } else {
            Icon::WindowMaximize
        };
        let buttons = [
            (Icon::WindowMinimize, WindowAction::Minimize),
            (maximize, WindowAction::ToggleMaximize),
            (Icon::Close, WindowAction::Close),
        ];
        for (icon, action) in buttons {
            if icon_button(ui, icon, IconState::Normal).clicked {
                ui.window(action);
            }
        }
    });
}
