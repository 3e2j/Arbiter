//! A file's bytes, sixteen to a row: the offset, each byte in hex, then the
//! bytes as ASCII. The view any file can show, and the only one until its
//! format has an editor.
//!
//! Dragging across either column selects bytes, lit in both. Ctrl+C copies
//! them as the ASCII column shows them, and Ctrl+Shift+C as hex, as does the
//! right click menu.

use std::{fmt::Write, ops::Range};

use crate::components::{ContextMenu, ListScroll, MenuItem, context_menu, indexed_list, rule};
use crate::keymap::{self, AppCommand, Chord, KeyContext};
use gui::{
    input::{Key, Modifiers, PointerButton},
    ui::{Element, TextHighlight, TextStyle, Ui},
};

use serde::{Deserialize, Serialize};

use crate::commands::Command;
use crate::theme;

/// Bytes to a row.
const ROW: usize = 16;
/// Where a row's bytes get an extra space, to count them by eye.
const HALF: usize = ROW / 2;
/// Each byte's place in its row, spaced as [`RowText::fill`] spaces its hex.
const HEADER: &str = "00 01 02 03 04 05 06 07  08 09 0A 0B 0C 0D 0E 0F";

/// While the panel is active, see [`Picking::active`].
const HEX: KeyContext = KeyContext("hex");
/// Copies the selection as the ASCII column shows it, Ctrl+C by default.
const COPY: AppCommand = AppCommand("hex.copy");
/// Copies the selection as hex, Ctrl+Shift+C by default.
const COPY_HEX: AppCommand = AppCommand("hex.copy-hex");
const SELECT_ALL: AppCommand = AppCommand("hex.select-all");

pub struct Hex {
    scroll: ListScroll,
    picking: Picking,
}

/// What the user has picked out of the bytes.
struct Picking {
    selection: Option<Selection>,
    /// The column a drag picks from, while the left button is held.
    dragging: Option<Column>,
    /// Whether the last press landed on the panel, so its keys are the
    /// panel's.
    active: bool,
    menu: ContextMenu,
}

/// Bytes by their place in the file, both ends included, so a press picks
/// the byte under it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Selection {
    anchor: usize,
    caret: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Column {
    Hex,
    Ascii,
}

/// What the rows in view saw of the pointer, as last pass laid them out.
#[derive(Default)]
struct Pointed {
    /// The row under the pointer, or the nearest in view above or below it.
    row: Option<usize>,
    /// Where the hex and the ASCII start across.
    lefts: Option<[f32; 2]>,
}

/// A row's text, kept between rows so a pass formats into the same three.
#[derive(Default)]
struct RowText {
    offset: String,
    hex: String,
    ascii: String,
}

/// The hex view's place, as a file's tab keeps it in the session.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
pub struct HexState {
    /// The first byte of the row at the top.
    offset: usize,
}

impl Hex {
    pub const NEW: Self = Self {
        scroll: ListScroll::TOP,
        picking: Picking::NONE,
    };

    /// The copies and Ctrl+A (Command on macOS), once for every hex view.
    pub const COMMANDS: [Command; 3] = {
        const C: Key = Key::Char('c');
        const COMMAND: Modifiers = Modifiers::COMMAND;
        [
            Command {
                id: COPY,
                name: "Copy",
                context: HEX,
                chords: &[
                    Chord::new(C, COMMAND),
                    Chord::new(Key::Insert, Modifiers::CTRL),
                ],
            },
            Command {
                id: COPY_HEX,
                name: "Copy as hex",
                context: HEX,
                chords: &[Chord::new(C, COMMAND.with(Modifiers::SHIFT))],
            },
            Command {
                id: SELECT_ALL,
                name: "Select all",
                context: HEX,
                chords: &[Chord::new(Key::Char('a'), COMMAND)],
            },
        ]
    };

    /// Scrolled where `state` left it. A list shorter than that clamps it.
    #[must_use]
    pub const fn from_state(state: &HexState) -> Self {
        Self {
            scroll: ListScroll::at(state.offset / ROW),
            ..Self::NEW
        }
    }

    #[must_use]
    pub const fn state(&self) -> HexState {
        HexState {
            offset: self.scroll.top * ROW,
        }
    }

    /// Lets go of the selection, for bytes read again.
    pub const fn reload(&mut self) {
        self.picking.selection = None;
    }

