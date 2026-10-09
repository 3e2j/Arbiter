//! What the keyboard and pointer did, as plain values. The window turns its
//! events into [`Event`]s, the host collects them into [`Input`] for a pass,
//! and the pass answers with [`Out`].
//!
//! Positions and distances are in logical pixels from the window's top left.

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
    Key(KeyPress),
    /// Typed text, with the keyboard layout and dead keys applied.
    Text(&'a str),
    Modifiers(Modifiers),
    /// The window lost focus, so whatever was held is let go of without an
    /// event for it.
    Unfocused,
}

/// Everything since the last pass. Kept between passes, so a pass allocates
/// nothing once warm.
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
    text: String,
}

/// What a pass asks of the window.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Out {
    pub cursor: Cursor,
}

impl Modifiers {
    pub const SHIFT: Self = Self(1);
    pub const CTRL: Self = Self(1 << 1);
    pub const ALT: Self = Self(1 << 2);
    /// The Windows key, or Command on macOS.
    pub const SUPER: Self = Self(1 << 3);

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
            Event::Key(press) => self.keys.push(press),
            Event::Text(text) => self.text.push_str(text),
            Event::Modifiers(modifiers) => self.modifiers = modifiers,
            Event::Unfocused => {
                self.held = 0;
                self.modifiers = Modifiers::default();
            }
        }
    }

    /// After a pass, so the next one only sees new events. What's held and
    /// where the pointer is stay.
    pub fn clear(&mut self) {
        self.pressed = 0;
        self.released = 0;
        self.scroll = [0.; 2];
        self.keys.clear();
        self.text.clear();
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

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_inside_one_pass_is_seen() {
        let mut input = Input::default();
        input.push(Event::Pressed(Button::Left));
        input.push(Event::Released(Button::Left));
        input.push(Event::Pressed(Button::Right));
        assert!(input.pressed(Button::Left) && input.released(Button::Left));
        assert!(!input.held(Button::Left) && input.held(Button::Right));
    }

    #[test]
    fn clearing_keeps_what_is_held() {
        let mut input = Input::default();
        input.push(Event::Pointer(Some([4., 5.])));
        input.push(Event::Pressed(Button::Middle));
        input.push(Event::Scroll([0., 3.]));
        input.push(Event::Scroll([0., 2.]));
        input.push(Event::Key(KeyPress {
            key: Key::Enter,
            repeat: false,
        }));
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
}
