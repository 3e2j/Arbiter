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
//!
//! Each [`Side`] of the main view hides or shows its bands at once. A hidden
//! band keeps its tabs and size, and gives its space to the main view.
//!
//! A [`Session`] saves every tab's panel, where it sits, and the bands'
//! sizes, for the next launch to lay out again.

mod dock;
mod dock_menu;
mod drag;
mod layout;
mod place;
mod session;
pub mod status_bar;
pub mod title_bar;

use gui::{
    input::{Input, Key, PointerButton},
    ui::{Direction, Element, Length, Ui},
};

use crate::panels::{Context, Event, Panel, Reply};

use crate::theme;
use dock::{Dock, dock_radius};
use dock_menu::Menu;
use drag::{Grip, TabDrag};
use layout::{Divider, Drag, Pass, fit};
use place::Band;
pub use place::{Place, Places, Side};
pub use session::Session;

/// The space between docks, and either side of the workspace, in logical
/// pixels.
const GUTTER: f32 = 6.;
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
    /// Indexed by [`Band`]. Only a band with tabs is hidden, so one can't
    /// open hidden.
    hidden: [bool; Band::COUNT],
    drag: Option<Drag>,
    /// The tab holding the left button this pass.
    grip: Option<Grip>,
    tab_drag: Option<TabDrag>,
    /// The menu that's open, which takes every press until it closes.
    menu: Option<Menu>,
    /// The dock the last press landed in, whose shown panel has the keys.
    focus: Option<Place>,
}

