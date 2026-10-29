//! What keys do, as a table the user can change.
//!
//! Each row binds a [`Chord`] in a [`KeyContext`] to a [`ChordAction`],
//! and [`Keymap::resolve`] finds what a press does, or [`Keymap::held`]
//! whether one is held.
//!
//! A later row wins over an earlier one, so the user's bindings go on top of
//! the defaults, and a row without an action unbinds the chord under it.
//!
//! A chord reads and writes as text, such as `ctrl+shift+c`, for a settings
//! file to keep, and names its keys for the user with [`Chord::caps`].

use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::str::FromStr;

use crate::edit::{Edit, Motion};
use gui::input::{Input, Key, KeyCode, KeyPress, Modifiers};

thread_local! {
    static CURRENT: RefCell<Keymap> = RefCell::new(Keymap::default());
}

/// What `press` does in the keymap in use, as [`Keymap::resolve`] finds it.
#[must_use]
pub fn resolve(press: KeyPress, contexts: &[KeyContext]) -> Option<ChordAction> {
    CURRENT.with_borrow(|keymap| keymap.resolve(press, contexts))
}

/// The commands `keys` do in `contexts`, as [`resolve`] finds them, in the
/// order pressed.
pub fn commands(keys: &[KeyPress], contexts: &[KeyContext]) -> impl Iterator<Item = AppCommand> {
    keys.iter()
        .filter_map(|&press| match resolve(press, contexts)? {
            ChordAction::Command(command) => Some(command),
            _ => None,
        })
}

/// Looks presses up in `keymap` from now on, in place of the one before.
pub fn set(keymap: Keymap) {
    CURRENT.set(keymap);
}

/// A key and the modifiers held with it: exactly those, or any for a key
/// held down, such as moving while Shift speeds it up.
///
/// Shift is left out with a character that has no case, such as `>`, since
/// some layouts need it to type that character and others don't, so `Ctrl+>`
/// is one chord either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Chord {
    pub key: ChordKey,
    /// `None` for any.
    pub modifiers: Option<Modifiers>,
}

/// Which key a chord means: by what the layout types, or by where it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChordKey {
    Key(Key),
    Code(KeyCode),
}

/// Where a binding applies, such as while typing in a field. By name, so a
/// settings file can say it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyContext(pub &'static str);

/// One of the app's own actions, by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AppCommand(pub &'static str);

/// What a chord does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChordAction {
    Edit(Edit<'static>),
    /// Done with a field, keeping what's typed.
    Confirm,
    Cancel,
    Command(AppCommand),
}

/// Rows by column, so finding a chord only reads chords.
pub struct Keymap {
    chords: Vec<Chord>,
    contexts: Vec<KeyContext>,
    actions: Vec<Option<ChordAction>>,
}

impl Chord {
    #[must_use]
    pub const fn new(key: Key, modifiers: Modifiers) -> Self {
        Self {
            key: ChordKey::Key(key),
            modifiers: Some(modifiers),
        }
    }

    /// By where the key is, whatever the layout types there.
    #[must_use]
    pub const fn at(code: KeyCode, modifiers: Modifiers) -> Self {
        Self {
            key: ChordKey::Code(code),
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
        matches!(self.key, ChordKey::Key(Key::Char(c)) if !c.is_alphabetic())
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
            ChordKey::Key(key) => key == press.key,
            ChordKey::Code(code) => Some(code) == press.code,
        };
        key && self
            .modifiers
            .is_none_or(|modifiers| modifiers == self.held(press.modifiers))
    }

    /// What `press` is as a chord, by what the layout types, or `None` for a
    /// modifier alone, which is held with a key rather than bound itself.
    #[must_use]
    pub fn pressed(press: KeyPress) -> Option<Self> {
        let alone = matches!(press.key, Key::Shift | Key::Ctrl | Key::Alt | Key::Super);
        (!alone).then(|| Self::new(press.key, press.modifiers).normal())
    }

    /// With Shift left out where it's [`Self::caseless`], as it compares.
    fn normal(self) -> Self {
        let modifiers = self.modifiers.map(|modifiers| self.held(modifiers));
        Self { modifiers, ..self }
    }

