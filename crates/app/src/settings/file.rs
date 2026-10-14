//! `settings.toml`: the user's settings, in the platform's folder for app
//! settings, shared by every project. Each page keeps its own table.
//!
//! - Linux and the BSDs: `$XDG_CONFIG_HOME/arbiter`, else `~/.config/arbiter`
//! - macOS: `~/Library/Application Support/Arbiter`
//! - Windows: `AppData\Roaming\Arbiter`
//!
//! Anything missing or unreadable reads as its default, one value at a time,
//! so a setting that's gone or renamed costs only itself.

use std::path::{Path, PathBuf};
use std::{fs, io};

use serde::{Deserialize, Deserializer, Serialize};

use super::appearance;

const NAME: &str = "settings.toml";

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    #[serde(deserialize_with = "lenient")]
    pub appearance: appearance::Saved,
}

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
    let text = match fs::read_to_string(path) {
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
pub fn write(path: &Path, saved: Saved) -> Result<(), String> {
    let text = toml::to_string(&saved).map_err(|err| err.to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, text).map_err(|err| format!("{}: {err}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|err| format!("{}: {err}", path.display()))
}

/// `T`, or its default with a warning when the value doesn't fit it.
pub fn lenient<'de, D: Deserializer<'de>, T: Deserialize<'de> + Default>(
    deserializer: D,
) -> Result<T, D::Error> {
    let value = toml::Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_else(|err| {
        tracing::warn!("a setting isn't readable, so it's back to its default: {err}");
        T::default()
    }))
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
        write(&path, saved).unwrap();
        assert_eq!(read(&path), saved);
    }

    #[test]
    fn a_value_that_doesnt_fit_reads_as_its_default_alone() {
        let text = "[appearance]\ntheme = \"gone\"\n";
        assert_eq!(toml::from_str::<Saved>(text).unwrap(), Saved::default());
        let unknown = "[appearance]\ntheme = \"twilight\"\nmissing = 1\n";
        let saved: Saved = toml::from_str(unknown).unwrap();
        assert_eq!(saved.appearance.theme, Palette::Twilight);
    }
}