impl Workspace {
    /// Offers `event` to every panel in turn, closing each that asks, until
    /// one shows it. Returns whether one did, brought to the front.
    pub fn offer(&mut self, event: &mut Event) -> bool {
        for dock in &mut self.docks {
            let mut at = 0;
            while let Some(panel) = dock.tabs.get_mut(at) {
                match panel.receive(event) {
                    Reply::Pass => at += 1,
                    Reply::Close => {
                        dock.remove(at);
                    }
                    Reply::Show => {
                        dock.active = at;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Adds `panel` as the last tab at `place`, and shows it and its band.
    pub fn add(&mut self, place: Place, panel: Panel) {
        let dock = &mut self.docks[place.index()];
        dock.active = dock.tabs.len();
        dock.tabs.push(panel);
        if let Some(band) = Band::of(place) {
            self.hidden[band.index()] = false;
        }
    }

    /// The panel shown in the dock with focus.
    pub fn focused(&mut self) -> Option<&mut Panel> {
        let dock = &mut self.docks[self.focus?.index()];
        dock.tabs.get_mut(dock.active)
    }

    /// Escape closes the menu.
    pub fn input(&mut self, input: &Input) {
        if input.keys().iter().any(|press| press.key == Key::Escape) {
            self.menu = None;
        }
    }

    /// Declares the bands in the order they're cut, outside in, each wrapping
    /// what's left, each shown panel given `cx`.
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        self.drop_tab(ui);
        for dock in &mut self.docks {
            dock.rect = None;
            dock.bar = None;
            dock.shown.clear();
        }
        self.grip = None;
        if ui.input().pressed(PointerButton::Left) {
            self.focus = None;
        }
        // The bars above and below sit flush against it.
        let element = Element {
            padding: [GUTTER, 0., GUTTER, 0.],
            ..Element::column()
        };
        ui.element(element, |ui| {
            let window = ui.rect();
            let pass = self.pass(window.map(|rect| [rect.w, rect.h]));
            ui.element(Element::row(), |ui| {
                self.band(ui, cx, &pass, Band::LeftOuter);
                self.band(ui, cx, &pass, Band::LeftInner);
                ui.element(Element::column(), |ui| {
                    self.dock(ui, cx, Place::Main, [Length::Grow; 2]);
                    self.band(ui, cx, &pass, Band::BelowMain);
                });
                self.band(ui, cx, &pass, Band::RightInner);
                self.band(ui, cx, &pass, Band::RightOuter);
            });
            self.band(ui, cx, &pass, Band::BottomRow);
            self.drag_tab(ui, window);
            self.menu(ui, window);
        });
    }

    /// Fits the bands into a workspace last laid out `size` big, padded by
    /// [`GUTTER`] either side. The drag carries over only if its divider asks
    /// for it again.
    fn pass(&mut self, size: Option<[f32; 2]>) -> Pass {
        let drag = self.drag.take();
        let Some(size) = size else {
            return Pass {
                sizes: self.sizes,
                spare: [0.; 2],
                drag,
            };
        };
        let used = Band::ALL.map(|band| self.in_use(band));
        let inner = [size[0] - 2. * GUTTER, size[1]];
        let (sizes, spare) = fit(self.sizes, used, inner, GUTTER);
        Pass { sizes, spare, drag }
    }

    fn has_tabs(&self, place: Place) -> bool {
        !self.docks[place.index()].tabs.is_empty()
    }

    fn band_has_tabs(&self, band: Band) -> bool {
        band.docks().iter().any(|&place| self.has_tabs(place))
    }

    fn in_use(&self, band: Band) -> bool {
        !self.hidden[band.index()] && self.band_has_tabs(band)
    }

    /// Whether any band on `side` has tabs, so there's anything to show.
    pub fn side_has_tabs(&self, side: Side) -> bool {
        side.bands().iter().any(|&band| self.band_has_tabs(band))
    }

    /// Whether any band on `side` is laid out.
    pub fn side_shown(&self, side: Side) -> bool {
        side.bands().iter().any(|&band| self.in_use(band))
    }

    /// Hides `side`'s bands while any is shown, or else shows them all.
    pub fn toggle_side(&mut self, side: Side) {
        let hide = self.side_shown(side);
        for &band in side.bands() {
            self.hidden[band.index()] = hide && self.band_has_tabs(band);
        }
    }

    /// The band, with its divider on the side facing the main view.
    fn band(&mut self, ui: &mut Ui, cx: &mut Context, pass: &Pass, band: Band) {
        if !self.in_use(band) {
            return;
        }
        let axis = band.axis();
        let divider = Divider::size(band, pass.sizes[band.index()], pass.spare[axis]);
        let mut size = [Length::Grow; 2];
        size[axis] = Length::Fixed(divider.value);
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
        ui.element(element, |ui| self.docks_in(ui, cx, pass, band));
        if after {
            self.divider(ui, pass, divider);
        }
    }

    /// The docks of `band` that have tabs, split by a divider when both do.
    fn docks_in(&mut self, ui: &mut Ui, cx: &mut Context, pass: &Pass, band: Band) {
        match *band.docks() {
            [first, second] if self.has_tabs(first) && self.has_tabs(second) => {
                let across = 1 - band.axis();
                let extent = ui.rect().map(|rect| [rect.w, rect.h][across] - GUTTER);
                let divider = Divider::ratio(band, self.ratios[band.index()], extent.unwrap_or(0.));
                // Shared evenly until the band has a size to split.
                let mut size = [Length::Grow; 2];
                if let Some(extent) = extent {
                    size[across] = Length::Fixed((divider.value * extent).round());
                }
                self.dock(ui, cx, first, size);
                self.divider(ui, pass, divider);
                self.dock(ui, cx, second, [Length::Grow; 2]);
            }
            _ => {
                for &place in band.docks() {
                    if self.has_tabs(place) {
                        self.dock(ui, cx, place, [Length::Grow; 2]);
                    }
                }
            }
        }
    }

    fn dock(&mut self, ui: &mut Ui, cx: &mut Context, place: Place, size: [Length; 2]) {
        let theme = theme::current();
        // Its border is the padding, so the tab bar meets it.
        let element = Element {
            size,
            background: Some(theme.color.surface),
            border: Some(theme.border()),
            radius: [dock_radius(theme.size); 4],
            clip: true,
            ..Element::column().padded(theme.size.line)
        };
        let dock = &mut self.docks[place.index()];
        let out = ui.element(element, |ui| {
            if ui.input().pressed(PointerButton::Left) && ui.hovered() {
                self.focus = Some(place);
            }
            dock.rect = ui.rect();
            dock.ui(ui, cx)
        });
        if let Some(index) = out.close {
            dock.remove(index);
            return;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::Kind;

    #[test]
    fn a_side_hides_only_bands_with_tabs_and_shows_them_again() {
        let session = Session::of(&[(Kind::FileTree, Place::LeftInnerTop)]);
        let mut workspace = Workspace::new(&session);
        assert!(workspace.side_shown(Side::Left));
        assert!(!workspace.side_has_tabs(Side::Right));

        workspace.toggle_side(Side::Left);
        assert!(!workspace.side_shown(Side::Left));
        assert!(workspace.side_has_tabs(Side::Left));
        assert!(!workspace.hidden[Band::LeftOuter.index()]);

        workspace.toggle_side(Side::Left);
        assert!(workspace.in_use(Band::LeftInner));
    }

    #[test]
    fn adding_to_a_hidden_band_shows_it() {
        let session = Session::of(&[(Kind::FileTree, Place::LeftInnerTop)]);
        let mut workspace = Workspace::new(&session);
        workspace.toggle_side(Side::Left);
        assert!(!workspace.in_use(Band::LeftInner));
        workspace.add(Place::LeftInnerBottom, Panel::new(Kind::FileTree).unwrap());
        assert!(workspace.in_use(Band::LeftInner));
        let tabs: usize = workspace.docks.iter().map(|dock| dock.tabs.len()).sum();
        assert_eq!(tabs, 2);
    }
}