    /// Its keys as the user knows them, modifiers first, one to a cap.
    pub fn caps(self) -> impl Iterator<Item = Cow<'static, str>> {
        let held = self.modifiers.unwrap_or_default();
        let modifiers = modifier_caps(held).map(Cow::Borrowed);
        let key = match self.key {
            ChordKey::Key(Key::F(n)) => Cow::Owned(format!("F{n}")),
            ChordKey::Key(Key::Char(' ')) => Cow::Borrowed("Space"),
            ChordKey::Key(Key::Char(c)) | ChordKey::Code(KeyCode(c)) => {
                Cow::Owned(c.to_uppercase().collect())
            }
            ChordKey::Key(key) => Cow::Borrowed(cap(key).unwrap_or_default()),
        };
        modifiers.chain(std::iter::once(key))
    }
}

/// The modifiers in the order a chord names them, by their caps. Their
/// names in its text are the same, lowercased.
const MODIFIERS: [(Modifiers, &str); 4] = [
    (Modifiers::CTRL, "Ctrl"),
    (Modifiers::ALT, "Alt"),
    (Modifiers::SHIFT, "Shift"),
    (Modifiers::SUPER, "Super"),
];

/// Those of `modifiers` held as the user knows them, in the order a chord
/// names them, one to a cap.
pub fn modifier_caps(modifiers: Modifiers) -> impl Iterator<Item = &'static str> {
    let mac = cfg!(target_os = "macos");
    MODIFIERS.into_iter().filter_map(move |(modifier, name)| {
        let name = match (modifier, mac) {
            (Modifiers::ALT, true) => "Option",
            (Modifiers::SUPER, true) => "Cmd",
            _ => name,
        };
        modifiers.contains(modifier).then_some(name)
    })
}

/// A key without a character's cap. Its name in a chord's text is the same,
/// lowercased.
const fn cap(key: Key) -> Option<&'static str> {
    Some(match key {
        Key::Shift => "Shift",
        Key::Ctrl => "Ctrl",
        Key::Alt => "Alt",
        Key::Super => "Super",
        Key::Enter => "Enter",
        Key::Escape => "Esc",
        Key::Tab => "Tab",
        Key::Backspace => "Backspace",
        Key::Delete => "Delete",
        Key::Insert => "Insert",
        Key::Left => "Left",
        Key::Right => "Right",
        Key::Up => "Up",
        Key::Down => "Down",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
        Key::F(_) | Key::Char(_) => return None,
    })
}

/// The key `name` is in a chord's text, lowercased.
fn named(name: &str) -> Option<Key> {
    if let Some(key) = KEYS
        .into_iter()
        .find(|&key| cap(key).is_some_and(|cap| cap.eq_ignore_ascii_case(name)))
    {
        return Some(key);
    }
    match name {
        "space" => return Some(Key::Char(' ')),
        "escape" => return Some(Key::Escape),
        _ => {}
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse().ok())
        && (1..=12).contains(&n)
    {
        return Some(Key::F(n));
    }
    single(name).map(Key::Char)
}

/// The only character in `text`.
fn single(text: &str) -> Option<char> {
    let mut chars = text.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

/// Text that doesn't read as a [`Chord`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParseChordError(pub String);

impl fmt::Display for ParseChordError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "`{}` isn't a key chord", self.0)
    }
}

impl std::error::Error for ParseChordError {}

impl fmt::Display for Chord {
    /// Lowercase, joined by `+`: the modifiers, or `any`, then the key, or
    /// `code:` and where it is for one bound by place.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.modifiers {
            None => f.write_str("any+")?,
            Some(held) => {
                for (modifier, name) in MODIFIERS {
                    if held.contains(modifier) {
                        write!(f, "{}+", name.to_lowercase())?;
                    }
                }
            }
        }
        match self.key {
            ChordKey::Code(KeyCode(c)) => write!(f, "code:{c}"),
            ChordKey::Key(Key::F(n)) => write!(f, "f{n}"),
            ChordKey::Key(Key::Char(' ')) => f.write_str("space"),
            ChordKey::Key(Key::Char(c)) => write!(f, "{c}"),
            ChordKey::Key(key) => f.write_str(&cap(key).unwrap_or_default().to_lowercase()),
        }
    }
}

