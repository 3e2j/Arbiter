//! A dock: its tab bar and the active tab's panel.

use gui::{
    canvas::{IconId, Rect},
    components::icon_button,
    input::{Button, Cursor},
    ui::{Align, Border, Direction, Element, Size, Sizes, Ui},
};

use std::ops::Range;

use crate::assets::{Icon, Icons};
use crate::panels::Panel;

pub(super) struct Tab {
    pub(super) panel: Panel,
}

#[derive(Default)]
pub(super) struct Dock {
    pub(super) tabs: Vec<Tab>,
    /// The tab whose panel is drawn.
    pub(super) active: usize,
    /// Each tab's width this pass, kept so a pass allocates nothing.
    pub(super) widths: Vec<f32>,
    /// Its rect last pass, `None` when it took no space.
    pub(super) rect: Option<Rect>,
    /// Its tab bar's rect last pass, `None` while it has no tabs.
    pub(super) bar: Option<Rect>,
    /// The tabs its bar showed last pass, by index in `tabs`, left to right.
    pub(super) shown: Vec<(usize, Rect)>,
}

/// Rounder than what sits inside it.
pub(super) const fn dock_radius(size: Sizes) -> f32 {
    size.radius * 2.
}

/// A tab holds a row, with a little room above and below it.
pub(super) const fn tab_height(size: Sizes) -> f32 {
    size.row + 2.
}

/// Between two tabs, and after the last one.
pub(super) const TAB_GAP: f32 = 2.;
/// Inside each end of the tab bar.
pub(super) const TAB_PADDING: f32 = 6.;

