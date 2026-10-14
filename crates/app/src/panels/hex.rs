//! A file's bytes, sixteen to a row: the offset, each byte in hex, then the
//! bytes as ASCII. What any file opens in until its format has an editor.
//!
//! Dragging across either column selects bytes, lit in both. Ctrl+C copies
//! them as the ASCII column shows them, and Ctrl+Shift+C as hex.

use std::{fmt::Write, io, ops::Range};

use gui::{
    components::{ListScroll, indexed_list, notice, rule},
    input::{Key, Modifiers, PointerButton},
    keymap::{AppCommand, Chord, ChordAction, KeyContext, Keymap},
    ui::{Element, TextHighlight, TextStyle, Ui},
};

use serde::{Deserialize, Serialize};

use crate::workspace::Places;

/// Bytes to a row.
const ROW: usize = 16;
/// Where a row's bytes get an extra space, to count them by eye.
const HALF: usize = ROW / 2;

/// While the panel is active, see [`Picking::active`].
const HEX: KeyContext = KeyContext("hex");
/// Copies the selection as the ASCII column shows it, Ctrl+C by default.
const COPY: ChordAction = ChordAction::Command(AppCommand("hex.copy"));
/// Copies the selection as hex, Ctrl+Shift+C by default.
const COPY_HEX: ChordAction = ChordAction::Command(AppCommand("hex.copy-hex"));
const SELECT_ALL: ChordAction = ChordAction::Command(AppCommand("hex.select-all"));

enum Bytes {
    /// Asked of the [`Loader`](crate::loader::Loader), not back yet.
    Loading,
    Read(Vec<u8>),
    /// Why it couldn't be read.
    Failed(String),
}

pub struct Hex {
    /// Relative to the project's root.
    path: String,
    bytes: Bytes,
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

/// A row's columns, kept between rows so a pass formats into the same three.
#[derive(Default)]
struct Columns {
    offset: String,
    hex: String,
    ascii: String,
}

/// A file's tab as the session keeps it. Hex is its only view until a
/// format has an editor, and each view keeps its own place, so flicking
/// between them doesn't lose it.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct FileState {
    /// Relative to the project's root.
    path: String,
    #[serde(default)]
    view: View,
    #[serde(default)]
    hex: HexState,
}

#[derive(Serialize, Deserialize, Clone, Copy, Default, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum View {
    #[default]
    Hex,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Eq)]
struct HexState {
    /// The first byte of the row at the top, which outlasts a change to
    /// [`ROW`].
    offset: usize,
}

impl Hex {
    /// A file's tab, not the hex view's, so the session's tab holds once
    /// the file can show in other views.
    pub const KIND: &str = "file";
    pub const PLACES: Places = Places::MAIN;

    /// Shows `path`, relative to the project's root, once it's
    /// [`Self::load`]ed.
    pub const fn new(path: String) -> Self {
        Self {
            path,
            bytes: Bytes::Loading,
            scroll: ListScroll::TOP,
            picking: Picking::NONE,
        }
    }

    /// Binds the copies and Ctrl+A (Command on macOS) in `keymap`, once for
    /// every hex view.
    pub fn bind(keymap: &mut Keymap) {
        let command = Modifiers::COMMAND;
        let c = Key::Char('c');
        keymap.bind(Chord::new(c, command), HEX, Some(COPY));
        keymap.bind(Chord::new(Key::Insert, Modifiers::CTRL), HEX, Some(COPY));
        let shifted = command.with(Modifiers::SHIFT);
        keymap.bind(Chord::new(c, shifted), HEX, Some(COPY_HEX));
        keymap.bind(Chord::new(Key::Char('a'), command), HEX, Some(SELECT_ALL));
    }

    /// Scrolled where `state` left it, once it's [`Self::load`]ed.
    pub fn from_state(state: FileState) -> Self {
        let View::Hex = state.view;
        Self {
            scroll: ListScroll {
                top: state.hex.offset / ROW,
                offset: 0.,
            },
            ..Self::new(state.path)
        }
    }

