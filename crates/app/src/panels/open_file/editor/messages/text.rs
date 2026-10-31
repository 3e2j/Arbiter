//! A message's text as the inspector edits it, wrapped to the panel, with
//! its tags as chips that move, select and delete whole.
//!
//! Typing `{` opens a [`choice_list`] of the tags the tables name, narrowed
//! by what's typed after it, name or notes. Enter or a click picks, and `{{`
//! types a `{`. A tag whose argument the tables name values for, like a
//! color, then opens a list of them, as clicking its chip does later.
//!
//! The Raw view is the same text with every tag written out raw, `{group:code}`
//! or `{group:code:args}` with the argument bytes in hex, and `{{` for a `{`.
//! It's how a game without tables, or a tag they don't name, gets one.
//! Copying from either view copies it raw, and pasting raw text into the Text
//! view turns its tags into chips.
//!
//! Every change is written as it's typed, unless it doesn't read as raw text
//! or the file's encoding can't hold it, which turns the box red with why
//! under it. Whenever it doesn't have the keyboard it shows the file's text.

use std::fmt::Write;

use formats::games::Entry;
use formats::games::tp::Version;
use formats::games::tp::jsystem::jmessage::{Args, Tag, find_tag, tags, text_color, values};
use formats::jsystem::jmessage::bmg::{Encoding, Message, MessageKey, TextSegment};
use gui::canvas::Color;
use gui::input::{CursorIcon, Key, Modifiers, PointerButton, TypedInput};
use gui::ui::{Align, Anchor, Element, Length, TextCaret, TextHighlight, TextStyle, Ui};

use super::{push_tag, tag_swatch, tag_tint};
use crate::components::{
    Atom, Chip, Choice, Dropdown, Run, Search, chip_on, choice_list, segmented, wrap,
};
use crate::edit::{self, Edit, Lines, Motion, Selection};
use crate::keymap::{self, ChordAction, KeyContext};
use crate::theme;

/// Supplementary Private Use Area-A, which no decoder produces. A tag stands
/// in the text as the character this far past it plus its index.
const PLANE: u32 = 0xF_0000;
const TAG_LEN: usize = '\u{F0000}'.len_utf8();
const CARET_WIDTH: f32 = 2.;
const MENU_WIDTH: f32 = 240.;

/// What a message's [`MessageText::ui`] keeps between passes, owned by the
/// inspector.
#[derive(Default)]
pub struct MessageText {
    /// The message it holds the text of.
    key: Option<MessageKey>,
    /// Showing the Raw view.
    raw: bool,
    text: String,
    selection: Selection,
    /// What each tag character stands for, by its index.
    tags: Vec<RawTag>,
    /// Each tag's chip label, end to end, and where each ends.
    labels: String,
    label_ends: Vec<usize>,
    /// Each tag's chip swatch.
    swatches: Vec<Option<Color>>,
    /// Laid out to the box's width, in order, and each line's.
    pieces: Vec<Piece>,
    lines: Vec<Line>,
    menu: Option<Menu>,
    /// Where Up and Down aim across, kept while they repeat.
    goal: Option<f32>,
    /// Why the text isn't written.
    refused: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RawTag {
    group: u8,
    code: u16,
    args: Box<[u8]>,
}

/// An open menu, picking a value for the tag at `of`, or a tag to insert
/// when `None`.
struct Menu {
    list: Dropdown,
    of: Option<usize>,
}

impl Menu {
    const TAGS: Self = Self {
        list: Dropdown::OPEN,
        of: None,
    };

    const fn values_of(index: usize) -> Self {
        Self {
            list: Dropdown::OPEN,
            of: Some(index),
        }
    }
}

/// A stretch of one line: text, or one tag.
#[derive(Clone, Copy, Debug)]
struct Piece {
    start: usize,
    end: usize,
    x: f32,
    width: f32,
    tag: Option<usize>,
}

/// One line as laid out, soft wrapped or ended by a line break.
#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    end: usize,
    /// Its top, from the text's.
    y: f32,
    /// Where its pieces end in `pieces`.
    pieces: usize,
}

