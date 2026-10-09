//! The window's contents: docks of tabs around the main view.
//!
//! Every dock has a fixed [`Place`]. The docks besides [`Place::Main`] sit in
//! [`Band`]s, strips cut off the window's edges in a fixed order, and a band
//! with no tabs takes no space.

use gui::{
    input::{Button, Cursor},
    ui::{Align, Border, Direction, Element, Size, Ui},
};

use crate::panels::Panel;

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

    /// Whether it's cut off the bottom, so it spans across and stacks its
    /// docks left to right.
    const fn is_row(self) -> bool {
        matches!(self, Self::BottomRow | Self::BelowMain)
    }

    const fn index(self) -> usize {
        self as usize
    }
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
    /// Indexed by [`Band`].
    sizes: [f32; Band::COUNT],
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            docks: Default::default(),
            sizes: Band::ALL.map(Band::default_size),
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
        let column = Element {
            gap,
            ..Element::column()
        };
        let row = Element {
            gap,
            ..Element::row()
        };
        ui.element(column.padded(gap), |ui| {
            ui.element(row, |ui| {
                self.band(ui, Band::LeftOuter);
                self.band(ui, Band::LeftInner);
                ui.element(column, |ui| {
                    self.dock(ui, Place::Main);
                    self.band(ui, Band::BelowMain);
                });
                self.band(ui, Band::RightInner);
                self.band(ui, Band::RightOuter);
            });
            self.band(ui, Band::BottomRow);
        });
    }

    fn band(&mut self, ui: &mut Ui, band: Band) {
        let theme = ui.theme();
        let docks = band.docks();
        if docks
            .iter()
            .all(|place| self.docks[place.index()].tabs.is_empty())
        {
            return;
        }
        let size = Size::Fixed(self.sizes[band.index()]);
        let element = Element {
            direction: if band.is_row() {
                Direction::LeftToRight
            } else {
                Direction::TopToBottom
            },
            size: if band.is_row() {
                [Size::Grow, size]
            } else {
                [size, Size::Grow]
            },
            gap: theme.size.gap,
            ..Element::DEFAULT
        };
        ui.element(element, |ui| {
            for &place in docks {
                if !self.docks[place.index()].tabs.is_empty() {
                    self.dock(ui, place);
                }
            }
        });
    }

    fn dock(&mut self, ui: &mut Ui, place: Place) {
        let theme = ui.theme();
        let element = Element {
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
