//! Editing text, shared by everything that does: a field, and later the code
//! editor. Keys become [`Edit`]s through [`Edit::from_key`], and [`apply`]
//! carries one out on a string and its [`Selection`].
//!
//! Every position is a byte offset into the text, between graphemes, so an
//! emoji or a letter with its accent moves and deletes as one.

use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

use crate::clipboard::Clipboard;
use crate::input::{Key, KeyPress, Modifiers};

/// What's selected, or just where typing goes when both ends are the same.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Selection {
    /// The end that moves.
    pub caret: usize,
    /// The end that stays while the selection is extended.
    pub anchor: usize,
}

/// How far a move or delete goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    Grapheme,
    /// Past anything between, then to the word's far end. Words stop at
    /// anything but letters and digits, such as the `/` and `_` in a path.
    Word,
    /// To the line's start or end.
    Line,
    /// To the text's start or end.
    Text,
}

/// One thing an editor does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edit<'a> {
    /// Without `extend`, a grapheme move first collapses a selection to its
    /// side.
    Move {
        motion: Motion,
        forward: bool,
        extend: bool,
    },
    /// The selection if there is one, otherwise from the caret.
    Delete {
        motion: Motion,
        forward: bool,
    },
    /// In place of the selection.
    Insert(&'a str),
    SelectAll,
    Copy,
    Cut,
    Paste,
    /// The primary selection, as a middle click pastes it, at the caret.
    PastePrimary,
}

/// How many lines the text can hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lines {
    /// Inserted line breaks become spaces.
    One,
    Many,
}

impl Selection {
    #[must_use]
    pub const fn at(at: usize) -> Self {
        Self {
            caret: at,
            anchor: at,
        }
    }

    /// From its start to its end, whichever way it was made.
    #[must_use]
    pub fn range(self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.caret == self.anchor
    }

    /// Back inside `text` and onto char boundaries, for one the caller set.
    fn clamp(&mut self, text: &str) {
        let clamp = |at: usize| {
            let mut at = at.min(text.len());
            while !text.is_char_boundary(at) {
                at -= 1;
            }
            at
        };
        self.caret = clamp(self.caret);
        self.anchor = clamp(self.anchor);
    }
}

impl Edit<'_> {
    /// What `press` means to an editor, with the platform's usual keys:
    /// Ctrl for words elsewhere, Option on macOS, where Command is for lines.
    #[must_use]
    pub fn from_key(press: KeyPress) -> Option<Self> {
        let mac = cfg!(target_os = "macos");
        let held = press.modifiers;
        let extend = held.contains(Modifiers::SHIFT);
        // Shift picks extending, or between copy and cut on Delete and Insert,
        // so it's left out when matching the rest.
        let with = |rest: Modifiers| held.only(rest) || held.only(rest.with(Modifiers::SHIFT));
        let plain = Modifiers::default();
        let word = if mac { Modifiers::ALT } else { Modifiers::CTRL };
        let motion = if with(plain) {
            Motion::Grapheme
        } else if with(word) {
            Motion::Word
        } else if mac && with(Modifiers::SUPER) {
            Motion::Line
        } else {
            Motion::Text
        };
        let moves = |motion, forward| Edit::Move {
            motion,
            forward,
            extend,
        };
        let deletes = |motion, forward| Edit::Delete { motion, forward };
        let command = held.only(Modifiers::COMMAND);
        Some(match press.key {
            Key::Left | Key::Right if motion != Motion::Text => {
                moves(motion, press.key == Key::Right)
            }
            Key::Home | Key::End if with(plain) => moves(Motion::Line, press.key == Key::End),
            Key::Home | Key::End if with(Modifiers::CTRL) => {
                moves(Motion::Text, press.key == Key::End)
            }
            Key::Delete if held.only(Modifiers::SHIFT) => Edit::Cut,
            Key::Backspace | Key::Delete if motion != Motion::Text => {
                deletes(motion, press.key == Key::Delete)
            }
            Key::Insert if held.only(Modifiers::CTRL) => Edit::Copy,
            Key::Insert if held.only(Modifiers::SHIFT) => Edit::Paste,
            Key::Char('a') if command => Edit::SelectAll,
            Key::Char('c') if command => Edit::Copy,
            Key::Char('x') if command => Edit::Cut,
            Key::Char('v') if command => Edit::Paste,
            _ => return None,
        })
    }
}

