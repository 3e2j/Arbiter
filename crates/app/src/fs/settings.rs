//! `settings.toml`, in the platform's folder for app settings, shared by
//! every project.
//!
//! - Linux and the BSDs: `$XDG_CONFIG_HOME/arbiter`, else `~/.config/arbiter`
//! - macOS: `~/Library/Application Support/Arbiter`
//! - Windows: `AppData\Roaming\Arbiter`

use std::io;
use std::path::{Path, PathBuf};

use crate::settings::Saved;

const NAME: &str = "settings.toml";

/// Where the file is. `None` when the platform's folder can't be found, so
/// nothing is kept.
pub fn path() -> Option<PathBuf> {
    let app = if cfg!(any(windows, target_os = "macos")) {
        "Arbiter"
    } else {
        "arbiter"
    };
    Some(dirs::config_dir()?.join(app).join(NAME))
}

/// The settings at `path`, or the defaults when there are none yet.
pub fn read(path: &Path) -> Saved {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Saved::default(),
        Err(err) => {
            tracing::warn!("can't read the settings, so they start fresh: {err}");
            return Saved::default();
        }
    };
    toml::from_str(&text).unwrap_or_else(|err| {
        tracing::warn!("the settings aren't readable, so they start fresh: {err}");
        Saved::default()
    })
}

/// Replaces the file at `path` with `saved`, never leaving half of it.
///
/// # Errors
///
/// If its folder can't be made, or the file can't be written.
pub fn write(path: &Path, saved: &Saved) -> Result<(), String> {
    let text = toml::to_string(saved).map_err(|err| err.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text).map_err(|err| format!("{}: {err}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|err| format!("{}: {err}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Palette;

    #[test]
    fn settings_read_back_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("arbiter").join(NAME);
        assert_eq!(read(&path), Saved::default());
        let mut saved = Saved::default();
        saved.appearance.theme = Palette::Twilight;
        write(&path, &saved).unwrap();
        assert_eq!(read(&path), saved);
    }
}
