//! Every format the app can edit, each decoded once and drawn by the module
//! of its own. The only place that lists them: a new format is a variant here
//! and a module of its own.

mod messages;

use formats::games::tp::Version;
use gui::ui::Ui;

pub use messages::{MessagesDoc, MessagesView};

pub enum Format {
    Messages(Box<MessagesDoc>),
}

impl Format {
    /// `bytes` decoded, their values named by `version`'s tables when given.
    /// `None` when no format claims them or they don't decode, which leaves
    /// them to hex.
    pub fn open(path: &str, bytes: &[u8], version: Option<Version>) -> Option<Self> {
        MessagesDoc::open(path, bytes, version).map(|doc| Self::Messages(Box::new(doc)))
    }

    /// Its editor's name on the switch.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Messages(_) => "Messages",
        }
    }

    /// The name of its `item`, as a [`Selection`](crate::panels::Selection)
    /// holds it, onto `out`.
    pub fn item_name(&self, item: u32, out: &mut String) {
        let Ok(item) = usize::try_from(item) else {
            return;
        };
        match self {
            Self::Messages(doc) => doc.item_name(item, out),
        }
    }

    /// Its `item`'s fields, in the inspector. Returns the label and help of
    /// the field under the pointer, if it has help.
    pub fn inspect(&self, ui: &mut Ui, item: u32) -> Option<(&'static str, String)> {
        let item = usize::try_from(item).ok()?;
        match self {
            Self::Messages(doc) => doc.inspect(ui, item),
        }
    }

    /// Back to bytes, as the file would be saved, or why it can't be.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        let encoded = match self {
            Self::Messages(doc) => doc.encode(),
        };
        encoded.map_err(|err| format!("Can't show it as hex: {err}"))
    }
}
