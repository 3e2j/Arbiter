//! A very temporary stand-in panel for testing.
//! Shows what `gui` draws: icons, hover and select, font fallback, colour emoji and triangles.
//!
//! To be deleted when a proper workspace / panels gets going

use gui::{
    canvas::{Color, Rect, Vertex},
    input::{Button, Cursor},
    ui::{Align, Direction, Element, Size, TextStyle, Ui},
};

use crate::assets::{Fonts, Icon, Icons};
use crate::{DIM, GAP, HOVER, ICON_SIZE, RADIUS, ROW, SELECTED, TEXT, TEXT_SIZE};

pub struct Showcase {
    fonts: Fonts,
    icons: Icons,
    /// The icon last clicked.
    selected: Option<Icon>,
}

impl Showcase {
    pub const TITLE: &str = "Showcase";

    pub fn new(fonts: Fonts, icons: Icons) -> Self {
        Self {
            fonts,
            icons,
            selected: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let strip = Element {
            direction: Direction::LeftToRight,
            gap: GAP / 2.,
            ..Element::DEFAULT
        };
        let cell = Element {
            radius: RADIUS / 2.,
            cursor: Some(Cursor::Pointer),
            ..Element::DEFAULT.padded(GAP / 4.)
        };
        ui.element(strip, |ui| {
            for icon in Icon::ALL {
                ui.element(cell, |ui| {
                    if ui.pressed(Button::Left) {
                        self.selected = Some(icon);
                    }
                    ui.style().background = if self.selected == Some(icon) {
                        Some(SELECTED)
                    } else {
                        ui.hovered().then_some(HOVER)
                    };
                    ui.icon(self.icons.get(icon), ICON_SIZE, DIM);
                });
            }
        });
        let lines = [
            (self.fonts.ui, "The quick brown fox jumps over the lazy dog"),
            (self.fonts.ui, "いろはにほへと ちりぬるを わかよたれそ"),
            (self.fonts.buffer, "0O 1lI {}[]() => != 0x1f4"),
            (self.fonts.ui, "From a system font: 한국어"),
            (self.fonts.ui, "Colour emoji (color atlas): 🦀"),
        ];
        let row = Element {
            size: [Size::Grow, Size::Fixed(ROW)],
            align: [Align::Start, Align::Center],
            ..Element::row()
        };
        for (font, line) in lines {
            ui.element(row, |ui| {
                let style = TextStyle {
                    font,
                    size: TEXT_SIZE,
                    color: TEXT,
                };
                ui.text(style, line);
            });
        }
        let size = ICON_SIZE * 3.;
        let triangle = Element {
            size: [Size::Fixed(size); 2],
            ..Element::DEFAULT
        };
        let mut painter = ui.custom(triangle);
        if let Some(Rect { x, y, .. }) = painter.rect {
            painter.triangles(
                &[
                    Vertex::new([x + size / 2., y], Color::hex(0xfa_4d_56)),
                    Vertex::new([x + size, y + size], Color::hex(0x42_be_65)),
                    Vertex::new([x, y + size], Color::hex(0x78_a9_ff)),
                ],
                &[0, 1, 2],
            );
        }
    }
}
