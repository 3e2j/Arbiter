//! The app's own actions, each with a name for the user and the chords that
//! do it until the user says otherwise. The keymap and the keybinds page both
//! read them from here.

use gui::keymap::{AppCommand, Chord, ChordAction, KeyContext, Keymap};

use crate::panels::{Files, Hex};
use crate::settings::Settings;

pub struct Command {
    pub id: AppCommand,
    pub name: &'static str,
    /// Where it works.
    pub context: KeyContext,
    pub chords: &'static [Chord],
}

/// Every command, by the part of the app it works in.
pub static GROUPS: [(&str, &[Command]); 3] = [
    ("General", &Settings::COMMANDS),
    ("File tree", &Files::COMMANDS),
    ("Hex view", &Hex::COMMANDS),
];

/// Every command, in [`GROUPS`]' order.
pub fn all() -> impl Iterator<Item = &'static Command> {
    GROUPS.iter().flat_map(|&(_, commands)| commands)
}

/// The editing keys, then `chords` of each of [`all`] in its order.
pub fn keymap<'a>(chords: impl IntoIterator<Item = &'a [Chord]>) -> Keymap {
    let mut keymap = Keymap::default();
    for (command, chords) in all().zip(chords) {
        let action = Some(ChordAction::Command(command.id));
        for &chord in chords {
            keymap.bind(chord, command.context, action);
        }
    }
    keymap
}
