//! Dragging a tab out of its bar and dropping it where [`Drops::at`] finds.

use gui::{
    canvas::Rect,
    input::{Button, Cursor},
    ui::{Align, Anchor, Border, Element, Size, Ui},
};

use super::dock::{Dock, TAB_GAP, TAB_PADDING, dock_radius, tab_height};
use super::layout::{Divider, cut, extent};
use super::place::{Band, Place, Places};
use super::{MIN_DOCK, MIN_MAIN, Workspace};

/// How far a held tab has to move, in logical pixels, before it's dragged
/// rather than clicked.
const DRAG_THRESHOLD: f32 = 4.;

/// A tab holding the left button.
#[derive(Clone, Copy, Debug)]
pub(super) struct Grip {
    pub(super) place: Place,
    pub(super) index: usize,
    /// Its rect last pass.
    pub(super) rect: Rect,
}

/// A held tab. It stays in its bar until it's let go, then moves to where
/// [`Drops::at`] says.
#[derive(Clone, Copy, Debug)]
pub(super) struct TabDrag {
    /// As it was the first pass it was held. Its copy under the pointer
    /// keeps the same offset from `rect`.
    grip: Grip,
    /// Where the pointer was the first pass it was held.
    start: [f32; 2],
    /// Set once the pointer goes [`DRAG_THRESHOLD`] from `start`.
    moving: bool,
}

/// Where a dragged tab would land, and what shows it.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Landing {
    place: Place,
    /// Where it goes among the place's tabs, counting them as they are now,
    /// the dragged one included.
    index: usize,
    marker: Marker,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Marker {
    /// A zero-width line across the tab row, where the tab goes.
    Between(Rect),
    /// The space the dock takes, or would take once it opens.
    Dock(Rect),
}

/// Width of the line that marks where a dragged tab lands.
const DROP_LINE: f32 = 2.;
/// How much of the accent fills a dock a dragged tab lands in.
const DROP_FILL: u8 = 0x1f;
/// How opaque the copy of a dragged tab is, so what it's over shows through.
const GHOST: u8 = 0xa0;

impl Workspace {
    /// Follows the tab holding the left button, and moves it where it's let
    /// go. While it's dragged, marks where it would land and shows a copy of
    /// it under the pointer, over a cover that keeps the pointer off
    /// everything else. `window` is the workspace's rect last pass.
    pub(super) fn drag_tab(&mut self, ui: &mut Ui, window: Option<Rect>) {
        let input = ui.input();
        // Let go outside the window, it stays where it was.
        let (Some(grip), Some(pointer)) = (self.grip, input.pointer()) else {
            self.tab_drag = None;
            return;
        };
        let mut drag = self
            .tab_drag
            .filter(|drag| drag.grip.place == grip.place && drag.grip.index == grip.index)
            .unwrap_or(TabDrag {
                grip,
                start: pointer,
                moving: false,
            });
        let [x, y] = drag.start;
        drag.moving |= (pointer[0] - x).hypot(pointer[1] - y) >= DRAG_THRESHOLD;
        let released = input.released(Button::Left);
        self.tab_drag = (!released).then_some(drag);
        if !drag.moving {
            return;
        }
        let Some(tab) = self.docks[grip.place.index()].tabs.get(grip.index) else {
            return;
        };
        let size = ui.theme().size;
        let drops = Drops {
            docks: &self.docks,
            sizes: &self.sizes,
            ratios: &self.ratios,
            gap: size.gap,
            tab_height: tab_height(size),
        };
        let landing = drops.at(grip, pointer, tab.panel.places());
        if released {
            if let Some(landing) = landing {
                move_tab(&mut self.docks, grip, landing);
            }
            return;
        }
        let copy = Rect {
            x: drag.grip.rect.x + pointer[0] - x,
            y: drag.grip.rect.y + pointer[1] - y,
            ..drag.grip.rect
        };
        drag_marks(ui, landing, copy, tab.panel.title(), window);
    }
}

