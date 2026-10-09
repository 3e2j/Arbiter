//! A dock's menu, opened from the button at the right end of its tab bar or
//! by right clicking a tab: the dock's tabs, to show or close, over a map of
//! every place to move the menu's tab to.

use gui::{
    canvas::Rect,
    input::{Button, Cursor, Key},
    ui::{Align, Anchor, Border, Direction, Element, Size, Ui},
};

use crate::assets::{Icon, Icons};

use super::Workspace;
use super::dock::{Dock, TAB_GAP, TAB_PADDING, dock_radius, tab_height};
use super::place::{Place, Places};

/// The open menu.
#[derive(Clone, Copy, Debug)]
pub(super) struct Menu {
    pub(super) place: Place,
    /// The tab the map moves, by index in the dock.
    pub(super) tab: usize,
    /// What it hangs from, as of the pass it opened.
    pub(super) from: Rect,
    /// Which of `from`'s edges it lines up with across: [`Align::Start`] for
    /// the left, anything else for the right.
    pub(super) align: Align,
}

/// What clicking part of the menu does.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Choice {
    Show(usize),
    Close(usize),
    MoveTo(Place),
    /// A press outside the menu.
    Dismiss,
}

const WIDTH: f32 = 216.;
/// The map's size in grid units, as `[width, height]`.
const MAP: [f32; 2] = [7., 5.5];
/// How much of the accent fills the place under the pointer on the map.
const MAP_HOVER: u8 = 0x40;

/// Where `place` sits on the map, as `[left, top, right, bottom]` in [`MAP`]
/// units, laid out as the bands are.
const fn cell(place: Place) -> [f32; 4] {
    match place {
        Place::LeftOuterTop => [0., 0., 1., 2.],
        Place::LeftOuterBottom => [0., 2., 1., 4.],
        Place::LeftInnerTop => [1., 0., 2., 2.],
        Place::LeftInnerBottom => [1., 2., 2., 4.],
        Place::Main => [2., 0., 5., 2.5],
        Place::BelowMain => [2., 2.5, 5., 4.],
        Place::RightInnerTop => [5., 0., 6., 2.],
        Place::RightInnerBottom => [5., 2., 6., 4.],
        Place::RightOuterTop => [6., 0., 7., 2.],
        Place::RightOuterBottom => [6., 2., 7., 4.],
        Place::BottomLeft => [0., 4., 3.5, 5.5],
        Place::BottomRight => [3.5, 4., 7., 5.5],
    }
}

/// `place`'s cell on a map `size` big, from the map's top left, on whole
/// pixels so cell edges stay sharp.
fn cell_rect(place: Place, size: [f32; 2]) -> Rect {
    let [left, top, right, bottom] = cell(place);
    let x = |at: f32| (at * size[0] / MAP[0]).round();
    let y = |at: f32| (at * size[1] / MAP[1]).round();
    Rect::new(x(left), y(top), x(right) - x(left), y(bottom) - y(top)).inset(TAB_GAP / 2.)
}

/// The top left of a `size` menu under `from`, lined up by `align`, or over
/// it when there's no room below, kept inside `window`.
fn hang(from: Rect, align: Align, [w, h]: [f32; 2], window: Rect) -> [f32; 2] {
    let x = if align == Align::Start {
        from.x
    } else {
        from.right() - w
    };
    let x = x.min(window.right() - w).max(window.x);
    let y = if from.bottom() + h <= window.bottom() {
        from.bottom()
    } else {
        (from.y - h).max(window.y)
    };
    [x, y]
}

impl Workspace {
    /// The open menu, over a cover that keeps the pointer off everything
    /// else and closes it when pressed, then does what was clicked. `window`
    /// is the workspace's rect last pass.
    pub(super) fn menu(&mut self, ui: &mut Ui, window: Option<Rect>) {
        let (Some(mut menu), Some(window)) = (self.menu, window) else {
            return;
        };
        let escaped = ui
            .input()
            .keys()
            .iter()
            .any(|press| press.key == Key::Escape);
        // Before it's declared, since nothing makes the pass run again to
        // drop it.
        if escaped || menu.tab >= self.docks[menu.place.index()].tabs.len() {
            self.menu = None;
            return;
        }
        let choice = menu_ui(ui, menu, &self.docks, &self.icons, window);
        let dock = &mut self.docks[menu.place.index()];
        match choice {
            None => return,
            Some(Choice::Show(index)) => {
                dock.active = index;
                menu.tab = index;
            }
            Some(Choice::Close(index)) => {
                dock.remove(index);
                if dock.tabs.is_empty() {
                    self.menu = None;
                    return;
                }
                menu.tab = match index.cmp(&menu.tab) {
                    std::cmp::Ordering::Equal => dock.active,
                    std::cmp::Ordering::Less => menu.tab - 1,
                    std::cmp::Ordering::Greater => menu.tab,
                };
            }
            Some(Choice::MoveTo(place)) => {
                if let Some(tab) = dock.remove(menu.tab) {
                    let target = &mut self.docks[place.index()];
                    target.tabs.push(tab);
                    target.active = target.tabs.len() - 1;
                }
                self.menu = None;
                return;
            }
            Some(Choice::Dismiss) => {
                self.menu = None;
                return;
            }
        }
        self.menu = Some(menu);
    }
}

