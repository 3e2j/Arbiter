//! A panel that only shows its title, standing in for one not written yet.

use gui::{components::notice, ui::Ui};

use crate::workspace::Places;

pub struct Placeholder {
    title: &'static str,
    places: Places,
}

impl Placeholder {
    /// Shown as `title`, and allowed in `places`, as the panel it stands in
    /// for would be.
    pub const fn new(title: &'static str, places: Places) -> Self {
        Self { title, places }
    }

    pub const fn title(&self) -> &str {
        self.title
    }

    pub const fn places(&self) -> Places {
        self.places
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        notice(ui, self.title);
    }
}
