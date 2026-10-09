//! A panel that only shows its title, standing in for one not written yet.

use gui::ui::{Align, Element, Ui};

pub struct Placeholder {
    title: &'static str,
}

impl Placeholder {
    pub const fn new(title: &'static str) -> Self {
        Self { title }
    }

    pub const fn title(&self) -> &str {
        self.title
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let theme = ui.theme();
        let middle = Element {
            align: [Align::Center; 2],
            ..Element::column()
        };
        ui.element(middle, |ui| {
            ui.text(theme.ui_text(theme.color.dim), self.title)
        });
    }
}
