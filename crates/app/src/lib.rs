//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod panels;
mod theme;
mod workspace;

use std::sync::Arc;

use assets::Icons;
use gui::{
    host::{App, Startup},
    input::Input,
    platform::window,
    ui::Ui,
};
pub use panels::Capture;
use panels::{Output, Panel, Placeholder, Showcase};
use project::{Log, Project};
use workspace::{Place, Places, Workspace};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Window(#[from] window::Error),
}

/// Opens the editor in a window, with `project` open if given, and blocks
/// until it's closed. `log` follows whichever project is open.
///
/// # Errors
///
/// When the editor can't start, or the window fails.
pub fn run(log: Arc<Log>, project: Option<Project>) -> Result<(), Error> {
    window::run::<Editor>((log, project))?;
    Ok(())
}

/// The editor, as one [`App`]. Owns what only Arbiter knows about.
// TODO: owns `Watch` once `app::watch` exists, which ignores `.arbiter/`, as
// the log and cache there change all the time.
struct Editor {
    workspace: Workspace,
    log: Arc<Log>,
    project: Option<Project>,
}

impl Editor {
    /// Logs into `project` from here on, in place of the one before.
    fn open(&mut self, project: Project) {
        if let Err(err) = self.log.attach(&project.root) {
            tracing::warn!("can't write a log in {}: {err}", project.root.display());
        }
        tracing::info!("opened {}", project.root.display());
        self.project = Some(project);
    }
}

impl App for Editor {
    const TITLE: &str = "Arbiter";
    const MIN_SIZE: [f32; 2] = [1024., 600.];
    type Args = (Arc<Log>, Option<Project>);

    fn new(
        start: &mut Startup,
        (log, project): Self::Args,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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
        let mut app = Self {
            workspace,
            log,
            project: None,
        };
        if let Some(project) = project {
            app.open(project);
        }
        Ok(app)
    }

    fn input(&mut self, input: &Input) {
        self.workspace.input(input);
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.workspace.ui(ui);
    }
}
