//! A very temporary stand-in panel for testing.
//! Shows what `gui` draws: icons, hover and select, font fallback, colour emoji,
//! triangles, buttons and a long scrolled list.
//!
//! To be deleted when a proper workspace / panels gets going

use gui::{
    canvas::{Color, Rect, Vertex},
    components::{ListScroll, Row, button, icon_button, list, row},
    ui::{Align, Direction, Element, Size, TextStyle, Ui},
};

use crate::assets::{Icon, Icons};

pub struct Showcase {
    icons: Icons,
    /// The icon last clicked.
    selected: Option<Icon>,
    /// Each with its id.
    rows: Vec<(u32, String)>,
    picked: Option<u32>,
    scroll: ListScroll,
}

impl Showcase {
    pub const TITLE: &str = "Showcase";

    pub fn new(icons: Icons) -> Self {
        let mut showcase = Self {
            icons,
            selected: None,
            rows: Vec::new(),
            picked: None,
            scroll: ListScroll::TOP,
        };
        for _ in 0..20 {
            showcase.add_row();
        }
        showcase
    }

    fn add_row(&mut self) {
        let id = self.rows.last().map_or(0, |&(id, _)| id + 1);
        self.rows.push((id, format!("Message {id}")));
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let theme = ui.theme();
        let size = theme.size;
        let strip = Element {
            direction: Direction::LeftToRight,
            gap: size.gap / 2.,
            align: [Align::Start, Align::Center],
            ..Element::DEFAULT
        };
        ui.element(strip, |ui| {
            for icon in Icon::ALL {
                if icon_button(ui, self.icons.get(icon)) {
                    self.selected = Some(icon);
                }
            }
        });
        let lines = [
            (theme.font.ui, "The quick brown fox jumps over the lazy dog"),
            (theme.font.ui, "いろはにほへと ちりぬるを わかよたれそ"),
            (theme.font.buffer, "0O 1lI {}[]() => != 0x1f4"),
            (theme.font.ui, "From a system font: 한국어"),
            (theme.font.ui, "Colour emoji (color atlas): 🦀"),
        ];
        let line = Element {
            size: [Size::Grow, Size::Fixed(size.row)],
            align: [Align::Start, Align::Center],
            ..Element::row()
        };
        for (font, text) in lines {
            ui.element(line, |ui| {
                let style = TextStyle {
                    font,
                    ..theme.ui_text(theme.color.text)
                };
                ui.text(style, text);
            });
        }
        let edge = size.icon * 3.;
        let triangle = Element {
            size: [Size::Fixed(edge); 2],
            ..Element::DEFAULT
        };
        let mut painter = ui.custom(triangle);
        if let Some(Rect { x, y, .. }) = painter.rect {
            painter.triangles(
                &[
                    Vertex::new([x + edge / 2., y], Color::hex(0xfa_4d_56)),
                    Vertex::new([x + edge, y + edge], Color::hex(0x42_be_65)),
                    Vertex::new([x, y + edge], Color::hex(0x78_a9_ff)),
                ],
                &[0, 1, 2],
            );
        }
        ui.element(strip, |ui| {
            if button(ui, "Add a row") {
                self.add_row();
            }
            if button(ui, "Remove the last") {
                self.rows.pop();
            }
            let picked = self
                .selected
                .map_or("No icon picked", |_| "An icon is picked");
            ui.text(theme.ui_text(theme.color.dim), picked);
        });
        let icon = self.icons.get(Icon::Message);
        list(
            ui,
            &mut self.scroll,
            &self.rows,
            size.row,
            |ui, (id, label)| {
                let shown = Row {
                    icon: Some(icon),
                    selected: self.picked == Some(*id),
                    ..Row::new(label)
                };
                if row(ui, shown) {
                    self.picked = Some(*id);
                }
            },
        );
    }
}
