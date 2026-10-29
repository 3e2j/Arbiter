//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel.

mod file_tree;
mod open_file;
mod output;

use gui::ui::Ui;
use serde::{Deserialize, Serialize};

use crate::workspace::Places;

pub use file_tree::FileTree;
pub use open_file::{Hex, OpenFile};
pub use output::Output;

/// What a panel is, as the session saves its tab.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum PanelKind {
    FileTree,
    OpenFile,
    Output,
}

impl PanelKind {
    /// The title a panel of this kind opens with.
    pub const fn title(self) -> &'static str {
        match self {
            Self::FileTree => FileTree::TITLE,
            Self::OpenFile => "File",
            Self::Output => Output::TITLE,
        }
    }
}

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    FileTree(FileTree),
    OpenFile(OpenFile),
    Output(Output),
}

impl Panel {
    /// A panel of `kind` given back what [`Self::state`] kept, or `None`
    /// when no panel is. A file's tab can't open without its state.
    pub fn open(kind: PanelKind, state: Option<&toml::Value>) -> Option<Self> {
        let mut panel = match kind {
            PanelKind::FileTree => Self::FileTree(FileTree::new()),
            PanelKind::OpenFile => {
                let Some(state) = state else {
                    tracing::warn!("the session has a file tab without its file");
                    return None;
                };
                return match state.clone().try_into() {
                    Ok(state) => Some(Self::OpenFile(OpenFile::from_state(state))),
                    Err(err) => {
                        tracing::warn!("a file tab in the session isn't readable: {err}");
                        None
                    }
                };
            }
            PanelKind::Output => Self::Output(Output::new()),
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
            Self::FileTree(_) => PanelKind::FileTree,
            Self::OpenFile(_) => PanelKind::OpenFile,
            Self::Output(_) => PanelKind::Output,
        }
    }

    /// What it keeps of itself in the session, if anything.
    pub fn state(&self) -> Option<toml::Value> {
        match self {
            Self::FileTree(panel) => toml::Value::try_from(panel.view())
                .inspect_err(|err| tracing::warn!("can't save the files panel's view: {err}"))
                .ok(),
            Self::OpenFile(panel) => toml::Value::try_from(panel.state())
                .inspect_err(|err| tracing::warn!("can't save {}'s tab: {err}", panel.path()))
                .ok(),
            Self::Output(_) => None,
        }
    }

    /// Takes back what [`Self::state`] kept. State it can't read leaves it
    /// as it was. A file's tab takes its state in [`Self::open`] instead.
    pub fn restore(&mut self, state: toml::Value) {
        match self {
            Self::FileTree(panel) => match state.try_into() {
                Ok(view) => panel.set_view(view),
                Err(err) => tracing::warn!("the files panel's view isn't readable: {err}"),
            },
            Self::OpenFile(_) | Self::Output(_) => {}
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::FileTree(_) => FileTree::TITLE,
            Self::OpenFile(panel) => panel.title(),
            Self::Output(_) => Output::TITLE,
        }
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::FileTree(_) => FileTree::PLACES,
            Self::OpenFile(_) => OpenFile::PLACES,
            Self::Output(_) => Places::BANDS,
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::FileTree(panel) => panel.ui(ui),
            Self::OpenFile(panel) => panel.ui(ui),
            Self::Output(panel) => panel.ui(ui),
        }
    }
}
