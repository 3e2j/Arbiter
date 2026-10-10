//! The window's contents: docks of tabs around the main view.
//!
//! Every dock has a fixed [`Place`]. The docks besides [`Place::Main`] sit in
//! [`Band`]s, strips cut off the window's edges in a fixed order, and a band
//! with no tabs takes no space.
//!
//! A [`Divider`] sits on each band's inner edge and between its two docks,
//! taking the place of the gap there. Dragging one sets a [`Split`](layout::Split),
//! and bands shrink to keep [`MIN_MAIN`] when the window does.
//!
//! A tab dragged out of its bar moves where it's let go, among the
//! [`Places`] its panel allows, as [`drag`] finds.
//!
//! Each dock's [`Menu`] shows, closes or moves its tabs, opened from the
//! button on its tab bar or by right clicking a tab.

mod dock;
mod drag;
mod layout;
mod menu;
mod place;

use gui::{
    input::{Input, Key},
    ui::{Border, Direction, Element, Size, Ui},
};

use crate::assets::Icons;
use crate::panels::Panel;

use dock::{Dock, Tab, dock_radius};
use drag::{Grip, TabDrag};
use layout::{Divider, Drag, Pass, fit};
use menu::Menu;
use place::Band;
pub use place::{Place, Places};

/// The smallest a dock gets, as `[width, height]` in logical pixels.
const MIN_DOCK: [f32; 2] = [144., 150.];
/// The smallest the main view gets, as `[width, height]` in logical pixels.
/// Bands shrink to keep it when the window does.
const MIN_MAIN: [f32; 2] = [640., 360.];

// Columns run the main view's height, and the bottom row the window's width,
// so a main view this size leaves room for two docks in either.
const _: () = assert!(MIN_MAIN[0] >= 2. * MIN_DOCK[0] && MIN_MAIN[1] >= 2. * MIN_DOCK[1]);

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
    /// The menu that's open, which takes every press until it closes.
    menu: Option<Menu>,
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
            menu: None,
            icons,
        }
    }

    /// Adds `panel` as the last tab in `place`.
    pub fn add(&mut self, panel: Panel, place: Place) {
        self.docks[place.index()].tabs.push(Tab { panel });
    }

    /// Escape closes the menu.
    pub fn input(&mut self, input: &Input) {
        if input.keys().iter().any(|press| press.key == Key::Escape) {
            self.menu = None;
        }
    }

    /// Declares the bands in the order they're cut, outside in, each wrapping
    /// what's left.
    pub fn ui(&mut self, ui: &mut Ui) {
        self.drop_tab(ui);
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
            self.menu(ui, window);
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
        let out = ui.element(element, |ui| {
            dock.rect = ui.rect();
            dock.ui(ui, icons)
        });
        if let Some((index, rect)) = out.held {
            self.grip = Some(Grip { place, index, rect });
        }
        if let Some((tab, from, align)) = out.menu {
            self.menu = Some(Menu {
                place,
                tab,
                from,
                align,
            });
        }
    }
}
