//! A panel that only shows its title, standing in for one not written yet.

use gui::ui::{Align, Element, TextStyle, Ui};

use crate::assets::Fonts;
use crate::{DIM, TEXT_SIZE};

pub struct Placeholder {
    title: &'static str,
    fonts: Fonts,
}

impl Placeholder {
    pub const fn new(title: &'static str, fonts: Fonts) -> Self {
        Self { title, fonts }
    }

    pub const fn title(&self) -> &str {
        self.title
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let middle = Element {
            align: [Align::Center; 2],
            ..Element::column()
        };
        ui.element(middle, |ui| {
            let style = TextStyle {
                font: self.fonts.ui,
                size: TEXT_SIZE,
                color: DIM,
            };
            ui.text(style, self.title);
        });
    }
}
