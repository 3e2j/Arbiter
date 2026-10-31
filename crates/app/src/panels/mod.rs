//! What the user works in, one per tab. Each [`Panel`] is one [`Kind`]'s
//! state, and knows no other panel. They reach each other by posting
//! [`Event`]s, which [`route`] offers to every panel in turn.

mod file_tree;
pub mod inspector;
mod open_file;
mod output;
pub mod route;

use gui::ui::Ui;
use project::Project;
use project::tree::Trees;
use serde::{Deserialize, Serialize};

use crate::fs::FileLoader;
use crate::keymap::{AppCommand, KeyContext};
use crate::workspace::{Place, Places};

pub use file_tree::FileTree;
use file_tree::FilesView;
pub use inspector::Inspector;
use inspector::{InspectorState, Page};
use open_file::OpenFileState;
pub use open_file::{Hex, OpenFile};
pub use output::Output;

/// Every kind of panel there is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    FileTree,
    OpenFile,
    Inspector,
    Output,
}

impl Kind {
    /// In the View menu's order.
    pub const ALL: [Self; 4] = [
        Self::FileTree,
        Self::OpenFile,
        Self::Output,
        Self::Inspector,
    ];

    /// Its tab's, unless [`Panel::title`] names it.
    pub const fn title(self) -> &'static str {
        match self {
            Self::FileTree => "Files",
            Self::OpenFile => "File",
            Self::Inspector => "Inspector",
            Self::Output => "Output",
        }
    }

    /// Where a new one opens.
    pub const fn opens_at(self) -> Place {
        match self {
            Self::FileTree => Place::LeftInnerTop,
            Self::OpenFile => Place::Main,
            Self::Inspector => Place::RightInnerTop,
            Self::Output => Place::BelowMain,
        }
    }

    /// Where its tab may sit.
    pub const fn places(self) -> Places {
        match self {
            Self::OpenFile => Places::MAIN,
            Self::FileTree | Self::Inspector | Self::Output => Places::BANDS,
        }
    }

    /// Where its keys are bound while it has focus, if anywhere.
    pub const fn key_context(self) -> Option<KeyContext> {
        match self {
            Self::FileTree => Some(file_tree::FILES),
            Self::OpenFile => Some(open_file::HEX),
            Self::Inspector | Self::Output => None,
        }
    }

    /// Whether the View menu offers it. A file opens from the file tree.
    pub const fn in_menu(self) -> bool {
        !matches!(self, Self::OpenFile)
    }
}

pub enum Panel {
    FileTree(FileTree),
    OpenFile(OpenFile),
    Inspector(Inspector),
    Output(Output),
}

/// A panel as the session keeps it, for [`Panel::open`] to read back.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(tag = "kind", content = "state", rename_all = "kebab-case")]
pub enum SavedTab {
    FileTree(FilesView),
    OpenFile(OpenFileState),
    Inspector(InspectorState),
    Output,
    /// One that couldn't be read, such as a kind from a newer Arbiter.
    #[serde(skip)]
    Unknown,
}

impl Panel {
    /// A new one of `kind`, or `None` for one that needs more to open, such
    /// as a file.
    pub fn new(kind: Kind) -> Option<Self> {
        Some(match kind {
            Kind::FileTree => Self::FileTree(FileTree::new(FilesView::default())),
            Kind::OpenFile => return None,
            Kind::Inspector => Self::Inspector(Inspector::new(None)),
            Kind::Output => Self::Output(Output::new()),
        })
    }

    /// One as `saved` kept it, or `None` for [`SavedTab::Unknown`].
    pub fn open(saved: &SavedTab) -> Option<Self> {
        Some(match saved {
            &SavedTab::FileTree(view) => Self::FileTree(FileTree::new(view)),
            SavedTab::OpenFile(state) => Self::OpenFile(OpenFile::from_state(state)),
            SavedTab::Inspector(state) => Self::Inspector(Inspector::from_state(state)),
            SavedTab::Output => Self::Output(Output::new()),
            SavedTab::Unknown => return None,
        })
    }

    pub fn saved(&self) -> SavedTab {
        match self {
            Self::FileTree(tree) => SavedTab::FileTree(tree.view()),
            Self::OpenFile(file) => SavedTab::OpenFile(file.state()),
            Self::Inspector(inspector) => SavedTab::Inspector(inspector.state()),
            Self::Output(_) => SavedTab::Output,
        }
    }

    pub const fn kind(&self) -> Kind {
        match self {
            Self::FileTree(_) => Kind::FileTree,
            Self::OpenFile(_) => Kind::OpenFile,
            Self::Inspector(_) => Kind::Inspector,
            Self::Output(_) => Kind::Output,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            Self::OpenFile(file) => file.title(),
            Self::Inspector(inspector) => inspector.title(),
            Self::FileTree(_) | Self::Output(_) => self.kind().title(),
        }
    }

    /// Does `commands`, pressed while it has focus, before its `ui`.
    pub fn act(&mut self, ui: &mut Ui, commands: &[AppCommand]) {
        match self {
            Self::FileTree(tree) => tree.act(commands),
            Self::OpenFile(file) => file.act(ui, commands),
            Self::Inspector(_) | Self::Output(_) => {}
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        match self {
            Self::FileTree(tree) => tree.ui(ui, cx),
            Self::OpenFile(file) => file.ui(ui, cx),
            Self::Inspector(inspector) => inspector.ui(ui, cx),
            Self::Output(output) => output.ui(ui),
        }
    }

    /// Takes `event` as it's offered, in turn with every other panel.
    pub fn receive(&mut self, event: &mut Event) -> Reply {
        match self {
            Self::FileTree(tree) => tree.receive(event),
            Self::OpenFile(file) => file.receive(event),
            Self::Inspector(inspector) => inspector.receive(event),
            Self::Output(_) => Reply::Pass,
        }
    }
}

/// What a panel's `ui` is given, and where it posts its events.
pub struct Context<'a> {
    pub project: Option<&'a Project>,
    /// Unless its thread couldn't start.
    pub loader: Option<&'a FileLoader>,
    posted: Vec<Event<'static>>,
}

/// Offered to every panel in turn until one shows it. [`route`] opens a
/// panel for one no panel shows, when it's one a panel can open for.
pub enum Event<'a> {
    /// A file, relative to the project's root, to show.
    Pick(String),
    /// Something clicked in an editor, to inspect.
    Inspect(Page),
    /// An inspector was just unlocked, so the unlocked one before it closes.
    Unlocked,
    /// The View menu asked for another panel of this kind.
    Add(Kind),
    /// The open project's trees, read again.
    Read(&'a Trees),
}

/// What a panel does with an [`Event`] it's offered.
pub enum Reply {
    /// Leaves it for the next panel.
    Pass,
    /// Takes it, so it's brought to the front and no other panel is offered
    /// it.
    Show,
    /// Leaves it, but closes.
    Close,
}

impl<'a> Context<'a> {
    pub const fn new(project: Option<&'a Project>, loader: Option<&'a FileLoader>) -> Self {
        Self {
            project,
            loader,
            posted: Vec::new(),
        }
    }

    pub fn post(&mut self, event: Event<'static>) {
        self.posted.push(event);
    }
}
