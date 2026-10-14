//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel.

mod files;
mod hex;
mod output;
mod placeholder;

use gui::ui::Ui;

use crate::workspace::Places;

pub use files::Files;
pub use hex::Hex;
pub use output::{Capture, Output};
pub use placeholder::Placeholder;

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    Files(Files),
    Hex(Hex),
    Output(Output),
    Placeholder(Placeholder),
}

impl Panel {
    /// A new panel of `kind`, or `None` when no panel is.
    pub fn open(kind: &str) -> Option<Self> {
        match kind {
            Files::KIND => Some(Self::Files(Files::new())),
            Output::KIND => Some(Self::Output(Output::new())),
            _ => Placeholder::open(kind).map(Self::Placeholder),
        }
    }

    /// What [`Self::open`] makes it from, which stays put when its title
    /// changes.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Files(_) => Files::KIND,
            Self::Hex(_) => Hex::KIND,
            Self::Output(_) => Output::KIND,
            Self::Placeholder(panel) => panel.title(),
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Files(_) => Files::TITLE,
            Self::Hex(panel) => panel.title(),
            Self::Output(_) => Output::TITLE,
            Self::Placeholder(panel) => panel.title(),
        }
    }

    /// Whether the session keeps it, which takes [`Self::open`] making it
    /// again from its kind alone. A file's panel needs its file too.
    pub const fn saved(&self) -> bool {
        !matches!(self, Self::Hex(_))
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::Files(_) => Files::PLACES,
            Self::Hex(_) => Hex::PLACES,
            Self::Output(_) => Places::BANDS,
            Self::Placeholder(panel) => panel.places(),
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::Files(panel) => panel.ui(ui),
            Self::Hex(panel) => panel.ui(ui),
            Self::Output(panel) => panel.ui(ui),
            Self::Placeholder(panel) => panel.ui(ui),
        }
    }
}
