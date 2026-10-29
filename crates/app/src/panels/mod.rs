//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel. A document's selection is a
//! key, which it builds an [`Inspection`] of.

mod file;
mod files;
mod hex;
mod inspector;
mod messages;
mod output;
mod popover;

use gui::ui::Ui;
use serde::{Deserialize, Serialize};

use crate::views::Inspection;
use crate::workspace::Places;

pub use file::File;
pub use files::Files;
pub use hex::Hex;
pub use inspector::Inspector;
pub use output::{Capture, Output};

/// What a panel is, as the session saves its tab.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum PanelKind {
    Files,
    File,
    Output,
    Inspector,
}

impl PanelKind {
    /// The title a panel of this kind opens with.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Files => Files::TITLE,
            Self::File => "File",
            Self::Output => Output::TITLE,
            Self::Inspector => Inspector::TITLE,
        }
    }
}

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    Files(Files),
    File(File),
    Output(Output),
    Inspector(Inspector),
}

impl Panel {
    /// A panel of `kind` given back what [`Self::state`] kept, or `None`
    /// when no panel is. A file's tab can't open without its state.
    pub fn open(kind: PanelKind, state: Option<&toml::Value>) -> Option<Self> {
        let mut panel = match kind {
            PanelKind::Files => Self::Files(Files::new()),
            PanelKind::File => {
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
            PanelKind::Output => Self::Output(Output::new()),
            PanelKind::Inspector => Self::Inspector(Inspector::new()),
        };
        if let Some(state) = state {
            panel.restore(state.clone());
        }
        Some(panel)
    }

    /// What [`Self::open`] makes it from, which stays put when its title
    /// changes.
    pub const fn kind(&self) -> PanelKind {
        match self {
            Self::Files(_) => PanelKind::Files,
            Self::File(_) => PanelKind::File,
            Self::Output(_) => PanelKind::Output,
            Self::Inspector(_) => PanelKind::Inspector,
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
            Self::Output(_) | Self::Inspector(_) => None,
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
            Self::File(_) | Self::Output(_) | Self::Inspector(_) => {}
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::Files(_) => Files::TITLE,
            Self::File(panel) => panel.title(),
            Self::Output(_) => Output::TITLE,
            Self::Inspector(panel) => panel.title(),
        }
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::Files(_) => Files::PLACES,
            Self::File(_) => File::PLACES,
            Self::Output(_) | Self::Inspector(_) => Places::BANDS,
        }
    }

    /// The document's path and the key of what's selected in it, when it's
    /// a document with something selected.
    pub fn selection(&self) -> Option<(&str, usize)> {
        match self {
            Self::File(panel) => Some((panel.path(), panel.selection()?)),
            Self::Files(_) | Self::Output(_) | Self::Inspector(_) => None,
        }
    }

    /// What `key` names in it, when it's a document.
    pub fn inspect(&self, key: usize) -> Option<Inspection> {
        match self {
            Self::File(panel) => panel.inspect(key),
            Self::Files(_) | Self::Output(_) | Self::Inspector(_) => None,
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::Files(panel) => panel.ui(ui),
            Self::File(panel) => panel.ui(ui),
            Self::Output(panel) => panel.ui(ui),
            Self::Inspector(panel) => panel.ui(ui),
        }
    }
}
