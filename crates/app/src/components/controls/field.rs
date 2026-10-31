//! A line of text the user types in.

use crate::edit::{self, Edit, Lines, Selection};
use crate::keymap::{self, ChordAction, KeyContext};
use crate::theme;
use gui::canvas::IconId;
use gui::clipboard::Clipboard;
use gui::input::{CursorIcon, Input, Modifiers, PointerButton, TypedInput};
use gui::ui::{Align, Direction, Element, Length, TextCaret, TextHighlight, TextStyle, Ui};

/// What a [`text_field`] holds, owned by the caller.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct TextField {
    pub text: String,
    pub selection: Selection,
    /// Set to take the keyboard next pass, such as from a shortcut.
    pub focus: bool,
    /// How far the text is moved left to keep the caret in view, in logical
    /// pixels.
    pub scroll: f32,
    /// Dimmed at the end, such as a unit.
    pub suffix: Option<&'static str>,
    /// Drawn in the error color, for text that can't be taken.
    pub invalid: bool,
}

const CARET_WIDTH: f32 = 2.;

/// What the pointer does to the field this pass.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pointing {
    Nothing,
    /// Puts the caret there, or with `extend` selects up to there.
    Click {
        extend: bool,
    },
    /// Selects from where the click was.
    Drag,
    /// Pastes the primary selection there.
    Middle,
}

/// What a pass of typing did.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct Typing {
    changed: bool,
    /// Confirmed or cancelled, which lets go of the keyboard.
    done: bool,
}

impl TextField {
    pub fn clear(&mut self) {
        self.text.clear();
        self.selection = Selection::default();
    }

    /// Applies `input`'s keys, as the keymap binds them, and text in the
    /// order they came.
    fn type_in(&mut self, input: &Input, clipboard: &mut Clipboard) -> Typing {
        let mut typing = Typing::default();
        for typed in input.typed() {
            let edit = match typed {
                TypedInput::Text(text) => Edit::Insert(text),
                TypedInput::Key(press) => match keymap::resolve(press, &[KeyContext::TEXT]) {
                    Some(ChordAction::Edit(edit)) => edit,
                    Some(ChordAction::Confirm | ChordAction::Cancel) => {
                        typing.done = true;
                        continue;
                    }
                    Some(ChordAction::Command(_)) | None => continue,
                },
            };
            let (text, selection) = (&mut self.text, &mut self.selection);
            typing.changed |= edit::apply(text, selection, edit, clipboard, Lines::One);
        }
        typing
    }

    /// Moves the caret to the pointer, or pastes there, in the text box
    /// drawn with `style`. Returns whether the text changed.
    fn point(&mut self, ui: &mut Ui, style: TextStyle, pointing: Pointing) -> bool {
        if pointing == Pointing::Nothing {
            return false;
        }
        // Where the pointer is in the text as last drawn, so past either end
        // while dragging, which scrolls to follow it.
        let (Some(rect), Some([x, _])) = (ui.rect(), ui.input().pointer()) else {
            return false;
        };
        let index = ui.index_for_x(style, &self.text, x - rect.x + self.scroll);
        match pointing {
            Pointing::Click { extend: false } | Pointing::Middle => {
                self.selection = Selection::at(index);
            }
            Pointing::Click { extend: true } | Pointing::Drag => self.selection.caret = index,
            Pointing::Nothing => {}
        }
        if pointing != Pointing::Middle {
            return false;
        }
        let edit = Edit::PastePrimary;
        let (text, selection) = (&mut self.text, &mut self.selection);
        edit::apply(text, selection, edit, ui.clipboard(), Lines::One)
    }
}

