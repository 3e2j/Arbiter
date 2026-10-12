//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod panels;
mod reader;
mod theme;
mod watch;
mod workspace;

use std::sync::Arc;

use assets::Icons;
use gui::{
    host::{App, Startup, Wake},
    input::Input,
    platform::window,
    ui::Ui,
};
pub use panels::Capture;
use panels::{Files, Output, Panel, Placeholder};
use project::{Log, Project};
use reader::Reader;
use watch::Watch;
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
struct Editor {
    workspace: Workspace,
    log: Arc<Log>,
    /// The open project's, unless it couldn't be watched.
    watch: Option<Watch>,
    /// The open project's, unless its thread couldn't start.
    reader: Option<Reader>,
    wake: Wake,
}

impl Editor {
    /// Logs into `project` from here on, in place of the one before.
    fn open(&mut self, project: Project) {
        if let Err(err) = self.log.attach(&project.root) {
            tracing::warn!("can't write a log in {}: {err}", project.root.display());
        }
        tracing::info!("opened {}", project.root.display());
        let root = project.root.clone();
        let project = Arc::new(project);
        self.reader = match Reader::new(project.clone(), self.wake.clone()) {
            Ok(reader) => Some(reader),
            Err(err) => {
                tracing::warn!("can't start reading the project's files: {err}");
                None
            }
        };
        if let Some(files) = self.files() {
            files.close();
        }
        // Its first batch asks for the first read, once watching.
        let matters = move |path: &str| project.trees_read(path);
        self.watch = match Watch::new(root, matters, self.wake.clone()) {
            Ok(watch) => Some(watch),
            Err(err) => {
                tracing::warn!("can't start the watch, so the files won't refresh: {err}");
                self.read_trees();
                None
            }
        };
    }

    fn read_trees(&self) {
        if let Some(reader) = &self.reader {
            reader.ask();
        }
    }

    fn files(&mut self) -> Option<&mut Files> {
        self.workspace.panels_mut().find_map(|panel| match panel {
            Panel::Files(files) => Some(files),
            _ => None,
        })
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
        workspace.add(editor("Messages"), Place::Main);
        workspace.add(
            Panel::Files(Files::new(icons, &mut start.keymap)),
            Place::LeftInnerTop,
        );
        workspace.add(tool("Search"), Place::LeftInnerTop);
        workspace.add(tool("Outline"), Place::LeftInnerBottom);
        workspace.add(tool("Inspector"), Place::RightInnerTop);
        workspace.add(Panel::Output(Output::new(icons)), Place::BelowMain);
        workspace.add(tool("Diagnostics"), Place::BelowMain);
        let mut app = Self {
            workspace,
            log,
            watch: None,
            reader: None,
            wake: start.wake.clone(),
        };
        if let Some(project) = project {
            app.open(project);
        }
        Ok(app)
    }

    fn input(&mut self, input: &Input) {
        if self.watch.as_ref().and_then(Watch::take).is_some() {
            self.read_trees();
        }
        // Catches what the watch can't see, such as a repo above the root.
        if input.focused() {
            self.read_trees();
        }
        match self.reader.as_ref().and_then(Reader::take) {
            Some(Ok(trees)) => {
                if let Some(files) = self.files() {
                    files.refresh(trees);
                }
            }
            Some(Err(err)) => tracing::warn!("can't read the project's files: {err}"),
            None => {}
        }
        self.workspace.input(input);
    }

    fn ui(&mut self, ui: &mut Ui) {
        self.workspace.ui(ui);
    }
}
