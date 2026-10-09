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
    platform::window,
    ui::Ui,
};
use panels::{Panel, Placeholder, Showcase};
use workspace::{Place, Workspace};

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
// TODO: owns `Watch` once `app::watch` exists.
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
        let mut workspace = Workspace::default();
        let placeholder = |title| Panel::Placeholder(Placeholder::new(title));
        workspace.add(Panel::Showcase(Showcase::new(icons)), Place::Main);
        workspace.add(placeholder("Messages"), Place::Main);
        workspace.add(placeholder("Files"), Place::LeftInnerTop);
        workspace.add(placeholder("Search"), Place::LeftInnerTop);
        workspace.add(placeholder("Outline"), Place::LeftInnerBottom);
        workspace.add(placeholder("Inspector"), Place::RightInnerTop);
        workspace.add(placeholder("Log"), Place::BelowMain);
        workspace.add(placeholder("Diagnostics"), Place::BelowMain);
        Ok(Self { workspace })
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.workspace.ui(ui);
    }
}