/// Carries out `edit` on `text`, reading and writing `clipboard` for copy and
/// paste. Returns whether the text changed.
pub fn apply(
    text: &mut String,
    selection: &mut Selection,
    edit: Edit,
    clipboard: &mut Clipboard,
    lines: Lines,
) -> bool {
    selection.clamp(text);
    match edit {
        Edit::Move {
            motion: Motion::Grapheme,
            forward,
            extend: false,
        } if !selection.is_empty() => {
            let range = selection.range();
            *selection = Selection::at(if forward { range.end } else { range.start });
            false
        }
        Edit::Move {
            motion,
            forward,
            extend,
        } => {
            selection.caret = to(text, selection.caret, motion, forward);
            if !extend {
                selection.anchor = selection.caret;
            }
            false
        }
        Edit::Delete { motion, forward } => {
            if selection.is_empty() {
                selection.anchor = to(text, selection.caret, motion, forward);
            }
            replace(text, selection, "")
        }
        Edit::Insert(typed) => insert(text, selection, typed, lines),
        Edit::SelectAll => {
            *selection = Selection {
                caret: text.len(),
                anchor: 0,
            };
            false
        }
        Edit::Copy | Edit::Cut => {
            let selected = text.get(selection.range()).unwrap_or_default();
            if !selected.is_empty() {
                clipboard.copy(selected);
            }
            edit == Edit::Cut && replace(text, selection, "")
        }
        Edit::Paste => clipboard
            .text()
            .is_some_and(|pasted| insert(text, selection, &pasted, lines)),
        Edit::PastePrimary => clipboard
            .primary()
            .is_some_and(|pasted| insert(text, selection, &pasted, lines)),
    }
}

/// Puts `typed` in place of the selection, dropping what a text of `lines`
/// can't hold.
fn insert(text: &mut String, selection: &mut Selection, typed: &str, lines: Lines) -> bool {
    let typed = typed.replace("\r\n", "\n");
    let typed: String = typed
        .chars()
        .filter_map(|c| match (c, lines) {
            ('\n' | '\r' | '\t', Lines::One) => Some(' '),
            ('\n' | '\t', Lines::Many) => Some(c),
            ('\r', Lines::Many) => Some('\n'),
            _ if c.is_control() => None,
            _ => Some(c),
        })
        .collect();
    replace(text, selection, &typed)
}

/// Returns whether anything was taken out or put in.
fn replace(text: &mut String, selection: &mut Selection, with: &str) -> bool {
    let range = selection.range();
    let changed = !range.is_empty() || !with.is_empty();
    text.replace_range(range.clone(), with);
    *selection = Selection::at(range.start + with.len());
    changed
}

/// Where `motion` from `at` lands.
fn to(text: &str, at: usize, motion: Motion, forward: bool) -> usize {
    let before = text.get(..at).unwrap_or_default();
    let after = text.get(at..).unwrap_or_default();
    match (motion, forward) {
        (Motion::Grapheme, false) => {
            let mut cursor = GraphemeCursor::new(at, text.len(), true);
            cursor.prev_boundary(text, 0).ok().flatten().unwrap_or(0)
        }
        (Motion::Grapheme, true) => {
            let mut cursor = GraphemeCursor::new(at, text.len(), true);
            let end = cursor.next_boundary(text, 0).ok().flatten();
            end.unwrap_or(text.len())
        }
        (Motion::Word, false) => {
            let mut graphemes = before
                .grapheme_indices(true)
                .rev()
                .skip_while(|&(_, g)| !in_word(g));
            graphemes
                .find(|&(_, g)| !in_word(g))
                .map_or(0, |(start, g)| start + g.len())
        }
        (Motion::Word, true) => {
            let mut graphemes = after
                .grapheme_indices(true)
                .skip_while(|&(_, g)| !in_word(g));
            at + graphemes
                .find(|&(_, g)| !in_word(g))
                .map_or(after.len(), |(start, _)| start)
        }
        (Motion::Line, false) => before.rfind('\n').map_or(0, |line| line + 1),
        (Motion::Line, true) => at + after.find('\n').unwrap_or(after.len()),
        (Motion::Text, false) => 0,
        (Motion::Text, true) => text.len(),
    }
}