    /// `bytes`, which aren't empty.
    pub fn ui(&mut self, ui: &mut Ui, bytes: &[u8]) {
        if ui.input().pressed(PointerButton::Left) {
            self.picking.active = ui.hovered();
        }
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let style = |color| TextStyle {
            font: theme.font.buffer,
            ..theme.ui_text(color)
        };
        self.picking.keys(ui, bytes);
        let heading = RowText {
            offset: "Offset  ".to_owned(),
            hex: HEADER.to_owned(),
            ascii: String::new(),
        };
        line(ui, style(c.dim), style(c.dim), &heading, false, None);
        rule(ui);
        let selected = self.picking.selection.map(Selection::range);
        let pointer = ui.input().pointer();
        let mut row_text = RowText::default();
        let mut seen = Pointed::default();
        let rows = bytes.len().div_ceil(ROW);
        // Holds the button for the rows, so a drag outlasts the row it
        // started on, and the thumb inside still takes its own.
        ui.element(Element::column(), |ui| {
            let held = ui.held(PointerButton::Left);
            indexed_list(ui, &mut self.scroll, rows, s.row, |ui, index| {
                let start = index * ROW;
                let row = bytes.get(start..).unwrap_or_default();
                let row = row.get(..ROW).unwrap_or(row);
                row_text.fill(start, row);
                let lit = selected.as_ref().and_then(|sel| lit(sel, start, row.len()));
                let rect = ui.rect();
                let lefts = line(ui, style(c.dim), style(c.text), &row_text, true, lit);
                if let (Some(rect), Some([_, y])) = (rect, pointer) {
                    if seen.row.is_none() || y >= rect.y {
                        seen.row = Some(index);
                    }
                    seen.lefts = seen.lefts.or(lefts);
                }
            });
            let at = |ui: &mut Ui, column: Option<Column>| {
                let ([x, _], Some(row), Some(lefts)) = (pointer?, seen.row, seen.lefts) else {
                    return None;
                };
                let column = column.or_else(|| Column::at(x, lefts))?;
                let x = x - column.left(lefts);
                let byte = row * ROW + column.byte(glyph(ui, style(c.text), HEADER, x));
                Some((column, byte.min(bytes.len() - 1)))
            };
            if self.picking.menu.open_on_right_press(ui) {
                self.picking.point(ui, at);
            } else {
                self.picking.drag(ui, held, bytes, at);
            }
        });
        self.picking.menu(ui, bytes);
    }
}

impl Picking {
    const NONE: Self = Self {
        selection: None,
        dragging: None,
        active: false,
        menu: ContextMenu::CLOSED,
    };

    /// Copies or selects as `ui`'s keys say, while the panel is active and
    /// nothing else has the keyboard.
    fn keys(&mut self, ui: &mut Ui, bytes: &[u8]) {
        if !self.active || ui.typing() {
            return;
        }
        let input = ui.input();
        for command in keymap::commands(input.keys(), &[HEX]) {
            self.act(ui, command, bytes);
        }
    }

    /// The right click menu while it's open, doing what's clicked.
    fn menu(&mut self, ui: &mut Ui, bytes: &[u8]) {
        let picked = !self.picked(bytes).is_empty();
        let items = [
            (MenuItem::new("Copy").enabled(picked), COPY),
            (MenuItem::new("Copy as Hex").enabled(picked), COPY_HEX),
            (MenuItem::new("Select All"), SELECT_ALL),
        ];
        let labels = items.map(|(item, _)| item);
        let clicked = context_menu(ui, &mut self.menu, &labels);
        if let Some((_, action)) = clicked.and_then(|index| items.get(index)) {
            self.act(ui, *action, bytes);
        }
    }

    /// Does `command` if it's one of the panel's, from its keys or its menu.
    fn act(&mut self, ui: &mut Ui, command: AppCommand, bytes: &[u8]) {
        let picked = self.picked(bytes);
        match command {
            COPY if !picked.is_empty() => ui.clipboard().copy(&as_shown(picked)),
            COPY_HEX if !picked.is_empty() => ui.clipboard().copy(&as_hex(picked)),
            SELECT_ALL => {
                self.selection = Some(Selection {
                    anchor: 0,
                    caret: bytes.len() - 1,
                });
                ui.clipboard().select(&as_shown(bytes));
            }
            _ => {}
        }
    }

    /// Selects the byte `at` finds under the pointer, for a right click,
    /// unless it's already selected so the menu acts on the selection.
    fn point(
        &mut self,
        ui: &mut Ui,
        at: impl FnOnce(&mut Ui, Option<Column>) -> Option<(Column, usize)>,
    ) {
        let Some((_, byte)) = at(ui, None) else {
            return;
        };
        let range = self.selection.map(Selection::range);
        if !range.is_some_and(|range| range.contains(&byte)) {
            self.selection = Some(Selection {
                anchor: byte,
                caret: byte,
            });
        }
    }

