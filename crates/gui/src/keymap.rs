//! What keys do, as a table the user can change.
//!
//! Each row binds a [`Chord`] in a [`Context`] to an [`Action`],
//! and [`Keymap::resolve`] finds what a press does, or [`Keymap::held`]
//! whether one is held.
//!
//! A later row wins over an earlier one, so the user's bindings go on top of
//! the defaults, and a row without an action unbinds the chord under it.

use crate::edit::{Edit, Motion};
use crate::input::{Code, Input, Key, KeyPress, Modifiers};

/// A key and the modifiers held with it: exactly those, or any for a key
/// held down, such as moving while Shift speeds it up.
///
/// Shift is left out with a character that has no case, such as `>`, since
/// some layouts need it to type that character and others don't, so `Ctrl+>`
/// is one chord either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Chord {
    pub key: Which,
    /// `None` for any.
    pub modifiers: Option<Modifiers>,
}

/// Which key a chord means: by what the layout types, or by where it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Which {
    Key(Key),
    Code(Code),
}

/// Where a binding applies, such as while typing in a field. By name, so a
/// settings file can say it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Context(pub &'static str);

/// One of the app's own actions, by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Command(pub &'static str);

/// What a chord does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Edit(Edit<'static>),
    /// Done with a field, keeping what's typed.
    Confirm,
    Cancel,
    Command(Command),
}

/// Rows by column, so finding a chord only reads chords.
pub struct Keymap {
    chords: Vec<Chord>,
    contexts: Vec<Context>,
    actions: Vec<Option<Action>>,
}

impl Chord {
    #[must_use]
    pub const fn new(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key: Which::Key(key),
            modifiers: Some(modifiers),
        }
    }

    /// By where the key is, whatever the layout types there.
    #[must_use]
    pub const fn at(code: Code, modifiers: Modifiers) -> Self {
        Self {
            key: Which::Code(code),
            modifiers: Some(modifiers),
        }
    }

    /// With whatever modifiers are held.
    #[must_use]
    pub const fn any_modifiers(self) -> Self {
        Self {
            modifiers: None,
            ..self
        }
    }

    fn caseless(self) -> bool {
        matches!(self.key, Which::Key(Key::Char(c)) if !c.is_alphabetic())
    }

    /// `modifiers` as this chord compares them.
    fn held(self, modifiers: Modifiers) -> Modifiers {
        if self.caseless() {
            modifiers.without(Modifiers::SHIFT)
        } else {
            modifiers
        }
    }

    fn matches(self, press: KeyPress) -> bool {
        let key = match self.key {
            Which::Key(key) => key == press.key,
            Which::Code(code) => Some(code) == press.code,
        };
        key && self
            .modifiers
            .is_none_or(|modifiers| modifiers == self.held(press.modifiers))
    }
}

impl Context {
    /// Anywhere, so it goes last among the contexts asked.
    pub const GLOBAL: Self = Self("global");
    /// While typing in a field or editor.
    pub const TEXT: Self = Self("text");
}

impl Keymap {
    /// Without any bindings.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            chords: Vec::new(),
            contexts: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// Binds `chord` in `context` over whatever it did there, or unbinds it
    /// when `action` is `None`.
    pub fn bind(&mut self, chord: Chord, context: Context, action: Option<Action>) {
        let modifiers = chord.modifiers.map(|modifiers| chord.held(modifiers));
        self.chords.push(Chord { modifiers, ..chord });
        self.contexts.push(context);
        self.actions.push(action);
    }

    /// What `press` does in the first of `contexts` that binds it, so the
    /// most specific goes first.
    #[must_use]
    pub fn resolve(&self, press: KeyPress, contexts: &[Context]) -> Option<Action> {
        let rows = || self.chords.iter().zip(&self.contexts);
        contexts
            .iter()
            .find_map(|&context| {
                let row = rows().rposition(|(c, &x)| x == context && c.matches(press))?;
                self.actions.get(row).copied()
            })
            .flatten()
    }

