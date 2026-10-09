//! The window's contents: docks of tabs around the main view.
//!
//! Every dock has a fixed [`Place`]. The docks besides [`Place::Main`] sit in
//! [`Band`]s, strips cut off the window's edges in a fixed order, and a band
//! with no tabs takes no space.
//!
//! A [`Divider`] sits on each band's inner edge and between its two docks,
//! taking the place of the gap there. Dragging one sets a [`Split`], and
//! bands shrink to keep [`MIN_MAIN`] when the window does.
//!
//! A tab dragged out of its bar moves where it's let go, among the
//! [`Places`] its panel allows, as [`Drops::at`] finds.

use gui::{
    canvas::{IconId, Rect},
    input::{Button, Cursor},
    ui::{Align, Anchor, Border, Direction, Element, Size, Sizes, Ui},
};

use std::ops::Range;

use gui::components::icon_button;

use crate::assets::{Icon, Icons};
use crate::panels::Panel;

/// The smallest a dock gets, as `[width, height]` in logical pixels.
const MIN_DOCK: [f32; 2] = [144., 150.];
/// The smallest the main view gets, as `[width, height]` in logical pixels.
/// Bands shrink to keep it when the window does.
const MIN_MAIN: [f32; 2] = [640., 360.];

// Columns run the main view's height, and the bottom row the window's width,
// so a main view this size leaves room for two docks in either.
const _: () = assert!(MIN_MAIN[0] >= 2. * MIN_DOCK[0] && MIN_MAIN[1] >= 2. * MIN_DOCK[1]);

/// A fixed place in the window that holds tabs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Place {
    /// What the bands leave in the middle of the window. It's in no band, and
    /// takes its space with or without tabs.
    Main,
    LeftOuterTop,
    LeftOuterBottom,
    LeftInnerTop,
    LeftInnerBottom,
    RightInnerTop,
    RightInnerBottom,
    RightOuterTop,
    RightOuterBottom,
    BelowMain,
    BottomLeft,
    BottomRight,
}

impl Place {
    pub const COUNT: usize = Self::ALL.len();

    const ALL: [Self; 12] = [
        Self::Main,
        Self::LeftOuterTop,
        Self::LeftOuterBottom,
        Self::LeftInnerTop,
        Self::LeftInnerBottom,
        Self::RightInnerTop,
        Self::RightInnerBottom,
        Self::RightOuterTop,
        Self::RightOuterBottom,
        Self::BelowMain,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    const fn index(self) -> usize {
        self as usize
    }
}

/// A set of [`Place`]s, one bit each.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Places(u16);

impl Places {
    pub const ALL: Self = Self::new(&Place::ALL);
    pub const MAIN: Self = Self::new(&[Place::Main]);
    /// The columns either side of the main view.
    pub const SIDES: Self = Self::new(&[
        Place::LeftOuterTop,
        Place::LeftOuterBottom,
        Place::LeftInnerTop,
        Place::LeftInnerBottom,
        Place::RightInnerTop,
        Place::RightInnerBottom,
        Place::RightOuterTop,
        Place::RightOuterBottom,
    ]);
    /// The rows below the main view.
    pub const BOTTOM: Self = Self::new(&[Place::BelowMain, Place::BottomLeft, Place::BottomRight]);
    /// Every place in a band, so all but [`Place::Main`].
    pub const BANDS: Self = Self::SIDES.union(Self::BOTTOM);

    const fn new(places: &[Place]) -> Self {
        let mut set = Self(0);
        let mut rest = places;
        while let [place, tail @ ..] = rest {
            set.0 |= Self::bit(*place);
            rest = tail;
        }
        set
    }

    const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn bit(place: Place) -> u16 {
        1 << place as u16
    }

    const fn contains(self, place: Place) -> bool {
        self.0 & Self::bit(place) != 0
    }
}

/// A strip cut off one edge of the window, holding up to two docks side by
/// side across it.
///
/// Listed in the order they're cut. The bottom row goes first so it spans the
/// window. Outer columns go before inner ones so they sit against the window
/// edge. `BelowMain` goes last so it spans only the main view.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Band {
    BottomRow,
    LeftOuter,
    LeftInner,
    RightOuter,
    RightInner,
    BelowMain,
}

impl Band {
    const COUNT: usize = Self::ALL.len();

