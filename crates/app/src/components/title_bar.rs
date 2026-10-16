//! The bar along the top of a window drawn without the system's frame.

use gui::ui::Ui;
use gui::ui::window::window_drag;

use super::toolbar;
use crate::theme;

/// A [`toolbar`] that moves the window as [`window_drag`] says, shaded while
/// held.
#[track_caller]
pub fn title_bar<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let (c, s) = (theme::current().color, theme::current().size);
    toolbar(ui, |ui| {
        let out = body(ui);
        let held = window_drag(ui);
        // Keeps inside the window's corners, which are only round while
        // it isn't maximized.
        let radius = if ui.input().maximized() {
            0.
        } else {
            s.window_radius
        };
        let style = ui.style();
        style.background = held.then_some(c.hover);
        style.radius = [radius, radius, 0., 0.];
        out
    })
}
