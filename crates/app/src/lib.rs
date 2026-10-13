//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod panels;
mod reader;
mod theme;
mod title_bar;
mod watch;
mod workspace;

use std::sync::Arc;

use gui::{
    components::window_edges,
    host::{App, Startup, Wake},
    input::Input,
    platform::window,
    ui::{Element, Ui},
};
pub use panels::Capture;
use panels::{Files, Output, Panel};
use project::{Log, Project};
use reader::TreeReader;
use watch::Watch;
use workspace::{Place, Session, Workspace};

/// How far in from the window's edges a drag resizes it, in logical pixels.
const RESIZE_EDGE: f32 = 8.;

/// Each panel's kind and place in a project without a session, in tab order.
const LAYOUT: [(&str, Place); 7] = [
    ("Messages", Place::Main),
    (Files::KIND, Place::LeftInnerTop),
    ("Search", Place::LeftInnerTop),
    ("Outline", Place::LeftInnerBottom),
    ("Inspector", Place::RightInnerTop),
    (Output::KIND, Place::BelowMain),
    ("Diagnostics", Place::BelowMain),
];

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Window(#[from] window::RunError),
}

/// Opens the editor in a window, with `project` open if given, and blocks
/// until it's closed. `log` follows whichever project is open.
///
/// # Errors
///
/// When the editor can't start, or the window fails.
pub fn run(log: Arc<Log>, project: Option<Project>) -> Result<(), AppError> {
    window::run::<Editor>((log, project))?;
    Ok(())
}

/// The editor, as one [`App`]. Owns what only Arbiter knows about.
struct Editor {
    workspace: Workspace,
    log: Arc<Log>,
    /// The open one, whose session the workspace is saved to on close.
    project: Option<Arc<Project>>,
    /// The open project's, unless it couldn't be watched.
    watch: Option<Watch>,
    /// The open project's, unless its thread couldn't start.
    reader: Option<TreeReader>,
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
        self.project = Some(project.clone());
        self.reader = match TreeReader::new(project.clone(), self.wake.clone()) {
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
    const DECORATED: bool = false;
    type Args = (Arc<Log>, Option<Project>);

    fn new(
        start: &mut Startup,
        (log, project): Self::Args,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        start.theme = theme::theme(assets::fonts(start.glyphs)?);
        assets::load_icons(start.glyphs)?;
        Files::bind(&mut start.keymap);
        let session = project
            .as_ref()
            .and_then(session)
            .unwrap_or_else(|| Session::of(&LAYOUT));
        let mut app = Self {
            workspace: Workspace::new(&session),
            log,
            project: None,
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
        match self.reader.as_ref().and_then(TreeReader::take) {
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

    fn on_close(&mut self) {
        let Some(project) = &self.project else {
            return;
        };
        let saved = match self.workspace.session().to_text() {
            Ok(text) => project.write_session(&text).map_err(|err| err.to_string()),
            Err(err) => Err(err.to_string()),
        };
        if let Err(err) = saved {
            tracing::warn!("can't save the session: {err}");
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        ui.element(Element::column(), |ui| {
            title_bar::ui(ui, Self::TITLE);
            self.workspace.ui(ui);
            window_edges(ui, RESIZE_EDGE);
        });
    }
}

/// `project`'s session, or `None` when it has none or it can't be read.
fn session(project: &Project) -> Option<Session> {
    let text = project
        .read_session()
        .inspect_err(|err| {
            tracing::warn!("can't read the session, so the workspace starts fresh: {err}");
        })
        .ok()??;
    Session::parse(&text)
        .inspect_err(|err| {
            tracing::warn!("the session isn't readable, so the workspace starts fresh: {err}");
        })
        .ok()
}
