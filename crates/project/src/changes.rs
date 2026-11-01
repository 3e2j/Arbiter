//! `changes/`: what the mod changes, stored against `base/`.
//!
//! A `.patch` holds an edited file's deltas against its base file, with the
//! base's hash to tell when that has moved on:
//!
//! ```toml
//! base = "xxh3:…"
//!
//! [[delta]]
//! edit = "text"
//! message = 12
//! text = "Hello, {0:0}!"
//! ```

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use diag::{Diagnostic, DiagnosticCode, Diagnostics, Severity};
use formats::{Decode, FormatError, Patch};
use serde::{Deserialize, Serialize};

use crate::{
    base::{self, BaseError},
    disk,
    hash::Hash,
};

pub const DIR: &str = "changes";

pub const BASE_CHANGED: DiagnosticCode = DiagnosticCode {
    id: "changes/base-changed",
    severity: Severity::Warning,
    summary: "the base file has changed since the patch was saved",
};

#[derive(Debug, thiserror::Error)]
pub enum ChangeError {
    #[error(transparent)]
    Disk(#[from] BaseError),
    #[error("{path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error(transparent)]
    Format(#[from] FormatError),
}

#[derive(Serialize, Deserialize)]
struct PatchFile<D> {
    base: Hash,
    #[serde(rename = "delta")]
    deltas: Vec<D>,
}

/// Where the patch for the game file at `path` sits under `changes`.
#[must_use]
pub fn patch_path(changes: &Path, path: &str) -> PathBuf {
    changes.join(format!("{path}.patch"))
}

/// The base file's `bytes` decoded, then again with the patch at `patch`
/// applied if there is one. Deltas that don't fit, and a base that has
/// changed since the patch was saved, go to `diag`.
///
/// # Errors
///
/// If the base doesn't decode, or the patch can't be read.
pub fn open<T: Decode + Patch + Clone>(
    patch: &Path,
    bytes: &[u8],
    diag: &mut Diagnostics,
) -> Result<(T, T), ChangeError> {
    let base = T::decode(bytes, diag)?;
    let mut document = base.clone();
    let text = match fs::read_to_string(patch) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok((base, document)),
        text => text.map_err(disk::io_err(patch))?,
    };
    let file: PatchFile<T::Delta> = toml::from_str(&text).map_err(|source| ChangeError::Read {
        path: patch.to_path_buf(),
        source,
    })?;
    if file.base != Hash::of(bytes) {
        diag.push(Diagnostic::new(
            &BASE_CHANGED,
            "the base file has changed since this patch was saved, so its deltas may not land where they were made",
        ));
    }
    document.patch(&file.deltas, diag);
    Ok((base, document))
}

/// Writes what turns `base` into `edited` to `patch`, with `hash`, the base
/// file's, or removes it when they're the same.
///
/// # Errors
///
/// If the patch can't be written or removed.
pub fn save<T: Patch>(patch: &Path, hash: Hash, base: &T, edited: &T) -> Result<(), ChangeError> {
    let deltas = T::diff(base, edited);
    if deltas.is_empty() {
        return match fs::remove_file(patch) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => {
                Err(disk::io_err(patch)(err).into())
            }
            _ => Ok(()),
        };
    }
    let file = PatchFile { base: hash, deltas };
    if let Some(dir) = patch.parent() {
        fs::create_dir_all(dir).map_err(disk::io_err(dir))?;
    }
    disk::write_atomic(patch, base::to_toml(&file, patch)?.as_bytes())?;
    Ok(())
}

/// Applies every change in `changes` to the base tree at `base`, and reports
/// each one that doesn't apply cleanly.
pub fn check(_changes: &Path, _base: &Path, _diag: &mut Diagnostics) {
    // TODO: open each patch against the staged base, once paths know their format.
}

#[cfg(test)]
mod tests {
    use formats::{
        ByteWriter, Encode,
        jsystem::jmessage::bmg::{Bmg, Encoding, Message, MessageKey, TextSegment},
    };

    use super::*;

    fn bmg(text: &[u8]) -> Bmg {
        Bmg {
            encoding: Encoding::Windows1252,
            attributes_len: 4,
            mid1: None,
            messages: vec![Message {
                id: 0,
                key: MessageKey(0),
                attributes: Box::new([0; 4]),
                text: vec![TextSegment::Text(text.to_vec())],
            }],
            flow: None,
            strings: None,
        }
    }

    fn bytes(bmg: &Bmg) -> Vec<u8> {
        let mut out = ByteWriter::new();
        bmg.encode(&mut out).unwrap();
        out.finish()
    }

    #[test]
    fn a_saved_patch_opens_as_the_edit() {
        let dir = tempfile::tempdir().unwrap();
        let patch = patch_path(dir.path(), "res/Msgus/zel_00.bmg");
        let base = bmg(b"Hi");
        let edited = bmg(b"Hello");
        let bytes = bytes(&base);

        let hash = Hash::of(&bytes);
        save(&patch, hash, &base, &edited).unwrap();
        let text = fs::read_to_string(&patch).unwrap();
        assert!(text.contains("[[delta]]") && text.contains("text = \"Hello\""));
        assert!(!text.contains("Hi"));

        let mut diag = Diagnostics::default();
        let opened = open::<Bmg>(&patch, &bytes, &mut diag).unwrap();
        assert_eq!(opened, (base.clone(), edited));
        assert_eq!(diag.items, []);

        save(&patch, hash, &base, &base).unwrap();
        assert!(!patch.exists());
        let opened = open::<Bmg>(&patch, &bytes, &mut diag).unwrap();
        assert_eq!(opened, (base.clone(), base));
    }

    #[test]
    fn a_changed_base_is_reported_and_still_patched() {
        let dir = tempfile::tempdir().unwrap();
        let patch = patch_path(dir.path(), "zel_00.bmg");
        let base = bmg(b"Hi");
        save(&patch, Hash::of(&bytes(&base)), &base, &bmg(b"Hello")).unwrap();

        let mut diag = Diagnostics::default();
        let (_, opened) = open::<Bmg>(&patch, &bytes(&bmg(b"Hey")), &mut diag).unwrap();
        assert_eq!(opened, bmg(b"Hello"));
        assert_eq!(diag.items.len(), 1);
        assert_eq!(diag.items[0].code, &BASE_CHANGED);
    }
}
