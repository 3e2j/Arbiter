//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod panels;
mod theme;
mod workspace;

use assets::Icons;
use gui::{
    host::{App, Startup},
    input::Input,
    platform::window,
    ui::Ui,
};
pub use panels::Capture;
use panels::{Output, Panel, Placeholder, Showcase};
use workspace::{Place, Places, Workspace};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Window(#[from] window::Error),
}

/// Opens the editor in a window and blocks until it's closed.
///
/// # Errors
///
/// When the editor can't start, or the window fails.
pub fn run() -> Result<(), Error> {
    window::run::<Editor>()?;
    Ok(())
}

/// The editor, as one [`App`]. Owns what only Arbiter knows about.
// TODO: owns `Watch` once `app::watch` exists, which ignores `.arbiter/`, as
// the log and cache there change all the time.
// TODO: attaches `project::Log` to each project it opens, once it opens one.
struct Editor {
    workspace: Workspace,
}

impl App for Editor {
    const TITLE: &str = "Arbiter";
    const MIN_SIZE: [f32; 2] = [1024., 600.];

    fn new(start: &mut Startup) -> Result<Self, Box<dyn std::error::Error>> {
        start.theme = theme::theme(assets::fonts(start.glyphs)?);
        let icons = Icons::load(start.glyphs)?;
        // TODO: from the session, once the workspace is saved.
        let mut workspace = Workspace::new(icons);
        let editor = |title| Panel::Placeholder(Placeholder::new(title, Places::MAIN));
        let tool = |title| Panel::Placeholder(Placeholder::new(title, Places::BANDS));
        workspace.add(Panel::Showcase(Showcase::new(icons)), Place::Main);
        workspace.add(editor("Messages"), Place::Main);
        workspace.add(tool("Files"), Place::LeftInnerTop);
        workspace.add(tool("Search"), Place::LeftInnerTop);
        workspace.add(tool("Outline"), Place::LeftInnerBottom);
        workspace.add(tool("Inspector"), Place::RightInnerTop);
        workspace.add(Panel::Output(Output::new(icons)), Place::BelowMain);
        workspace.add(tool("Diagnostics"), Place::BelowMain);
        Ok(Self { workspace })
    }

    fn input(&mut self, input: &Input) {
        self.workspace.input(input);
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.workspace.ui(ui);
    }
}
