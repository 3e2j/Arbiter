//! How Arbiter looks: the theme, and how big it's drawn.

use crate::components::{rule, segmented, spacer};
use gui::{
    input::{CursorIcon, PointerButton},
    ui::{Align, Border, Direction, Element, Length, Ui},
};

use serde::{Deserialize, Serialize};

use super::file::lenient;
use super::widgets::{beside, label, row, sized};
use crate::assets::Icon;
use crate::theme::Palette;

/// Fits a theme's name under its picture.
const CARD: f32 = 110.;

/// What's kept of the page in the settings file.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    #[serde(deserialize_with = "lenient")]
    pub theme: Palette,
    #[serde(deserialize_with = "lenient")]
    pub zoom: Zoom,
}

/// How much bigger than the system's scale everything is drawn. In fixed
/// steps, so the glyph atlas doesn't fill with sizes that are never reused.
/// Kept as its percent.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(try_from = "u16", into = "u16")]
pub enum Zoom {
    P90,
    #[default]
    P100,
    P110,
    P125,
    P150,
}

#[derive(Default)]
pub struct Appearance {
    palette: Palette,
    /// Under the pointer, worn until it leaves.
    preview: Option<Palette>,
    /// Handed to gui last, by [`Self::take_changed`].
    worn: Palette,
    zoom: Zoom,
    /// Handed to gui last, by [`Self::take_zoom`].
    zoomed: Zoom,
}

impl Zoom {
    const ALL: [Self; 5] = [Self::P90, Self::P100, Self::P110, Self::P125, Self::P150];

    const fn percent(self) -> u16 {
        match self {
            Self::P90 => 90,
            Self::P100 => 100,
            Self::P110 => 110,
            Self::P125 => 125,
            Self::P150 => 150,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::P90 => "90%",
            Self::P100 => "100%",
            Self::P110 => "110%",
            Self::P125 => "125%",
            Self::P150 => "150%",
        }
    }

    /// The next level up, or down, staying at the ends.
    fn step(self, up: bool) -> Self {
        let levels = Self::ALL.into_iter();
        let next = if up {
            levels.skip_while(|&zoom| zoom != self).nth(1)
        } else {
            levels.rev().skip_while(|&zoom| zoom != self).nth(1)
        };
        next.unwrap_or(self)
    }

    /// As gui takes it, 1 for as is.
    pub fn factor(self) -> f32 {
        f32::from(self.percent()) / 100.
    }
}

impl From<Zoom> for u16 {
    fn from(zoom: Zoom) -> Self {
        zoom.percent()
    }
}

impl TryFrom<u16> for Zoom {
    type Error = String;

    fn try_from(percent: u16) -> Result<Self, Self::Error> {
        Self::ALL
            .into_iter()
            .find(|zoom| zoom.percent() == percent)
            .ok_or_else(|| format!("{percent}% isn't a zoom level"))
    }
}

impl Appearance {
    /// As `saved` left it, worn from the start.
    pub fn new(saved: Saved) -> Self {
        Self {
            palette: saved.theme,
            worn: saved.theme,
            zoom: saved.zoom,
            zoomed: saved.zoom,
            ..Self::default()
        }
    }

    pub const fn saved(&self) -> Saved {
        Saved {
            theme: self.palette,
            zoom: self.zoom,
        }
    }

    pub const fn palette(&self) -> Palette {
        self.palette
    }

    pub const fn zoom(&self) -> Zoom {
        self.zoom
    }

    pub const fn previewing(&self) -> bool {
        self.preview.is_some()
    }

    /// Drops the preview, for when the page stops being drawn.
    pub const fn leave(&mut self) {
        self.preview = None;
    }

    /// The palette to wear, if it changed since the last call.
    pub fn take_changed(&mut self) -> Option<Palette> {
        let wear = self.preview.unwrap_or(self.palette);
        (wear != self.worn).then(|| {
            self.worn = wear;
            wear
        })
    }

    pub fn step_zoom(&mut self, up: bool) {
        self.zoom = self.zoom.step(up);
    }

    pub fn reset_zoom(&mut self) {
        self.zoom = Zoom::default();
    }

    /// The zoom to draw at, if it changed since the last call.
    pub fn take_zoom(&mut self) -> Option<Zoom> {
        (self.zoom != self.zoomed).then(|| {
            self.zoomed = self.zoom;
            self.zoom
        })
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.theme(ui);
        rule(ui);
        self.zoom_row(ui);
    }

