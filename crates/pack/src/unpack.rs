//! A disc's files written out under a directory, at the same relative paths
//! Dusklight's `overlay/` uses.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

use crate::disc::{self, Disc};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Disc(#[from] disc::Error),
    #[error("the disc names a file {0:?}, which isn't a plain relative path")]
    Path(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug)]
pub struct Unpacked {
    pub id: String,
    pub revision: u8,
    pub files: usize,
    pub bytes: u64,
}

/// Every path is checked before anything is written, so a hostile disc can't
/// write outside `out` or leave half a tree behind.
///
/// # Errors
///
/// If the disc can't be read, names a path that isn't plain and relative, or
/// a file can't be written.
pub fn unpack(disc: &Path, out: &Path) -> Result<Unpacked, Error> {
    let mut disc = Disc::open(disc)?;
    if let Some(file) = disc.files.iter().find(|f| !is_plain(&f.path)) {
        return Err(Error::Path(file.path.clone()));
    }

    let mut bytes = 0;
    let mut made_dir = None;
    for file in &disc.files {
        let dest = out.join(&file.path);
        let io_err = |source| Error::Io {
            path: dest.clone(),
            source,
        };
        // Files come in file system order, so siblings share a parent.
        let parent = dest.parent().unwrap_or(out);
        if made_dir.as_deref() != Some(parent) {
            fs::create_dir_all(parent).map_err(io_err)?;
            made_dir = Some(parent.to_path_buf());
        }
        let mut reader = disc.reader.open(file).map_err(io_err)?;
        let mut writer = fs::File::create(&dest).map_err(io_err)?;
        bytes += io::copy(&mut reader, &mut writer).map_err(io_err)?;
    }

    Ok(Unpacked {
        id: disc.id,
        revision: disc.revision,
        files: disc.files.len(),
        bytes,
    })
}

// Here to avoid any discs from writing where they shouldn't be.
fn is_plain(path: &str) -> bool {
    let path = Path::new(path);
    path.components().next().is_some()
        && path.components().all(|c| matches!(c, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_relative_paths() {
        assert!(is_plain("res/Msgus/bmgres.arc"));
        assert!(!is_plain(""));
        assert!(!is_plain("/etc/passwd"));
        assert!(!is_plain("res/../../x"));
        assert!(!is_plain("./res"));
    }
}