impl FromStr for Chord {
    type Err = ParseChordError;

    /// As [`Display`](fmt::Display) writes it, in any case, and with `cmd`
    /// for [`Modifiers::COMMAND`].
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || ParseChordError(text.to_owned());
        let lower = text.to_lowercase();
        // `+` is a key too, as in `ctrl++`.
        let (held, key) = match lower.rsplit_once('+') {
            Some((held, "")) => (held.strip_suffix('+').unwrap_or(held), "+"),
            Some(split) => split,
            None => ("", lower.as_str()),
        };
        let mut modifiers = Some(Modifiers::default());
        for name in held.split('+').filter(|name| !name.is_empty()) {
            let modifier = match name {
                "any" => {
                    modifiers = None;
                    continue;
                }
                "cmd" => Modifiers::COMMAND,
                _ => {
                    MODIFIERS
                        .into_iter()
                        .find(|(_, known)| known.eq_ignore_ascii_case(name))
                        .ok_or_else(bad)?
                        .0
                }
            };
            modifiers = modifiers.map(|held| held.with(modifier));
        }
        let key = if let Some(code) = key.strip_prefix("code:") {
            ChordKey::Code(KeyCode(single(code).ok_or_else(bad)?))
        } else {
            ChordKey::Key(named(key).ok_or_else(bad)?)
        };
        Ok(Self { key, modifiers }.normal())
    }
}

/// The keys [`cap`] names.
const KEYS: [Key; 18] = [
    Key::Shift,
    Key::Ctrl,
    Key::Alt,
    Key::Super,
    Key::Enter,
    Key::Escape,
    Key::Tab,
    Key::Backspace,
    Key::Delete,
    Key::Insert,
    Key::Left,
    Key::Right,
    Key::Up,
    Key::Down,
    Key::Home,
    Key::End,
    Key::PageUp,
    Key::PageDown,
];

impl KeyContext {
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
    pub fn bind(&mut self, chord: Chord, context: KeyContext, action: Option<ChordAction>) {
        self.chords.push(chord.normal());
        self.contexts.push(context);
        self.actions.push(action);
    }

