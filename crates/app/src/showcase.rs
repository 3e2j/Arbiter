//! A very temporary stand-in panel for testing.
//! Shows what `gui` draws: icons, hover and select, font fallback, colour emoji and triangles.
//!
//! To be deleted when a proper workspace / panels gets going

use gui::{
    canvas::{Color, Rect, Vertex},
    input::{Button, Cursor},
    layout::{Align, Direction, Element, Size, TextStyle},
    ui::Ui,
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
    pub fn new(fonts: Fonts, icons: Icons) -> Self {
        Self {
            fonts,
            icons,
            selected: None,
        }
    }

    /// Declares the panel's contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        let Self { fonts, icons, .. } = *self;
        let input = ui.input();
        let strip = Element {
            direction: Direction::LeftToRight,
            gap: GAP / 2.,
            ..Element::DEFAULT
        };
        ui.element("icons", strip, |ui| {
            for icon in Icon::ALL {
                let hovered = ui
                    .peek(icon)
                    .zip(input.pointer())
                    .is_some_and(|(rect, at)| rect.contains(at));
                if hovered {
                    ui.cursor(Cursor::Pointer);
                    if input.pressed(Button::Left) {
                        self.selected = Some(icon);
                    }
                }
                let fill = if self.selected == Some(icon) {
                    Some(SELECTED)
                } else {
                    hovered.then_some(HOVER)
                };
                let cell = Element {
                    background: fill.map(Color::hex),
                    radius: RADIUS / 2.,
                    ..Element::DEFAULT.padded(GAP / 4.)
                };
                ui.element(icon, cell, |ui| {
                    ui.icon("icon", icons.get(icon), ICON_SIZE, Color::hex(DIM));
                });
            }
        });
        let lines = [
            (fonts.ui, "The quick brown fox jumps over the lazy dog"),
            (fonts.ui, "いろはにほへと ちりぬるを わかよたれそ"),
            (fonts.buffer, "0O 1lI {}[]() => != 0x1f4"),
            (fonts.ui, "From a system font: 한국어"),
            (fonts.ui, "Colour emoji (color atlas): 🦀"),
        ];
        let row = Element {
            size: [Size::Grow, Size::Fixed(ROW)],
            align: [Align::Start, Align::Center],
            ..Element::row()
        };
        for ((font, line), salt) in lines.into_iter().zip(0u8..) {
            ui.element(("line", salt), row, |ui| {
                let style = TextStyle {
                    font,
                    size: TEXT_SIZE,
                    color: Color::hex(TEXT),
                };
                ui.text("text", style, line);
            });
        }
        let size = ICON_SIZE * 3.;
        let triangle = Element {
            size: [Size::Fixed(size); 2],
            ..Element::DEFAULT
        };
        let mut painter = ui.custom("triangle", triangle);
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
