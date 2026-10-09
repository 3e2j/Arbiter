//! A very temporary stand-in panel for testing.
//! Shows what `gui` draws: icons, hover and select, font fallback, colour emoji and triangles.
//!
//! To be deleted when a proper workspace / panels gets going

use gui::{
    canvas::{Color, Glyphs, Rect, Vertex},
    input::{Button, Cursor, Input, Out},
    layout::{Align, Direction, Element, Id, Layout, Size, TextStyle},
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

    /// Declares the panel's contents into the box `layout` has open.
    pub fn ui(
        &mut self,
        id: Id,
        layout: &mut Layout,
        glyphs: &mut Glyphs,
        input: &Input,
        out: &mut Out,
    ) {
        let Self { fonts, icons, .. } = *self;
        layout.open(
            id.child("icons"),
            Element {
                direction: Direction::LeftToRight,
                gap: GAP / 2.,
                ..Element::DEFAULT
            },
        );
        for icon in Icon::ALL {
            let cell = id.child(icon);
            let hovered = layout
                .peek(cell)
                .zip(input.pointer())
                .is_some_and(|(rect, at)| rect.contains(at));
            if hovered {
                out.cursor = Cursor::Pointer;
                if input.pressed(Button::Left) {
                    self.selected = Some(icon);
                }
            }
            let fill = if self.selected == Some(icon) {
                Some(SELECTED)
            } else {
                hovered.then_some(HOVER)
            };
            let element = Element {
                background: fill.map(Color::hex),
                radius: RADIUS / 2.,
                ..Element::DEFAULT.padded(GAP / 4.)
            };
            layout.open(cell, element);
            layout.icon(
                cell.child("icon"),
                icons.get(icon),
                ICON_SIZE,
                Color::hex(DIM),
            );
            layout.close();
        }
        layout.close();
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
            let row_id = id.child(("line", salt));
            layout.open(row_id, row);
            let style = TextStyle {
                font,
                size: TEXT_SIZE,
                color: Color::hex(TEXT),
            };
            layout.text(glyphs, row_id.child("text"), style, line);
            layout.close();
        }
        let size = ICON_SIZE * 3.;
        let triangle = Element {
            size: [Size::Fixed(size); 2],
            ..Element::DEFAULT
        };
        let mut painter = layout.custom(id.child("triangle"), triangle);
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
        layout.close();
    }
}
