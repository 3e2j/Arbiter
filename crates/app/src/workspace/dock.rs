//! A dock: its tab bar and the active tab's panel.

use crate::components::{Clicks, IconState, icon_button, rule};
use gui::{
    canvas::Rect,
    input::{CursorIcon, PointerButton},
    ui::{Align, Anchor, Direction, Element, Length, ThemeSizes, Ui},
};

use std::ops::Range;

use crate::assets::Icon;
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
pub(super) const fn dock_radius(size: ThemeSizes) -> f32 {
    size.radius * 2.
}

/// A tab holds a row, with a little room above and below it.
pub(super) const fn tab_height(size: ThemeSizes) -> f32 {
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
    /// A tab under the pointer shows a close button over its right end.
    ///
    pub(super) fn ui(&mut self, ui: &mut Ui) -> DockOut {
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
            size: [Length::Grow, Length::Fixed(tab_height + 4. - size.line)],
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
            on_line(ui, Length::Fixed(TAB_PADDING), |_| {});
            let tabs = (0..).zip(self.tabs.iter().zip(&self.widths).enumerate());
            for (key, (i, (tab, &width))) in tabs.skip(range.start).take(range.len()) {
                let fits = (room - TAB_GAP).max(0.);
                let state = Self::tab(ui, key, tab.panel.title(), width.min(fits), i == active);
                if state.left == Some(Left::Pressed) {
                    active = i;
                }
                if let Some(rect) = state.rect {
                    self.shown.push((i, rect));
                    if state.left == Some(Left::Held) {
                        out.held = Some((i, rect));
                    }
                    if state.menu {
                        out.menu = Some((i, rect, Align::Start));
                    }
                }
                if state.left == Some(Left::Closed) {
                    out.close = Some(i);
                }
                on_line(ui, Length::Fixed(TAB_GAP), |_| {});
            }
            on_line(ui, Length::Grow, |_| {});
            let buttons = Element {
                direction: Direction::LeftToRight,
                ..Element::DEFAULT
            };
            on_line(ui, Length::Fit, |ui| {
                ui.element(buttons, |ui| {
                    if !all {
                        let last = self.tabs.len() - 1;
                        let state = |enabled| {
                            if enabled {
                                IconState::Normal
                            } else {
                                IconState::Disabled
                            }
                        };
                        if icon_button(ui, Icon::ChevronLeft, state(active > 0)).clicked {
                            active -= 1;
                        }
                        if icon_button(ui, Icon::ChevronRight, state(active < last)).clicked {
                            active += 1;
                        }
                    }
                    let (rect, clicked) = ui.element(Element::DEFAULT, |ui| {
                        (
                            ui.rect(),
                            icon_button(ui, Icon::Menu, IconState::Normal).clicked,
                        )
                    });
                    if let Some(rect) = rect.filter(|_| clicked) {
                        out.menu = Some((active, rect, Align::End));
                    }
                });
            });
            on_line(ui, Length::Fixed(TAB_PADDING), |_| {});
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
    /// While the pointer is on it, a close button floats over its right end,
    /// in the room set aside for it, so the title doesn't move.
    fn tab(ui: &mut Ui, key: u32, title: &str, width: f32, active: bool) -> TabState {
        let theme = ui.theme();
        let (color, size) = (theme.color, theme.size);
        let radius = [size.radius, size.radius, 0., 0.];
        let element = Element {
            size: [Length::Fixed(width), Length::Fixed(tab_height(size))],
            clip: true,
            cursor: Some(CursorIcon::Pointer),
            ..Element::DEFAULT
        };
        ui.keyed(key, element, |ui| {
            let hovered = ui.hovered();
            let close = if hovered {
                close_button(ui)
            } else {
                Clicks::default()
            };
            let left = if close.clicked {
                Some(Left::Closed)
            } else if ui.dragged(PointerButton::Left).is_some() {
                Some(Left::Held)
            } else if ui.pressed(PointerButton::Left) && !close.pressed {
                Some(Left::Pressed)
            } else {
                None
            };
            let state = TabState {
                rect: ui.rect(),
                left,
                menu: ui.pressed(PointerButton::Right),
            };
            let squeezed = ui.measure(theme.ui_text(color.text), title)[0] > width;
            let mut label = Element {
                size: [Length::Grow; 2],
                align: [Align::Center; 2],
                radius,
                ..Element::DEFAULT
            };
            if squeezed {
                label.align[0] = Align::Start;
                label.padding[0] = size.icon_gap;
            }
            let strip = |fill| Element {
                size: [Length::Grow, Length::Fixed(size.line)],
                background: Some(fill),
                ..Element::DEFAULT
            };
            if active {
                // Inside its border, and down over the bar's line.
                let style = ui.style();
                style.background = Some(color.surface);
                style.border = Some(theme.border());
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
    left: Option<Left>,
    /// Whether the right button went down over it, opening its menu.
    menu: bool,
}

/// What the left button did to a tab this pass.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Left {
    Pressed,
    /// It holds the button, so it can be dragged.
    Held,
    /// Its close button was clicked.
    Closed,
}

/// What a dock's tab bar saw this pass.
#[derive(Default)]
pub(super) struct DockOut {
    /// The tab holding the left button, by index, with its rect.
    pub(super) held: Option<(usize, Rect)>,
    /// The tab whose menu opens, by index, with what the menu hangs from and
    /// which of its edges the menu lines up with.
    pub(super) menu: Option<(usize, Rect, Align)>,
    /// The tab whose close button was clicked, by index.
    pub(super) close: Option<usize>,
}

/// A tab's close button, floating over the right end of the open tab and
/// centred on the tab above the bar's line.
fn close_button(ui: &mut Ui) -> Clicks {
    let theme = ui.theme();
    let (color, size) = (theme.color, theme.size);
    let element = Element {
        size: [Length::Fixed(size.row); 2],
        align: [Align::Center; 2],
        radius: [size.radius; 4],
        float: Some(Anchor::Parent {
            parent: [Align::End, Align::Center],
            own: [Align::End, Align::Center],
            offset: [-size.icon_gap / 2., -size.line / 2.],
            clipped: true,
        }),
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let lit = if ui.hovered() { color.text } else { color.dim };
        ui.icon(Icon::Close, size.icon, lit);
        Clicks {
            pressed: ui.pressed(PointerButton::Left),
            clicked: ui.clicked(PointerButton::Left),
        }
    })
}

/// A piece of the tab bar `width` wide, with the bar's line along its bottom
/// under what `body` declares.
#[track_caller]
fn on_line(ui: &mut Ui, width: Length, body: impl FnOnce(&mut Ui)) {
    let theme = ui.theme();
    let size = theme.size;
    let cell = Element {
        size: [width, Length::Fixed(tab_height(size))],
        ..Element::DEFAULT
    };
    let above = Element {
        size: [Length::Grow; 2],
        align: [Align::Center; 2],
        ..Element::DEFAULT
    };
    ui.element(cell, |ui| {
        ui.element(above, body);
        rule(ui);
    });
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