    pub fn state(&self) -> FileState {
        FileState {
            path: self.path.clone(),
            view: View::Hex,
            hex: HexState {
                offset: self.scroll.top * ROW,
            },
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// The file's name.
    pub fn title(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// Shows what reading the file gave, where it was scrolled to, with
    /// nothing selected. A list that's now shorter clamps it.
    pub fn load(&mut self, read: io::Result<Vec<u8>>) {
        self.picking.selection = None;
        self.bytes = match read {
            Ok(bytes) => Bytes::Read(bytes),
            Err(err) => Bytes::Failed(format!("Can't read {}: {err}", self.path)),
        };
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        if ui.input().pressed(PointerButton::Left) {
            self.picking.active = ui.hovered();
        }
        let bytes = match &self.bytes {
            Bytes::Loading => return notice(ui, "Loading…"),
            Bytes::Failed(err) => return notice(ui, err),
            Bytes::Read(bytes) if bytes.is_empty() => return notice(ui, "Empty file"),
            Bytes::Read(bytes) => bytes,
        };
        let theme = ui.theme();
        let (c, s) = (theme.color, theme.size);
        let style = |color| TextStyle {
            font: theme.font.buffer,
            ..theme.ui_text(color)
        };
        self.picking.keys(ui, bytes);
        let mut heading = Columns::default();
        header(&mut heading);
        line(ui, style(c.dim), style(c.dim), &heading, false, None);
        rule(ui);
        let selected = self.picking.selection.map(Selection::range);
        let pointer = ui.input().pointer();
        let mut columns = Columns::default();
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
                columns.fill(start, row);
                let lit = selected.as_ref().and_then(|sel| lit(sel, start, row.len()));
                let rect = ui.rect();
                let lefts = line(ui, style(c.dim), style(c.text), &columns, true, lit);
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
                let byte = row * ROW + column.byte(glyph(ui, style(c.text), &heading.hex, x));
                Some((column, byte.min(bytes.len() - 1)))
            };
            self.picking.drag(ui, held, bytes, at);
        });
    }
}

impl Picking {
    const NONE: Self = Self {
        selection: None,
        dragging: None,
        active: false,
    };

