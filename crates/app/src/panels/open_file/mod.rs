//! A file's tab: its bytes, in hex, or in the editor for its format with a
//! switch above to flick back to hex. Each view keeps its own place.

mod editor;
mod hex;

use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};

use formats::games::tp::Version;
use gui::ui::{Element, Length, Ui};
use serde::{Deserialize, Serialize};

use crate::components::{notice, segmented, toolbar};
use crate::fs::Loaded;
use crate::keymap::AppCommand;
use crate::workspace::Places;
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
    /// Until [`OpenFile::load`] is given them to wait on.
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
    pub const PLACES: Places = Places::MAIN;

    /// Shows `path`, relative to the project's root, once it's
    /// [`Self::load`]ed.
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

    /// Where `state` left it, once it's [`Self::load`]ed.
    pub fn from_state(state: OpenFileState) -> Self {
        Self {
            view: state.view,
            hex: Hex::from_state(&state.hex),
            ..Self::new(state.path)
        }
    }

    pub fn state(&self) -> OpenFileState {
        OpenFileState {
            path: self.path.clone(),
            view: self.view,
            hex: self.hex.state(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// The file's name.
    pub fn title(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Shows the bytes `loading` brings, once they come, their values named
    /// by `version`'s tables when given.
    pub fn load(&mut self, loading: Receiver<Loaded>, version: Option<Version>) {
        self.bytes = Bytes::Loading(loading);
        self.version = version;
    }

    /// Takes the bytes it's waiting on, if they came.
    fn poll(&mut self) {
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

    /// Does `commands`, the [`HEX`] ones pressed while it has
    /// focus, once there are bytes to and hex shows them.
    pub fn act(&mut self, ui: &mut Ui, commands: &[AppCommand]) {
        if let Bytes::Read(bytes) = &self.bytes
            && !bytes.is_empty()
            && self.shows_hex()
        {
            self.hex.act(ui, commands, bytes);
        }
    }

    const fn shows_hex(&self) -> bool {
        matches!(self.view, View::Hex) || self.editor.is_none()
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.poll();
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
            (View::Editor, Some(editor)) => editor.ui(ui),
            _ if bytes.is_empty() => notice(ui, "Empty file"),
            _ => self.hex.ui(ui, bytes),
        }
    }
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
