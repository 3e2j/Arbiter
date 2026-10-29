//! The user's settings as `settings.toml` keeps them. Each page keeps its
//! own table.
//!
//! Anything missing or unreadable reads as its default, one value at a time,
//! so a setting that's gone or renamed costs only itself.

use serde::{Deserialize, Deserializer, Serialize};

use super::{appearance, editor, keybinds};

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug, Default)]
#[serde(default)]
pub struct Saved {
    #[serde(deserialize_with = "lenient")]
    pub appearance: appearance::Saved,
    #[serde(deserialize_with = "lenient")]
    pub editor: editor::Saved,
    #[serde(deserialize_with = "lenient")]
    pub keybinds: keybinds::Saved,
}

/// A switch that's on until it's turned off, which a plain `bool` can't
/// default to.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(transparent)]
pub struct OnByDefault(pub bool);

impl Default for OnByDefault {
    fn default() -> Self {
        Self(true)
    }
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
    use crate::settings::Zoom;
    use crate::theme::Palette;

    #[test]
    fn a_value_that_doesnt_fit_reads_as_its_default_alone() {
        let text = "[appearance]\ntheme = \"gone\"\n";
        assert_eq!(toml::from_str::<Saved>(text).unwrap(), Saved::default());
        let unknown = "[appearance]\ntheme = \"twilight\"\nmissing = 1\n";
        let saved: Saved = toml::from_str(unknown).unwrap();
        assert_eq!(saved.appearance.theme, Palette::Twilight);
    }

    #[test]
    fn a_switch_on_by_default_stays_on_when_missing_or_unreadable() {
        let saved: Saved = toml::from_str("").unwrap();
        assert!(saved.editor.middle_click_paste.0);
        let off: Saved = toml::from_str("[editor]\nmiddle_click_paste = false\n").unwrap();
        assert!(!off.editor.middle_click_paste.0);
        let bad: Saved = toml::from_str("[editor]\nmiddle_click_paste = 3\n").unwrap();
        assert!(bad.editor.middle_click_paste.0);
    }

    #[test]
    fn zoom_reads_as_its_percent_and_off_levels_as_the_default() {
        let saved: Saved = toml::from_str("[appearance]\nzoom = 125\n").unwrap();
        assert_eq!(saved.appearance.zoom, Zoom::P125);
        let off = "[appearance]\ntheme = \"twilight\"\nzoom = 130\n";
        let saved: Saved = toml::from_str(off).unwrap();
        assert_eq!(saved.appearance.zoom, Zoom::default());
        assert_eq!(saved.appearance.theme, Palette::Twilight);
    }
}