fn in_word(grapheme: &str) -> bool {
    grapheme.chars().next().is_some_and(char::is_alphanumeric)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `text` with `|` for the caret and, if there is one, `^` for the
    /// anchor, as a string and its selection.
    fn parse(marked: &str) -> (String, Selection) {
        let mut text = String::new();
        let mut selection = Selection::default();
        let mut anchor = None;
        for c in marked.chars() {
            match c {
                '|' => selection.caret = text.len(),
                '^' => anchor = Some(text.len()),
                _ => text.push(c),
            }
        }
        selection.anchor = anchor.unwrap_or(selection.caret);
        (text, selection)
    }

    /// Applies each edit to `from`, in the marks of [`parse`], and checks
    /// it ends as `want`.
    fn check(from: &str, edits: &[Edit], want: &str) {
        let (mut text, mut selection) = parse(from);
        let mut clipboard = Clipboard::default();
        for &edit in edits {
            apply(&mut text, &mut selection, edit, &mut clipboard, Lines::One);
        }
        assert_eq!((text, selection), parse(want), "{from} to {want}");
    }

    const fn moves(motion: Motion, forward: bool) -> Edit<'static> {
        Edit::Move {
            motion,
            forward,
            extend: false,
        }
    }

    const fn extends(motion: Motion, forward: bool) -> Edit<'static> {
        Edit::Move {
            motion,
            forward,
            extend: true,
        }
    }

    const fn deletes(motion: Motion, forward: bool) -> Edit<'static> {
        Edit::Delete { motion, forward }
    }

    fn press(key: Key, modifiers: Modifiers) -> Option<Edit<'static>> {
        Edit::from_key(KeyPress {
            key,
            repeat: false,
            modifiers,
        })
    }

    #[test]
    fn typing_goes_in_at_the_caret() {
        check("z|l", &[Edit::Insert("e")], "ze|l");
    }

    #[test]
    fn graphemes_move_and_delete_whole() {
        let family = "👨‍👩‍👧";
        let from = format!("a{family}|b");
        check(&from, &[deletes(Motion::Grapheme, false)], "a|b");
        check(
            &from,
            &[moves(Motion::Grapheme, false)],
            &format!("a|{family}b"),
        );
        check("ae\u{301}|", &[deletes(Motion::Grapheme, false)], "a|");
    }

    #[test]
    fn words_stop_at_path_separators() {
        let left = moves(Motion::Word, false);
        check("res/Msgus/zel_03|", &[left, left], "res/Msgus/|zel_03");
        check(
            "res/|Msgus/zel",
            &[moves(Motion::Word, true)],
            "res/Msgus|/zel",
        );
        check(
            "res/Msgus/|zel",
            &[deletes(Motion::Word, false)],
            "res/|zel",
        );
    }

    #[test]
    fn select_all_then_typing_replaces_it() {
        check("ze|l", &[Edit::SelectAll, Edit::Insert("bmg")], "bmg|");
    }

    #[test]
    fn shift_extends_and_an_arrow_collapses() {
        let right = extends(Motion::Grapheme, true);
        check("a|bcd", &[right, right], "a^bc|d");
        check("a^bc|d", &[moves(Motion::Grapheme, false)], "a|bcd");
        check("a^bc|d", &[moves(Motion::Word, false)], "|abcd");
    }

    #[test]
    fn deleting_takes_the_selection_first() {
        check("a^bc|d", &[deletes(Motion::Word, false)], "a|d");
    }

    #[test]
    fn cut_then_paste_moves_text() {
        let edits = [Edit::Cut, moves(Motion::Text, true), Edit::Paste];
        check("|zel^_03", &edits, "_03zel|");
    }

    #[test]
    fn a_middle_click_pastes_the_primary_selection() {
        let (mut text, mut selection) = parse("ab|");
        let mut clipboard = Clipboard::default();
        clipboard.select("zel");
        let edit = Edit::PastePrimary;
        apply(&mut text, &mut selection, edit, &mut clipboard, Lines::One);
        let want = if clipboard.pastes_primary() {
            "abzel|"
        } else {
            "ab|"
        };
        assert_eq!((text, selection), parse(want));
    }

    #[test]
    fn copying_nothing_keeps_the_clipboard() {
        let start = moves(Motion::Text, false);
        check(
            "^ab|c",
            &[Edit::Copy, start, Edit::Copy, Edit::Paste],
            "ab|abc",
        );
    }

    #[test]
    fn one_line_turns_breaks_into_spaces() {
        check("|", &[Edit::Insert("a\r\nb\tc\u{7}")], "a b c|");
    }

    #[test]
    fn many_lines_keep_breaks_and_move_by_line() {
        let (mut text, mut selection) = parse("|");
        let mut clipboard = Clipboard::default();
        let edit = Edit::Insert("ab\r\ncd");
        apply(&mut text, &mut selection, edit, &mut clipboard, Lines::Many);
        let home = moves(Motion::Line, false);
        apply(&mut text, &mut selection, home, &mut clipboard, Lines::Many);
        assert_eq!((text, selection), parse("ab\n|cd"));
    }

    #[test]
    fn a_selection_past_the_end_is_brought_back() {
        let mut text = "ab".to_owned();
        let mut selection = Selection::at(9);
        let edit = deletes(Motion::Grapheme, false);
        apply(
            &mut text,
            &mut selection,
            edit,
            &mut Clipboard::default(),
            Lines::One,
        );
        assert_eq!((text.as_str(), selection), ("a", Selection::at(1)));
    }

    #[test]
    fn keys_map_to_edits() {
        let none = Modifiers::default();
        let shift = Modifiers::SHIFT;
        let command = Modifiers::COMMAND;
        assert_eq!(press(Key::Char('a'), command), Some(Edit::SelectAll));
        assert_eq!(press(Key::Char('a'), none), None);
        assert_eq!(press(Key::Char('v'), command), Some(Edit::Paste));
        assert_eq!(press(Key::Insert, shift), Some(Edit::Paste));
        assert_eq!(press(Key::Delete, shift), Some(Edit::Cut));
        assert_eq!(
            press(Key::Left, shift),
            Some(extends(Motion::Grapheme, false))
        );
        assert_eq!(press(Key::End, none), Some(moves(Motion::Line, true)));
        assert_eq!(
            press(Key::Backspace, none),
            Some(deletes(Motion::Grapheme, false))
        );
    }
}
