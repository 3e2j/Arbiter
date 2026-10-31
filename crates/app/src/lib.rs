//! The editor: its panels, menus, settings and the open project. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod commands;
pub mod components;
pub mod edit;
mod fs;
pub mod keymap;
mod panels;
mod settings;
mod theme;
mod workspace;

use std::sync::Arc;

use components::ContextMenu;
use formats::games::tp::Version;
use fs::{FileLoader, TreeReader, Watch};
use gui::{
    host::{App, Startup, Wake, WindowStyle},
    input::Input,
    platform::window,
    ui::{Element, Ui, window::window_edges},
};
use keymap::{AppCommand, KeyContext};
use panels::{FileTree, OpenFile, Panel, PanelKind, route};
use project::{LOG, Project};
use settings::Settings;
use theme::ThemeFonts;
use workspace::{Session, Workspace, status_bar, title_bar};

/// How far in from the window's edges a drag resizes it, in logical pixels.
const RESIZE_EDGE: f32 = 4.;

/// What the View menu offers, in order.
const MENU_PANELS: [PanelKind; 3] = [PanelKind::FileTree, PanelKind::Output, PanelKind::Inspector];

/// What a project without a session opens, in tab order. The inspector
/// waits for a selection.
const STARTING_LAYOUT: [PanelKind; 2] = [PanelKind::FileTree, PanelKind::Output];

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Window(#[from] window::RunError),
}

/// Opens the editor in a window, with `project` open if given, and blocks
/// until it's closed. The [`LOG`] follows whichever project is open.
///
/// # Errors
///
/// When the editor can't start, or the window fails.
pub fn run(project: Option<Project>) -> Result<(), AppError> {
    window::run::<Editor>(project)?;
    Ok(())
}

/// The editor, as one [`App`].
struct Editor {
    workspace: Workspace,
    settings: Settings,
    view: ContextMenu,
    /// Kept for each theme the settings pick.
    fonts: ThemeFonts,
    /// The open one, whose session the workspace is saved to on close.
    project: Option<Arc<Project>>,
    /// The open project's, unless it couldn't be watched.
    watch: Option<Watch>,
    /// The open project's, unless its thread couldn't start.
    reader: Option<TreeReader>,
    /// Unless its thread couldn't start.
    loader: Option<FileLoader>,
    wake: Wake,
}

impl Editor {
    /// Logs into `project` from here on, in place of the one before.
    fn open(&mut self, project: Project) {
        if let Err(err) = LOG.attach(&project.root) {
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
        for files in self.file_trees() {
            files.close();
        }
        // The session's file tabs, which open before the project does.
        if let Some(loader) = &self.loader {
            for panel in self.workspace.panels_mut() {
                if let Panel::OpenFile(file) = panel {
                    let version = version(&project, file.path());
                    file.load(loader.ask(root.join(file.path())), version);
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
            .show(|panel| matches!(panel, Panel::OpenFile(file) if file.path() == path))
        {
            return;
        }
        let mut file = OpenFile::new(path.to_owned());
        file.load(loader.ask(project.root.join(path)), version(project, path));
        self.workspace
            .add(PanelKind::OpenFile.place(), Panel::OpenFile(file));
    }

    /// What the keys pressed this pass do: as text while a box has the
    /// keyboard, else in the focused panel, then anywhere.
    fn commands(&mut self, ui: &Ui) -> Vec<AppCommand> {
        if self.settings.recording() {
            return Vec::new();
        }
        let first = if ui.typing() {
            Some(KeyContext::TEXT)
        } else {
            self.workspace
                .focused()
                .and_then(|panel| panel.key_context())
        };
        // Asking GLOBAL twice finds nothing more.
        let contexts = [first.unwrap_or(KeyContext::GLOBAL), KeyContext::GLOBAL];
        keymap::commands(ui.input().keys(), &contexts).collect()
    }

    /// Adds a panel of `kind` at its place, or for an inspector, shows the
    /// unlocked one if there is one.
    fn add_panel(&mut self, kind: PanelKind) {
        if kind == PanelKind::Inspector && route::show_unlocked(&mut self.workspace) {
            return;
        }
        if let Some(panel) = Panel::open(kind, None) {
            self.workspace.add(kind.place(), panel);
        }
        // A new files panel has no tree until it's read.
        if kind == PanelKind::FileTree {
            self.read_trees();
        }
    }

    fn file_trees(&mut self) -> impl Iterator<Item = &mut FileTree> {
        self.workspace.panels_mut().filter_map(|panel| match panel {
            Panel::FileTree(files) => Some(files),
            _ => None,
        })
    }
}

impl App for Editor {
    const TITLE: &str = "Arbiter";
    const MIN_SIZE: [f32; 2] = [1024., 600.];
    const DECORATED: bool = false;
    type Args = Option<Project>;

    fn new(start: &mut Startup, project: Self::Args) -> Result<Self, Box<dyn std::error::Error>> {
        let settings = Settings::load();
        let fonts = assets::fonts(start.glyphs)?;
        assets::load_icons(start.glyphs)?;
        theme::set(theme::theme(settings.palette(), fonts));
        start.zoom = settings.zoom().factor();
        keymap::set(settings.keymap());
        let session = project
            .as_ref()
            .and_then(session)
            .unwrap_or_else(|| Session::of(&STARTING_LAYOUT.map(|kind| (kind, kind.place()))));
        let mut app = Self {
            workspace: Workspace::new(&session),
            settings,
            view: ContextMenu::CLOSED,
            fonts,
            project: None,
            watch: None,
            reader: None,
            loader: FileLoader::new(start.wake.clone())
                .inspect_err(|err| tracing::warn!("can't start reading files to open: {err}"))
                .ok(),
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
                // Each sorts its own, so each takes a copy but the last.
                let mut files = self.file_trees().peekable();
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
        self.settings.sync(ui);
        let commands = self.commands(ui);
        self.settings.act(&commands);
        if let Some(panel) = self.workspace.focused() {
            panel.act(ui, &commands);
        }
        ui.element(Element::column(), |ui| {
            title_bar::ui(ui, Self::TITLE, &mut self.settings, &mut self.view);
            // Settings covers everything under the title bar.
            ui.element(Element::column(), |ui| {
                self.workspace.ui(ui);
                let picked: Vec<_> = self
                    .workspace
                    .panels_mut()
                    .filter_map(|panel| match panel {
                        Panel::FileTree(files) => files.take_picked(),
                        _ => None,
                    })
                    .collect();
                for path in &picked {
                    self.open_file(path);
                }
                route::update(&mut self.workspace);
                status_bar::ui(ui, &mut self.workspace);
                self.settings.panel(ui);
            });
            if let Some(kind) = title_bar::view_menu(ui, &mut self.view, &MENU_PANELS) {
                self.add_panel(kind);
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

/// The TP version whose base `path` is in, relative to `project`'s root.
fn version(project: &Project, path: &str) -> Option<Version> {
    let (id, edition) = project.edition_of(path)?;
    Version::from_disc(id, edition.revision)
}
