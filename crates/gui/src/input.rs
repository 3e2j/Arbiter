//! What the keyboard and pointer did, as plain values. The window turns its
//! events into [`Event`]s, the host collects them into [`Input`] for a pass,
//! and the pass answers with [`Out`].
//!
//! Positions and distances are in logical pixels from the window's top left.

use std::ops::Range;

/// A pointer button.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Left = 1,
    Right = 1 << 1,
    Middle = 1 << 2,
    /// The side button nearer the user's wrist, usually back.
    Back = 1 << 3,
    Forward = 1 << 4,
}

impl Button {
    pub const ALL: [Self; 5] = [
        Self::Left,
        Self::Right,
        Self::Middle,
        Self::Back,
        Self::Forward,
    ];

    /// Its place in [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Middle => 2,
            Self::Back => 3,
            Self::Forward => 4,
        }
    }
}

/// The keys the editor acts on. Any other key with a character is
/// [`Key::Char`], and the rest aren't reported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// `F(1)` to `F(12)`.
    F(u8),
    /// What the keyboard layout types, lowercased, for shortcuts. Typed text
    /// is [`Input::text`].
    Char(char),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyPress {
    pub key: Key,
    /// Sent again because the key is held.
    pub repeat: bool,
    /// Held as it went down, which can differ from [`Input::modifiers`] when
    /// a pass has several presses.
    pub modifiers: Modifiers,
}

/// A key or typed text, as [`Input::typed`] gives them in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Typed<'a> {
    Key(KeyPress),
    Text(&'a str),
}

/// The modifier keys held, as bits.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Modifiers(u8);

/// The pointer's shape over the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Cursor {
    #[default]
    Default,
    /// Over something clickable.
    Pointer,
    /// Over text that can be selected or typed in.
    Text,
    /// Over a divider between boxes side by side.
    ResizeH,
    /// Over a divider between boxes above one another.
    ResizeV,
    /// Over something that can be dragged.
    Grab,
    /// While something is dragged.
    Grabbing,
    /// While something dragged can't be dropped here.
    NotAllowed,
}

/// One thing the user did.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Event<'a> {
    /// The pointer moved to here, or `None` when it left the window.
    Pointer(Option<[f32; 2]>),
    Pressed(Button),
    Released(Button),
    /// How far to scroll. Positive `y` scrolls toward the top, as a wheel
    /// turned away from the user does.
    Scroll([f32; 2]),
    Key {
        key: Key,
        repeat: bool,
    },
    /// Typed text, with the keyboard layout and dead keys applied.
    /// Dropped while a shortcut's modifier is held, since some platforms type
    /// the letter of Ctrl+A too.
    Text(&'a str),
    Modifiers(Modifiers),
    /// The window lost focus, so whatever was held is let go of without an
    /// event for it.
    Unfocused,
}

/// Everything since the last pass. Kept between passes, so a pass allocates
/// nothing once warm.
///
/// Each button goes down or up at most once a pass, so a box that takes a
/// press has it before the release. A second change waits for the next pass,
/// with every event after it.
#[derive(Default)]
pub struct Input {
    pointer: Option<[f32; 2]>,
    /// [`Button`] bits.
    held: u8,
    pressed: u8,
    released: u8,
    scroll: [f32; 2],
    modifiers: Modifiers,
    keys: Vec<KeyPress>,
    /// How much of `text` came before each of `keys`.
    keys_at: Vec<usize>,
    text: String,
    /// What waits for the next pass, in order.
    later: Vec<Later>,
    /// What the text in `later` typed.
    later_text: String,
    /// Whether anything came in since the last pass.
    fresh: bool,
}

/// An [`Event`] waiting for the next pass, its text kept apart.
enum Later {
    Event(Event<'static>),
    Text(Range<usize>),
}

/// What a pass asks of the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Out {
    pub cursor: Cursor,
    /// The frame had input, or some waited for the next, so it should draw
    /// again straight away.
    pub again: bool,
}

impl Modifiers {
    pub const SHIFT: Self = Self(1);
    pub const CTRL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    /// The Windows key, or Command on macOS.
    pub const SUPER: Self = Self(1 << 3);
    /// What shortcuts such as copy are held with: Command on macOS, Ctrl
    /// elsewhere.
    pub const COMMAND: Self = if cfg!(target_os = "macos") {
        Self::SUPER
    } else {
        Self::CTRL
    };

    /// Whether these let a key type, rather than make it a shortcut. Ctrl
    /// with Alt is `AltGr` on Windows, which types.
    #[must_use]
    pub const fn types(self) -> bool {
        let ctrl = self.contains(Self::CTRL) && !self.contains(Self::ALT);
        !ctrl && !self.contains(Self::SUPER)
    }

