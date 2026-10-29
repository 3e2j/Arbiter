//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod commands;
pub mod components;
mod disk;
pub mod edit;
pub mod keymap;
mod panels;
mod settings;
mod theme;
mod views;
mod workspace;

use std::sync::Arc;

use components::ContextMenu;
use disk::{Loader, TreeReader, Watch};
use formats::jsystem::jmessage::games::tables;
use gui::{
    host::{App, Startup, Wake, WindowStyle},
    input::Input,
    platform::window,
    ui::{Element, Ui, window::window_edges},
};
pub use panels::Capture;
use panels::{File, Files, Inspector, Output, Panel, PanelKind};
use project::{Log, Project};
use settings::Settings;
use theme::ThemeFonts;
use workspace::{Place, Session, Workspace, status_bar, title_bar};

/// How far in from the window's edges a drag resizes it, in logical pixels.
const RESIZE_EDGE: f32 = 4.;

/// Each panel's kind and place in a project without a session, in tab order.
/// The View menu offers them, and adds another where it sits here.
/// Where a live inspector opens beside pinned ones, as in [`LAYOUT`].
const INSPECTOR_PLACE: Place = Place::RightInnerTop;

const LAYOUT: [(PanelKind, Place); 3] = [
    (PanelKind::Files, Place::LeftInnerTop),
    (PanelKind::Output, Place::BelowMain),
    (PanelKind::Inspector, INSPECTOR_PLACE),
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
    settings: Settings,
    view: ContextMenu,
    /// Kept for each theme the settings pick.
    fonts: ThemeFonts,
    log: Arc<Log>,
    /// The open one, whose session the workspace is saved to on close.
    project: Option<Arc<Project>>,
    /// The open project's, unless it couldn't be watched.
    watch: Option<Watch>,
    /// The open project's, unless its thread couldn't start.
    reader: Option<TreeReader>,
    /// Unless its thread couldn't start.
    loader: Option<Loader>,
    wake: Wake,
    /// The active document's path and the key of what's selected in it, as
    /// of last pass.
    selection: Option<(String, usize)>,
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
        for files in self.files() {
            files.close();
        }
        // The session's file tabs, which open before the project does.
        if let Some(loader) = &self.loader {
            for panel in self.workspace.panels_mut() {
                if let Panel::File(file) = panel {
                    loader.ask(root.join(file.path()));
                }
            }
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

    /// Shows the file at `path`, relative to the open project's root, in a
    /// tab of the main view, the one it's already in if it's open.
    fn open_file(&mut self, path: &str) {
        let (Some(project), Some(loader)) = (&self.project, &self.loader) else {
            return;
        };
        if self
            .workspace
            .show(|panel| matches!(panel, Panel::File(file) if file.path() == path))
        {
            return;
        }
        loader.ask(project.root.join(path));
        self.workspace
            .add(Place::Main, Panel::File(File::new(path.to_owned())));
    }

    /// Settles which inspector is live, then has it show the selection.
    ///
    /// A new selection while every inspector is pinned opens a live one
    /// beside them. Claiming inspectors close the live one, the first taking
    /// its place and the rest closing too.
    fn inspect(&mut self) {
        let selection = self.workspace.document().and_then(Panel::selection);
        let changed = self
            .selection
            .as_ref()
            .map(|(path, key)| (path.as_str(), *key))
            != selection;
        if changed {
            self.selection = selection.map(|(path, key)| (path.to_owned(), key));
        }
        let mut pinned = false;
        let mut unpinned = false;
        let mut claiming = false;
        for inspector in self.inspectors() {
            pinned |= inspector.is_pinned();
            unpinned |= !inspector.is_pinned();
            claiming |= inspector.is_claiming();
        }
        if changed && self.selection.is_some() && pinned && !unpinned {
            self.workspace
                .add(INSPECTOR_PLACE, Panel::Inspector(Inspector::new()));
            claiming = true;
        }
        if claiming {
            let mut kept = false;
            self.workspace.close(|panel| match panel {
                Panel::Inspector(inspector) if inspector.is_live() => true,
                Panel::Inspector(inspector) if inspector.is_claiming() => {
                    std::mem::replace(&mut kept, true)
                }
                _ => false,
            });
            if let Some(inspector) = self.inspectors().find(|inspector| inspector.is_claiming()) {
                inspector.go_live();
            }
        }
        let selection = self
            .selection
            .as_ref()
            .map(|(path, key)| (path.as_str(), *key));
        let stale = self.workspace.panels().any(|panel| match panel {
            Panel::Inspector(inspector) => inspector.is_live() && !inspector.shows(selection),
            _ => false,
        });
        if !stale {
            return;
        }
        let shown = selection.and_then(|(path, key)| {
            let inspection = self.workspace.document()?.inspect(key)?;
            Some((path, key, inspection))
        });
        let live = self.workspace.panels_mut().find_map(|panel| match panel {
            Panel::Inspector(inspector) if inspector.is_live() => Some(inspector),
            _ => None,
        });
        if let Some(live) = live {
            live.show(shown);
        }
    }

    fn inspectors(&mut self) -> impl Iterator<Item = &mut Inspector> {
        self.workspace.panels_mut().filter_map(|panel| match panel {
            Panel::Inspector(inspector) => Some(inspector),
            _ => None,
        })
    }

    fn files(&mut self) -> impl Iterator<Item = &mut Files> {
        self.workspace.panels_mut().filter_map(|panel| match panel {
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
        let settings = Settings::load();
        let fonts = assets::fonts(start.glyphs)?;
        assets::load_icons(start.glyphs)?;
        theme::set(theme::theme(settings.palette(), fonts));
        start.zoom = settings.zoom().factor();
        keymap::set(settings.keymap());
        let session = project
            .as_ref()
            .and_then(session)
            .unwrap_or_else(|| Session::of(&LAYOUT));
        let mut app = Self {
            workspace: Workspace::new(&session),
            settings,
            view: ContextMenu::CLOSED,
            fonts,
            log,
            project: None,
            watch: None,
            reader: None,
            loader: Loader::new(start.wake.clone())
                .inspect_err(|err| tracing::warn!("can't start reading files to open: {err}"))
                .ok(),
            wake: start.wake.clone(),
            selection: None,
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
                // Each sorts its own, so each takes a copy but the last.
                let mut files = self.files().peekable();
                while let Some(panel) = files.next() {
                    if files.peek().is_none() {
                        panel.refresh(trees);
                        break;
                    }
                    panel.refresh(trees.clone());
                }
            }
            Some(Err(err)) => tracing::warn!("can't read the project's files: {err}"),
            None => {}
        }
        let project = self.project.as_deref();
        for (path, read) in self.loader.iter().flat_map(Loader::take) {
            let file = self.workspace.panels_mut().find_map(|panel| match panel {
                Panel::File(file) if project.is_some_and(|p| p.root.join(file.path()) == path) => {
                    Some(file)
                }
                _ => None,
            });
            // Closed before it was read.
            if let Some(file) = file {
                let edition = project.and_then(|project| project.edition_of(file.path()));
                let tables = edition.and_then(|(id, edition)| tables(id, edition.revision));
                file.load(read, tables);
            }
        }
        self.workspace.input(input);
        if let Some(palette) = self.settings.take_changed() {
            theme::set(theme::theme(palette, self.fonts));
        }
    }

    fn window_style(&self) -> WindowStyle {
        let theme = theme::current();
        WindowStyle {
            background: theme.color.page,
            border: theme.border(),
            radius: theme.size.window_radius,
        }
    }

    fn take_zoom(&mut self) -> Option<f32> {
        self.settings.take_zoom().map(settings::Zoom::factor)
    }

    fn on_close(&mut self) {
        self.settings.save();
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
            title_bar::ui(ui, Self::TITLE, &mut self.settings, &mut self.view);
            // Settings covers everything under the title bar.
            ui.element(Element::column(), |ui| {
                Output::deliver(self.workspace.panels_mut().filter_map(|panel| match panel {
                    Panel::Output(output) => Some(output),
                    _ => None,
                }));
                self.inspect();
                self.workspace.ui(ui);
                let picked: Vec<_> = self
                    .workspace
                    .panels_mut()
                    .filter_map(|panel| match panel {
                        Panel::Files(files) => files.take_picked(),
                        _ => None,
                    })
                    .collect();
                for path in &picked {
                    self.open_file(path);
                }
                status_bar::ui(ui, &mut self.workspace);
                self.settings.panel(ui);
            });
            let added = title_bar::view_menu(ui, &mut self.view, &mut self.workspace, &LAYOUT);
            if let Some(kind) = added {
                // A new files panel has no tree until it's read.
                if kind == PanelKind::Files {
                    self.read_trees();
                }
            }
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
