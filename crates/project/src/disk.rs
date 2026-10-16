//! File system writes shared across the project: read-only trees and atomic files.

use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
};

use crate::base::BaseError;

pub(crate) fn write_readonly(path: &Path, bytes: &[u8]) -> Result<(), BaseError> {
    let err = io_err(path);
    let mut file = File::create(path).map_err(err)?;
    file.write_all(bytes).map_err(io_err(path))?;
    let mut perms = file.metadata().map_err(io_err(path))?.permissions();
    perms.set_readonly(true);
    file.set_permissions(perms).map_err(io_err(path))
}

/// Removes a tree of read-only files. A missing tree is fine.
pub(crate) fn remove_tree(path: &Path) -> Result<(), BaseError> {
    // Unix only needs the directories writable. Windows refuses to delete a
    // read-only file, so the flag comes off first.
    #[cfg(windows)]
    clear_readonly(path)?;
    match fs::remove_dir_all(path) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(io_err(path)(err)),
        _ => Ok(()),
    }
}

#[cfg(windows)]
fn clear_readonly(path: &Path) -> Result<(), BaseError> {
    let entries = match fs::read_dir(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        entries => entries.map_err(io_err(path))?,
    };
    for entry in entries {
        let path = entry.map_err(io_err(path))?.path();
        let meta = fs::symlink_metadata(&path).map_err(io_err(&path))?;
        if meta.is_dir() {
            clear_readonly(&path)?;
        } else {
            let mut perms = meta.permissions();
            perms.set_readonly(false);
            fs::set_permissions(&path, perms).map_err(io_err(&path))?;
        }
    }
    Ok(())
}

/// Written beside its destination and renamed over it, so the path never
/// holds half a file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BaseError> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(io_err(path))
}

pub(crate) fn io_err(path: &Path) -> impl FnOnce(io::Error) -> BaseError + '_ {
    |source| BaseError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_files_are_readonly_and_removable() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("GZ2E01");
        fs::create_dir_all(tree.join("res")).unwrap();
        let file = tree.join("res/al.bmd");
        write_readonly(&file, b"al.bmd").unwrap();

        assert!(fs::metadata(&file).unwrap().permissions().readonly());
        assert!(fs::write(&file, b"edited").is_err());

        remove_tree(&tree).unwrap();
        assert!(!tree.exists());
        remove_tree(&tree).unwrap();
    }
}
