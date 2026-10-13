//! A panel that only shows its title, standing in for one not written yet.

use gui::{components::notice, ui::Ui};

use crate::workspace::Places;

/// The panels not written yet, by title, and where each may sit.
const STANDINS: [(&str, Places); 5] = [
    ("Messages", Places::MAIN),
    ("Search", Places::BANDS),
    ("Outline", Places::BANDS),
    ("Inspector", Places::BANDS),
    ("Diagnostics", Places::BANDS),
];

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

    /// The stand-in titled `title`, as its kind.
    pub fn open(title: &str) -> Option<Self> {
        let (title, places) = STANDINS
            .into_iter()
            .find(|&(standin, _)| standin == title)?;
        Some(Self::new(title, places))
    }

    pub const fn title(&self) -> &'static str {
        self.title
    }

    pub const fn places(&self) -> Places {
        self.places
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        notice(ui, self.title);
    }
}