/// Declares `menu` inside `window`: a row per tab of its dock over a map of
/// the places, as [`tab_row`] and [`map_cell`] draw them. Returns what was
/// clicked.
fn menu_ui(
    ui: &mut Ui,
    menu: Menu,
    docks: &[Dock; Place::COUNT],
    icons: &Icons,
    window: Rect,
) -> Option<Choice> {
    let theme = ui.theme();
    let (color, size) = (theme.color, theme.size);
    let dock = &docks[menu.place.index()];
    let allowed = dock
        .tabs
        .get(menu.tab)
        .map_or(Places::default(), |tab| tab.panel.places());
    let inner = TAB_PADDING.mul_add(-2., WIDTH);
    let map = [inner, (inner * MAP[1] / MAP[0]).round()];
    let rows: f32 = dock.tabs.iter().map(|_| tab_height(size)).sum();
    let height = TAB_PADDING.mul_add(3., rows + map[1]);
    let at = hang(menu.from, menu.align, [WIDTH, height], window);

    let cover = Element {
        size: [Size::Fixed(window.w), Size::Fixed(window.h)],
        float: Some(Anchor::At([window.x, window.y])),
        ..Element::DEFAULT
    };
    let dismissed = ui.element(cover, |ui| {
        ui.pressed(Button::Left) || ui.pressed(Button::Right)
    });
    // Declared after the cover, so it's on top of it.
    let element = Element {
        size: [Size::Fixed(WIDTH), Size::Fixed(height)],
        padding: [TAB_PADDING; 4],
        gap: TAB_PADDING,
        background: Some(color.surface),
        border: Some(Border {
            width: size.line,
            color: color.line,
        }),
        radius: [dock_radius(size); 4],
        float: Some(Anchor::At(at)),
        ..Element::DEFAULT
    };
    let chosen = ui.element(element, |ui| {
        let across = Element {
            size: [Size::Grow, Size::Fit],
            ..Element::DEFAULT
        };
        let mut chosen = None;
        ui.element(across, |ui| {
            for (key, (index, tab)) in (0..).zip(dock.tabs.iter().enumerate()) {
                let picked = ui.keyed(key, across, |ui| {
                    tab_row(ui, icons, tab.panel.title(), index == menu.tab)
                });
                chosen = chosen.or(picked.map(|row| match row {
                    RowClick::Show => Choice::Show(index),
                    RowClick::Close => Choice::Close(index),
                }));
            }
        });
        let map_element = Element {
            size: [Size::Fixed(map[0]), Size::Fixed(map[1])],
            ..Element::DEFAULT
        };
        ui.element(map_element, |ui| {
            for place in Place::ALL {
                let state = if place == menu.place {
                    CellState::Own
                } else if !allowed.contains(place) {
                    CellState::Blocked
                } else if docks[place.index()].tabs.is_empty() {
                    CellState::Empty
                } else {
                    CellState::Occupied
                };
                if map_cell(ui, cell_rect(place, map), state) {
                    chosen = chosen.or(Some(Choice::MoveTo(place)));
                }
            }
        });
        chosen
    });
    chosen.or(dismissed.then_some(Choice::Dismiss))
}

/// What a click on a [`tab_row`] does.
#[derive(Clone, Copy)]
enum RowClick {
    Show,
    Close,
}

