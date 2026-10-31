//! A file's tab: its bytes, in hex, or in the editor for its format with a
//! switch above to flick back to hex. Each view keeps its own place. The
//! file itself is the [`Documents`]', so every tab on it shows the same.

mod editor;
mod hex;

use gui::ui::{Element, Length, Ui};
use serde::{Deserialize, Serialize};

use super::{Context, Selection};
use crate::components::{notice, segmented, toolbar};
use crate::documents::{Document, Documents};
use crate::keymap::AppCommand;
use editor::MessagesView;
use hex::HexState;

/// The switch's width, enough for both views' names.
const SWITCH: f32 = 180.;

pub use editor::Format;
pub use hex::{HEX, Hex};

pub struct OpenFile {
    /// Relative to the project's root.
    path: String,
    view: View,
    hex: Hex,
    messages: MessagesView,
    /// A decoded file encoded again for hex, once hex first shows it, with
    /// how many edits it had then.
    encoded: Option<(u32, Result<Vec<u8>, String>)>,
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
    /// Shows `path`, relative to the project's root, once it's read.
    pub fn new(path: String) -> Self {
        Self {
            path,
            view: View::Editor,
            hex: Hex::NEW,
            messages: MessagesView::default(),
            encoded: None,
        }
    }

    /// Where `state` left it, once it's read.
    pub(super) fn from_state(state: &OpenFileState) -> Self {
        Self {
            view: state.view,
            hex: Hex::from_state(&state.hex),
            ..Self::new(state.path.clone())
        }
    }

    pub(super) fn state(&self) -> OpenFileState {
        OpenFileState {
            path: self.path.clone(),
            view: self.view,
            hex: self.hex.state(),
        }
    }

    /// Relative to the project's root.
    pub(super) fn path(&self) -> &str {
        &self.path
    }

    /// The file's name.
    pub(super) fn title(&self) -> &str {
        file_name(&self.path)
    }

    /// Does `commands`, the [`HEX`] ones pressed while it has focus, once
    /// hex shows bytes.
    pub(super) fn act(&mut self, ui: &mut Ui, commands: &[AppCommand], documents: &Documents) {
        let shown = (documents.get(&self.path), self.view, &self.encoded);
        let ((Some(Document::Raw(bytes)), ..)
        | (Some(Document::Decoded(_)), View::Hex, Some((_, Ok(bytes))))) = shown
        else {
            return;
        };
        if !bytes.is_empty() {
            self.hex.act(ui, commands, bytes);
        }
    }

    pub(super) fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        let format = match cx.documents.open(&self.path, cx.project) {
            None | Some(Document::Loading(..)) => return notice(ui, "Loading…"),
            Some(Document::Failed(err)) => return notice(ui, err),
            Some(Document::Raw(bytes)) => return hex_or_empty(ui, &mut self.hex, bytes),
            Some(Document::Decoded(format)) => format,
        };
        let at = match self.view {
            View::Editor => 0,
            View::Hex => 1,
        };
        let switch = Element {
            size: [Length::Fixed(SWITCH), Length::Fit],
            ..Element::DEFAULT
        };
        let picked = toolbar(ui, |ui| {
            ui.element(switch, |ui| segmented(ui, &[format.name(), "Hex"], at))
        });
        match picked {
            Some(0) => self.view = View::Editor,
            Some(1) => self.view = View::Hex,
            _ => {}
        }
        match (self.view, &*format) {
            (View::Editor, Format::Messages(doc)) => {
                let picked = self.messages.ui(ui, doc);
                if let Some(item) = picked.and_then(|index| u32::try_from(index).ok()) {
                    let path = self.path.clone();
                    cx.select(Selection { path, item });
                }
            }
            (View::Hex, _) => {
                let edits = format.edits();
                let encoded = match &mut self.encoded {
                    Some((at, encoded)) if *at == edits => encoded,
                    slot => {
                        let encoded = format
                            .encode()
                            .inspect_err(|err| tracing::warn!("{}: {err}", self.path));
                        &mut slot.insert((edits, encoded)).1
                    }
                };
                match encoded {
                    Ok(bytes) => hex_or_empty(ui, &mut self.hex, bytes),
                    Err(err) => notice(ui, err),
                }
            }
        }
    }
}

fn hex_or_empty(ui: &mut Ui, hex: &mut Hex, bytes: &[u8]) {
    if bytes.is_empty() {
        notice(ui, "Empty file");
    } else {
        hex.ui(ui, bytes);
    }
}

pub(super) fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
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
        let file = OpenFile::from_state(&state());
        assert_eq!(file.title(), "a.bin");
        assert_eq!(file.state(), state());
    }
}
