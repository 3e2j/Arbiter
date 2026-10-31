//! A file's tab: its bytes, in hex, or in the editor for its format with a
//! switch above to flick back to hex. Each view keeps its own place.

mod editor;
mod hex;

use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};

use formats::games::tp::Version;
use gui::ui::{Element, Length, Ui};
use project::Project;
use serde::{Deserialize, Serialize};

use super::{Context, Event, Panel, PanelDesc, Reply};
use crate::components::{notice, segmented, toolbar};
use crate::fs::Loaded;
use crate::keymap::AppCommand;
use crate::workspace::{Place, Places};
use editor::Editor;
use hex::HexState;

/// The switch's width, enough for both views' names.
const SWITCH: f32 = 180.;

pub use hex::{HEX, Hex};

pub struct OpenFile {
    /// Relative to the project's root.
    path: String,
    bytes: Bytes,
    /// Names what the file's values mean, when it's in a TP edition's base.
    version: Option<Version>,
    view: View,
    hex: Hex,
    /// `None` until read, or for a format without one.
    editor: Option<Editor>,
}

enum Bytes {
    /// Until its `ui` has a project and loader to ask.
    Unasked,
    /// Asked of the [`FileLoader`](crate::fs::FileLoader), not back yet.
    Loading(Receiver<Loaded>),
    Read(Vec<u8>),
    /// Why it couldn't be read.
    Failed(String),
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum View {
    /// The format's editor, or hex for a format without one.
    #[default]
    Editor,
    Hex,
}

/// A file's tab as the session keeps it.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct OpenFileState {
    /// Relative to the project's root.
    path: String,
    #[serde(default)]
    view: View,
    #[serde(default)]
    hex: HexState,
}

impl OpenFile {
    pub const DESC: PanelDesc = PanelDesc {
        id: "open-file",
        title: "File",
        opens_at: Place::Main,
        places: Places::MAIN,
        key_context: Some(HEX),
        in_menu: false,
        open: |state| {
            if state.is_none() {
                tracing::warn!("the session has a file tab without its file");
            }
            Some(Box::new(Self::from_state(super::read("File", state)?)))
        },
    };

    /// Shows `path`, relative to the project's root, once it's read.
    pub const fn new(path: String) -> Self {
        Self {
            path,
            bytes: Bytes::Unasked,
            version: None,
            view: View::Editor,
            hex: Hex::NEW,
            editor: None,
        }
    }

    /// Where `state` left it, once it's read.
    fn from_state(state: OpenFileState) -> Self {
        Self {
            view: state.view,
            hex: Hex::from_state(&state.hex),
            ..Self::new(state.path)
        }
    }

    fn state(&self) -> OpenFileState {
        OpenFileState {
            path: self.path.clone(),
            view: self.view,
            hex: self.hex.state(),
        }
    }

    /// Asks for its bytes once there's a project to read them from, then
    /// takes them once they come.
    fn poll(&mut self, cx: &Context) {
        if let (Bytes::Unasked, Some(project), Some(loader)) = (&self.bytes, cx.project, cx.loader)
        {
            self.bytes = Bytes::Loading(loader.ask(project.root.join(&self.path)));
            self.version = version(project, &self.path);
        }
        let Bytes::Loading(loading) = &self.bytes else {
            return;
        };
        let read = match loading.try_recv() {
            Ok(read) => read,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(io::Error::other("the loader stopped")),
        };
        self.hex.reload();
        self.editor = None;
        self.bytes = match read {
            Ok(bytes) => {
                self.editor = Editor::open(&self.path, &bytes, self.version);
                Bytes::Read(bytes)
            }
            Err(err) => Bytes::Failed(format!("Can't read {}: {err}", self.path)),
        };
    }

    const fn shows_hex(&self) -> bool {
        matches!(self.view, View::Hex) || self.editor.is_none()
    }
}

impl Panel for OpenFile {
    fn desc(&self) -> &'static PanelDesc {
        &Self::DESC
    }

    /// The file's name.
    fn title(&self) -> &str {
        file_name(&self.path)
    }

    fn saved(&self) -> Option<toml::Value> {
        super::save(&self.state(), &self.path)
    }

    /// Shows itself for a pick of its own file.
    fn receive(&mut self, event: &mut Event) -> Reply {
        match event {
            Event::Pick(path) if *path == self.path => Reply::Show,
            _ => Reply::Pass,
        }
    }

    /// Does `commands`, the [`HEX`] ones pressed while it has
    /// focus, once there are bytes to and hex shows them.
    fn act(&mut self, ui: &mut Ui, commands: &[AppCommand]) {
        if let Bytes::Read(bytes) = &self.bytes
            && !bytes.is_empty()
            && self.shows_hex()
        {
            self.hex.act(ui, commands, bytes);
        }
    }

    fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        self.poll(cx);
        let bytes = match &self.bytes {
            Bytes::Unasked | Bytes::Loading(_) => return notice(ui, "Loading…"),
            Bytes::Failed(err) => return notice(ui, err),
            Bytes::Read(bytes) => bytes,
        };
        if let Some(editor) = &self.editor {
            let at = match self.view {
                View::Editor => 0,
                View::Hex => 1,
            };
            let switch = Element {
                size: [Length::Fixed(SWITCH), Length::Fit],
                ..Element::DEFAULT
            };
            let picked = toolbar(ui, |ui| {
                ui.element(switch, |ui| segmented(ui, &[editor.name(), "Hex"], at))
            });
            match picked {
                Some(0) => self.view = View::Editor,
                Some(1) => self.view = View::Hex,
                _ => {}
            }
        }
        match (self.view, &mut self.editor) {
            (View::Editor, Some(editor)) => {
                editor.ui(ui);
                if let Some(page) = editor.take_inspected(file_name(&self.path)) {
                    cx.post(Event::Inspect(page));
                }
            }
            _ if bytes.is_empty() => notice(ui, "Empty file"),
            _ => self.hex.ui(ui, bytes),
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The TP version whose base `path` is in, relative to `project`'s root.
fn version(project: &Project, path: &str) -> Option<Version> {
    let (id, edition) = project.edition_of(path)?;
    Version::from_disc(id, edition.revision)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_state_keeps_its_view() {
        let state = || OpenFileState {
            path: "files/a.bin".to_owned(),
            view: View::Hex,
            hex: HexState::default(),
        };
        let file = OpenFile::from_state(state());
        assert_eq!(file.title(), "a.bin");
        assert_eq!(file.state(), state());
    }
}