impl MessageText {
    /// `message`'s text in an editor with a Text and Raw switch over it,
    /// its tags named by `version`'s tables. Returns its help while it's
    /// under the pointer and not being typed in, and the text typed, once it
    /// reads and `encoding` holds it.
    #[track_caller]
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        message: &Message,
        encoding: Encoding,
        version: Option<Version>,
    ) -> (Option<&'static str>, Option<Vec<TextSegment>>) {
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let switch = Element {
            size: [Length::Grow, Length::Fit],
            padding: [s.gap, s.gap / 4., s.gap, s.gap / 2.],
            ..Element::DEFAULT
        };
        if let Some(view) = ui.element(switch, |ui| {
            segmented(ui, &["Text", "Raw"], self.raw.into())
        }) {
            self.raw = view == 1;
            self.key = None;
        }
        let padding = s.gap / 2.;
        let element = Element {
            size: [Length::Grow, Length::Fit],
            min: [0., 3. * s.row + 2. * padding],
            padding: [padding; 4],
            background: Some(c.sunken),
            border: Some(theme.border()),
            radius: [s.radius; 4],
            clip: true,
            cursor: Some(CursorIcon::Text),
            ..Element::column()
        };
        let outer = Element {
            size: [Length::Grow, Length::Fit],
            padding: [s.gap, 0., s.gap, s.gap / 2.],
            ..Element::column()
        };
        let (hovered, typed) = ui.element(outer, |ui| {
            ui.element(element, |ui| {
                let typed = self.edit_box(ui, message, encoding, version, padding);
                (ui.hovered() && !ui.focused(), typed)
            })
        });
        if let Some(refused) = &self.refused {
            let note = TextStyle {
                size: s.text - 1.,
                ..theme.ui_text(c.err)
            };
            ui.element(outer, |ui| ui.text(note, refused));
        }
        let help = hovered.then_some(match (self.raw, version) {
            (true, _) => {
                "A tag is {group:code}, or {group:code:args} with its argument bytes in hex. {{ is a {."
            }
            (false, Some(_)) => "Type { to pick a tag, or {{ for a {.",
            (false, None) => "There are no tag tables for this game. Add tags in the Raw view.",
        });
        if !typed {
            return (help, None);
        }
        let segments = self.segments(encoding);
        self.refused = segments.as_ref().err().cloned();
        ui.redraw_in(0.);
        (help, segments.ok())
    }

    /// The open box's text, as typed and pointed at. Returns whether it
    /// changed.
    fn edit_box(
        &mut self,
        ui: &mut Ui,
        message: &Message,
        encoding: Encoding,
        version: Option<Version>,
        padding: f32,
    ) -> bool {
        let theme = theme::current();
        let c = theme.color;
        // The menu floats outside the box, so a press on it is claimed here.
        let opened = self.menu.is_some();
        if ui.pressed(PointerButton::Left)
            || ui.pressed(PointerButton::Middle)
            || (self.on_menu() && ui.input().pressed(PointerButton::Left))
        {
            ui.focus();
        }
        let focused = ui.focused();
        if !focused || self.key != Some(message.key) {
            self.load(message, encoding, version);
        }
        let style = theme.ui_text(if self.refused.is_some() {
            c.err
        } else {
            c.text
        });
        let origin = ui.rect().map(|rect| [rect.x + padding, rect.y + padding]);
        let width = ui
            .rect()
            .map_or(f32::INFINITY, |rect| rect.w - 2. * padding - CARET_WIDTH);
        self.lay_out(ui, style, width);
        let before = self.selection;
        let mut changed = false;
        if let Some(origin) = origin {
            changed |= self.point(ui, style, version, origin);
        }
        if focused {
            let (typed, done) = self.type_in(ui, style, width, version);
            changed |= typed;
            if done {
                ui.unfocus();
                self.menu = None;
            }
        }
        if changed {
            self.lay_out(ui, style, width);
        }
        self.draw(ui, style, focused);
        // Not on the pass it opens, which would search for its `{`.
        if focused && opened && self.menu.is_some() {
            changed |= self.menu(ui, version, [padding; 2]);
        }
        if self.selection != before && !self.selection.is_empty() {
            let mut raw = String::new();
            self.write_raw(self.selection.range(), &mut raw);
            ui.clipboard().select(&raw);
        }
        if focused {
            ui.style().border = Some(gui::ui::Border {
                color: if self.refused.is_some() {
                    c.err
                } else {
                    c.accent
                },
                ..theme.border()
            });
        }
        changed
    }

    /// `message`'s text, its tags as chips unless in the Raw view.
    fn load(&mut self, message: &Message, encoding: Encoding, version: Option<Version>) {
        self.key = Some(message.key);
        self.text.clear();
        self.tags.clear();
        self.labels.clear();
        self.label_ends.clear();
        self.swatches.clear();
        self.menu = None;
        self.goal = None;
        self.refused = None;
        for segment in &message.text {
            match segment {
                TextSegment::Text(bytes) => {
                    let text = encoding.decode(bytes);
                    if self.raw {
                        escape(&text, &mut self.text);
                    } else {
                        self.text.push_str(&text);
                    }
                }
                &TextSegment::Tag {
                    group,
                    code,
                    ref args,
                } => {
                    let tag = RawTag {
                        group,
                        code,
                        args: args.as_slice().into(),
                    };
                    if self.raw {
                        tag.write(&mut self.text);
                    } else {
                        self.push_tag(tag, version);
                    }
                }
            }
        }
        self.selection = Selection::at(self.text.len().min(self.selection.caret));
    }

    /// `tag` onto the end of the text, as a chip.
    fn push_tag(&mut self, tag: RawTag, version: Option<Version>) {
        let Some(ch) = tag_char(self.tags.len()) else {
            return;
        };
        self.label_tag(tag, version);
        self.text.push(ch);
    }

    /// `tag` onto the tags, with its chip's label and swatch.
    fn label_tag(&mut self, tag: RawTag, version: Option<Version>) {
        push_tag(&mut self.labels, version, tag.group, tag.code, &tag.args);
        self.label_ends.push(self.labels.len());
        self.swatches
            .push(tag_swatch(version, tag.group, tag.code, &tag.args));
        self.tags.push(tag);
    }

    /// Every chip's label and swatch again, once a tag's arguments change.
    fn relabel(&mut self, version: Option<Version>) {
        self.labels.clear();
        self.label_ends.clear();
        self.swatches.clear();
        for tag in &self.tags {
            push_tag(&mut self.labels, version, tag.group, tag.code, &tag.args);
            self.label_ends.push(self.labels.len());
            self.swatches
                .push(tag_swatch(version, tag.group, tag.code, &tag.args));
        }
    }

    fn swatch(&self, index: usize) -> Option<Color> {
        self.swatches.get(index).copied().flatten()
    }

    /// The text as the file holds it.
    fn segments(&self, encoding: Encoding) -> Result<Vec<TextSegment>, String> {
        if self.raw {
            let mut chips = Self::default();
            chips.read_raw(&self.text, None)?;
            return chips.segments(encoding);
        }
        let mut segments = Vec::new();
        let mut run = String::new();
        let flush = |run: &mut String, segments: &mut Vec<TextSegment>| {
            if run.is_empty() {
                return Ok(());
            }
            let bytes = encoding
                .encode(run)
                .ok_or_else(|| unencodable(run, encoding))?;
            segments.push(TextSegment::Text(bytes));
            run.clear();
            Ok::<_, String>(())
        };
        for ch in self.text.chars() {
            let Some(tag) = tag_index(ch).and_then(|at| self.tags.get(at)) else {
                run.push(ch);
                continue;
            };
            flush(&mut run, &mut segments)?;
            segments.push(TextSegment::Tag {
                group: tag.group,
                code: tag.code,
                args: tag.args.to_vec(),
            });
        }
        flush(&mut run, &mut segments)?;
        Ok(segments)
    }

    /// `raw` onto the end of the text, its tags as chips.
    fn read_raw(&mut self, raw: &str, version: Option<Version>) -> Result<(), String> {
        let mut rest = raw;
        while let Some(open) = rest.find('{') {
            self.text.push_str(rest.get(..open).unwrap_or_default());
            let after = rest.get(open + 1..).unwrap_or_default();
            if let Some(after) = after.strip_prefix('{') {
                self.text.push('{');
                rest = after;
                continue;
            }
            let at = raw.len() - after.len() - 1;
            let tag = after
                .find('}')
                .and_then(|close| Some((RawTag::read(after.get(..close)?)?, close)));
            let Some((tag, close)) = tag else {
                return Err(format!(
                    "The {{ at {at} doesn't open a group:code tag. Write {{{{ for a {{. Not written."
                ));
            };
            self.push_tag(tag, version);
            rest = after.get(close + 1..).unwrap_or_default();
        }
        self.text.push_str(rest);
        Ok(())
    }

    /// The text in `range` written out raw onto `out`.
    fn write_raw(&self, range: std::ops::Range<usize>, out: &mut String) {
        let text = self.text.get(range).unwrap_or_default();
        if self.raw {
            out.push_str(text);
            return;
        }
        let mut chunk = [0; 4];
        for ch in text.chars() {
            match tag_index(ch).and_then(|at| self.tags.get(at)) {
                Some(tag) => tag.write(out),
                None => escape(ch.encode_utf8(&mut chunk), out),
            }
        }
    }

    /// The label of the tag at `index`.
    fn label(&self, index: usize) -> &str {
        let start = index
            .checked_sub(1)
            .and_then(|before| self.label_ends.get(before))
            .copied()
            .unwrap_or(0);
        let end = self.label_ends.get(index).copied().unwrap_or(start);
        self.labels.get(start..end).unwrap_or_default()
    }

    /// The text broken onto lines `width` wide, line breaks included.
    fn lay_out(&mut self, ui: &mut Ui, style: TextStyle, width: f32) {
        let mut pieces = std::mem::take(&mut self.pieces);
        let mut lines = std::mem::take(&mut self.lines);
        pieces.clear();
        lines.clear();
        let height = theme::current().size.row;
        let (mut start, mut y) = (0, 0.);
        for paragraph in self.text.split('\n') {
            let mut runs = Vec::new();
            let mut from = 0;
            for (at, ch) in paragraph.char_indices() {
                let Some(index) = tag_index(ch).filter(|&index| index < self.tags.len()) else {
                    continue;
                };
                if from < at {
                    runs.push(Run::Text(paragraph.get(from..at).unwrap_or_default()));
                }
                runs.push(Run::Atom(index, self.label(index)));
                from = at + TAG_LEN;
            }
            if from < paragraph.len() {
                runs.push(Run::Text(paragraph.get(from..).unwrap_or_default()));
            }
            let measure = |ui: &mut Ui, run: Run<usize>| match run {
                Run::Text(text) => ui.measure(style, text)[0],
                Run::Atom(index, label) => Chip(self.swatch(index)).width(ui, label),
            };
            let broken = wrap(runs, width, |run| measure(ui, run));
            let (mut at, mut x) = (start, 0.);
            let mut row = None;
            for (line, run) in broken {
                if row != Some(line) {
                    if row.is_some() {
                        lines.push(Line {
                            start,
                            end: at,
                            y,
                            pieces: pieces.len(),
                        });
                        (start, y) = (at, y + height);
                    }
                    (row, x) = (Some(line), 0.);
                }
                let (len, tag) = match run {
                    Run::Text(text) => (text.len(), None),
                    Run::Atom(index, _) => (TAG_LEN, Some(index)),
                };
                let width = measure(ui, run);
                pieces.push(Piece {
                    start: at,
                    end: at + len,
                    x,
                    width,
                    tag,
                });
                at += len;
                x += width;
            }
            lines.push(Line {
                start,
                end: at,
                y,
                pieces: pieces.len(),
            });
            (start, y) = (at + 1, y + height);
        }
        self.pieces = pieces;
        self.lines = lines;
    }

    /// The pieces of the line at `index`.
    fn line_pieces(&self, index: usize) -> &[Piece] {
        let first = index
            .checked_sub(1)
            .and_then(|before| self.lines.get(before))
            .map_or(0, |line| line.pieces);
        let last = self.lines.get(index).map_or(first, |line| line.pieces);
        self.pieces.get(first..last).unwrap_or_default()
    }

    /// Where a click at `[x, y]` from the text's top left puts the caret.
    fn index_at(&self, ui: &mut Ui, style: TextStyle, [x, y]: [f32; 2]) -> usize {
        let height = theme::current().size.row;
        let below = self.lines.iter().position(|line| y < line.y + height);
        let row = below.unwrap_or(self.lines.len().saturating_sub(1));
        let Some(line) = self.lines.get(row) else {
            return 0;
        };
        let Some(piece) = self
            .line_pieces(row)
            .iter()
            .rev()
            .find(|piece| x >= piece.x)
        else {
            return line.start;
        };
        if piece.tag.is_some() {
            return if x < piece.x + piece.width / 2. {
                piece.start
            } else {
                piece.end
            };
        }
        let text = self.text.get(piece.start..piece.end).unwrap_or_default();
        piece.start + ui.index_for_x(style, text, x - piece.x)
    }

    /// The tag whose chip is at `[x, y]` from the text's top left.
    fn tag_at(&self, [x, y]: [f32; 2]) -> Option<usize> {
        let height = theme::current().size.row;
        let row = self
            .lines
            .iter()
            .position(|line| (line.y..line.y + height).contains(&y))?;
        self.line_pieces(row)
            .iter()
            .find(|piece| (piece.x..piece.x + piece.width).contains(&x))?
            .tag
    }

    /// The line the chip of the tag at `index` is on, and how far across.
    fn chip_at(&self, index: usize) -> (usize, f32) {
        (0..self.lines.len())
            .find_map(|row| {
                let piece = self
                    .line_pieces(row)
                    .iter()
                    .find(|piece| piece.tag == Some(index))?;
                Some((row, piece.x))
            })
            .unwrap_or_default()
    }

    /// The caret's line, and how far across it is.
    fn caret_at(&self, ui: &mut Ui, style: TextStyle) -> (usize, f32) {
        let caret = self.selection.caret;
        let row = self
            .lines
            .iter()
            .position(|line| (line.start..=line.end).contains(&caret))
            .unwrap_or(0);
        let piece = self
            .line_pieces(row)
            .iter()
            .find(|piece| (piece.start..=piece.end).contains(&caret));
        let x = piece.map_or(0., |piece| match piece.tag {
            Some(_) if caret == piece.start => piece.x,
            Some(_) => piece.x + piece.width,
            None => {
                let text = self.text.get(piece.start..piece.end).unwrap_or_default();
                piece.x + ui.x_for_index(style, text, caret - piece.start)
            }
        });
        (row, x)
    }

    /// Moves the caret to the pointer, or pastes there, and opens a chip's
    /// values when it's clicked. Returns whether the text changed.
    fn point(
        &mut self,
        ui: &mut Ui,
        style: TextStyle,
        version: Option<Version>,
        [left, top]: [f32; 2],
    ) -> bool {
        let Some(pointer) = ui.input().pointer() else {
            return false;
        };
        let on_menu = self.on_menu();
        let middle = ui.pressed(PointerButton::Middle) && ui.clipboard().pastes_primary();
        let press = ui.pressed(PointerButton::Left);
        let dragged = ui.held(PointerButton::Left);
        if on_menu || !(press || dragged || middle) {
            return false;
        }
        let at = [pointer[0] - left, pointer[1] - top];
        let index = self.index_at(ui, style, at);
        self.goal = None;
        let extend = ui.input().modifiers().contains(Modifiers::SHIFT);
        if (press && !extend) || middle {
            self.selection = Selection::at(index);
        } else {
            self.selection.caret = index;
        }
        // Only the press, as the passes while it's held would close what it
        // opened.
        if press || middle {
            let valued = |raw: &RawTag| {
                version
                    .and_then(|version| find_tag(version, raw.group, raw.code))
                    .is_some_and(|tag| tag.values.is_some())
            };
            self.menu = self
                .tag_at(at)
                .filter(|&tag| press && !extend && self.tags.get(tag).is_some_and(valued))
                .map(Menu::values_of);
        }
        middle
            && ui
                .clipboard()
                .primary()
                .is_some_and(|pasted| self.paste(&pasted))
    }

    /// Whether the pointer was over the menu last pass.
    fn on_menu(&self) -> bool {
        self.menu.as_ref().is_some_and(|menu| menu.list.over())
    }

    /// `pasted` at the caret: raw text with its tags as chips in the Text
    /// view, or as it is if it doesn't read.
    fn paste(&mut self, pasted: &str) -> bool {
        let pasted: String = pasted
            .chars()
            .filter(|&ch| tag_index(ch).is_none())
            .collect();
        if self.raw {
            return self.insert(&pasted);
        }
        let range = self.selection.range();
        let mut chips = Self {
            tags: std::mem::take(&mut self.tags),
            labels: std::mem::take(&mut self.labels),
            label_ends: std::mem::take(&mut self.label_ends),
            swatches: std::mem::take(&mut self.swatches),
            ..Self::default()
        };
        // Tags keep the label the raw text gave them, named or not.
        let read = chips.read_raw(&pasted, None).is_ok();
        self.tags = chips.tags;
        self.labels = chips.labels;
        self.label_ends = chips.label_ends;
        self.swatches = chips.swatches;
        let with = if read { chips.text } else { pasted };
        let changed = !range.is_empty() || !with.is_empty();
        self.text.replace_range(range.clone(), &with);
        self.selection = Selection::at(range.start + with.len());
        changed
    }

    fn insert(&mut self, typed: &str) -> bool {
        let (text, selection) = (&mut self.text, &mut self.selection);
        let mut unused = gui::clipboard::Clipboard::default();
        edit::apply(
            text,
            selection,
            Edit::Insert(typed),
            &mut unused,
            Lines::Many,
        )
    }

    /// Applies the keys and text typed. Returns whether the text changed,
    /// and whether Escape let go.
    fn type_in(
        &mut self,
        ui: &mut Ui,
        style: TextStyle,
        width: f32,
        version: Option<Version>,
    ) -> (bool, bool) {
        let (mut changed, mut done) = (false, false);
        let input = ui.input();
        for typed in input.typed() {
            match typed {
                TypedInput::Text(text) => changed |= self.type_text(text, version),
                TypedInput::Key(press) => {
                    if let Some(menu) = &self.menu {
                        // The menu takes these, and the `{` menu the text typed.
                        let search = menu.of.is_none();
                        match press.key {
                            Key::Up
                            | Key::Down
                            | Key::Enter
                            | Key::Tab
                            | Key::Escape
                            | Key::Shift
                            | Key::Alt => continue,
                            Key::Backspace if search => continue,
                            Key::Char(_) if search && press.modifiers.types() => continue,
                            _ => self.menu = None,
                        }
                    }
                    match press.key {
                        Key::Enter => {
                            changed |= self.insert("\n");
                            continue;
                        }
                        Key::Escape => {
                            done = true;
                            continue;
                        }
                        Key::Up | Key::Down => {
                            if changed {
                                self.lay_out(ui, style, width);
                            }
                            let (row, x) = self.caret_at(ui, style);
                            let x = *self.goal.get_or_insert(x);
                            let row = if press.key == Key::Up {
                                row.checked_sub(1)
                            } else {
                                Some(row + 1).filter(|&row| row < self.lines.len())
                            };
                            let height = theme::current().size.row;
                            let index = match row.and_then(|row| self.lines.get(row)) {
                                Some(line) => self.index_at(ui, style, [x, line.y + height / 2.]),
                                None if press.key == Key::Up => 0,
                                None => self.text.len(),
                            };
                            self.selection.caret = index;
                            if !press.modifiers.contains(Modifiers::SHIFT) {
                                self.selection.anchor = index;
                            }
                            continue;
                        }
                        _ => {}
                    }
                    self.goal = None;
                    let Some(ChordAction::Edit(edit)) = keymap::resolve(press, &[KeyContext::TEXT])
                    else {
                        continue;
                    };
                    changed |= self.edit(ui, edit);
                }
            }
        }
        (changed, done)
    }

    /// Typed `text`, opening the menu at a `{` in the Text view when
    /// `version` has tables, or narrowing it while it's open.
    fn type_text(&mut self, text: &str, version: Option<Version>) -> bool {
        self.goal = None;
        let mut changed = false;
        for ch in text.chars().filter(|&ch| tag_index(ch).is_none()) {
            if let Some(menu) = self.menu.as_ref().filter(|menu| menu.of.is_none()) {
                if ch == '{' && menu.list.query().is_empty() {
                    self.menu = None;
                    changed |= self.insert("{");
                }
            } else if ch == '{' && !self.raw && version.is_some() {
                self.menu = Some(Menu::TAGS);
            } else {
                self.menu = None;
                let mut chunk = [0; 4];
                changed |= self.insert(ch.encode_utf8(&mut chunk));
            }
        }
        changed
    }

    /// Carries out `edit`, copying and pasting raw text.
    fn edit(&mut self, ui: &mut Ui, edit: Edit) -> bool {
        match edit {
            Edit::Copy | Edit::Cut => {
                let mut raw = String::new();
                self.write_raw(self.selection.range(), &mut raw);
                if !raw.is_empty() {
                    ui.clipboard().copy(&raw);
                }
                let delete = Edit::Delete {
                    motion: Motion::Grapheme,
                    forward: true,
                };
                edit == Edit::Cut && !raw.is_empty() && self.apply(ui, delete)
            }
            Edit::Paste => ui
                .clipboard()
                .text()
                .is_some_and(|pasted| self.paste(&pasted)),
            Edit::PastePrimary => ui
                .clipboard()
                .primary()
                .is_some_and(|pasted| self.paste(&pasted)),
            _ => self.apply(ui, edit),
        }
    }

    fn apply(&mut self, ui: &mut Ui, edit: Edit) -> bool {
        let (text, selection) = (&mut self.text, &mut self.selection);
        edit::apply(text, selection, edit, ui.clipboard(), Lines::Many)
    }

    /// Each line, with the selection under it and the caret while `focused`.
    fn draw(&self, ui: &mut Ui, style: TextStyle, focused: bool) {
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let range = self.selection.range();
        let caret = focused.then_some(self.selection.caret);
        let bar = |index| TextCaret {
            index,
            width: CARET_WIDTH,
            color: c.text,
        };
        let mut drawn = caret.is_none();
        let mut highlight = Vec::new();
        // The last color chip's, until the next.
        let mut tint = style.color;
        for (row, line) in self.lines.iter().enumerate() {
            ui.element(Element::line(s.row), |ui| {
                for piece in self.line_pieces(row) {
                    let here = caret.filter(|_| !drawn);
                    if let Some(index) = piece.tag {
                        if let Some(swatch) = self.swatch(index) {
                            let args = self.tags.get(index).map(|tag| &*tag.args);
                            tint =
                                tag_tint(swatch, args.unwrap_or_default()).unwrap_or(style.color);
                        }
                        if here == Some(piece.start) {
                            ui.text_layered(style, "", &[], Some(bar(0)));
                            drawn = true;
                        }
                        let selected = range.start <= piece.start && piece.end <= range.end;
                        let fill = if focused && selected {
                            c.selection
                        } else {
                            c.selected
                        };
                        chip_on(ui, self.label(index), self.swatch(index), fill);
                        continue;
                    }
                    let text = self.text.get(piece.start..piece.end).unwrap_or_default();
                    highlight.clear();
                    let start = range.start.clamp(piece.start, piece.end) - piece.start;
                    let end = range.end.clamp(piece.start, piece.end) - piece.start;
                    if focused && start < end {
                        highlight.push(TextHighlight {
                            range: start..end,
                            color: c.selection,
                        });
                    }
                    let at = here
                        .filter(|caret| (piece.start..=piece.end).contains(caret))
                        .map(|caret| bar(caret - piece.start));
                    drawn |= at.is_some();
                    let tinted = TextStyle {
                        color: tint,
                        ..style
                    };
                    ui.text_layered(tinted, text, &highlight, at);
                }
                if caret.filter(|_| !drawn) == Some(line.end) {
                    ui.text_layered(style, "", &[], Some(bar(0)));
                    drawn = true;
                }
            });
        }
    }

    /// The open menu, under the caret for the `{` menu or under the chip it
    /// picks a value for, `padding` in from the box's top left. Returns
    /// whether a pick changed the text.
    #[track_caller]
    fn menu(&mut self, ui: &mut Ui, version: Option<Version>, [left, top]: [f32; 2]) -> bool {
        let Some(of) = self.menu.as_ref().map(|menu| menu.of) else {
            return false;
        };
        let theme = theme::current();
        let (row, x) = match of {
            Some(index) => self.chip_at(index),
            None => self.caret_at(ui, theme.ui_text(theme.color.text)),
        };
        let below = top + self.lines.get(row).map_or(0., |line| line.y) + theme.size.row;
        let float = Anchor::Parent {
            parent: [Align::Start; 2],
            own: [Align::Start; 2],
            offset: [left + x, below],
            clipped: false,
        };
        match of {
            Some(index) => self.value_menu(ui, version, index, float),
            None => self.tag_menu(ui, version, float),
        }
    }

    /// The tags `version`'s tables name, to insert one.
    #[track_caller]
    fn tag_menu(&mut self, ui: &mut Ui, version: Option<Version>, float: Anchor) -> bool {
        let Some(menu) = &mut self.menu else {
            return false;
        };
        let rows: Vec<&Tag> = version.into_iter().flat_map(tags).collect();
        let codes: Vec<String> = rows
            .iter()
            .map(|tag| {
                if tag.any_code {
                    format!("{}.*", tag.group)
                } else {
                    format!("{}.{}", tag.group, tag.code)
                }
            })
            .collect();
        let choices: Vec<Choice> = rows
            .iter()
            .zip(&codes)
            .map(|(tag, code)| Choice::new(tag.name).detail(code).note(tag.notes))
            .collect();
        let search = Search {
            title: "Twilight Princess tags",
            placeholder: "Search tags",
        };
        let list = &mut menu.list;
        let chosen = choice_list(ui, list, &choices, None, Some(search), MENU_WIDTH, float);
        if !list.is_open() {
            self.menu = None;
        }
        let Some(tag) = chosen.picked.and_then(|at| rows.get(at)) else {
            return false;
        };
        let index = self.tags.len();
        let raw = RawTag {
            group: tag.group,
            code: tag.code,
            args: new_args(tag.args, 0),
        };
        let inserted = self.insert_tag(raw, version);
        self.menu = (inserted && tag.values.is_some()).then(|| Menu::values_of(index));
        inserted
    }

    /// The values `version`'s tables name for the argument of the tag at
    /// `index`, with a swatch for each color, to change it to one.
    #[track_caller]
    fn value_menu(
        &mut self,
        ui: &mut Ui,
        version: Option<Version>,
        index: usize,
        float: Anchor,
    ) -> bool {
        let found = self.tags.get(index).and_then(|raw| {
            let version = version?;
            let tag = find_tag(version, raw.group, raw.code)?;
            let rows: Vec<&Entry> = values(version, tag.values?).collect();
            let selected = rows
                .iter()
                .position(|entry| new_args(tag.args, entry.value) == raw.args);
            Some((tag, rows, selected))
        });
        let (Some((tag, rows, selected)), Some(menu)) = (found, &mut self.menu) else {
            self.menu = None;
            return false;
        };
        let details: Vec<String> = rows.iter().map(|entry| entry.value.to_string()).collect();
        let choices: Vec<Choice> = rows
            .iter()
            .zip(&details)
            .map(|(entry, detail)| {
                let choice = Choice::new(entry.name).detail(detail).note(entry.notes);
                match (tag.args, u8::try_from(entry.value)) {
                    (Args::Color, Ok(color)) => choice.swatch(Color::hex(text_color(color))),
                    _ => choice,
                }
            })
            .collect();
        let list = &mut menu.list;
        let chosen = choice_list(ui, list, &choices, selected, None, MENU_WIDTH, float);
        if !list.is_open() {
            self.menu = None;
        }
        let Some(entry) = chosen.picked.and_then(|at| rows.get(at)) else {
            return false;
        };
        self.menu = None;
        let args = new_args(tag.args, entry.value);
        let Some(raw) = self.tags.get_mut(index).filter(|raw| raw.args != args) else {
            return false;
        };
        raw.args = args;
        self.relabel(version);
        true
    }

    /// `tag` as a chip in place of the selection.
    fn insert_tag(&mut self, tag: RawTag, version: Option<Version>) -> bool {
        let Some(ch) = tag_char(self.tags.len()) else {
            return false;
        };
        self.label_tag(tag, version);
        let mut chunk = [0; 4];
        self.insert(ch.encode_utf8(&mut chunk))
    }
}

