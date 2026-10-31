//! The strip along the window's top, drawn in place of the system's. Menus
//! go right of the title, and what else goes in it, such as settings, goes
//! left of the window's buttons.
//!
//! The View menu lists each kind of panel whose desc puts it there. Picking
//! one adds another where that kind opens, as many as are wanted.

use crate::components::{
    ContextMenu, IconState, MenuItem, context_menu, icon_button, spacer, toolbar,
};
use gui::{
    input::{CursorIcon, PointerButton, WindowAction},
    ui::{Align, Element, Length, Ui, window::window_drag},
};

use crate::assets::Icon;
use crate::panels::{PANELS, PanelDesc};
use crate::settings::Settings;
use crate::theme;

/// `title`, the View button, then the settings and the window's buttons at
/// the right end.
pub fn ui(ui: &mut Ui, title: &str, settings: &mut Settings, view: &mut ContextMenu) {
    let theme = theme::current();
    bar(ui, |ui| {
        // With the bar's own half gap, two gaps in, about where the panels'
        // contents start.
        let inset = Element {
            padding: [1.5 * theme.size.gap, 0., theme.size.gap, 0.],
            ..Element::DEFAULT
        };
        ui.element(inset, |ui| ui.text(theme.ui_text(theme.color.dim), title));
        menu_button(ui, "View", view);
        spacer(ui);
        settings.gear(ui);
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

/// A [`toolbar`] that moves the window as [`window_drag`] says, shaded while
/// held.
#[track_caller]
fn bar<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
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

/// `label` as flat text, opening `menu` under it when pressed. Lit while
/// it's open.
#[track_caller]
fn menu_button(ui: &mut Ui, label: &str, menu: &mut ContextMenu) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        size: [Length::Fit, Length::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        align: [Align::Center; 2],
        radius: [s.radius; 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        // Holding the press keeps the bar from taking it as a window drag.
        let held = ui.held(PointerButton::Left);
        let lit = held || menu.is_open() || ui.hovered();
        ui.style().background = lit.then_some(c.hover);
        let text = if lit { c.text } else { c.dim };
        ui.text(theme.ui_text(text), label);
        // The open menu's cover takes the press that closes it.
        if ui.pressed(PointerButton::Left)
            && let Some(rect) = ui.rect()
        {
            menu.open_at([rect.x, rect.bottom()]);
        }
    });
}

/// The View menu while it's open, over everything declared before it.
/// Returns the kind picked.
pub fn view_menu(ui: &mut Ui, menu: &mut ContextMenu) -> Option<&'static PanelDesc> {
    if !menu.is_open() {
        return None;
    }
    let kinds: Vec<_> = PANELS.iter().filter(|desc| desc.in_menu).collect();
    let items: Vec<_> = kinds.iter().map(|desc| MenuItem::new(desc.title)).collect();
    kinds.get(context_menu(ui, menu, &items)?).copied()
}

#[cfg(test)]
mod tests {
    use gui::canvas::IconId;
    use gui::input::Event;
    use gui::ui::testing::Kept;

    use super::*;
    use crate::theme::sizes;

    /// A title bar 300 across holding a button, which it moves the window
    /// around. Returns what the pass asked of the window.
    fn title_bar_pass(kept: &mut Kept) -> Option<WindowAction> {
        kept.pass(|ui| {
            bar(ui, |ui| {
                icon_button(ui, IconId::default(), IconState::Normal)
            })
        });
        kept.out.action
    }

    #[test]
    fn dragging_a_title_bar_moves_the_window_but_not_from_its_buttons() {
        let mut kept = Kept::default();
        sizes(|size| size.row = 20.);
        kept.input.push(Event::Pointer(Some([5., 5.])));
        title_bar_pass(&mut kept);
        kept.input.push(Event::Pressed(PointerButton::Left));
        title_bar_pass(&mut kept);
        kept.input.push(Event::Pointer(Some([10., 5.])));
        assert_eq!(title_bar_pass(&mut kept), None);
        kept.input.push(Event::Released(PointerButton::Left));
        title_bar_pass(&mut kept);
        kept.input.push(Event::Pointer(Some([200., 5.])));
        title_bar_pass(&mut kept);
        kept.input.push(Event::Pressed(PointerButton::Left));
        title_bar_pass(&mut kept);
        // Held but not moved, so a double click can still come.
        assert_eq!(title_bar_pass(&mut kept), None);
        kept.input.push(Event::Pointer(Some([210., 5.])));
        assert_eq!(title_bar_pass(&mut kept), Some(WindowAction::Move));
    }

    /// Clicks the title bar with `press`. Returns what the release asked of
    /// the window.
    fn click_title_bar(kept: &mut Kept, press: Event<'static>) -> Option<WindowAction> {
        kept.input.push(press);
        title_bar_pass(kept);
        title_bar_pass(kept);
        kept.input.push(Event::Released(PointerButton::Left));
        title_bar_pass(kept)
    }

    #[test]
    fn double_clicking_a_title_bar_toggles_maximized() {
        let mut kept = Kept::default();
        sizes(|size| size.row = 20.);
        kept.input.push(Event::Pointer(Some([200., 5.])));
        title_bar_pass(&mut kept);
        let first = Event::Pressed(PointerButton::Left);
        assert_eq!(click_title_bar(&mut kept, first), None);
        let second = Event::PressedAgain(PointerButton::Left);
        assert_eq!(
            click_title_bar(&mut kept, second),
            Some(WindowAction::ToggleMaximize)
        );
    }
}
