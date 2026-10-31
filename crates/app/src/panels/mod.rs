//! What the user works in, one per tab. Each [`Panel`] is one [`Kind`]'s
//! state, and knows no other panel. What a panel asks of the others is a
//! [`Request`], done once every panel is drawn.

mod file_tree;
pub mod inspector;
mod open_file;
mod output;

use gui::ui::Ui;
use project::Project;
use project::tree::Trees;
use serde::{Deserialize, Serialize};

use crate::documents::Documents;
use crate::keymap::{AppCommand, KeyContext};
use crate::workspace::{Place, Places, Workspace};

pub use file_tree::FileTree;
use file_tree::FilesView;
pub use inspector::Inspector;
use inspector::{InspectorState, Page};
use open_file::OpenFileState;
pub use open_file::{Format, Hex, OpenFile};
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
    pub fn act(&mut self, ui: &mut Ui, commands: &[AppCommand], documents: &Documents) {
        match self {
            Self::FileTree(tree) => tree.act(commands),
            Self::OpenFile(file) => file.act(ui, commands, documents),
            Self::Inspector(_) | Self::Output(_) => {}
        }
    }

    /// Declares its contents into the box `ui` has open.
    pub fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        match self {
            Self::FileTree(tree) => tree.ui(ui, cx),
            Self::OpenFile(file) => file.ui(ui, cx),
            Self::Inspector(inspector) => inspector.ui(ui),
            Self::Output(output) => output.ui(ui),
        }
    }

    /// Takes the open project's trees, read again, as a file tree's own
    /// copy to sort.
    pub fn read(&mut self, trees: &Trees) {
        if let Self::FileTree(tree) = self {
            tree.refresh(trees.clone());
        }
    }

    /// The file it shows, relative to the project's root, if it's a file's tab.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::OpenFile(file) => Some(file.path()),
            _ => None,
        }
    }

    const fn unlocked_inspector(&mut self) -> Option<&mut Inspector> {
        match self {
            Self::Inspector(inspector) if !inspector.locked() => Some(inspector),
            _ => None,
        }
    }
}

/// What a panel's `ui` is given.
pub struct Context<'a> {
    pub project: Option<&'a Project>,
    pub documents: &'a mut Documents,
    /// What the panels asked for this pass.
    pub requests: Vec<Request>,
}

/// What a panel asks of the workspace, done once every panel is drawn.
pub enum Request {
    /// Shows a file, relative to the project's root, in its tab or a new one.
    Open(String),
    /// Shows a panel of this kind from the View menu. A new one opens unless
    /// it's an inspector and an unlocked one is open.
    Add(Kind),
    /// Shows what was clicked in an editor in the unlocked inspector, or a
    /// new one.
    Inspect(Page),
}

impl Request {
    /// Does it in `workspace`, a new file tree given `trees` if they're read.
    pub fn apply(self, workspace: &mut Workspace, trees: Option<&Trees>) {
        let panel = match self {
            Self::Open(path) => {
                let open = |panel: &mut Panel| matches!(panel, Panel::OpenFile(file) if file.path() == path);
                if workspace.show(open) {
                    return;
                }
                Panel::OpenFile(OpenFile::new(path))
            }
            Self::Add(Kind::Inspector)
                if workspace.show(|panel| panel.unlocked_inspector().is_some()) =>
            {
                return;
            }
            Self::Add(kind) => {
                let Some(mut panel) = Panel::new(kind) else {
                    return;
                };
                if let Some(trees) = trees {
                    panel.read(trees);
                }
                panel
            }
            Self::Inspect(page) => {
                let mut page = Some(page);
                let shown = workspace.show(|panel| {
                    let Some(inspector) = panel.unlocked_inspector() else {
                        return false;
                    };
                    inspector.show(page.take().unwrap_or_default());
                    true
                });
                if shown {
                    return;
                }
                Panel::Inspector(Inspector::new(page))
            }
        };
        workspace.add(panel.kind().opens_at(), panel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Session;

    fn count(workspace: &mut Workspace, kind: Kind) -> usize {
        workspace
            .panels()
            .filter(|panel| panel.kind() == kind)
            .count()
    }

    #[test]
    fn requests_show_what_is_open_before_adding() {
        let mut workspace = Workspace::new(&Session::default());
        let open = || Request::Open("files/a.bin".to_owned());
        open().apply(&mut workspace, None);
        open().apply(&mut workspace, None);
        assert_eq!(count(&mut workspace, Kind::OpenFile), 1);

        Request::Add(Kind::Inspector).apply(&mut workspace, None);
        Request::Add(Kind::Inspector).apply(&mut workspace, None);
        assert_eq!(count(&mut workspace, Kind::Inspector), 1);

        for panel in workspace.panels() {
            if let Panel::Inspector(inspector) = panel {
                *inspector = Inspector::from_state(&toml::from_str("locked = true").unwrap());
            }
        }
        Request::Inspect(Page::default()).apply(&mut workspace, None);
        assert_eq!(count(&mut workspace, Kind::Inspector), 2);
    }
}