/// Marks where a dragged tab would `land`, and shows a copy of it titled
/// `title` at `copy`. A cover over the `window` keeps the pointer off
/// everything else, and shows whether the tab can land there.
fn drag_marks(
    ui: &mut Ui,
    landing: Option<Landing>,
    copy: Rect,
    title: &str,
    window: Option<Rect>,
) {
    let theme = ui.theme();
    let (color, size) = (theme.color, theme.size);
    let float = |rect: Rect| Element {
        size: [Size::Fixed(rect.w), Size::Fixed(rect.h)],
        float: Some(Anchor::At([rect.x, rect.y])),
        ..Element::DEFAULT
    };
    match landing.map(|landing| landing.marker) {
        Some(Marker::Between(line)) => {
            let line = Rect {
                x: DROP_LINE.mul_add(-0.5, line.x),
                w: DROP_LINE,
                ..line
            };
            let element = Element {
                background: Some(color.accent),
                radius: [DROP_LINE / 2.; 4],
                ..float(line)
            };
            ui.element(element, |_| {});
        }
        Some(Marker::Dock(rect)) => {
            let element = Element {
                background: Some(color.accent.alpha(DROP_FILL)),
                border: Some(Border {
                    width: DROP_LINE,
                    color: color.accent,
                }),
                radius: [dock_radius(size); 4],
                ..float(rect)
            };
            ui.element(element, |_| {});
        }
        None => {}
    }
    let element = Element {
        align: [Align::Center; 2],
        background: Some(color.surface.alpha(GHOST)),
        border: Some(Border {
            width: size.line,
            color: color.accent.alpha(GHOST),
        }),
        radius: [size.radius; 4],
        clip: true,
        ..float(copy)
    };
    ui.element(element, |ui| {
        ui.text(theme.ui_text(color.text.alpha(GHOST)), title);
    });
    // Declared last, so it's on top.
    if let Some(window) = window {
        let cover = Element {
            cursor: Some(if landing.is_some() {
                Cursor::Grabbing
            } else {
                Cursor::NotAllowed
            }),
            ..float(window)
        };
        ui.element(cover, |_| {});
    }
}

/// Takes the tab `grip` holds out of its dock in `docks` and puts it where
/// `landing` says, showing it there.
fn move_tab(docks: &mut [Dock; Place::COUNT], grip: Grip, landing: Landing) {
    let Some(tab) = docks[grip.place.index()].remove(grip.index) else {
        return;
    };
    let mut index = landing.index;
    if landing.place == grip.place && index > grip.index {
        index -= 1;
    }
    let target = &mut docks[landing.place.index()];
    let index = index.min(target.tabs.len());
    target.tabs.insert(index, tab);
    target.active = index;
}

/// What a dragged tab can land on, from where things were laid out last
/// pass.
struct Drops<'a> {
    docks: &'a [Dock; Place::COUNT],
    /// As in [`Workspace`].
    sizes: &'a [f32; Band::COUNT],
    ratios: &'a [f32; Band::COUNT],
    gap: f32,
    tab_height: f32,
}

