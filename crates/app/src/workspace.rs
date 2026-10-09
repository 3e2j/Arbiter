//! The window's contents: docks of tabs around the main view.
//!
//! Every dock has a fixed [`Place`]. The docks besides [`Place::Main`] sit in
//! [`Band`]s, strips cut off the window's edges in a fixed order, and a band
//! with no tabs takes no space.
//!
//! A [`Divider`] sits on each band's inner edge and between its two docks,
//! taking the place of the gap there. Dragging one sets a [`Split`], and
//! bands shrink to keep [`MIN_MAIN`] when the window does.

use gui::{
    input::{Button, Cursor},
    ui::{Align, Border, Direction, Element, Size, Ui},
};

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
    pub const COUNT: usize = 12;

    const fn index(self) -> usize {
        self as usize
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

struct Tab {
    panel: Panel,
}

#[derive(Default)]
struct Dock {
    tabs: Vec<Tab>,
    /// The tab whose panel is drawn.
    shown: usize,
}

pub struct Workspace {
    docks: [Dock; Place::COUNT],
    /// Indexed by [`Band`], as the user last set them. [`fit`] may lay them
    /// out smaller.
    sizes: [f32; Band::COUNT],
    /// Indexed by [`Band`].
    ratios: [f32; Band::COUNT],
    drag: Option<Drag>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            docks: Default::default(),
            sizes: Band::ALL.map(Band::default_size),
            ratios: [0.5; Band::COUNT],
            drag: None,
        }
    }
}

impl Workspace {
    /// Adds `panel` as the last tab in `place`.
    pub fn add(&mut self, panel: Panel, place: Place) {
        self.docks[place.index()].tabs.push(Tab { panel });
    }

    /// Declares the bands in the order they're cut, outside in, each wrapping
    /// what's left.
    pub fn ui(&mut self, ui: &mut Ui) {
        let gap = ui.theme().size.gap;
        ui.element(Element::column().padded(gap), |ui| {
            let pass = self.pass(ui.rect().map(|rect| [rect.w, rect.h]), gap);
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
        let element = Element {
            size,
            gap: theme.size.gap,
            background: Some(theme.color.surface),
            border: Some(Border {
                width: theme.size.line,
                color: theme.color.line,
            }),
            // Rounder than what sits inside it.
            radius: theme.size.radius * 2.,
            clip: true,
            ..Element::column().padded(theme.size.gap)
        };
        let dock = &mut self.docks[place.index()];
        ui.element(element, |ui| dock.ui(ui));
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

impl Dock {
    /// Its tab bar, then the shown tab's panel under it.
    fn ui(&mut self, ui: &mut Ui) {
        if self.tabs.is_empty() {
            return;
        }
        let theme = ui.theme();
        let size = theme.size;
        let bar = Element {
            direction: Direction::LeftToRight,
            size: [Size::Grow, Size::Fit],
            gap: size.gap / 2.,
            ..Element::DEFAULT
        };
        let tab = Element {
            size: [Size::Fit, Size::Fixed(size.row)],
            padding: [size.gap, 0., size.gap, 0.],
            align: [Align::Start, Align::Center],
            radius: size.radius,
            cursor: Some(Cursor::Pointer),
            ..Element::DEFAULT
        };
        let mut shown = self.shown;
        ui.element(bar, |ui| {
            for (i, tab_of) in (0..).zip(&self.tabs) {
                ui.element(tab, |ui| {
                    if ui.pressed(Button::Left) {
                        shown = i;
                    }
                    let current = shown == i;
                    ui.style().background = if current {
                        Some(theme.color.selected)
                    } else {
                        ui.hovered().then_some(theme.color.hover)
                    };
                    let color = if current {
                        theme.color.text
                    } else {
                        theme.color.dim
                    };
                    ui.text(theme.ui_text(color), tab_of.panel.title());
                });
            }
        });
        self.shown = shown;
        if let Some(tab) = self.tabs.get_mut(self.shown) {
            ui.element(Element::column(), |ui| tab.panel.ui(ui));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
