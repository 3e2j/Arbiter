//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel.

mod files;
mod output;
mod placeholder;
mod showcase;

use gui::ui::Ui;

use crate::workspace::Places;

pub use files::Files;
pub use output::{Capture, Output};
pub use placeholder::Placeholder;
pub use showcase::Showcase;

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    Files(Files),
    Output(Output),
    Placeholder(Placeholder),
    Showcase(Showcase),
}

impl Panel {
    pub fn title(&self) -> &str {
        match self {
            Self::Files(_) => Files::TITLE,
            Self::Output(_) => Output::TITLE,
            Self::Placeholder(panel) => panel.title(),
            Self::Showcase(_) => Showcase::TITLE,
        }
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::Files(_) => Files::PLACES,
            Self::Output(_) => Places::BANDS,
            Self::Placeholder(panel) => panel.places(),
            Self::Showcase(_) => Places::ALL,
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::Files(panel) => panel.ui(ui),
            Self::Output(panel) => panel.ui(ui),
            Self::Placeholder(panel) => panel.ui(ui),
            Self::Showcase(panel) => panel.ui(ui),
        }
    }
}