impl Drops<'_> {
    /// Where the tab `grip` holds, dragged to `point`, would land among the
    /// places in `allowed`.
    ///
    /// Over a tab bar, it goes between the tabs there. Over the rest of a
    /// dock, it goes last, unless `point` is where a closed dock would open,
    /// as [`Self::opens`] allows.
    fn at(&self, grip: Grip, point: [f32; 2], allowed: Places) -> Option<Landing> {
        let (place, rect) = Place::ALL.into_iter().find_map(|place| {
            let rect = self.docks[place.index()].rect?;
            rect.contains(point).then_some((place, rect))
        })?;
        let dock = &self.docks[place.index()];
        if allowed.contains(place)
            && let Some(bar) = dock.bar.filter(|bar| bar.contains(point))
        {
            return Some(self.between(place, bar, point[0]));
        }
        let alone = place == grip.place && dock.tabs.len() == 1;
        let main = self.docks[Place::Main.index()].rect;
        let opening = Place::ALL
            .into_iter()
            .filter(|&closed| self.opens(place, closed, allowed, alone))
            .filter_map(|closed| Some((closed, self.preview(closed)?)))
            .filter(|(_, preview)| preview.contains(point))
            // Previews from the main view's corners overlap, and the band on
            // the nearer edge wins.
            .min_by(|(a, _), (b, _)| reach(*a, main, point).total_cmp(&reach(*b, main, point)));
        if let Some((closed, preview)) = opening {
            return Some(Landing {
                place: closed,
                index: 0,
                marker: Marker::Dock(preview),
            });
        }
        allowed.contains(place).then_some(Landing {
            place,
            index: dock.tabs.len(),
            marker: Marker::Dock(rect),
        })
    }

    /// Where among the tabs in `place`'s tab `bar` a tab dropped at `x`
    /// goes: before the first shown tab whose middle is right of `x`.
    fn between(&self, place: Place, bar: Rect, x: f32) -> Landing {
        let shown = &self.docks[place.index()].shown;
        let after = shown
            .iter()
            .take_while(|(_, rect)| rect.w.mul_add(0.5, rect.x) <= x)
            .count();
        let half = TAB_GAP / 2.;
        let (index, at) = match (shown.get(after), shown.last()) {
            (Some(&(index, rect)), _) => (index, rect.x - half),
            (None, Some(&(index, rect))) => (index + 1, rect.right() + half),
            (None, None) => (0, bar.x + TAB_PADDING),
        };
        let line = Rect::new(at, bar.bottom() - self.tab_height, 0., self.tab_height);
        Landing {
            place,
            index,
            marker: Marker::Between(line),
        }
    }

    fn has_tabs(&self, place: Place) -> bool {
        !self.docks[place.index()].tabs.is_empty()
    }

    /// Whether a tab dragged over `over` can open `closed`, a place in
    /// `allowed` with no tabs: the other half of `over`'s band, unless the
    /// tab is `alone` there, or from the main view the first allowed dock of
    /// an empty band in [`Band::AROUND_MAIN`].
    fn opens(&self, over: Place, closed: Place, allowed: Places, alone: bool) -> bool {
        let Some(band) = Band::of(closed) else {
            return false;
        };
        if self.has_tabs(closed) || !allowed.contains(closed) {
            return false;
        }
        if band.docks().iter().any(|&place| self.has_tabs(place)) {
            !alone && Band::of(over) == Some(band)
        } else {
            over == Place::Main
                && Band::AROUND_MAIN.contains(&band)
                && band.docks().iter().find(|&&first| allowed.contains(first)) == Some(&closed)
        }
    }

    /// Roughly where `closed` would sit if it opened: split off its band's
    /// open dock, or cut off the main view's edge at its band's size.
    fn preview(&self, closed: Place) -> Option<Rect> {
        let band = Band::of(closed)?;
        let first = band.docks().first() == Some(&closed);
        if let Some(&open) = band
            .docks()
            .iter()
            .find(|&&place| place != closed && self.has_tabs(place))
        {
            let rect = self.docks[open.index()].rect?;
            let across = 1 - band.axis();
            let extent = extent(rect, across) - self.gap;
            let share = Divider::ratio(band, self.ratios[band.index()], extent).value;
            let [before, after] = cut(rect, across, (share * extent).round(), self.gap);
            return Some(if first { before } else { after });
        }
        let mut main = self.docks[Place::Main.index()].rect?;
        let axis = band.axis();
        // Columns run past the main view, beside the band below it.
        if axis == 0
            && let Some(below) = self.docks[Place::BelowMain.index()].rect
        {
            main.h = below.bottom() - main.y;
        }
        let room = extent(main, axis);
        let size = self.sizes[band.index()]
            .min(room - self.gap - MIN_MAIN[axis])
            .max(MIN_DOCK[axis])
            .min(room);
        Some(if band.grows() > 0. {
            cut(main, axis, size, 0.)[0]
        } else {
            cut(main, axis, room - size, 0.)[1]
        })
    }
}

/// How far `point` is from the edge of `main` that `place`'s band is cut
/// from.
fn reach(place: Place, main: Option<Rect>, point: [f32; 2]) -> f32 {
    let (Some(band), Some(main)) = (Band::of(place), main) else {
        return f32::INFINITY;
    };
    let axis = band.axis();
    let start = [main.x, main.y][axis];
    if band.grows() > 0. {
        point[axis] - start
    } else {
        start + extent(main, axis) - point[axis]
    }
}