/// A tab's row, filling the menu across, titled `title` from the left. The
/// `selected` one is lit. A close button shows at its right end while the
/// pointer is on the row.
fn tab_row(ui: &mut Ui, icons: &Icons, title: &str, selected: bool) -> Option<RowClick> {
    let theme = ui.theme();
    let (color, size) = (theme.color, theme.size);
    let row = tab_height(size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Size::Grow, Size::Fixed(row)],
        align: [Align::Start, Align::Center],
        radius: [size.radius; 4],
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let hovered = ui.hovered();
        ui.style().background = if selected {
            Some(color.selected)
        } else {
            hovered.then_some(color.hover)
        };
        let label = Element {
            size: [Size::Grow; 2],
            padding: [size.icon_gap, 0., size.icon_gap, 0.],
            align: [Align::Start, Align::Center],
            clip: true,
            ..Element::DEFAULT
        };
        let text = if selected { color.text } else { color.dim };
        ui.element(label, |ui| {
            ui.text(theme.ui_text(text), title);
        });
        let close = Element {
            size: [Size::Fixed(row); 2],
            align: [Align::Center; 2],
            ..Element::DEFAULT
        };
        let closed = ui.element(close, |ui| {
            if hovered {
                let lit = if ui.hovered() { color.text } else { color.dim };
                ui.icon(icons.get(Icon::Close), size.icon, lit);
            }
            ui.clicked(Button::Left)
        });
        if closed {
            Some(RowClick::Close)
        } else {
            ui.clicked(Button::Left).then_some(RowClick::Show)
        }
    })
}

/// How a place looks on the map.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CellState {
    /// The menu's own place.
    Own,
    /// The menu's tab can't go there. Greyed out.
    Blocked,
    Occupied,
    Empty,
}

/// A place on the map at `rect`, from the map's top left. Returns whether it
/// was clicked, never for [`CellState::Own`] or [`CellState::Blocked`].
fn map_cell(ui: &mut Ui, rect: Rect, state: CellState) -> bool {
    let theme = ui.theme();
    let (color, size) = (theme.color, theme.size);
    let enabled = matches!(state, CellState::Occupied | CellState::Empty);
    let element = Element {
        size: [Size::Fixed(rect.w), Size::Fixed(rect.h)],
        radius: [size.radius / 2.; 4],
        float: Some(Anchor::Parent {
            parent: [Align::Start; 2],
            own: [Align::Start; 2],
            offset: [rect.x, rect.y],
            clipped: false,
        }),
        cursor: Some(match state {
            CellState::Own => Cursor::Default,
            CellState::Blocked => Cursor::NotAllowed,
            CellState::Occupied | CellState::Empty => Cursor::Pointer,
        }),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let border = |color| {
            Some(Border {
                width: size.line,
                color,
            })
        };
        let (fill, line) = match state {
            CellState::Own => (color.selected, border(color.accent)),
            CellState::Blocked => (color.page, None),
            _ if ui.hovered() => (color.accent.alpha(MAP_HOVER), border(color.accent)),
            CellState::Occupied => (color.hover, border(color.line)),
            CellState::Empty => (color.surface, border(color.line)),
        };
        let style = ui.style();
        style.background = Some(fill);
        style.border = line;
        enabled && ui.clicked(Button::Left)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Rect = Rect::new(0., 0., 800., 600.);

    #[test]
    fn hang_lines_up_under_what_it_hangs_from() {
        let from = Rect::new(300., 20., 26., 26.);
        assert_eq!(hang(from, Align::End, [216., 100.], WINDOW), [110., 46.]);
        assert_eq!(hang(from, Align::Start, [216., 100.], WINDOW), [300., 46.]);
    }

    #[test]
    fn hang_stays_inside_the_window() {
        let left = Rect::new(10., 20., 26., 26.);
        assert_eq!(hang(left, Align::End, [216., 100.], WINDOW)[0], 0.);
        let right = Rect::new(700., 20., 60., 26.);
        assert_eq!(hang(right, Align::Start, [216., 100.], WINDOW)[0], 584.);
        let low = Rect::new(300., 550., 26., 26.);
        assert_eq!(hang(low, Align::End, [216., 100.], WINDOW)[1], 450.);
    }

    #[test]
    fn the_map_has_every_place_without_overlaps() {
        let map = [204., 160.];
        let cells = Place::ALL.map(|place| (place, cell_rect(place, map)));
        for (at, &(place, a)) in (1..).zip(&cells) {
            assert!(a.x >= 0. && a.y >= 0., "{place:?}");
            assert!(a.right() <= map[0] && a.bottom() <= map[1], "{place:?}");
            for &(other, b) in &cells[at..] {
                assert!(!a.overlaps(b), "{place:?} overlaps {other:?}");
            }
        }
    }
}