    /// Moves the selection to the byte `at` finds under the pointer while
    /// the button's `held`, from the column the drag started in. A press
    /// off both columns lets go of it, and Shift extends it. Letting go
    /// offers it as the primary selection.
    fn drag(
        &mut self,
        ui: &mut Ui,
        held: bool,
        bytes: &[u8],
        at: impl FnOnce(&mut Ui, Option<Column>) -> Option<(Column, usize)>,
    ) {
        if !held {
            if self.dragging.take().is_some() {
                let picked = self.picked(bytes);
                if !picked.is_empty() {
                    ui.clipboard().select(&as_shown(picked));
                }
            }
            return;
        }
        let Some((column, byte)) = at(ui, self.dragging) else {
            self.selection = None;
            return;
        };
        let extend = ui.input().modifiers().contains(Modifiers::SHIFT);
        let selection = match self.selection {
            Some(selection) if self.dragging.is_some() || extend => Selection {
                caret: byte,
                ..selection
            },
            _ => Selection {
                anchor: byte,
                caret: byte,
            },
        };
        self.dragging = Some(column);
        self.selection = Some(selection);
    }

    /// The selected bytes, empty without a selection.
    fn picked<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        let range = self.selection.map(Selection::range).unwrap_or_default();
        bytes.get(range).unwrap_or_default()
    }
}

impl Selection {
    fn range(self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret) + 1
    }
}

impl Column {
    /// The column `x` is in, given where each starts across: one runs on
    /// to where the next starts.
    fn at(x: f32, [hex, ascii]: [f32; 2]) -> Option<Self> {
        if x >= ascii {
            Some(Self::Ascii)
        } else if x >= hex {
            Some(Self::Hex)
        } else {
            None
        }
    }

    fn left(self, [hex, ascii]: [f32; 2]) -> f32 {
        match self {
            Self::Hex => hex,
            Self::Ascii => ascii,
        }
    }

    /// The byte of a row, from its first, that `glyph` of the column's text
    /// shows, or the nearest end.
    fn byte(self, glyph: usize) -> usize {
        let byte = match self {
            // The extra space at the half belongs to the byte before it.
            Self::Hex if glyph >= 3 * HALF => (glyph - 1) / 3,
            Self::Hex => glyph / 3,
            Self::Ascii => glyph,
        };
        byte.min(ROW - 1)
    }
}

/// Where byte `at` of a row starts in its hex.
fn hex_at(at: usize) -> usize {
    at * 3 + usize::from(at >= HALF)
}

/// The glyph of `ruler` drawn in `style` that `x` falls in, from its left
/// edge. The font is monospaced, so it stands in for any row's text.
fn glyph(ui: &mut Ui, style: TextStyle, ruler: &str, x: f32) -> usize {
    let index = ui.index_for_x(style, ruler, x);
    if index > 0 && x < ui.x_for_index(style, ruler, index) {
        index - 1
    } else {
        index
    }
}

/// What of `selected` falls in the row of `len` bytes from `start`, from
/// the row's first byte.
fn lit(selected: &Range<usize>, start: usize, len: usize) -> Option<Range<usize>> {
    let from = selected.start.max(start) - start;
    let to = selected.end.min(start + len).checked_sub(start)?;
    (from < to).then_some(from..to)
}

/// As the ASCII column shows them.
fn as_shown(bytes: &[u8]) -> String {
    bytes.iter().copied().map(shown).collect()
}

/// Two hex digits a byte, a space between each.
fn as_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for byte in bytes {
        if !out.is_empty() {
            out.push(' ');
        }
        let _ = write!(out, "{byte:02X}");
    }
    out
}

/// How the ASCII column shows `byte`, a dot when it isn't printable.
fn shown(byte: u8) -> char {
    if byte.is_ascii_graphic() || byte == b' ' {
        char::from(byte)
    } else {
        '.'
    }
}

impl RowText {
    /// Up to [`ROW`] bytes from `offset`, the hex padded out to a whole row's
    /// width so the ASCII lines up under a short last row.
    fn fill(&mut self, offset: usize, row: &[u8]) {
        self.offset.clear();
        let _ = write!(self.offset, "{offset:08X}");
        self.hex.clear();
        self.ascii.clear();
        for at in 0..ROW {
            if at > 0 {
                self.hex.push(' ');
            }
            if at == HALF {
                self.hex.push(' ');
            }
            match row.get(at) {
                Some(byte) => {
                    let _ = write!(self.hex, "{byte:02X}");
                }
                None => self.hex.push_str("  "),
            }
        }
        self.ascii.extend(row.iter().copied().map(shown));
    }
}