/// Fills its parent across, sunken, with `icon` before the text, then
/// `placeholder` dimmed while it's empty, then its suffix. Clicking it takes
/// the keyboard and puts the caret there, dragging or Shift clicking selects,
/// and Enter or Escape let go. Returns whether the text changed this pass.
#[track_caller]
pub fn text_field(
    ui: &mut Ui,
    field: &mut TextField,
    icon: Option<IconId>,
    placeholder: &str,
) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        background: Some(c.sunken),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        clip: true,
        cursor: Some(CursorIcon::Text),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let before = field.selection;
        // Asked every pass, as the press pass is where the box claims it.
        let dragged = ui.held(PointerButton::Left);
        let pointing = if ui.pressed(PointerButton::Middle) && ui.clipboard().pastes_primary() {
            Pointing::Middle
        } else if ui.pressed(PointerButton::Left) {
            let extend = ui.input().modifiers().contains(Modifiers::SHIFT);
            Pointing::Click { extend }
        } else if dragged {
            Pointing::Drag
        } else {
            Pointing::Nothing
        };
        if ui.pressed(PointerButton::Left) || ui.pressed(PointerButton::Middle) || field.focus {
            ui.focus();
            field.focus = false;
        }
        let focused = ui.focused();
        let mut typing = Typing::default();
        if focused {
            let input = ui.input();
            typing = field.type_in(input, ui.clipboard());
            if typing.done {
                ui.unfocus();
            }
        }
        let border = field.invalid.then_some(c.err);
        let border = border.or(focused.then_some(c.accent));
        if let (Some(color), Some(border)) = (border, &mut ui.style().border) {
            border.color = color;
        }
        if let Some(icon) = icon {
            ui.icon(icon, s.icon, c.dim);
        }
        let text = Element {
            direction: Direction::LeftToRight,
            size: [Length::Grow, Length::Fit],
            clip: true,
            ..Element::DEFAULT
        };
        ui.element(text, |ui| {
            let style = theme.ui_text(if field.invalid { c.err } else { c.text });
            typing.changed |= field.point(ui, style, pointing);
            let x = ui.x_for_index(style, &field.text, field.selection.caret);
            // Keeps the caret in view, and no more scrolled than the text
            // needs once it's shorter.
            let shown = focused && !typing.done;
            if let (true, Some(rect)) = (shown, ui.rect()) {
                let view = rect.w - CARET_WIDTH;
                let width = ui.x_for_index(style, &field.text, field.text.len());
                field.scroll = field.scroll.max(x - view).min(x).min(width - view).max(0.);
            } else if !focused {
                field.scroll = 0.;
            }
            ui.style().offset[0] = -field.scroll;
            let caret = shown.then_some(TextCaret {
                index: field.selection.caret,
                width: CARET_WIDTH,
                color: c.text,
            });
            if field.text.is_empty() {
                ui.text_layered(theme.ui_text(c.dim), placeholder, &[], caret);
            } else {
                let selection = TextHighlight {
                    range: field.selection.range(),
                    color: c.selection,
                };
                let highlights = if focused {
                    std::slice::from_ref(&selection)
                } else {
                    &[]
                };
                ui.text_layered(style, &field.text, highlights, caret);
            }
        });
        if let Some(suffix) = field.suffix {
            ui.text(theme.ui_text(c.dim), suffix);
        }
        // Offered as the primary selection whenever it changes.
        let selected = field.text.get(field.selection.range()).unwrap_or_default();
        if field.selection != before && !selected.is_empty() {
            ui.clipboard().select(selected);
        }
        typing.changed
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gui::input::{Event, Key, Modifiers};

    fn input(modifiers: Modifiers, keys: &[Key], text: &str) -> Input {
        let mut input = Input::default();
        input.push(Event::Modifiers(modifiers));
        for &key in keys {
            input.push(Event::Key {
                key,
                code: None,
                repeat: false,
            });
        }
        input.push(Event::Text(text));
        input
    }

    #[test]
    fn ctrl_a_selects_rather_than_types() {
        let mut field = TextField {
            text: "zel_03".to_owned(),
            ..TextField::default()
        };
        let input = input(Modifiers::COMMAND, &[Key::Char('a')], "a");
        let typing = field.type_in(&input, &mut Clipboard::default());
        assert!(!typing.changed);
        assert_eq!(field.selection.range(), 0..6);
    }

    #[test]
    fn copy_and_paste_go_through_the_clipboard() {
        let mut clipboard = Clipboard::default();
        let mut field = TextField {
            text: "bmg".to_owned(),
            ..TextField::default()
        };
        let keys = [
            Key::Char('a'),
            Key::Char('c'),
            Key::Char('v'),
            Key::Char('v'),
        ];
        let input = input(Modifiers::COMMAND, &keys, "");
        field.type_in(&input, &mut clipboard);
        assert_eq!(field.text, "bmgbmg");
        assert_eq!(clipboard.text().as_deref(), Some("bmg"));
    }

    #[test]
    fn enter_lets_go() {
        let mut field = TextField::default();
        let input = input(Modifiers::default(), &[Key::Enter], "");
        let typing = field.type_in(&input, &mut Clipboard::default());
        assert!(typing.done && !typing.changed);
    }
}