impl Dock {
    /// Its tab bar, then the active tab's panel under it.
    ///
    /// The bar is darker than the dock, with a line along its bottom that the
    /// tabs sit on. The active tab is the dock's colour and covers the line
    /// under it, so it reads as part of the panel below. The menu button sits
    /// at the right end. Tabs that would be cut off there are left out, the
    /// active one never, and arrows beside the menu step through them all.
    ///
    pub(super) fn ui(&mut self, ui: &mut Ui, icons: &Icons) -> DockOut {
        let mut out = DockOut::default();
        if self.tabs.is_empty() {
            return out;
        }
        let theme = ui.theme();
        let (color, size) = (theme.color, theme.size);
        let tab_height = tab_height(size);
        let top = dock_radius(size) - size.line;
        let bar = Element {
            direction: Direction::LeftToRight,
            size: [Size::Grow, Size::Fixed(tab_height + 4. - size.line)],
            align: [Align::Start, Align::End],
            background: Some(color.page),
            radius: [top, top, 0., 0.],
            // Otherwise it's as wide as its tabs, so they'd always fit.
            clip: true,
            ..Element::DEFAULT
        };
        // Room for a close button either side, so the title stays centred.
        let sides = 2. * (tab_height + size.icon_gap);
        let text = theme.ui_text(color.text);
        self.widths.clear();
        let mut active = self.active;
        ui.element(bar, |ui| {
            self.bar = ui.rect();
            for tab in &self.tabs {
                let width = ui.measure(text, tab.panel.title())[0] + sides;
                self.widths.push(width.ceil());
            }
            let room = |buttons: f32| {
                ui.rect()
                    .map_or(f32::INFINITY, |bar| bar.w - 2. * TAB_PADDING - buttons)
            };
            let (room, crowded) = (room(size.row), room(3. * size.row));
            let range = visible(&self.widths, TAB_GAP, room, crowded, active);
            let all = range.len() == self.tabs.len();
            let room = if all { room } else { crowded };
            on_line(ui, Size::Fixed(TAB_PADDING), |_| {});
            let tabs = (0..).zip(self.tabs.iter().zip(&self.widths).enumerate());
            for (key, (i, (tab, &width))) in tabs.skip(range.start).take(range.len()) {
                let fits = (room - TAB_GAP).max(0.);
                let state = Self::tab(ui, key, tab.panel.title(), width.min(fits), i == active);
                if state.pressed {
                    active = i;
                }
                if let Some(rect) = state.rect {
                    self.shown.push((i, rect));
                    if state.held {
                        out.held = Some((i, rect));
                    }
                    if state.menu {
                        out.menu = Some((i, rect, Align::Start));
                    }
                }
                on_line(ui, Size::Fixed(TAB_GAP), |_| {});
            }
            on_line(ui, Size::Grow, |_| {});
            let buttons = Element {
                direction: Direction::LeftToRight,
                ..Element::DEFAULT
            };
            on_line(ui, Size::Fit, |ui| {
                ui.element(buttons, |ui| {
                    if !all {
                        let last = self.tabs.len() - 1;
                        if arrow(ui, icons.get(Icon::ChevronLeft), active > 0) {
                            active -= 1;
                        }
                        if arrow(ui, icons.get(Icon::ChevronRight), active < last) {
                            active += 1;
                        }
                    }
                    let (rect, clicked) = ui.element(Element::DEFAULT, |ui| {
                        (ui.rect(), icon_button(ui, icons.get(Icon::Menu)))
                    });
                    if let Some(rect) = rect.filter(|_| clicked) {
                        out.menu = Some((active, rect, Align::End));
                    }
                });
            });
            on_line(ui, Size::Fixed(TAB_PADDING), |_| {});
        });
        self.active = active;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            ui.element(Element::column().padded(size.gap), |ui| tab.panel.ui(ui));
        }
        out
    }

    /// Takes out the tab at `index`, showing the one that slides into its
    /// place, or the new last.
    pub(super) fn remove(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        if self.active > index {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        Some(tab)
    }

    /// One tab `width` wide, keyed by `key`, its top corners rounded and its
    /// title centred, or from the left when it's narrower than the title asks.
    fn tab(ui: &mut Ui, key: u32, title: &str, width: f32, active: bool) -> TabState {
        let theme = ui.theme();
        let (color, size) = (theme.color, theme.size);
        let radius = [size.radius, size.radius, 0., 0.];
        let element = Element {
            size: [Size::Fixed(width), Size::Fixed(tab_height(size))],
            clip: true,
            cursor: Some(Cursor::Pointer),
            ..Element::DEFAULT
        };
        ui.keyed(key, element, |ui| {
            let state = TabState {
                rect: ui.rect(),
                pressed: ui.pressed(Button::Left),
                held: ui.dragged(Button::Left).is_some(),
                menu: ui.pressed(Button::Right),
            };
            let hovered = ui.hovered();
            let squeezed = ui.measure(theme.ui_text(color.text), title)[0] > width;
            let mut label = Element {
                size: [Size::Grow; 2],
                align: [Align::Center; 2],
                radius,
                ..Element::DEFAULT
            };
            if squeezed {
                label.align[0] = Align::Start;
                label.padding[0] = size.icon_gap;
            }
            let strip = |fill| Element {
                size: [Size::Grow, Size::Fixed(size.line)],
                background: Some(fill),
                ..Element::DEFAULT
            };
            if active {
                // Inside its border, and down over the bar's line.
                let style = ui.style();
                style.background = Some(color.surface);
                style.border = Some(Border {
                    width: size.line,
                    color: color.line,
                });
                style.radius = radius;
                style.padding = [size.line, 0., size.line, 0.];
                ui.element(label, |ui| {
                    ui.text(theme.ui_text(color.text), title);
                });
                ui.element(strip(color.surface), |_| {});
            } else {
                label.background = Some(if hovered { color.selected } else { color.hover });
                ui.element(label, |ui| {
                    ui.text(theme.ui_text(color.dim), title);
                });
                ui.element(strip(color.line), |_| {});
            }
            state
        })
    }
}

/// What a tab saw this pass.
struct TabState {
    /// Its rect last pass.
    pub(super) rect: Option<Rect>,
    pressed: bool,
    /// Whether it holds the left button, so it can be dragged.
    held: bool,
    /// Whether the right button went down over it, opening its menu.
    menu: bool,
}

