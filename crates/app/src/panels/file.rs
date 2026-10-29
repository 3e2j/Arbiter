//! A file's tab: its bytes, in hex.

use std::io;

use gui::ui::Ui;
use serde::{Deserialize, Serialize};

use crate::components::{Hex, HexState, notice};
use crate::workspace::Places;

pub struct File {
    /// Relative to the project's root.
    path: String,
    bytes: Bytes,
    hex: Hex,
}

enum Bytes {
    /// Asked of the [`Loader`](crate::disk::Loader), not back yet.
    Loading,
    Read(Vec<u8>),
    /// Why it couldn't be read.
    Failed(String),
}

/// A file's tab as the session keeps it.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct FileState {
    /// Relative to the project's root.
    path: String,
    #[serde(default)]
    hex: HexState,
}

impl File {
    pub const PLACES: Places = Places::MAIN;

    /// Shows `path`, relative to the project's root, once it's
    /// [`Self::load`]ed.
    pub const fn new(path: String) -> Self {
        Self {
            path,
            bytes: Bytes::Loading,
            hex: Hex::NEW,
        }
    }

    /// Where `state` left it, once it's [`Self::load`]ed.
    pub fn from_state(state: FileState) -> Self {
        Self {
            hex: Hex::from_state(&state.hex),
            ..Self::new(state.path)
        }
    }

    pub fn state(&self) -> FileState {
        FileState {
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

    /// Shows what reading the file gave.
    pub fn load(&mut self, read: io::Result<Vec<u8>>) {
        self.hex.reload();
        self.bytes = match read {
            Ok(bytes) => Bytes::Read(bytes),
            Err(err) => Bytes::Failed(format!("Can't read {}: {err}", self.path)),
        };
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        match &self.bytes {
            Bytes::Loading => notice(ui, "Loading…"),
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
        let state = || FileState {
            path: "files/a.bin".to_owned(),
            hex: HexState::default(),
        };
        let file = File::from_state(state());
        assert_eq!(file.title(), "a.bin");
        assert_eq!(file.state(), state());
    }
}