    /// Copies or selects as `ui`'s keys say, while the panel is active and
    /// nothing else has the keyboard.
    fn keys(&mut self, ui: &mut Ui, bytes: &[u8]) {
        if !self.active || ui.typing() {
            return;
        }
        let input = ui.input();
        for &press in input.keys() {
            let action = ui.keymap().resolve(press, &[HEX]);
            let picked = self.picked(bytes);
            match action {
                Some(COPY) if !picked.is_empty() => ui.clipboard().copy(&as_shown(picked)),
                Some(COPY_HEX) if !picked.is_empty() => ui.clipboard().copy(&as_hex(picked)),
                Some(SELECT_ALL) => {
                    self.selection = Some(Selection {
                        anchor: 0,
                        caret: bytes.len() - 1,
                    });
                    ui.clipboard().select(&as_shown(bytes));
                }
                _ => {}
            }
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
        if self.selection != Some(selection) {
            self.selection = Some(selection);
            // The rows were lit before the drag moved it.
            ui.stale();
        }
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

impl Columns {
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

/// Names the columns: each byte's place in its row, over its hex.
fn header(columns: &mut Columns) {
    columns.offset.clear();
    columns.offset.push_str("Offset  ");
    columns.hex.clear();
    for at in 0..ROW {
        if at > 0 {
            columns.hex.push(' ');
        }
        if at == HALF {
            columns.hex.push(' ');
        }
        let _ = write!(columns.hex, "{at:02X}");
    }
    columns.ascii.clear();
}

/// One row of `columns`, the offset in `dim` and the rest in `text`, lit
/// under the pointer while `hover`, with the `selected` bytes, from the
/// row's first, lit in both columns. Returns where the hex and the ASCII
/// started across last pass.
fn line(
    ui: &mut Ui,
    dim: TextStyle,
    text: TextStyle,
    columns: &Columns,
    hover: bool,
    selected: Option<Range<usize>>,
) -> Option<[f32; 2]> {
    let theme = ui.theme();
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
        ui.text(dim, &columns.offset);
        let hex = ui.text_layered(text, &columns.hex, hex.as_slice(), None)?;
        if columns.ascii.is_empty() {
            return None;
        }
        let ascii = ui.text_layered(dim, &columns.ascii, ascii.as_slice(), None)?;
        Some([hex.x, ascii.x])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_shows_its_offset_hex_and_ascii() {
        let mut columns = Columns::default();
        columns.fill(0x20, b"RARC\x00\x01 ~\xff0123456");
        assert_eq!(columns.offset, "00000020");
        assert_eq!(
            columns.hex,
            "52 41 52 43 00 01 20 7E  FF 30 31 32 33 34 35 36"
        );
        assert_eq!(columns.ascii, "RARC.. ~.0123456");
    }

    #[test]
    fn a_short_row_pads_its_hex_to_a_whole_row() {
        let mut columns = Columns::default();
        columns.fill(0x10, b"ab");
        assert_eq!(columns.hex.len(), ROW * 3);
        assert!(columns.hex.starts_with("61 62 "));
        assert_eq!(columns.ascii, "ab");
    }

    #[test]
    fn its_state_keeps_the_top_rows_first_byte() {
        let state = |offset| FileState {
            path: "files/a.bin".to_owned(),
            view: View::Hex,
            hex: HexState { offset },
        };
        let hex = Hex::from_state(state(0x1A35));
        assert_eq!(hex.title(), "a.bin");
        assert_eq!(hex.state(), state(0x1A30));
    }

    #[test]
    fn a_selection_copies_as_shown_or_as_hex() {
        let bytes = b"RARC\x00\x01 ~";
        assert_eq!(as_shown(bytes), "RARC.. ~");
        assert_eq!(as_hex(bytes), "52 41 52 43 00 01 20 7E");
        assert_eq!(as_hex(&[]), "");
    }

    #[test]
    fn a_selection_includes_both_ends_either_way() {
        let forward = Selection {
            anchor: 3,
            caret: 5,
        };
        let back = Selection {
            anchor: 5,
            caret: 3,
        };
        assert_eq!(forward.range(), 3..6);
        assert_eq!(back.range(), 3..6);
        assert_eq!(
            Selection {
                anchor: 4,
                caret: 4
            }
            .range(),
            4..5
        );
    }

    #[test]
    fn each_glyph_of_a_bytes_hex_picks_it() {
        let mut columns = Columns::default();
        columns.fill(0, &[0; ROW]);
        for at in 0..ROW {
            let start = hex_at(at);
            assert_eq!(columns.hex.get(start..start + 2), Some("00"));
            assert_eq!(Column::Hex.byte(start), at);
            assert_eq!(Column::Hex.byte(start + 1), at);
            assert_eq!(Column::Ascii.byte(at), at);
        }
        // The spaces after a byte, the half's two included, pick it.
        assert_eq!(Column::Hex.byte(hex_at(HALF) - 1), HALF - 1);
        assert_eq!(Column::Hex.byte(hex_at(HALF) - 2), HALF - 1);
        assert_eq!(Column::Hex.byte(columns.hex.len() + 5), ROW - 1);
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
        let (mut heading, mut row) = (Columns::default(), Columns::default());
        header(&mut heading);
        row.fill(0, &[0; ROW]);
        assert_eq!(heading.offset.len(), row.offset.len());
        assert_eq!(heading.hex.len(), row.hex.len());
    }
}
