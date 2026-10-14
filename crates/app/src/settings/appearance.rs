//! How Arbiter looks: the theme.

use gui::{
    components::spacer,
    input::{CursorIcon, PointerButton},
    ui::{Align, Border, Direction, Element, Length, Ui},
};

use super::widgets::{label, row, sized};
use crate::assets::Icon;
use crate::theme::Palette;

/// Fits a theme's name under its picture.
const CARD: f32 = 110.;

#[derive(Default)]
pub struct Appearance {
    palette: Palette,
    /// Under the pointer, worn until it leaves.
    preview: Option<Palette>,
    /// Handed to gui last, by [`Self::take_changed`].
    worn: Palette,
}

impl Appearance {
    pub const fn palette(&self) -> Palette {
        self.palette
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

    pub fn ui(&mut self, ui: &mut Ui) {
        let changed = self.palette != Palette::default();
        let mut hovered = None;
        row(ui, |ui| {
            if label(ui, "Theme", "Hover to try one on.", changed) {
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
