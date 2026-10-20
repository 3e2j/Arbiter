//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel.

mod file;
mod files;
mod hex;
mod messages;
mod output;
mod popover;

use gui::ui::Ui;

use crate::workspace::Places;

pub use file::File;
pub use files::Files;
pub use hex::Hex;
pub use output::{Capture, Output};

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    Files(Files),
    File(File),
    Output(Output),
}

impl Panel {
    /// A panel of `kind` given back what [`Self::state`] kept, or `None`
    /// when no panel is. A file's tab can't open without its state.
    pub fn open(kind: &str, state: Option<&toml::Value>) -> Option<Self> {
        let mut panel = match kind {
            Files::KIND => Self::Files(Files::new()),
            File::KIND => {
                let Some(state) = state else {
                    tracing::warn!("the session has a file tab without its file");
                    return None;
                };
                return match state.clone().try_into() {
                    Ok(state) => Some(Self::File(File::from_state(state))),
                    Err(err) => {
                        tracing::warn!("a file tab in the session isn't readable: {err}");
                        None
                    }
                };
            }
            Output::KIND => Self::Output(Output::new()),
            _ => return None,
        };
        if let Some(state) = state {
            panel.restore(state.clone());
        }
        Some(panel)
    }

    /// What [`Self::open`] makes it from, which stays put when its title
    /// changes.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Files(_) => Files::KIND,
            Self::File(_) => File::KIND,
            Self::Output(_) => Output::KIND,
        }
    }

    /// What it keeps of itself in the session, if anything.
    pub fn state(&self) -> Option<toml::Value> {
        match self {
            Self::Files(panel) => toml::Value::try_from(panel.view())
                .inspect_err(|err| tracing::warn!("can't save the files panel's view: {err}"))
                .ok(),
            Self::File(panel) => toml::Value::try_from(panel.state())
                .inspect_err(|err| tracing::warn!("can't save {}'s tab: {err}", panel.path()))
                .ok(),
            Self::Output(_) => None,
        }
    }

    /// Takes back what [`Self::state`] kept. State it can't read leaves it
    /// as it was. A file's tab takes its state in [`Self::open`] instead.
    pub fn restore(&mut self, state: toml::Value) {
        match self {
            Self::Files(panel) => match state.try_into() {
                Ok(view) => panel.set_view(view),
                Err(err) => tracing::warn!("the files panel's view isn't readable: {err}"),
            },
            Self::File(_) | Self::Output(_) => {}
        }
    }

    /// The title a panel of `kind` opens with.
    pub fn title_of(kind: &str) -> &str {
        match kind {
            Files::KIND => Files::TITLE,
            Output::KIND => Output::TITLE,
            _ => kind,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Files(_) => Files::TITLE,
            Self::File(panel) => panel.title(),
            Self::Output(_) => Output::TITLE,
        }
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::Files(_) => Files::PLACES,
            Self::File(_) => File::PLACES,
            Self::Output(_) => Places::BANDS,
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::Files(panel) => panel.ui(ui),
            Self::File(panel) => panel.ui(ui),
            Self::Output(panel) => panel.ui(ui),
        }
    }
}