impl RawTag {
    /// `group:code`, or `group:code:args` with the args in hex, as between
    /// a raw tag's braces.
    fn read(text: &str) -> Option<Self> {
        let mut parts = text.trim().splitn(3, ':');
        let group = parts.next()?.trim().parse().ok()?;
        let code = parts.next()?.trim().parse().ok()?;
        let hex = parts.next().unwrap_or_default().trim();
        if !hex.len().is_multiple_of(2) {
            return None;
        }
        let args = (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok())
            .collect::<Option<_>>()?;
        Some(Self { group, code, args })
    }

    /// As [`Self::read`] reads it, in braces, onto `out`.
    fn write(&self, out: &mut String) {
        let _ = write!(out, "{{{}:{}", self.group, self.code);
        if !self.args.is_empty() {
            out.push(':');
            for byte in &self.args {
                let _ = write!(out, "{byte:02X}");
            }
        }
        out.push('}');
    }
}

/// A new tag's arguments, holding `value`.
fn new_args(args: Args, value: u16) -> Box<[u8]> {
    let [high, low] = value.to_be_bytes();
    match args {
        Args::None => Box::new([]),
        Args::U8 | Args::Color | Args::Ruby => Box::new([low]),
        Args::U16 => Box::new([high, low]),
        Args::U32 => Box::new([0, 0, high, low]),
    }
}