#[cfg(test)]
mod tests {
    use super::super::dock::Tab;
    use super::*;
    use crate::panels::{Panel, Placeholder};

    const GAP: f32 = 8.;

    /// The height of a tab bar's tabs.
    const TAB: f32 = 26.;

    fn dock(titles: &[&'static str], rect: Rect) -> Dock {
        let tabs = titles.iter().map(|&title| Tab {
            panel: Panel::Placeholder(Placeholder::new(title, Places::ALL)),
        });
        let shown = (0..).zip(titles).map(|(i, _): (u16, _)| {
            let x = f32::from(i).mul_add(74., rect.x + 6.);
            (usize::from(i), Rect::new(x, rect.y + 4., 72., TAB))
        });
        Dock {
            tabs: tabs.collect(),
            rect: Some(rect),
            bar: (!titles.is_empty()).then_some(Rect::new(rect.x, rect.y, rect.w, 30.)),
            shown: shown.collect(),
            ..Dock::default()
        }
    }

    /// `left` in [`Place::LeftInnerTop`], each tab 72 wide and 2 apart from
    /// 6, beside an empty main view, with everything else closed.
    fn left_column(left: &[&'static str]) -> [Dock; Place::COUNT] {
        let mut docks: [Dock; Place::COUNT] = Default::default();
        docks[Place::LeftInnerTop.index()] = dock(left, Rect::new(0., 0., 240., 900.));
        docks[Place::Main.index()] = dock(&[], Rect::new(248., 0., 1192., 900.));
        docks
    }

    fn drops(docks: &[Dock; Place::COUNT]) -> Drops<'_> {
        const SIZES: [f32; Band::COUNT] = [200., 240., 240., 240., 280., 240.];
        const RATIOS: [f32; Band::COUNT] = [0.5; Band::COUNT];
        Drops {
            docks,
            sizes: &SIZES,
            ratios: &RATIOS,
            gap: GAP,
            tab_height: TAB,
        }
    }

    /// A tab dragged in from a dock the tests don't lay out.
    const DRAGGED: Grip = Grip {
        place: Place::BottomRight,
        index: 0,
        rect: Rect::new(0., 0., 0., 0.),
    };

    fn land(
        docks: &[Dock; Place::COUNT],
        point: [f32; 2],
        allowed: Places,
    ) -> Option<(Place, usize)> {
        drops(docks)
            .at(DRAGGED, point, allowed)
            .map(|landing| (landing.place, landing.index))
    }

    #[test]
    fn dropping_on_a_tab_bar_goes_between_tabs() {
        let docks = left_column(&["Files", "Search"]);
        let landing = drops(&docks).at(DRAGGED, [100., 20.], Places::ALL);
        assert_eq!(
            landing,
            Some(Landing {
                place: Place::LeftInnerTop,
                index: 1,
                marker: Marker::Between(Rect::new(79., 4., 0., TAB)),
            })
        );
        let landing = drops(&docks).at(DRAGGED, [200., 20.], Places::ALL);
        assert_eq!(
            landing.map(|landing| (landing.index, landing.marker)),
            Some((2, Marker::Between(Rect::new(153., 4., 0., TAB))))
        );
        assert_eq!(
            land(&docks, [10., 20.], Places::ALL),
            Some((Place::LeftInnerTop, 0))
        );
    }

    #[test]
    fn dropping_on_a_dock_body_goes_last() {
        let docks = left_column(&["Files", "Search"]);
        let landing = drops(&docks).at(DRAGGED, [100., 100.], Places::ALL);
        assert_eq!(
            landing,
            Some(Landing {
                place: Place::LeftInnerTop,
                index: 2,
                marker: Marker::Dock(Rect::new(0., 0., 240., 900.)),
            })
        );
    }

    #[test]
    fn dropping_on_an_empty_half_splits_the_band() {
        let docks = left_column(&["Files", "Search"]);
        let landing = drops(&docks).at(DRAGGED, [100., 600.], Places::ALL);
        // Half of 900 less the gap each.
        assert_eq!(
            landing,
            Some(Landing {
                place: Place::LeftInnerBottom,
                index: 0,
                marker: Marker::Dock(Rect::new(0., 454., 240., 446.)),
            })
        );
    }

    #[test]
    fn a_lone_tab_cant_split_its_own_band() {
        let docks = left_column(&["Files"]);
        let grip = Grip {
            place: Place::LeftInnerTop,
            ..DRAGGED
        };
        let landing = drops(&docks).at(grip, [100., 600.], Places::ALL);
        assert_eq!(
            landing.map(|landing| (landing.place, landing.index)),
            Some((Place::LeftInnerTop, 1))
        );
        assert_eq!(
            land(&docks, [100., 600.], Places::ALL),
            Some((Place::LeftInnerBottom, 0))
        );
    }

    #[test]
    fn the_main_view_opens_the_nearest_band_around_it() {
        let docks = left_column(&["Files"]);
        let landing = drops(&docks).at(DRAGGED, [1400., 400.], Places::ALL);
        assert_eq!(
            landing.map(|landing| (landing.place, landing.marker)),
            Some((
                Place::RightInnerTop,
                Marker::Dock(Rect::new(1160., 0., 280., 900.))
            ))
        );
        assert_eq!(
            land(&docks, [800., 850.], Places::ALL),
            Some((Place::BelowMain, 0))
        );
        // In both previews, 40 from the right and 50 from the bottom.
        assert_eq!(
            land(&docks, [1400., 850.], Places::ALL),
            Some((Place::RightInnerTop, 0))
        );
        assert_eq!(
            land(&docks, [800., 300.], Places::ALL),
            Some((Place::Main, 0))
        );
        // The left column is open, so the main view doesn't open the one
        // outside it.
        assert_eq!(
            land(&docks, [300., 300.], Places::ALL),
            Some((Place::Main, 0))
        );
    }

    #[test]
    fn drops_stay_in_allowed_places() {
        let docks = left_column(&["Files"]);
        assert_eq!(land(&docks, [800., 300.], Places::BANDS), None);
        assert_eq!(
            land(&docks, [1400., 400.], Places::BANDS),
            Some((Place::RightInnerTop, 0))
        );
        assert_eq!(land(&docks, [100., 20.], Places::MAIN), None);
        assert_eq!(land(&docks, [100., 100.], Places::MAIN), None);
        assert_eq!(
            land(&docks, [1400., 400.], Places::MAIN),
            Some((Place::Main, 0))
        );
    }

    #[test]
    fn nothing_lands_in_a_gap() {
        let docks = left_column(&["Files"]);
        assert_eq!(land(&docks, [244., 400.], Places::ALL), None);
    }

    fn titles(dock: &Dock) -> Vec<&str> {
        dock.tabs.iter().map(|tab| tab.panel.title()).collect()
    }

    #[test]
    fn moving_a_tab_along_its_bar_counts_it_where_it_was() {
        let mut docks = left_column(&["Files", "Search", "Outline"]);
        let grip = Grip {
            place: Place::LeftInnerTop,
            ..DRAGGED
        };
        let landing = drops(&docks).at(grip, [160., 20.], Places::ALL);
        assert_eq!(landing.map(|landing| landing.index), Some(2));
        if let Some(landing) = landing {
            move_tab(&mut docks, grip, landing);
        }
        let left = &docks[Place::LeftInnerTop.index()];
        assert_eq!(titles(left), ["Search", "Files", "Outline"]);
        assert_eq!(left.active, 1);
    }

    #[test]
    fn moving_a_tab_out_shows_its_neighbour() {
        let mut docks = left_column(&["Files", "Search", "Outline"]);
        docks[Place::LeftInnerTop.index()].active = 2;
        let grip = Grip {
            place: Place::LeftInnerTop,
            index: 2,
            ..DRAGGED
        };
        let landing = Landing {
            place: Place::Main,
            index: 0,
            marker: Marker::Dock(Rect::default()),
        };
        move_tab(&mut docks, grip, landing);
        let left = &docks[Place::LeftInnerTop.index()];
        assert_eq!(titles(left), ["Files", "Search"]);
        assert_eq!(left.active, 1);
        assert_eq!(titles(&docks[Place::Main.index()]), ["Outline"]);
    }
}
