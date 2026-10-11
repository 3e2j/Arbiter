//! Copy and paste, through the system's clipboard so other apps share it.
//!
//! What the app copies can carry its own data alongside the text, such as the
//! files behind a list of paths. Other apps only see the text, and a paste
//! only gets the data back while the clipboard still holds that text, so
//! anything copied since in another app wins.
//!
//! On Linux and the BSDs there's also the primary selection: whatever was
//! last selected, in any app, which a middle click pastes.

use std::any::Any;

/// Owned by the host, and reached through [`Ui::clipboard`](crate::ui::Ui).
pub struct Clipboard {
    /// `None` without one, such as in tests, where it only holds `own` and
    /// `own_primary`.
    system: Option<arboard::Clipboard>,
    own: Option<Own>,
    own_primary: Option<String>,
    /// Whether a middle click pastes the primary selection, where there is
    /// one. On by default. The desktop's own setting isn't read, though a
    /// compositor that withholds the primary selection leaves nothing to
    /// paste.
    // TODO: from the user's Settings.
    pub middle_click_paste: bool,
}

/// Whether the platform has a primary selection. A compositor can still
/// withhold it, and then reading it gives nothing.
const PRIMARY: bool = cfg!(any(target_os = "linux", target_os = "freebsd"));

/// What this app copied last.
struct Own {
    text: String,
    data: Option<Box<dyn Any>>,
}

impl Clipboard {
    /// The system's clipboard, or one only this app sees when it can't be
    /// reached.
    #[must_use]
    pub fn system() -> Self {
        Self {
            system: arboard::Clipboard::new().ok(),
            ..Self::default()
        }
    }

    /// Whether a middle click should paste [`Self::primary`].
    #[must_use]
    pub const fn pastes_primary(&self) -> bool {
        PRIMARY && self.middle_click_paste
    }

    /// Offers `text` as the primary selection, for when the selection changes.
    /// Does nothing where there's no primary selection.
    pub fn select(&mut self, text: &str) {
        if !PRIMARY {
            return;
        }
        match &mut self.system {
            Some(system) => primary::set(system, text),
            None => self.own_primary = Some(text.to_owned()),
        }
    }

    /// The primary selection, `None` where there isn't one.
    pub fn primary(&mut self) -> Option<String> {
        if !PRIMARY {
            return None;
        }
        match &mut self.system {
            Some(system) => primary::get(system),
            None => self.own_primary.clone(),
        }
    }

    pub fn copy(&mut self, text: &str) {
        self.put(text, None);
    }

    /// Copies `text` for other apps, and `data` for a paste in this one.
    pub fn copy_with<T: Any>(&mut self, text: &str, data: T) {
        self.put(text, Some(Box::new(data)));
    }

    fn put(&mut self, text: &str, data: Option<Box<dyn Any>>) {
        // A system that refuses still leaves `own`, so a paste in this app
        // works.
        if let Some(system) = &mut self.system {
            system.set_text(text).ok();
        }
        self.own = Some(Own {
            text: text.to_owned(),
            data,
        });
    }

    /// `None` when it's empty or holds something other than text.
    pub fn text(&mut self) -> Option<String> {
        match &mut self.system {
            Some(system) => system.get_text().ok(),
            None => self.own.as_ref().map(|own| own.text.clone()),
        }
    }

    /// What [`Self::copy_with`] copied, while the clipboard still holds its
    /// text and it's a `T`.
    pub fn data<T: Any>(&mut self) -> Option<&T> {
        let text = self.text()?;
        let own = self.own.as_ref().filter(|own| own.text == text)?;
        own.data.as_ref()?.downcast_ref()
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            system: None,
            own: None,
            own_primary: None,
            middle_click_paste: true,
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod primary {
    use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

    pub fn set(system: &mut arboard::Clipboard, text: &str) {
        let set = system.set().clipboard(LinuxClipboardKind::Primary);
        set.text(text).ok();
    }

    pub fn get(system: &mut arboard::Clipboard) -> Option<String> {
        let get = system.get().clipboard(LinuxClipboardKind::Primary);
        get.text().ok()
    }
}

/// Never reached, as [`PRIMARY`] is false.
#[cfg(not(any(target_os = "linux", target_os = "freebsd")))]
mod primary {
    pub fn set(_: &mut arboard::Clipboard, _: &str) {}

    pub fn get(_: &mut arboard::Clipboard) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_comes_back_with_its_text() {
        let mut clipboard = Clipboard::default();
        assert_eq!(clipboard.text(), None);
        clipboard.copy_with("a.bmg", 7_u32);
        assert_eq!(clipboard.text().as_deref(), Some("a.bmg"));
        assert_eq!(clipboard.data::<u32>(), Some(&7));
        assert_eq!(clipboard.data::<i64>(), None);
        clipboard.copy("plain");
        assert_eq!(clipboard.data::<u32>(), None);
    }
}