/// One row of `row_text`, the offset in `dim` and the rest in `text`, lit
/// under the pointer while `hover`, with the `selected` bytes, from the
/// row's first, lit in both columns. Returns where the hex and the ASCII
/// started across last pass.
fn line(
    ui: &mut Ui,
    dim: TextStyle,
    text: TextStyle,
    row_text: &RowText,
    hover: bool,
    selected: Option<Range<usize>>,
) -> Option<[f32; 2]> {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: 2. * s.gap,
        clip: true,
        ..Element::line(s.row)
    };
    let highlight = |range| TextHighlight {
        range,
        color: c.selection,
    };
    let hex = (selected.clone()).map(|at| highlight(hex_at(at.start)..hex_at(at.end - 1) + 2));
    let ascii = selected.map(highlight);
    ui.element(element, |ui| {
        if hover {
            ui.style().background = ui.hovered().then_some(c.hover);
        }
        ui.text(dim, &row_text.offset);
        let hex = ui.text_layered(text, &row_text.hex, hex.as_slice(), None)?;
        if row_text.ascii.is_empty() {
            return None;
        }
        let ascii = ui.text_layered(dim, &row_text.ascii, ascii.as_slice(), None)?;
        Some([hex.x, ascii.x])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_shows_its_offset_hex_and_ascii() {
        let mut row_text = RowText::default();
        row_text.fill(0x20, b"RARC\x00\x01 ~\xff0123456");
        assert_eq!(row_text.offset, "00000020");
        assert_eq!(
            row_text.hex,
            "52 41 52 43 00 01 20 7E  FF 30 31 32 33 34 35 36"
        );
        assert_eq!(row_text.ascii, "RARC.. ~.0123456");
    }

    #[test]
    fn a_short_row_pads_its_hex_to_a_whole_row() {
        let mut row_text = RowText::default();
        row_text.fill(0x10, b"ab");
        assert_eq!(row_text.hex.len(), ROW * 3);
        assert!(row_text.hex.starts_with("61 62 "));
        assert_eq!(row_text.ascii, "ab");
    }

    #[test]
    fn its_state_keeps_the_top_rows_first_byte() {
        let hex = Hex::from_state(&HexState { offset: 0x1A35 });
        assert_eq!(hex.state(), HexState { offset: 0x1A30 });
    }

    #[test]
    fn a_selection_copies_as_shown_or_as_hex() {
        let bytes = b"RARC\x00\x01 ~";
        assert_eq!(as_shown(bytes), "RARC.. ~");
        assert_eq!(as_hex(bytes), "52 41 52 43 00 01 20 7E");
        assert_eq!(as_hex(&[]), "");
    }

    #[test]
    fn each_glyph_of_a_bytes_hex_picks_it() {
        let mut row_text = RowText::default();
        row_text.fill(0, &[0; ROW]);
        for at in 0..ROW {
            let start = hex_at(at);
            assert_eq!(row_text.hex.get(start..start + 2), Some("00"));
            assert_eq!(Column::Hex.byte(start), at);
            assert_eq!(Column::Hex.byte(start + 1), at);
            assert_eq!(Column::Ascii.byte(at), at);
        }
        // The spaces after a byte, the half's two included, pick it.
        assert_eq!(Column::Hex.byte(hex_at(HALF) - 1), HALF - 1);
        assert_eq!(Column::Hex.byte(hex_at(HALF) - 2), HALF - 1);
        assert_eq!(Column::Hex.byte(row_text.hex.len() + 5), ROW - 1);
    }

    #[test]
    fn a_selection_lights_its_part_of_each_row() {
        let selected = 0x0E..0x22;
        assert_eq!(lit(&selected, 0x00, ROW), Some(0x0E..ROW));
        assert_eq!(lit(&selected, 0x10, ROW), Some(0..ROW));
        assert_eq!(lit(&selected, 0x20, ROW), Some(0..2));
        assert_eq!(lit(&selected, 0x30, ROW), None);
        assert_eq!(lit(&(0x30..0x40), 0x10, ROW), None);
        assert_eq!(lit(&(0x20..0x40), 0x20, 3), Some(0..3));
    }

    #[test]
    fn the_header_lines_up_with_a_row() {
        let mut row = RowText::default();
        row.fill(0, &[0; ROW]);
        assert_eq!(HEADER.len(), row.hex.len());
    }
}
