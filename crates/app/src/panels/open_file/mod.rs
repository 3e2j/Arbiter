//! A file's tab: its bytes, in hex.

mod hex;

use std::io;
use std::sync::mpsc::{Receiver, TryRecvError};

use gui::ui::Ui;
use serde::{Deserialize, Serialize};

use crate::components::notice;
use crate::fs::Loaded;
use crate::keymap::AppCommand;
use crate::workspace::Places;
use hex::HexState;

pub use hex::{HEX, Hex};

pub struct OpenFile {
    /// Relative to the project's root.
    path: String,
    bytes: Bytes,
    hex: Hex,
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

/// A file's tab as the session keeps it.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct OpenFileState {
    /// Relative to the project's root.
    path: String,
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
            hex: Hex::NEW,
        }
    }

    /// Where `state` left it, once it's [`Self::load`]ed.
    pub fn from_state(state: OpenFileState) -> Self {
        Self {
            hex: Hex::from_state(&state.hex),
            ..Self::new(state.path)
        }
    }

    pub fn state(&self) -> OpenFileState {
        OpenFileState {
            path: self.path.clone(),
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

    /// Shows the bytes `loading` brings, once they come.
    pub fn load(&mut self, loading: Receiver<Loaded>) {
        self.bytes = Bytes::Loading(loading);
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
        self.bytes = match read {
            Ok(bytes) => Bytes::Read(bytes),
            Err(err) => Bytes::Failed(format!("Can't read {}: {err}", self.path)),
        };
    }

    /// Does `commands`, the [`HEX`] ones pressed while it has
    /// focus, once there are bytes to.
    pub fn act(&mut self, ui: &mut Ui, commands: &[AppCommand]) {
        if let Bytes::Read(bytes) = &self.bytes
            && !bytes.is_empty()
        {
            self.hex.act(ui, commands, bytes);
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.poll();
        match &self.bytes {
            Bytes::Unasked | Bytes::Loading(_) => notice(ui, "Loading…"),
            Bytes::Failed(err) => notice(ui, err),
            Bytes::Read(bytes) if bytes.is_empty() => notice(ui, "Empty file"),
            Bytes::Read(bytes) => self.hex.ui(ui, bytes),
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
            hex: HexState::default(),
        };
        let file = OpenFile::from_state(state());
        assert_eq!(file.title(), "a.bin");
        assert_eq!(file.state(), state());
    }
}