    /// Whether exactly these are held.
    #[must_use]
    pub const fn only(self, other: Self) -> bool {
        self.0 == other.0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl Input {
    pub fn push(&mut self, event: Event) {
        if self.later.is_empty() && !self.changes_twice(event) {
            self.apply(event);
            return;
        }
        let later = match event {
            Event::Pointer(at) => Event::Pointer(at),
            Event::Pressed(button) => Event::Pressed(button),
            Event::Released(button) => Event::Released(button),
            Event::Scroll(by) => Event::Scroll(by),
            Event::Key { key, repeat } => Event::Key { key, repeat },
            Event::Text(text) => {
                let start = self.later_text.len();
                self.later_text.push_str(text);
                self.later.push(Later::Text(start..self.later_text.len()));
                return;
            }
            Event::Modifiers(modifiers) => Event::Modifiers(modifiers),
            Event::Unfocused => Event::Unfocused,
        };
        self.later.push(Later::Event(later));
    }

    /// After a frame, takes in what waited for it, up to the next button that
    /// would change twice. Returns whether anything came in, so the window
    /// should draw again.
    pub fn trickle(&mut self) -> bool {
        let mut later = std::mem::take(&mut self.later);
        let text = std::mem::take(&mut self.later_text);
        let mut taken = 0;
        for waiting in &later {
            let event = match waiting {
                Later::Event(event) => *event,
                Later::Text(range) => Event::Text(text.get(range.clone()).unwrap_or_default()),
            };
            if self.changes_twice(event) {
                break;
            }
            self.apply(event);
            taken += 1;
        }
        later.drain(..taken);
        self.later = later;
        self.later_text = text;
        if self.later.is_empty() {
            self.later_text.clear();
        }
        taken > 0
    }

    /// Whether `event` presses or releases a button that already went down
    /// or up this pass.
    fn changes_twice(&self, event: Event) -> bool {
        let changed = self.pressed | self.released;
        matches!(event, Event::Pressed(button) | Event::Released(button)
            if changed & button as u8 != 0)
    }

    fn apply(&mut self, event: Event) {
        self.fresh = true;
        match event {
            Event::Pointer(at) => self.pointer = at,
            Event::Pressed(button) => {
                self.held |= button as u8;
                self.pressed |= button as u8;
            }
            Event::Released(button) => {
                self.held &= !(button as u8);
                self.released |= button as u8;
            }
            Event::Scroll([x, y]) => {
                self.scroll[0] += x;
                self.scroll[1] += y;
            }
            Event::Key { key, repeat } => {
                let modifiers = self.modifiers;
                self.keys.push(KeyPress {
                    key,
                    repeat,
                    modifiers,
                });
                self.keys_at.push(self.text.len());
            }
            Event::Text(text) if self.modifiers.types() => self.text.push_str(text),
            Event::Text(_) => {}
            Event::Modifiers(modifiers) => self.modifiers = modifiers,
            Event::Unfocused => {
                self.held = 0;
                self.modifiers = Modifiers::default();
            }
        }
    }

    /// After a pass, so the next one only sees new events. What's held and
    /// where the pointer is stay, and what waits stays waiting until
    /// [`Self::trickle`].
    pub fn clear(&mut self) {
        self.fresh = false;
        self.pressed = 0;
        self.released = 0;
        self.scroll = [0.; 2];
        self.keys.clear();
        self.keys_at.clear();
        self.text.clear();
    }

    /// Whether anything came in since the last pass.
    #[must_use]
    pub const fn fresh(&self) -> bool {
        self.fresh
    }

    /// `None` while it's outside the window.
    #[must_use]
    pub const fn pointer(&self) -> Option<[f32; 2]> {
        self.pointer
    }

    #[must_use]
    pub const fn held(&self, button: Button) -> bool {
        self.held & button as u8 != 0
    }

    /// Went down since the last pass, even if it's back up.
    #[must_use]
    pub const fn pressed(&self, button: Button) -> bool {
        self.pressed & button as u8 != 0
    }

    /// Went up since the last pass.
    #[must_use]
    pub const fn released(&self, button: Button) -> bool {
        self.released & button as u8 != 0
    }

    /// As [`Event::Scroll`], summed since the last pass.
    #[must_use]
    pub const fn scroll(&self) -> [f32; 2] {
        self.scroll
    }

    #[must_use]
    pub const fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    /// In the order they were pressed.
    #[must_use]
    pub fn keys(&self) -> &[KeyPress] {
        &self.keys
    }

    /// Everything typed since the last pass, without the order against
    /// [`Self::keys`], which [`Self::typed`] keeps.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The keys and typed text in the order they came, for an editor where
    /// typing then moving differs from moving then typing.
    pub fn typed(&self) -> impl Iterator<Item = Typed<'_>> {
        let mut from = 0;
        let ends = self.keys_at.iter().copied().chain([self.text.len()]);
        let keys = self.keys.iter().copied().map(Some).chain([None]);
        ends.zip(keys).flat_map(move |(at, key)| {
            let text = self.text.get(from..at).unwrap_or_default();
            from = at;
            let text = (!text.is_empty()).then_some(Typed::Text(text));
            text.into_iter().chain(key.map(Typed::Key))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_inside_one_pass_waits_to_come_up() {
        let mut input = Input::default();
        input.push(Event::Pressed(Button::Left));
        input.push(Event::Pressed(Button::Right));
        input.push(Event::Released(Button::Left));
        // After the release, so it waits too.
        input.push(Event::Text("a"));
        assert!(input.pressed(Button::Left) && input.held(Button::Left));
        assert!(input.held(Button::Right) && input.text().is_empty());
        input.clear();
        assert!(input.trickle());
        assert!(input.released(Button::Left) && !input.held(Button::Left));
        assert_eq!(input.text(), "a");
        input.clear();
        assert!(!input.trickle() && !input.fresh());
    }

    #[test]
    fn any_event_is_fresh_until_cleared() {
        let mut input = Input::default();
        assert!(!input.fresh());
        input.push(Event::Pointer(Some([1., 2.])));
        assert!(input.fresh());
        input.clear();
        assert!(!input.fresh());
    }

    #[test]
    fn each_trickle_takes_one_change_of_a_button() {
        let mut input = Input::default();
        for _ in 0..2 {
            input.push(Event::Pressed(Button::Left));
            input.push(Event::Released(Button::Left));
        }
        let mut changes = 1;
        while {
            input.clear();
            input.trickle()
        } {
            changes += 1;
        }
        assert_eq!(changes, 4);
        assert!(!input.held(Button::Left));
    }

    #[test]
    fn clearing_keeps_what_is_held() {
        let mut input = Input::default();
        input.push(Event::Pointer(Some([4., 5.])));
        input.push(Event::Pressed(Button::Middle));
        input.push(Event::Scroll([0., 3.]));
        input.push(Event::Scroll([0., 2.]));
        input.push(Event::Key {
            key: Key::Enter,
            repeat: false,
        });
        input.push(Event::Text("hé"));
        assert_eq!(input.scroll(), [0., 5.]);
        input.clear();
        assert_eq!(input.pointer(), Some([4., 5.]));
        assert!(input.held(Button::Middle) && !input.pressed(Button::Middle));
        assert_eq!(input.scroll(), [0.; 2]);
        assert!(input.keys().is_empty() && input.text().is_empty());
    }

    #[test]
    fn losing_focus_lets_go() {
        let mut input = Input::default();
        input.push(Event::Pressed(Button::Left));
        input.push(Event::Modifiers(Modifiers::CTRL.with(Modifiers::SHIFT)));
        assert!(input.modifiers().contains(Modifiers::CTRL));
        input.push(Event::Unfocused);
        assert!(!input.held(Button::Left));
        assert_eq!(input.modifiers(), Modifiers::default());
    }

    fn key(input: &mut Input, key: Key) {
        input.push(Event::Key { key, repeat: false });
    }

    #[test]
    fn keys_and_text_keep_their_order() {
        let mut input = Input::default();
        input.push(Event::Text("ab"));
        key(&mut input, Key::Left);
        key(&mut input, Key::Left);
        input.push(Event::Text("c"));
        let typed: Vec<_> = input.typed().collect();
        let left = |typed: &Typed| matches!(typed, Typed::Key(press) if press.key == Key::Left);
        assert_eq!(typed.len(), 4);
        assert_eq!(typed[0], Typed::Text("ab"));
        assert!(left(&typed[1]) && left(&typed[2]));
        assert_eq!(typed[3], Typed::Text("c"));
    }

    #[test]
    fn a_shortcut_types_nothing() {
        let mut input = Input::default();
        input.push(Event::Modifiers(Modifiers::CTRL));
        key(&mut input, Key::Char('a'));
        input.push(Event::Text("a"));
        input.push(Event::Modifiers(Modifiers::CTRL.with(Modifiers::ALT)));
        input.push(Event::Text("@"));
        assert_eq!(input.text(), "@");
        assert_eq!(input.keys()[0].modifiers, Modifiers::CTRL);
    }
}