    /// Whether a key down in `input` does `action`, as [`Self::resolve`]
    /// would find it.
    #[must_use]
    pub fn held(&self, action: Action, input: &Input, contexts: &[Context]) -> bool {
        input
            .held_keys()
            .any(|press| self.resolve(press, contexts) == Some(action))
    }
}

impl Default for Keymap {
    /// Each platform's usual editing keys: Ctrl for words elsewhere, Option
    /// on macOS, where Command is for lines.
    fn default() -> Self {
        let mut keymap = Self::empty();
        let mut bind = |key, modifiers, edit| {
            let action = Some(Action::Edit(edit));
            keymap.bind(Chord::new(key, modifiers), Context::TEXT, action);
        };
        let mac = cfg!(target_os = "macos");
        let (none, shift) = (Modifiers::default(), Modifiers::SHIFT);
        let word = if mac { Modifiers::ALT } else { Modifiers::CTRL };
        let line = mac.then_some((Modifiers::SUPER, Motion::Line));
        let by = [(none, Motion::Grapheme), (word, Motion::Word)];
        for (held, motion) in by.into_iter().chain(line) {
            for (key, forward) in [(Key::Left, false), (Key::Right, true)] {
                for (held, extend) in [(held, false), (held.with(shift), true)] {
                    let edit = Edit::Move {
                        motion,
                        forward,
                        extend,
                    };
                    bind(key, held, edit);
                }
            }
            for (key, forward) in [(Key::Backspace, false), (Key::Delete, true)] {
                for held in [held, held.with(shift)] {
                    bind(key, held, Edit::Delete { motion, forward });
                }
            }
        }
        for (held, motion) in [(none, Motion::Line), (Modifiers::CTRL, Motion::Text)] {
            for (key, forward) in [(Key::Home, false), (Key::End, true)] {
                for (held, extend) in [(held, false), (held.with(shift), true)] {
                    let edit = Edit::Move {
                        motion,
                        forward,
                        extend,
                    };
                    bind(key, held, edit);
                }
            }
        }
        let command = Modifiers::COMMAND;
        bind(Key::Char('a'), command, Edit::SelectAll);
        bind(Key::Char('c'), command, Edit::Copy);
        bind(Key::Char('x'), command, Edit::Cut);
        bind(Key::Char('v'), command, Edit::Paste);
        bind(Key::Insert, Modifiers::CTRL, Edit::Copy);
        bind(Key::Insert, shift, Edit::Paste);
        // Over Shift+Delete deleting forward.
        bind(Key::Delete, shift, Edit::Cut);
        keymap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key, modifiers: Modifiers) -> KeyPress {
        KeyPress {
            key,
            code: None,
            repeat: false,
            modifiers,
        }
    }