/// What a dock's tab bar saw this pass.
#[derive(Default)]
pub(super) struct DockOut {
    /// The tab holding the left button, by index, with its rect.
    pub(super) held: Option<(usize, Rect)>,
    /// The tab whose menu opens, by index, with what the menu hangs from and
    /// which of its edges the menu lines up with.
    pub(super) menu: Option<(usize, Rect, Align)>,
}

/// A piece of the tab bar `width` wide, with the bar's line along its bottom
/// under what `body` declares.
#[track_caller]
fn on_line(ui: &mut Ui, width: Size, body: impl FnOnce(&mut Ui)) {
    let theme = ui.theme();
    let size = theme.size;
    let cell = Element {
        size: [width, Size::Fixed(tab_height(size))],
        ..Element::DEFAULT
    };
    let above = Element {
        size: [Size::Grow; 2],
        align: [Align::Center; 2],
        ..Element::DEFAULT
    };
    let line = Element {
        size: [Size::Grow, Size::Fixed(size.line)],
        background: Some(theme.color.line),
        ..Element::DEFAULT
    };
    ui.element(cell, |ui| {
        ui.element(above, body);
        ui.element(line, |_| {});
    });
}

/// A button that steps between tabs, dimmed and inert when there's no tab
/// that way. Returns whether it was clicked.
#[track_caller]
fn arrow(ui: &mut Ui, icon: IconId, enabled: bool) -> bool {
    if enabled {
        return icon_button(ui, icon);
    }
    let theme = ui.theme();
    let size = theme.size;
    let element = Element {
        size: [Size::Fixed(size.row); 2],
        align: [Align::Center; 2],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.icon(icon, size.icon, theme.color.dim.alpha(0x60));
    });
    false
}

/// Which of the tabs `widths` wide show, each followed by `gap`. All of them
/// when they fit in `room`, otherwise as many as fit in `crowded`, which
/// leaves space for the arrows. Those start from the left, but `active` is
/// always among them, so the tabs left of it go first.
fn visible(widths: &[f32], gap: f32, room: f32, crowded: f32, active: usize) -> Range<usize> {
    let Some(last) = widths.len().checked_sub(1) else {
        return 0..0;
    };
    let total: f32 = widths.iter().map(|width| width + gap).sum();
    if total <= room {
        return 0..widths.len();
    }
    let active = active.min(last);
    let mut used = widths[active] + gap;
    let mut start = active;
    while start > 0 && used + widths[start - 1] + gap <= crowded {
        start -= 1;
        used += widths[start] + gap;
    }
    let mut end = active + 1;
    while end < widths.len() && used + widths[end] + gap <= crowded {
        used += widths[end] + gap;
        end += 1;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_shows_every_tab_that_fits() {
        assert_eq!(visible(&[50., 50., 50.], 2., 156., 100., 2), 0..3);
        assert_eq!(visible(&[], 2., 0., 0., 0), 0..0);
    }

    #[test]
    fn visible_leaves_out_tabs_past_the_buttons() {
        // 156 would fit them all, but the arrows take some of it.
        assert_eq!(visible(&[50., 50., 50.], 2., 155., 110., 0), 0..2);
        assert_eq!(visible(&[50., 50., 50.], 2., 155., 110., 1), 0..2);
    }

    #[test]
    fn visible_drops_from_the_left_to_keep_the_active_tab() {
        assert_eq!(visible(&[50., 50., 50., 50.], 2., 155., 110., 3), 2..4);
        assert_eq!(visible(&[50., 50., 50., 50.], 2., 155., 110., 2), 1..3);
    }

    #[test]
    fn visible_keeps_a_active_tab_too_wide_for_the_bar() {
        assert_eq!(visible(&[50., 300., 50.], 2., 155., 110., 1), 1..2);
    }
}