    const ALL: [Self; 6] = [
        Self::BottomRow,
        Self::LeftOuter,
        Self::LeftInner,
        Self::RightOuter,
        Self::RightInner,
        Self::BelowMain,
    ];

    /// The bands against the main view. A tab dragged over the main view can
    /// open one of these. The others only take tabs while they're open.
    const AROUND_MAIN: [Self; 3] = [Self::LeftInner, Self::RightInner, Self::BelowMain];

    /// The band holding `place`, or `None` for [`Place::Main`].
    fn of(place: Place) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|band| band.docks().contains(&place))
    }

    /// Top then bottom for a column, left then right for a row.
    const fn docks(self) -> &'static [Place] {
        match self {
            Self::BottomRow => &[Place::BottomLeft, Place::BottomRight],
            Self::LeftOuter => &[Place::LeftOuterTop, Place::LeftOuterBottom],
            Self::LeftInner => &[Place::LeftInnerTop, Place::LeftInnerBottom],
            Self::RightOuter => &[Place::RightOuterTop, Place::RightOuterBottom],
            Self::RightInner => &[Place::RightInnerTop, Place::RightInnerBottom],
            Self::BelowMain => &[Place::BelowMain],
        }
    }

    /// Its width for a column, or its height for a row, in logical pixels.
    const fn default_size(self) -> f32 {
        match self {
            Self::BottomRow => 200.,
            Self::LeftOuter | Self::LeftInner | Self::RightOuter | Self::BelowMain => 240.,
            Self::RightInner => 280.,
        }
    }

    /// The axis its size is along: 0 for a column's width, 1 for a row's
    /// height. A row is cut off the bottom, so it spans across and stacks its
    /// docks left to right.
    const fn axis(self) -> usize {
        match self {
            Self::BottomRow | Self::BelowMain => 1,
            Self::LeftOuter | Self::LeftInner | Self::RightOuter | Self::RightInner => 0,
        }
    }

    /// How its size changes as its divider moves right or down. The divider
    /// is on its inner edge, so it's after the band on the left and before it
    /// everywhere else.
    const fn grows(self) -> f32 {
        match self {
            Self::LeftOuter | Self::LeftInner => 1.,
            Self::BottomRow | Self::RightOuter | Self::RightInner | Self::BelowMain => -1.,
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// A value one [`Divider`] sets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Split {
    /// The band's width or height. Grows into the main view.
    Size(Band),
    /// The first dock's share of the band, while both its docks have tabs.
    Ratio(Band),
}

/// A line between two boxes, dragged along `axis` to set `split`.
#[derive(Clone, Copy, Debug)]
struct Divider {
    split: Split,
    axis: usize,
    /// The split as laid out, which is less than asked for when the window
    /// is too small.
    value: f32,
    /// How far the value moves per logical pixel the pointer moves.
    rate: f32,
    min: f32,
    max: f32,
}

/// The divider being dragged.
#[derive(Clone, Copy, Debug)]
struct Drag {
    split: Split,
    /// Where the pointer has taken the split before it's clamped, so past a
    /// limit the divider waits for the pointer to come back to it.
    wanted: f32,
}

/// What the bands are laid out from this pass.
struct Pass {
    /// Indexed by [`Band`], after [`fit`].
    sizes: [f32; Band::COUNT],
    /// How far the bands along each axis can grow before the main view
    /// reaches [`MIN_MAIN`].
    spare: [f32; 2],
    drag: Option<Drag>,
}

/// How far a held tab has to move, in logical pixels, before it's dragged
/// rather than clicked.
const DRAG_THRESHOLD: f32 = 4.;

/// A tab holding the left button.
#[derive(Clone, Copy, Debug)]
struct Grip {
    place: Place,
    index: usize,
    /// Its rect last pass.
    rect: Rect,
}

/// A held tab. It stays in its bar until it's let go, then moves to where
/// [`Drops::at`] says.
#[derive(Clone, Copy, Debug)]
struct TabDrag {
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

struct Tab {
    panel: Panel,
}

#[derive(Default)]
struct Dock {
    tabs: Vec<Tab>,
    /// The tab whose panel is drawn.
    active: usize,
    /// Each tab's width this pass, kept so a pass allocates nothing.
    widths: Vec<f32>,
    /// Its rect last pass, `None` when it took no space.
    rect: Option<Rect>,
    /// Its tab bar's rect last pass, `None` while it has no tabs.
    bar: Option<Rect>,
    /// The tabs its bar showed last pass, by index in `tabs`, left to right.
    shown: Vec<(usize, Rect)>,
}

pub struct Workspace {
    docks: [Dock; Place::COUNT],
    /// Indexed by [`Band`], as the user last set them. [`fit`] may lay them
    /// out smaller.
    sizes: [f32; Band::COUNT],
    /// Indexed by [`Band`].
    ratios: [f32; Band::COUNT],
    drag: Option<Drag>,
    /// The tab holding the left button this pass.
    grip: Option<Grip>,
    tab_drag: Option<TabDrag>,
    icons: Icons,
}

impl Workspace {
    /// With no tabs, each band at its default size.
    pub fn new(icons: Icons) -> Self {
        Self {
            docks: Default::default(),
            sizes: Band::ALL.map(Band::default_size),
            ratios: [0.5; Band::COUNT],
            drag: None,
            grip: None,
            tab_drag: None,
            icons,
        }
    }

    /// Adds `panel` as the last tab in `place`.
    pub fn add(&mut self, panel: Panel, place: Place) {
        self.docks[place.index()].tabs.push(Tab { panel });
    }

    /// Declares the bands in the order they're cut, outside in, each wrapping
    /// what's left.
    pub fn ui(&mut self, ui: &mut Ui) {
        let gap = ui.theme().size.gap;
        for dock in &mut self.docks {
            dock.rect = None;
            dock.bar = None;
            dock.shown.clear();
        }
        self.grip = None;
        ui.element(Element::column().padded(gap), |ui| {
            let window = ui.rect();
            let pass = self.pass(window.map(|rect| [rect.w, rect.h]), gap);
            ui.element(Element::row(), |ui| {
                self.band(ui, &pass, Band::LeftOuter);
                self.band(ui, &pass, Band::LeftInner);
                ui.element(Element::column(), |ui| {
                    self.dock(ui, Place::Main, [Size::Grow; 2]);
                    self.band(ui, &pass, Band::BelowMain);
                });
                self.band(ui, &pass, Band::RightInner);
                self.band(ui, &pass, Band::RightOuter);
            });
            self.band(ui, &pass, Band::BottomRow);
            self.drag_tab(ui, window);
        });
    }

    /// Fits the bands into a workspace last laid out `size` big, padded by
    /// `gap`. The drag carries over only if its divider asks for it again.
    fn pass(&mut self, size: Option<[f32; 2]>, gap: f32) -> Pass {
        let drag = self.drag.take();
        let Some(size) = size else {
            return Pass {
                sizes: self.sizes,
                spare: [0.; 2],
                drag,
            };
        };
        let used = Band::ALL.map(|band| self.in_use(band));
        let (sizes, spare) = fit(self.sizes, used, size.map(|extent| extent - 2. * gap), gap);
        Pass { sizes, spare, drag }
    }

    fn has_tabs(&self, place: Place) -> bool {
        !self.docks[place.index()].tabs.is_empty()
    }

    fn in_use(&self, band: Band) -> bool {
        band.docks().iter().any(|&place| self.has_tabs(place))
    }

    /// The band, with its divider on the side facing the main view.
    fn band(&mut self, ui: &mut Ui, pass: &Pass, band: Band) {
        if !self.in_use(band) {
            return;
        }
        let axis = band.axis();
        let divider = Divider::size(band, pass.sizes[band.index()], pass.spare[axis]);
        let mut size = [Size::Grow; 2];
        size[axis] = Size::Fixed(divider.value);
        let element = Element {
            direction: if axis == 0 {
                Direction::TopToBottom
            } else {
                Direction::LeftToRight
            },
            size,
            ..Element::DEFAULT
        };
        let after = band.grows() > 0.;
        if !after {
            self.divider(ui, pass, divider);
        }
        ui.element(element, |ui| self.docks_in(ui, pass, band));
        if after {
            self.divider(ui, pass, divider);
        }
    }

    /// The docks of `band` that have tabs, split by a divider when both do.
    fn docks_in(&mut self, ui: &mut Ui, pass: &Pass, band: Band) {
        match *band.docks() {
            [first, second] if self.has_tabs(first) && self.has_tabs(second) => {
                let across = 1 - band.axis();
                let gap = ui.theme().size.gap;
                let extent = ui.rect().map(|rect| [rect.w, rect.h][across] - gap);
                let divider = Divider::ratio(band, self.ratios[band.index()], extent.unwrap_or(0.));
                // Shared evenly until the band has a size to split.
                let mut size = [Size::Grow; 2];
                if let Some(extent) = extent {
                    size[across] = Size::Fixed((divider.value * extent).round());
                }
                self.dock(ui, first, size);
                self.divider(ui, pass, divider);
                self.dock(ui, second, [Size::Grow; 2]);
            }
            _ => {
                for &place in band.docks() {
                    if self.has_tabs(place) {
                        self.dock(ui, place, [Size::Grow; 2]);
                    }
                }
            }
        }
    }

    /// As thick as the gap it stands in, and drawn in the line colour while
    /// it's under the pointer or dragged.
    fn divider(&mut self, ui: &mut Ui, pass: &Pass, divider: Divider) {
        let theme = ui.theme();
        let mut size = [Size::Grow; 2];
        size[divider.axis] = Size::Fixed(theme.size.gap);
        let element = Element {
            size,
            cursor: Some(if divider.axis == 0 {
                Cursor::ResizeH
            } else {
                Cursor::ResizeV
            }),
            ..Element::DEFAULT
        };
        ui.element(element, |ui| {
            let dragged = ui.dragged(Button::Left);
            let Some(moved) = dragged.map(|moved| moved[divider.axis]) else {
                return;
            };
            let started = pass.drag.filter(|drag| drag.split == divider.split);
            // A press that hasn't moved leaves the split as the user set it,
            // not as `fit` laid it out.
            if started.is_none() && moved == 0. {
                return;
            }
            if let (None, Split::Size(band)) = (started, divider.split) {
                // What's on screen becomes what's asked for along this axis,
                // or `fit` would share the shrinking out anew as this band
                // moves, and push the others the other way.
                for other in Band::ALL
                    .into_iter()
                    .filter(|other| other.axis() == band.axis())
                {
                    self.sizes[other.index()] = pass.sizes[other.index()];
                }
            }
            let from = started.map_or(divider.value, |drag| drag.wanted);
            let wanted = moved.mul_add(divider.rate, from);
            self.drag = Some(Drag {
                split: divider.split,
                wanted,
            });
            let value = divider.clamp(wanted);
            match divider.split {
                Split::Size(band) => self.sizes[band.index()] = value,
                Split::Ratio(band) => self.ratios[band.index()] = value,
            }
        });
    }

    fn dock(&mut self, ui: &mut Ui, place: Place, size: [Size; 2]) {
        let theme = ui.theme();
        // Its border is the padding, so the tab bar meets it.
        let element = Element {
            size,
            background: Some(theme.color.surface),
            border: Some(Border {
                width: theme.size.line,
                color: theme.color.line,
            }),
            radius: [dock_radius(theme.size); 4],
            clip: true,
            ..Element::column().padded(theme.size.line)
        };
        let (dock, icons) = (&mut self.docks[place.index()], &self.icons);
        let held = ui.element(element, |ui| {
            dock.rect = ui.rect();
            dock.ui(ui, icons)
        });
        if let Some((index, rect)) = held {
            self.grip = Some(Grip { place, index, rect });
        }
    }

    /// Follows the tab holding the left button, and moves it where it's let
    /// go. While it's dragged, marks where it would land and shows a copy of
    /// it under the pointer, over a cover that keeps the pointer off
    /// everything else. `window` is the workspace's rect last pass.
    fn drag_tab(&mut self, ui: &mut Ui, window: Option<Rect>) {
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

impl Divider {
    /// On the inner edge of `band`, laid out `value` big, which can grow by
    /// `spare`.
    fn size(band: Band, value: f32, spare: f32) -> Self {
        let axis = band.axis();
        let min = MIN_DOCK[axis];
        Self {
            split: Split::Size(band),
            axis,
            value,
            rate: band.grows(),
            min,
            max: (value + spare).floor().max(min),
        }
    }

    /// Between the docks of `band`, which are `extent` long together across
    /// it, so the first's share is `value`.
    fn ratio(band: Band, value: f32, extent: f32) -> Self {
        let axis = 1 - band.axis();
        // A collapsed band would make the rate infinite.
        let extent = extent.max(1.);
        let min = (MIN_DOCK[axis] / extent).min(0.5);
        let max = 1. - min;
        Self {
            split: Split::Ratio(band),
            axis,
            value: value.clamp(min, max),
            rate: extent.recip(),
            min,
            max,
        }
    }

    /// `wanted` within its limits. Sizes land on whole pixels so dock edges
    /// stay sharp.
    fn clamp(&self, wanted: f32) -> f32 {
        let value = wanted.clamp(self.min, self.max);
        match self.split {
            Split::Size(_) => value.round(),
            Split::Ratio(_) => value,
        }
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
    let source = &mut docks[grip.place.index()];
    if grip.index >= source.tabs.len() {
        return;
    }
    let tab = source.tabs.remove(grip.index);
    if source.active > grip.index {
        source.active -= 1;
    }
    source.active = source.active.min(source.tabs.len().saturating_sub(1));
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

/// `rect`'s width along axis 0, or height along axis 1.
const fn extent(rect: Rect, axis: usize) -> f32 {
    if axis == 0 { rect.w } else { rect.h }
}

/// `rect` cut in two along `axis`, the first `at` long, with `gap` between.
fn cut(rect: Rect, axis: usize, at: f32, gap: f32) -> [Rect; 2] {
    let second = extent(rect, axis) - at - gap;
    if axis == 0 {
        [
            Rect { w: at, ..rect },
            Rect {
                x: rect.x + at + gap,
                w: second,
                ..rect
            },
        ]
    } else {
        [
            Rect { h: at, ..rect },
            Rect {
                y: rect.y + at + gap,
                h: second,
                ..rect
            },
        ]
    }
}

/// Lays out `wanted` sizes for the `used` bands in a workspace `inner` big,
/// with a `divider` thick divider beside each band. Returns the sizes and
/// how far the bands along each axis can still grow.
///
/// Where the bands along an axis would leave the main view under
/// [`MIN_MAIN`], each gives up space in proportion to how far it's above
/// [`MIN_DOCK`], so they all reach it together. A main view already under its
/// limit stops growth but doesn't pull the dividers back, which would make
/// them jump on click.
fn fit(
    wanted: [f32; Band::COUNT],
    used: [bool; Band::COUNT],
    inner: [f32; 2],
    divider: f32,
) -> ([f32; Band::COUNT], [f32; 2]) {
    let mut sizes = wanted;
    let mut spare = [0.; 2];
    for (axis, spare) in spare.iter_mut().enumerate() {
        let bands = || {
            Band::ALL
                .into_iter()
                .filter(move |band| band.axis() == axis && used[band.index()])
        };
        let smallest = MIN_DOCK[axis];
        let mut budget = inner[axis] - MIN_MAIN[axis];
        let (mut total, mut floor) = (0., 0.);
        for band in bands() {
            budget -= divider;
            total += sizes[band.index()];
            floor += smallest;
        }
        if total > budget {
            let excess = total - floor;
            let keep = if excess > 0. {
                ((budget - floor) / excess).clamp(0., 1.)
            } else {
                0.
            };
            total = 0.;
            for band in bands() {
                let size = &mut sizes[band.index()];
                *size = (*size - smallest).mul_add(keep, smallest);
                total += *size;
            }
        }
        *spare = (budget - total).max(0.);
    }
    (sizes, spare)
}

/// Rounder than what sits inside it.
const fn dock_radius(size: Sizes) -> f32 {
    size.radius * 2.
}

/// A tab holds a row, with a little room above and below it.
const fn tab_height(size: Sizes) -> f32 {
    size.row + 2.
}

/// Between two tabs, and after the last one.
const TAB_GAP: f32 = 2.;
/// Inside each end of the tab bar.
const TAB_PADDING: f32 = 6.;

impl Dock {
    /// Its tab bar, then the active tab's panel under it.
    ///
    /// The bar is darker than the dock, with a line along its bottom that the
    /// tabs sit on. The active tab is the dock's colour and covers the line
    /// under it, so it reads as part of the panel below. The menu button sits
    /// at the right end. Tabs that would be cut off there are left out, the
    /// active one never, and arrows beside the menu step through them all.
    ///
    /// Returns the tab holding the left button, by index, with its rect.
    fn ui(&mut self, ui: &mut Ui, icons: &Icons) -> Option<(usize, Rect)> {
        if self.tabs.is_empty() {
            return None;
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
        let mut held = None;
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
                        held = Some((i, rect));
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
                    // TODO: the menu that moves tabs between docks.
                    icon_button(ui, icons.get(Icon::Menu));
                });
            });
            on_line(ui, Size::Fixed(TAB_PADDING), |_| {});
        });
        self.active = active;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            ui.element(Element::column().padded(size.gap), |ui| tab.panel.ui(ui));
        }
        held
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
    rect: Option<Rect>,
    pressed: bool,
    /// Whether it holds the left button, so it can be dragged.
    held: bool,
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
    use crate::panels::Placeholder;

    const GAP: f32 = 8.;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn used(bands: &[Band]) -> [bool; Band::COUNT] {
        Band::ALL.map(|band| bands.contains(&band))
    }

    fn defaults() -> [f32; Band::COUNT] {
        Band::ALL.map(Band::default_size)
    }

    #[test]
    fn fit_keeps_sizes_with_room() {
        let used = used(&[Band::LeftInner, Band::RightInner, Band::BelowMain]);
        let (sizes, spare) = fit(defaults(), used, [1440., 900.], GAP);
        assert_eq!(sizes.map(f32::to_bits), defaults().map(f32::to_bits));
        // 1440 less the main view, two dividers, 240 and 280.
        assert!(close(spare[0], 264.));
        // 900 less the main view, one divider and 240.
        assert!(close(spare[1], 292.));
    }

    #[test]
    fn fit_shrinks_in_proportion_above_the_minimum() {
        let used = used(&[Band::LeftInner, Band::RightInner]);
        let (sizes, spare) = fit(defaults(), used, [1000., 900.], GAP);
        let left = sizes[Band::LeftInner.index()];
        let right = sizes[Band::RightInner.index()];
        assert!(close(left + right, 1000. - 640. - 2. * GAP));
        assert!(close(
            (left - MIN_DOCK[0]) / (right - MIN_DOCK[0]),
            (240. - MIN_DOCK[0]) / (280. - MIN_DOCK[0]),
        ));
        assert!(close(spare[0], 0.));
    }

    #[test]
    fn fit_stops_at_the_minimum() {
        let used = used(&[Band::LeftInner, Band::RightInner]);
        let (sizes, _) = fit(defaults(), used, [500., 900.], GAP);
        assert!(close(sizes[Band::LeftInner.index()], MIN_DOCK[0]));
        assert!(close(sizes[Band::RightInner.index()], MIN_DOCK[0]));
    }

    #[test]
    fn fit_ignores_unused_bands() {
        let used = used(&[Band::LeftInner]);
        let (sizes, spare) = fit(defaults(), used, [1000., 900.], GAP);
        assert_eq!(sizes.map(f32::to_bits), defaults().map(f32::to_bits));
        assert!(close(spare[0], 1000. - 640. - GAP - 240.));
        assert!(close(spare[1], 900. - 360.));
    }

    #[test]
    fn fit_keeps_what_it_laid_out() {
        let used = used(&[Band::LeftInner, Band::RightInner]);
        let (mut laid, _) = fit(defaults(), used, [1000., 900.], GAP);
        let left = laid[Band::LeftInner.index()];
        laid[Band::RightInner.index()] -= 20.;
        let (sizes, spare) = fit(laid, used, [1000., 900.], GAP);
        assert!(close(sizes[Band::LeftInner.index()], left));
        assert!(close(spare[0], 20.));
    }

    #[test]
    fn size_divider_clamps_to_whole_pixels() {
        let divider = Divider::size(Band::RightInner, 280., 50.5);
        assert!(close(divider.clamp(0.), MIN_DOCK[0]));
        assert!(close(divider.clamp(1000.), 330.));
        assert!(close(divider.clamp(300.4), 300.));
    }

    #[test]
    fn ratio_divider_leaves_each_dock_its_minimum() {
        let divider = Divider::ratio(Band::LeftInner, 0.5, 600.);
        assert!(close(divider.clamp(0.), MIN_DOCK[1] / 600.));
        assert!(close(divider.clamp(1.), 1. - MIN_DOCK[1] / 600.));
        // Too short for both, so it stays split evenly.
        let divider = Divider::ratio(Band::LeftInner, 0.9, 200.);
        assert!(close(divider.value, 0.5));
    }

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
