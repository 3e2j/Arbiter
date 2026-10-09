//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel.

mod placeholder;
mod showcase;

use gui::ui::Ui;

use crate::workspace::Places;

pub use placeholder::Placeholder;
pub use showcase::Showcase;

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    Placeholder(Placeholder),
    Showcase(Showcase),
}

impl Panel {
    pub fn title(&self) -> &str {
        match self {
            Self::Placeholder(panel) => panel.title(),
            Self::Showcase(_) => Showcase::TITLE,
        }
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::Placeholder(panel) => panel.places(),
            Self::Showcase(_) => Places::ALL,
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::Placeholder(panel) => panel.ui(ui),
            Self::Showcase(panel) => panel.ui(ui),
        }
    }
}
