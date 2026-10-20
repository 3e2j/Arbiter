//! A file's tab: its bytes, and the views that show them.
//!
//! Hex shows any file. A format with an editor opens in it instead, with a
//! switch above to flick back to hex. Each view keeps its own place.

use std::io;

use gui::ui::{Element, Length, Ui};
use project::Document;
use project::jsystem::jmessage::Bmg;
use serde::{Deserialize, Serialize};

use super::hex::{Hex, HexState};
use super::messages::Messages;
use crate::components::{notice, segmented, toolbar};
use crate::workspace::Places;

/// The switch's width, enough for both views' names.
const SWITCH: f32 = 180.;

pub struct File {
    /// Relative to the project's root.
    path: String,
    bytes: Bytes,
    view: View,
    hex: Hex,
    /// `None` until read, or for a file whose format has no editor.
    /// Boxed, as most files have none and it's the bulk of the tab.
    editor: Option<Box<Editor>>,
}

enum Bytes {
    /// Asked of the [`Loader`](crate::disk::Loader), not back yet.
    Loading,
    Read(Vec<u8>),
    /// Why it couldn't be read.
    Failed(String),
}

/// The editor for the file's format.
enum Editor {
    Messages(Messages),
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
pub struct FileState {
    /// Relative to the project's root.
    path: String,
    #[serde(default)]
    view: View,
    #[serde(default)]
    hex: HexState,
}

impl File {
    pub const KIND: &str = "file";
    pub const PLACES: Places = Places::MAIN;

    /// Shows `path`, relative to the project's root, once it's
    /// [`Self::load`]ed.
    pub const fn new(path: String) -> Self {
        Self {
            path,
            bytes: Bytes::Loading,
            view: View::Editor,
            hex: Hex::NEW,
            editor: None,
        }
    }

    /// In the view `state` left it in, once it's [`Self::load`]ed.
    pub fn from_state(state: FileState) -> Self {
        Self {
            view: state.view,
            hex: Hex::from_state(&state.hex),
            ..Self::new(state.path)
        }
    }

    pub fn state(&self) -> FileState {
        FileState {
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

    /// Shows what reading the file gave, with nothing selected.
    pub fn load(&mut self, read: io::Result<Vec<u8>>) {
        self.hex.reload();
        self.editor = None;
        self.bytes = match read {
            Ok(bytes) => {
                self.editor = Editor::open(&self.path, &bytes).map(Box::new);
                Bytes::Read(bytes)
            }
            Err(err) => Bytes::Failed(format!("Can't read {}: {err}", self.path)),
        };
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let bytes = match &self.bytes {
            Bytes::Loading => return notice(ui, "Loading…"),
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
        match (self.view, self.editor.as_deref_mut()) {
            (View::Editor, Some(Editor::Messages(messages))) => messages.ui(ui),
            _ if bytes.is_empty() => notice(ui, "Empty file"),
            _ => self.hex.ui(ui, bytes),
        }
    }
}

impl Editor {
    /// The editor for `bytes`' format, or `None` when it has none or they
    /// don't decode, which leaves them to hex.
    fn open(path: &str, bytes: &[u8]) -> Option<Self> {
        if !Document::<Bmg>::detect(bytes) {
            return None;
        }
        match Document::<Bmg>::decode(path.to_owned(), bytes) {
            Ok((doc, diag)) => {
                for item in &diag.items {
                    tracing::warn!("{path}: {}", item.message);
                }
                Some(Self::Messages(Messages::new(doc)))
            }
            Err(err) => {
                tracing::warn!("can't read {path} as a message file: {err}");
                None
            }
        }
    }

    const fn name(&self) -> &'static str {
        match self {
            Self::Messages(_) => "Messages",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn its_state_keeps_its_view() {
        let state = || FileState {
            path: "files/a.bin".to_owned(),
            view: View::Hex,
            hex: HexState::default(),
        };
        let file = File::from_state(state());
        assert_eq!(file.title(), "a.bin");
        assert_eq!(file.state(), state());
    }

    #[test]
    fn bytes_without_an_editor_stay_hex() {
        let mut file = File::new("a.bin".to_owned());
        file.load(Ok(b"RARC".to_vec()));
        assert!(file.editor.is_none());
        file.load(Ok(b"MESGbmg1 broken".to_vec()));
        assert!(file.editor.is_none());
    }
}