    /// What `press` does in the first of `contexts` that binds it, so the
    /// most specific goes first.
    #[must_use]
    pub fn resolve(&self, press: KeyPress, contexts: &[KeyContext]) -> Option<ChordAction> {
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
    pub fn held(&self, action: ChordAction, input: &Input, contexts: &[KeyContext]) -> bool {
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
            let action = Some(ChordAction::Edit(edit));
            keymap.bind(Chord::new(key, modifiers), KeyContext::TEXT, action);
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
        for (key, action) in [
            (Key::Enter, ChordAction::Confirm),
            (Key::Escape, ChordAction::Cancel),
        ] {
            keymap.bind(Chord::new(key, none), KeyContext::TEXT, Some(action));
        }
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
        match keymap.resolve(press(key, modifiers), &[KeyContext::TEXT]) {
            Some(ChordAction::Edit(edit)) => Some(edit),
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
        keymap.bind(a, KeyContext::TEXT, Some(ChordAction::Edit(Edit::Paste)));
        assert_eq!(edit(&keymap, Key::Char('a'), command), Some(Edit::Paste));
        keymap.bind(a, KeyContext::TEXT, None);
        assert_eq!(edit(&keymap, Key::Char('a'), command), None);
    }

    #[test]
    fn the_first_context_that_binds_a_chord_wins() {
        let mut keymap = Keymap::empty();
        let slash = Chord::new(Key::Char('/'), Modifiers::default());
        let search = ChordAction::Command(AppCommand("search"));
        keymap.bind(slash, KeyContext::GLOBAL, Some(search));
        let slash = press(Key::Char('/'), Modifiers::default());
        let both = [KeyContext::TEXT, KeyContext::GLOBAL];
        assert_eq!(keymap.resolve(slash, &both), Some(search));
        let chord = Chord::new(Key::Char('/'), Modifiers::default());
        keymap.bind(chord, KeyContext::TEXT, None);
        assert_eq!(keymap.resolve(slash, &both), None);
        assert_eq!(keymap.resolve(slash, &[KeyContext::GLOBAL]), Some(search));
    }

    #[test]
    fn shift_is_left_out_with_a_caseless_character() {
        let mut keymap = Keymap::empty();
        let (ctrl, shift) = (Modifiers::CTRL, Modifiers::SHIFT);
        let zoom = ChordAction::Command(AppCommand("zoom"));
        keymap.bind(
            Chord::new(Key::Char('>'), ctrl),
            KeyContext::GLOBAL,
            Some(zoom),
        );
        let global = [KeyContext::GLOBAL];
        for held in [ctrl, ctrl.with(shift)] {
            let press = press(Key::Char('>'), held);
            assert_eq!(keymap.resolve(press, &global), Some(zoom), "{held:?}");
        }
        let undo = ChordAction::Command(AppCommand("undo"));
        keymap.bind(
            Chord::new(Key::Char('z'), ctrl),
            KeyContext::GLOBAL,
            Some(undo),
        );
        let redo = press(Key::Char('z'), ctrl.with(shift));
        assert_eq!(keymap.resolve(redo, &global), None);
    }

    #[test]
    fn a_chord_reads_back_as_written() {
        let (ctrl, shift) = (Modifiers::CTRL, Modifiers::SHIFT);
        let chords = [
            (Chord::new(Key::Char('c'), ctrl.with(shift)), "ctrl+shift+c"),
            (Chord::new(Key::Char('+'), ctrl), "ctrl++"),
            (Chord::new(Key::Char('+'), Modifiers::default()), "+"),
            (Chord::new(Key::PageDown, Modifiers::ALT), "alt+pagedown"),
            (Chord::new(Key::F(5), Modifiers::default()), "f5"),
            (Chord::new(Key::Char(' '), ctrl), "ctrl+space"),
            (
                Chord::at(KeyCode('w'), Modifiers::default()).any_modifiers(),
                "any+code:w",
            ),
        ];
        for (chord, text) in chords {
            assert_eq!(chord.to_string(), text);
            assert_eq!(text.parse(), Ok(chord), "{text}");
        }
        let command = Chord::new(Key::Char('f'), Modifiers::COMMAND);
        assert_eq!("CMD+F".parse(), Ok(command));
        assert_eq!("ctrl+esc".parse(), "ctrl+escape".parse::<Chord>());
        for bad in ["", "hyper+c", "ctrl+ab", "f13", "code:ww"] {
            assert!(bad.parse::<Chord>().is_err(), "{bad}");
        }
    }

    #[test]
    fn a_pressed_chord_leaves_out_shift_where_it_compares_without() {
        let held = Modifiers::CTRL.with(Modifiers::SHIFT);
        let plus = Chord::pressed(press(Key::Char('+'), held));
        assert_eq!(plus, Some(Chord::new(Key::Char('+'), Modifiers::CTRL)));
        let c = Chord::pressed(press(Key::Char('c'), held));
        assert_eq!(c, Some(Chord::new(Key::Char('c'), held)));
        assert_eq!(Chord::pressed(press(Key::Shift, Modifiers::SHIFT)), None);
        let caps: Vec<_> = Chord::new(Key::PageUp, held).caps().collect();
        assert_eq!(caps, ["Ctrl", "Shift", "PageUp"]);
    }

    #[test]
    fn a_key_bound_by_place_is_held_with_any_modifiers() {
        use gui::input::Event;
        let mut keymap = Keymap::empty();
        let fly = KeyContext("fly");
        let forward = ChordAction::Command(AppCommand("forward"));
        let fast = ChordAction::Command(AppCommand("fast"));
        let w = Chord::at(KeyCode('w'), Modifiers::default()).any_modifiers();
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
            code: Some(KeyCode('w')),
            repeat: false,
        });
        assert!(held(&input, forward) && held(&input, fast));
        assert!(!keymap.held(forward, &input, &[KeyContext::GLOBAL]));
        input.push(Event::KeyReleased {
            key: Key::Char('z'),
            code: Some(KeyCode('w')),
        });
        assert!(!held(&input, forward) && held(&input, fast));
    }
}
