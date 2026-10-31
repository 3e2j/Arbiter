//! What the user works in, one per tab. Each panel is its own state behind
//! [`Panel`], declared once by its [`PanelDesc`] in [`PANELS`], and knows no
//! other panel. They reach each other by posting [`Event`]s, which [`route`]
//! offers to every panel in turn.

mod file_tree;
pub mod inspector;
mod open_file;
mod output;
pub mod route;

use gui::ui::Ui;
use project::Project;
use project::tree::Trees;
use serde::{Serialize, de::DeserializeOwned};

use crate::fs::FileLoader;
use crate::keymap::{AppCommand, KeyContext};
use crate::workspace::{Place, Places};

pub use file_tree::FileTree;
pub use inspector::Inspector;
use inspector::Page;
pub use open_file::{Hex, OpenFile};
pub use output::Output;

/// Every kind of panel there is. A session's tab whose id isn't here is
/// left out.
pub static PANELS: [PanelDesc; 4] = [
    FileTree::DESC,
    OpenFile::DESC,
    Output::DESC,
    Inspector::DESC,
];

/// What's known of a kind of panel before one exists. Each panel declares
/// its own as `DESC`.
pub struct PanelDesc {
    /// What the session saves its tabs as.
    pub id: &'static str,
    /// Its tab's, unless [`Panel::title`] names it.
    pub title: &'static str,
    /// Where a new one opens.
    pub opens_at: Place,
    /// Where its tab may sit.
    pub places: Places,
    /// Where its keys are bound while it has focus, if anywhere.
    pub key_context: Option<KeyContext>,
    /// Whether the View menu offers it.
    pub in_menu: bool,
    /// One given back what [`Panel::saved`] kept, or `None` when none can
    /// open from it.
    pub open: fn(Option<&toml::Value>) -> Option<Box<dyn Panel>>,
}

/// What every panel does, each with a default for a panel that doesn't.
pub trait Panel {
    fn desc(&self) -> &'static PanelDesc;

    /// Declares its contents into the box `ui` has open.
    fn ui(&mut self, ui: &mut Ui, cx: &mut Context);

    fn title(&self) -> &str {
        self.desc().title
    }

    /// Does `commands`, pressed while it has focus, before its `ui`.
    fn act(&mut self, _ui: &mut Ui, _commands: &[AppCommand]) {}

    /// What it keeps of itself in the session, if anything, for
    /// [`PanelDesc::open`] to read back.
    fn saved(&self) -> Option<toml::Value> {
        None
    }

    /// Takes `event` as it's offered, in turn with every other panel.
    fn receive(&mut self, _event: &mut Event) -> Reply {
        Reply::Pass
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
    Add(&'static PanelDesc),
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

/// The kind of panel `id` names, if any.
pub fn find(id: &str) -> Option<&'static PanelDesc> {
    PANELS.iter().find(|desc| desc.id == id)
}

/// What [`Panel::saved`] kept, if there is any and it's readable, for a
/// panel titled `title`.
fn read<T: DeserializeOwned>(title: &str, state: Option<&toml::Value>) -> Option<T> {
    state?
        .clone()
        .try_into()
        .inspect_err(|err| tracing::warn!("a {title} tab in the session isn't readable: {err}"))
        .ok()
}

/// `state` as the session keeps it, for `what`'s tab.
fn save(state: &impl Serialize, what: &str) -> Option<toml::Value> {
    toml::Value::try_from(state)
        .inspect_err(|err| tracing::warn!("can't save {what}'s tab: {err}"))
        .ok()
}
