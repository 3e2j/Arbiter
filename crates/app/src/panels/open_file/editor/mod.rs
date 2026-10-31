//! The editor for each format the app can edit. The only place that lists
//! them: a new format is a variant here and a module of its own.

mod messages;

use formats::games::tp::Version;
use gui::ui::Ui;

use messages::Messages;

pub enum Editor {
    Messages(Box<Messages>),
}

impl Editor {
    /// The editor for `bytes`, their values named by `version`'s tables when
    /// given. `None` when no format claims them or they don't decode, which
    /// leaves them to hex.
    pub fn open(path: &str, bytes: &[u8], version: Option<Version>) -> Option<Self> {
        Messages::open(path, bytes, version).map(|messages| Self::Messages(Box::new(messages)))
    }

    /// Its name on the switch.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Messages(_) => "Messages",
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        match self {
            Self::Messages(messages) => messages.ui(ui),
        }
    }
}