    fn edit(keymap: &Keymap, key: Key, modifiers: Modifiers) -> Option<Edit<'static>> {
        match keymap.resolve(press(key, modifiers), &[Context::TEXT]) {
            Some(Action::Edit(edit)) => Some(edit),
            _ => None,
        }
    }

    #[test]
    fn keys_map_to_edits() {
        let keymap = Keymap::default();
        let none = Modifiers::default();
        let shift = Modifiers::SHIFT;
        let command = Modifiers::COMMAND;
        let edit = |key, modifiers| edit(&keymap, key, modifiers);
        assert_eq!(edit(Key::Char('a'), command), Some(Edit::SelectAll));
        assert_eq!(edit(Key::Char('a'), none), None);
        assert_eq!(edit(Key::Char('v'), command), Some(Edit::Paste));
        assert_eq!(edit(Key::Insert, shift), Some(Edit::Paste));
        assert_eq!(edit(Key::Delete, shift), Some(Edit::Cut));
        let extend = Edit::Move {
            motion: Motion::Grapheme,
            forward: false,
            extend: true,
        };
        assert_eq!(edit(Key::Left, shift), Some(extend));
        let end = Edit::Move {
            motion: Motion::Line,
            forward: true,
            extend: false,
        };
        assert_eq!(edit(Key::End, none), Some(end));
        let back = Edit::Delete {
            motion: Motion::Grapheme,
            forward: false,
        };
        assert_eq!(edit(Key::Backspace, none), Some(back));
    }

    #[test]
    fn a_later_binding_wins_and_none_unbinds() {
        let mut keymap = Keymap::default();
        let command = Modifiers::COMMAND;
        let a = Chord::new(Key::Char('a'), command);
        keymap.bind(a, Context::TEXT, Some(Action::Edit(Edit::Paste)));
        assert_eq!(edit(&keymap, Key::Char('a'), command), Some(Edit::Paste));
        keymap.bind(a, Context::TEXT, None);
        assert_eq!(edit(&keymap, Key::Char('a'), command), None);
    }

    #[test]
    fn the_first_context_that_binds_a_chord_wins() {
        let mut keymap = Keymap::empty();
        let slash = Chord::new(Key::Char('/'), Modifiers::default());
        let search = Action::Command(Command("search"));
        keymap.bind(slash, Context::GLOBAL, Some(search));
        let slash = press(Key::Char('/'), Modifiers::default());
        let both = [Context::TEXT, Context::GLOBAL];
        assert_eq!(keymap.resolve(slash, &both), Some(search));
        let chord = Chord::new(Key::Char('/'), Modifiers::default());
        keymap.bind(chord, Context::TEXT, None);
        assert_eq!(keymap.resolve(slash, &both), None);
        assert_eq!(keymap.resolve(slash, &[Context::GLOBAL]), Some(search));
    }

    #[test]
    fn shift_is_left_out_with_a_caseless_character() {
        let mut keymap = Keymap::empty();
        let (ctrl, shift) = (Modifiers::CTRL, Modifiers::SHIFT);
        let zoom = Action::Command(Command("zoom"));
        keymap.bind(
            Chord::new(Key::Char('>'), ctrl),
            Context::GLOBAL,
            Some(zoom),
        );
        let global = [Context::GLOBAL];
        for held in [ctrl, ctrl.with(shift)] {
            let press = press(Key::Char('>'), held);
            assert_eq!(keymap.resolve(press, &global), Some(zoom), "{held:?}");
        }
        let undo = Action::Command(Command("undo"));
        keymap.bind(
            Chord::new(Key::Char('z'), ctrl),
            Context::GLOBAL,
            Some(undo),
        );
        let redo = press(Key::Char('z'), ctrl.with(shift));
        assert_eq!(keymap.resolve(redo, &global), None);
    }

    #[test]
    fn a_key_bound_by_place_is_held_with_any_modifiers() {
        use crate::input::Event;
        let mut keymap = Keymap::empty();
        let fly = Context("fly");
        let forward = Action::Command(Command("forward"));
        let fast = Action::Command(Command("fast"));
        let w = Chord::at(Code('w'), Modifiers::default()).any_modifiers();
        keymap.bind(w, fly, Some(forward));
        let shift = Chord::new(Key::Shift, Modifiers::default()).any_modifiers();
        keymap.bind(shift, fly, Some(fast));
        let mut input = Input::default();
        let held = |input: &Input, action| keymap.held(action, input, &[fly]);
        input.push(Event::Modifiers(Modifiers::SHIFT));
        input.push(Event::Key {
            key: Key::Shift,
            code: None,
            repeat: false,
        });
        // Where W is on a US layout, which types Z on a French one.
        input.push(Event::Key {
            key: Key::Char('z'),
            code: Some(Code('w')),
            repeat: false,
        });
        assert!(held(&input, forward) && held(&input, fast));
        assert!(!keymap.held(forward, &input, &[Context::GLOBAL]));
        input.push(Event::KeyReleased {
            key: Key::Char('z'),
            code: Some(Code('w')),
        });
        assert!(!held(&input, forward) && held(&input, fast));
    }
}
