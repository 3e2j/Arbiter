//! The strip along the window's top, drawn in place of the system's. Menus
//! go right of the title, and what else goes in it, such as settings, goes
//! left of the window's buttons.
//!
//! The View menu lists the panels a project opens with. Picking one adds
//! another where that kind first sits, as many as are wanted.

use crate::components::{
    ContextMenu, IconState, MenuItem, context_menu, icon_button, spacer, title_bar,
};
use gui::{
    input::{CursorIcon, PointerButton, WindowAction},
    ui::{Align, Element, Length, Ui},
};

use crate::assets::Icon;
use crate::panels::Panel;
use crate::settings::Settings;
use crate::theme;

use super::{Place, Workspace};

/// `title`, the View button, then the settings and the window's buttons at
/// the right end.
pub fn ui(ui: &mut Ui, title: &str, settings: &mut Settings, view: &mut ContextMenu) {
    let theme = theme::current();
    title_bar(ui, |ui| {
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
        let open = menu.is_open();
        let hovered = ui.hovered();
        ui.style().background = (open || hovered).then_some(c.hover);
        let text = if open || hovered { c.text } else { c.dim };
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
/// `panels` are the kinds it offers, each with where it opens. Returns the
/// kind of the panel it added.
pub fn view_menu(
    ui: &mut Ui,
    menu: &mut ContextMenu,
    workspace: &mut Workspace,
    panels: &[(&'static str, Place)],
) -> Option<&'static str> {
    if !menu.is_open() {
        return None;
    }
    let items: Vec<_> = panels
        .iter()
        .map(|&(kind, _)| MenuItem::new(Panel::title_of(kind)))
        .collect();
    let &(kind, place) = panels.get(context_menu(ui, menu, &items)?)?;
    workspace.add(place, Panel::open(kind, None)?);
    Some(kind)
}