/// `text` onto `out` with each `{` doubled, as raw text writes it.
fn escape(text: &str, out: &mut String) {
    for piece in text.split_inclusive('{') {
        out.push_str(piece);
        if piece.ends_with('{') {
            out.push('{');
        }
    }
}

fn tag_char(index: usize) -> Option<char> {
    let index = u32::try_from(index).ok().filter(|&index| index < 0xFFFE)?;
    char::from_u32(PLANE + index)
}

fn tag_index(ch: char) -> Option<usize> {
    let index = u32::from(ch)
        .checked_sub(PLANE)
        .filter(|&index| index < 0xFFFE)?;
    usize::try_from(index).ok()
}

/// Why `run` isn't written.
fn unencodable(run: &str, encoding: Encoding) -> String {
    let ch = run
        .chars()
        .find(|ch| encoding.encode(ch.encode_utf8(&mut [0; 4])).is_none())
        .unwrap_or('?');
    format!("The file's encoding, {encoding:?}, can't hold {ch:?}. Not written.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gui::input::Event;
    use gui::ui::testing::Kept;

    fn message(text: Vec<TextSegment>) -> Message {
        Message {
            id: 1,
            key: MessageKey(0),
            attributes: Box::new([]),
            text,
        }
    }

    fn sample() -> Vec<TextSegment> {
        vec![
            TextSegment::Text(b"a{b".to_vec()),
            TextSegment::Tag {
                group: 0,
                code: 7,
                args: vec![0, 30],
            },
            TextSegment::Text(b"\nc".to_vec()),
        ]
    }

    fn loaded(raw: bool) -> MessageText {
        let mut text = MessageText {
            raw,
            ..MessageText::default()
        };
        text.load(
            &message(sample()),
            Encoding::Windows1252,
            Some(Version::GcnUsa),
        );
        text
    }

    #[test]
    fn raw_text_writes_tags_out_and_reads_back() {
        let raw = loaded(true);
        assert_eq!(raw.text, "a{{b{0:7:001E}\nc");
        assert_eq!(raw.segments(Encoding::Windows1252), Ok(sample()));
    }

    #[test]
    fn chips_copy_as_raw_text_and_write_back() {
        let chips = loaded(false);
        assert_eq!(chips.label(0), "Pause 30");
        let mut raw = String::new();
        chips.write_raw(0..chips.text.len(), &mut raw);
        assert_eq!(raw, "a{{b{0:7:001E}\nc");
        assert_eq!(chips.segments(Encoding::Windows1252), Ok(sample()));
    }

    #[test]
    fn what_doesnt_read_or_encode_is_refused() {
        let mut raw = loaded(true);
        raw.text = "a {x} b".to_owned();
        assert!(raw.segments(Encoding::Windows1252).is_err());
        raw.text = "{0:7:1}".to_owned();
        assert!(raw.segments(Encoding::Windows1252).is_err());
        raw.text = "\u{30ea}".to_owned();
        assert!(raw.segments(Encoding::Windows1252).is_err());
        assert!(raw.segments(Encoding::ShiftJis).is_ok());
    }

    #[test]
    fn pasting_raw_text_makes_chips_and_other_text_stays_as_it_is() {
        let mut chips = loaded(false);
        chips.selection = Selection::at(0);
        assert!(chips.paste("{0:0}!"));
        assert!(chips.paste("{nope}"));
        let mut raw = String::new();
        chips.write_raw(0..chips.text.len(), &mut raw);
        assert_eq!(raw, "{0:0}!{{nope}a{{b{0:7:001E}\nc");
    }

    #[test]
    fn a_brace_opens_the_menu_and_two_type_one() {
        let version = Some(Version::GcnUsa);
        let mut chips = loaded(false);
        chips.selection = Selection::at(0);
        assert!(!chips.type_text("{", version));
        assert!(chips.menu.is_some());
        assert!(chips.type_text("{", version));
        assert!(chips.menu.is_none());
        assert!(chips.text.starts_with('{'));
        let mut raw = loaded(true);
        raw.selection = Selection::at(0);
        assert!(raw.type_text("{", version));
        assert!(raw.menu.is_none());
        assert!(chips.type_text("{", None));
        assert!(chips.menu.is_none());
    }

    /// A box over an empty message, driven pass by pass as the window
    /// would, keeping the text written last.
    struct Driver {
        kept: Kept,
        text: MessageText,
        held: Message,
        written: Option<Vec<TextSegment>>,
    }

    impl Driver {
        /// Clicked into, so it has the keyboard.
        fn new() -> Self {
            theme::sizes(|size| {
                size.row = 20.;
                size.text = 12.;
            });
            let mut driver = Self {
                kept: Kept::default(),
                text: MessageText::default(),
                held: message(Vec::new()),
                written: None,
            };
            driver.pass();
            driver.pass();
            driver.click([50., 70.]);
            driver
        }

        fn pass(&mut self) {
            let Self {
                kept,
                text,
                held,
                written,
            } = self;
            let (_, typed) =
                kept.pass(|ui| text.ui(ui, held, Encoding::Windows1252, Some(Version::GcnUsa)));
            *written = typed.or(written.take());
        }

        fn push(&mut self, event: Event<'static>) {
            self.kept.input.push(event);
            self.pass();
        }

        fn click(&mut self, at: [f32; 2]) {
            self.push(Event::Pointer(Some(at)));
            self.push(Event::Pressed(PointerButton::Left));
            // The host runs passes while it's held, before it comes up.
            self.pass();
            self.push(Event::Released(PointerButton::Left));
        }

        fn key(&mut self, key: Key) {
            self.push(Event::Key {
                key,
                code: None,
                repeat: false,
            });
        }

        /// Each character's key, Shift first for a capital or brace, then
        /// its text, as the window sends them.
        fn type_text(&mut self, typed: &'static str) {
            for (at, ch) in typed.char_indices() {
                if ch.is_uppercase() || "{}:".contains(ch) {
                    self.key(Key::Shift);
                }
                self.key(Key::Char(ch.to_ascii_lowercase()));
                self.push(Event::Text(typed.get(at..at + ch.len_utf8()).unwrap()));
            }
        }
    }

    #[test]
    fn down_and_enter_pick_from_the_menu() {
        let mut driver = Driver::new();
        driver.type_text("Hi{");
        driver.key(Key::Down);
        driver.key(Key::Down);
        driver.key(Key::Enter);
        // The first is marked as it opens.
        let third = tags(Version::GcnUsa).nth(2).unwrap();
        assert_eq!(
            driver.written,
            Some(vec![
                TextSegment::Text(b"Hi".to_vec()),
                TextSegment::Tag {
                    group: third.group,
                    code: third.code,
                    args: new_args(third.args, 0).into(),
                },
            ])
        );
    }

    #[test]
    fn clicking_a_menu_row_picks_it() {
        let mut driver = Driver::new();
        driver.type_text("{");
        driver.pass();
        // The menu is held in the window, so its first row is under its
        // title and search box at its top.
        driver.click([10., 63.]);
        assert!(driver.text.menu.is_none());
        let tag = TextSegment::Tag {
            group: 0,
            code: 0,
            args: Vec::new(),
        };
        assert_eq!(driver.written, Some(vec![tag]));
    }

    #[test]
    fn typing_after_a_brace_searches_and_enter_picks() {
        let mut driver = Driver::new();
        driver.type_text("{pause");
        driver.key(Key::Enter);
        let pause = TextSegment::Tag {
            group: 0,
            code: 7,
            args: vec![0, 0],
        };
        assert_eq!(driver.written, Some(vec![pause]));
    }

    #[test]
    fn a_color_opens_its_colors_and_clicking_its_chip_opens_them_again() {
        let color = |index| TextSegment::Tag {
            group: 255,
            code: 0,
            args: vec![index],
        };
        let mut driver = Driver::new();
        driver.type_text("{color");
        driver.key(Key::Enter);
        assert_eq!(driver.written, Some(vec![color(0)]));
        assert!(
            driver
                .text
                .menu
                .as_ref()
                .is_some_and(|menu| menu.of == Some(0))
        );
        driver.pass();
        driver.key(Key::Down);
        driver.key(Key::Enter);
        assert_eq!(driver.written, Some(vec![color(1)]));
        assert_eq!(driver.text.label(0), "Color Red");
        assert!(driver.text.swatch(0).is_some());
        assert!(driver.text.menu.is_none());
        // The text's top left is at [0, 26], and the chip is 6 wide.
        driver.click([3., 36.]);
        assert!(
            driver
                .text
                .menu
                .as_ref()
                .is_some_and(|menu| menu.of == Some(0))
        );
    }

    #[test]
    fn the_menu_searches_notes_too() {
        let mut driver = Driver::new();
        driver.type_text("{epona");
        driver.key(Key::Enter);
        let horse = TextSegment::Tag {
            group: 0,
            code: 34,
            args: Vec::new(),
        };
        assert_eq!(driver.written, Some(vec![horse]));
    }

    #[test]
    fn enter_breaks_the_line() {
        let mut driver = Driver::new();
        driver.type_text("a");
        driver.key(Key::Enter);
        assert_eq!(
            driver.written,
            Some(vec![TextSegment::Text(b"a\n".to_vec())])
        );
    }
}