    fn theme(&mut self, ui: &mut Ui) {
        let changed = self.palette != Palette::default();
        let mut hovered = None;
        row(ui, |ui| {
            if label(ui, "Theme", Some("Hover to try one on."), changed) {
                self.palette = Palette::default();
            }
            let cards = Element {
                direction: Direction::LeftToRight,
                padding: [0., 8., 0., 0.],
                gap: 8.,
                ..Element::DEFAULT
            };
            ui.element(cards, |ui| {
                for palette in Palette::ALL {
                    let (over, clicked) = card(ui, palette, palette == self.palette);
                    if over {
                        hovered = Some(palette);
                    }
                    if clicked {
                        self.palette = palette;
                    }
                }
            });
        });
        self.preview = hovered;
    }

    fn zoom_row(&mut self, ui: &mut Ui) {
        let changed = self.zoom != Zoom::default();
        let labels = Zoom::ALL.map(Zoom::label);
        let at = (0..)
            .zip(Zoom::ALL)
            .find_map(|(n, zoom)| (zoom == self.zoom).then_some(n))
            .unwrap_or_default();
        row(ui, |ui| {
            let (reset, picked) =
                beside(ui, "Zoom", None, changed, |ui| segmented(ui, &labels, at));
            if reset {
                self.zoom = Zoom::default();
            }
            if let Some(zoom) = picked.and_then(|n| Zoom::ALL.get(usize::from(n))) {
                self.zoom = *zoom;
            }
        });
    }
}

/// A theme as a picture of the workspace in its colours, then its name.
/// Returns whether it's under the pointer, and whether it was clicked.
fn card(ui: &mut Ui, palette: Palette, on: bool) -> (bool, bool) {
    let theme = ui.theme();
    let c = theme.color;
    let p = palette.colors();
    let pad = 6.;
    let element = Element {
        size: [Length::Fixed(CARD), Length::Fit],
        padding: [pad; 4],
        gap: 6.,
        background: Some(c.sunken),
        radius: [7.; 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    let picture = Element {
        size: [Length::Grow, Length::Fixed(46.)],
        gap: 1.,
        background: Some(p.line),
        radius: [4.; 4],
        ..Element::DEFAULT
    };
    let bar = Element {
        size: [Length::Grow, Length::Fixed(8.)],
        background: Some(p.page),
        radius: [4., 4., 0., 0.],
        ..Element::DEFAULT
    };
    let below = Element {
        gap: 1.,
        ..Element::row()
    };
    let side = Element {
        size: [Length::Fixed((CARD - 2. * pad) * 0.3), Length::Grow],
        background: Some(p.surface),
        radius: [0., 0., 0., 4.],
        ..Element::DEFAULT
    };
    let view = Element {
        padding: [6.; 4],
        background: Some(p.sunken),
        radius: [0., 0., 4., 0.],
        ..Element::column()
    };
    let focus = Element {
        border: Some(Border {
            width: 1.,
            color: p.accent,
        }),
        ..Element::column()
    };
    let name = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fit],
        padding: [2., 0., 2., 0.],
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    let tick = Element {
        size: [Length::Fixed(12.); 2],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let over = ui.hovered();
        let border = if on {
            Border {
                width: 2. * theme.size.line,
                color: c.accent,
            }
        } else if over {
            Border {
                color: c.dim,
                ..theme.border()
            }
        } else {
            theme.border()
        };
        ui.style().border = Some(border);
        ui.element(picture, |ui| {
            ui.element(bar, |_| ());
            ui.element(below, |ui| {
                ui.element(side, |_| ());
                ui.element(view, |ui| ui.element(focus, |_| ()));
            });
        });
        ui.element(name, |ui| {
            ui.text(sized(theme, -1., c.text), palette.name());
            spacer(ui);
            ui.element(tick, |ui| {
                if on {
                    ui.icon(Icon::Check, 12., c.accent);
                }
            });
        });
        (over, ui.clicked(PointerButton::Left))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_steps_through_its_levels_and_stays_at_the_ends() {
        assert_eq!(Zoom::P100.step(true), Zoom::P110);
        assert_eq!(Zoom::P100.step(false), Zoom::P90);
        assert_eq!(Zoom::P150.step(true), Zoom::P150);
        assert_eq!(Zoom::P90.step(false), Zoom::P90);
    }
}
