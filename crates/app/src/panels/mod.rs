//! What the user works in, one per tab. Each panel is its own state plus a
//! title and a `ui`, and knows no other panel. [`route`] connects them.

mod file_tree;
pub mod inspector;
mod open_file;
mod output;
pub mod route;

use gui::ui::Ui;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::keymap::{AppCommand, KeyContext};
use crate::workspace::{Place, Places};

pub use file_tree::FileTree;
pub use inspector::Inspector;
pub use open_file::{Hex, OpenFile};
pub use output::Output;

/// What a panel is, as the session saves its tab.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum PanelKind {
    FileTree,
    OpenFile,
    Output,
    Inspector,
}

impl PanelKind {
    /// The title a panel of this kind opens with.
    pub const fn title(self) -> &'static str {
        match self {
            Self::FileTree => FileTree::TITLE,
            Self::OpenFile => "File",
            Self::Output => Output::TITLE,
            Self::Inspector => Inspector::TITLE,
        }
    }

    /// Where a new panel of this kind opens.
    pub const fn place(self) -> Place {
        match self {
            Self::FileTree => Place::LeftInnerTop,
            Self::OpenFile => Place::Main,
            Self::Output => Place::BelowMain,
            Self::Inspector => Place::RightInnerTop,
        }
    }
}

/// Every panel, by type, so a tab holds one inline.
pub enum Panel {
    FileTree(FileTree),
    OpenFile(OpenFile),
    Output(Output),
    Inspector(Inspector),
}

impl Panel {
    /// A panel of `kind` given back what [`Self::state`] kept, or `None`
    /// when no panel is. A file's tab can't open without its state.
    pub fn open(kind: PanelKind, state: Option<&toml::Value>) -> Option<Self> {
        Some(match kind {
            PanelKind::FileTree => {
                Self::FileTree(FileTree::new(read(kind, state).unwrap_or_default()))
            }
            PanelKind::OpenFile => {
                if state.is_none() {
                    tracing::warn!("the session has a file tab without its file");
                }
                Self::OpenFile(OpenFile::from_state(read(kind, state)?))
            }
            PanelKind::Output => Self::Output(Output::new()),
            PanelKind::Inspector => Self::Inspector(Inspector::from_state(
                &read(kind, state).unwrap_or_default(),
            )),
        })
    }

    /// What [`Self::open`] makes it from, which stays put when its title
    /// changes.
    pub const fn kind(&self) -> PanelKind {
        match self {
            Self::FileTree(_) => PanelKind::FileTree,
            Self::OpenFile(_) => PanelKind::OpenFile,
            Self::Output(_) => PanelKind::Output,
            Self::Inspector(_) => PanelKind::Inspector,
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
            Self::Inspector(panel) => toml::Value::try_from(panel.state())
                .inspect_err(|err| tracing::warn!("can't save an inspector's tab: {err}"))
                .ok(),
            Self::Output(_) => None,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::FileTree(_) => FileTree::TITLE,
            Self::OpenFile(panel) => panel.title(),
            Self::Output(_) => Output::TITLE,
            Self::Inspector(panel) => panel.title(),
        }
    }

    /// Where its tab may sit.
    pub const fn places(&self) -> Places {
        match self {
            Self::FileTree(_) => FileTree::PLACES,
            Self::OpenFile(_) => OpenFile::PLACES,
            Self::Output(_) | Self::Inspector(_) => Places::BANDS,
        }
    }

    /// Where its keys are bound while it has focus, if anywhere.
    pub const fn key_context(&self) -> Option<KeyContext> {
        match self {
            Self::FileTree(_) => Some(file_tree::FILES),
            Self::OpenFile(_) => Some(open_file::HEX),
            Self::Output(_) | Self::Inspector(_) => None,
        }
    }

    /// Does `commands`, pressed while it has focus, before its `ui`.
    pub fn act(&mut self, ui: &mut Ui, commands: &[AppCommand]) {
        match self {
            Self::FileTree(panel) => panel.act(commands),
            Self::OpenFile(panel) => panel.act(ui, commands),
            Self::Output(_) | Self::Inspector(_) => {}
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::FileTree(panel) => panel.ui(ui),
            Self::OpenFile(panel) => panel.ui(ui),
            Self::Output(panel) => panel.ui(ui),
            Self::Inspector(panel) => panel.ui(ui),
        }
    }
}

/// What [`Panel::state`] kept, if there is any and it's readable.
fn read<T: DeserializeOwned>(kind: PanelKind, state: Option<&toml::Value>) -> Option<T> {
    state?
        .clone()
        .try_into()
        .inspect_err(|err| {
            tracing::warn!(
                "a {} tab in the session isn't readable: {err}",
                kind.title()
            );
        })
        .ok()
}
