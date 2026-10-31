//! A message file's messages, one to a row in file order, each row holds:
//! its position, the id the game looks it up by, then its text with tags as chips.
//!
//! A file without MID1 is looked up by position, so it has no id column.
//!
//! The TP tables name the tags. Without them, a tag shows as `{group:code}`.
//!
//! Find above narrows the list to the messages whose text holds the query,
//! never counting a tag's. A query of only digits also finds the message
//! with that id.
//!
//! Clicking a message inspects it, as [`inspect`] shows it.

mod inspect;
mod lines;

use std::fmt::Write;

use diag::Diagnostics;
use formats::games::tp::Version;
use formats::games::tp::jsystem::jmessage::{Args, find_tag};
use formats::jsystem::jmessage::bmg::{Bmg, TextSegment};
use formats::{ByteWriter, Decode, Encode};
use gui::ui::{Direction, Element, Length, TextStyle, Ui};

use crate::components::{
    ListScroll, TextField, fit_list, line_box, notice, row, rule, text_field, toggle, toolbar,
};
use crate::theme;
use lines::{Find, Lines, line};

const ID: &str = "ID";
const POSITION: &str = "Pos";

/// A message file, as every tab and the inspector see it.
pub struct MessagesDoc {
    bmg: Bmg,
    /// Whose tables name the tags and the records' fields.
    version: Option<Version>,
    /// By position. `None` for a file without MID1.
    ids: Option<Vec<u16>>,
    /// The widest id's or position's, or its heading's, so the text lines
    /// up after it.
    digits: usize,
    /// Each message's text, by position.
    lines: Lines,
}

/// How one tab looks at a [`MessagesDoc`].
#[derive(Default)]
pub struct MessagesView {
    scroll: ListScroll,
    search: TextField,
    /// Find matches case.
    case: bool,
    /// Find matches whole words only.
    word: bool,
    /// Whether `shown` and `count` are what find finds now.
    found: bool,
    /// The positions find leaves in the list, in order.
    shown: Vec<usize>,
    /// What find found, or how many messages there are.
    count: String,
    /// The position of the message last clicked.
    selected: Option<usize>,
}

impl MessagesDoc {
    /// `bytes` as messages, or `None` when they aren't a message file or
    /// don't decode. Logs what decoding noticed, with `path`.
    pub fn open(path: &str, bytes: &[u8], version: Option<Version>) -> Option<Self> {
        if !Bmg::detect(bytes) {
            return None;
        }
        let mut diag = Diagnostics::default();
        match Bmg::decode(bytes, &mut diag) {
            Ok(bmg) => {
                for item in &diag.items {
                    tracing::warn!("{path}: {}", item.message);
                }
                Some(Self::new(bmg, version))
            }
            Err(err) => {
                tracing::warn!("can't read {path} as a message file: {err}");
                None
            }
        }
    }

    /// `bmg`'s messages, its tags named by `version`'s tables when given.
    pub fn new(bmg: Bmg, version: Option<Version>) -> Self {
        let ids = bmg
            .mid1
            .map(|_| bmg.messages.iter().map(|m| m.id).collect::<Vec<_>>());
        let widest = ids
            .iter()
            .flatten()
            .copied()
            .map(usize::from)
            .chain(bmg.messages.len().checked_sub(1))
            .max()
            .unwrap_or(0);
        let lines = lines(&bmg, version);
        Self {
            bmg,
            version,
            ids,
            digits: widest.to_string().len().max(POSITION.len()),
            lines,
        }
    }

    pub fn encode(&self) -> formats::Result<Vec<u8>> {
        let mut out = ByteWriter::new();
        self.bmg.encode(&mut out)?;
        Ok(out.finish())
    }
}

impl MessagesView {
    /// Narrows the list to what find now finds in `doc`, from the top.
    fn refind(&mut self, doc: &MessagesDoc) {
        let len = doc.lines.len();
        let mut find = Find::new(&self.search.text, self.case, self.word);
        self.found = true;
        self.shown.clear();
        self.count.clear();
        self.scroll = ListScroll::TOP;
        if find.is_empty() {
            self.shown.extend(0..len);
            let _ = write!(self.count, "{len} {}", messages(len));
            return;
        }
        let number = self.search.text.trim().parse::<usize>().ok();
        let mut hits = 0;
        for index in 0..len {
            let found = find.count(&doc.lines, index)
                + usize::from(number == Some(label(doc.ids.as_deref(), index)));
            if found > 0 {
                self.shown.push(index);
                hits += found;
            }
        }
        let rows = self.shown.len();
        let _ = write!(self.count, "{hits} in {rows} {}", messages(rows));
    }

    /// Returns the position of the message clicked, if one was.
    pub fn ui(&mut self, ui: &mut Ui, doc: &MessagesDoc) -> Option<usize> {
        if doc.lines.len() == 0 {
            notice(ui, "No messages");
            return None;
        }
        if !self.found {
            self.refind(doc);
        }
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let number_style = |color| TextStyle {
            font: theme.font.buffer,
            ..theme.ui_text(color)
        };
        let (text_style, dim_style) = (number_style(c.text), number_style(c.dim));
        let height = s.row - s.gap / 2.;
        let element = Element {
            direction: Direction::LeftToRight,
            size: [Length::Grow, Length::Fit],
            padding: [s.gap / 2., s.gap / 2., 2. * s.scroll_bar, s.gap / 2.],
            gap: 2. * s.gap,
            clip: true,
            ..Element::DEFAULT
        };
        let refind = toolbar(ui, |ui| {
            let typed = text_field(ui, &mut self.search, None, "Find text or id");
            let case = toggle(ui, "Aa", self.case);
            let word = toggle(ui, "W", self.word);
            ui.text(theme.ui_text(c.dim), &self.count);
            self.case ^= case;
            self.word ^= word;
            typed || case || word
        });
        if refind {
            self.refind(doc);
        }
        if self.shown.is_empty() {
            notice(ui, "Nothing found");
            return None;
        }
        let digits = doc.digits;
        let ids = doc.ids.as_deref();
        let mut position = String::new();
        let mut id = String::new();
        let _ = write!(position, "{POSITION:>digits$}");
        ui.element(element, |ui| {
            ui.element(line_box(height), |ui| ui.text(dim_style, &position));
            if ids.is_some() {
                let _ = write!(id, "{ID:>digits$}");
                ui.element(line_box(height), |ui| ui.text(dim_style, &id));
            }
            ui.element(line_box(height), |ui| ui.text(dim_style, "Text"));
        });
        rule(ui);
        let mut picked = None;
        fit_list(ui, &mut self.scroll, self.shown.len(), |ui, at| {
            let Some(&index) = self.shown.get(at) else {
                return;
            };
            position.clear();
            let _ = write!(position, "{index:>digits$}");
            let selected = self.selected == Some(index);
            let (clicked, ()) = row(ui, element, selected, |ui| {
                ui.element(line_box(height), |ui| ui.text(dim_style, &position));
                if let Some(&number) = ids.and_then(|ids| ids.get(index)) {
                    id.clear();
                    let _ = write!(id, "{number:>digits$}");
                    ui.element(line_box(height), |ui| ui.text(text_style, &id));
                }
                line(ui, &doc.lines, index, height);
            });
            if clicked {
                self.selected = Some(index);
                picked = Some(index);
            }
        });
        picked
    }
}

/// The number a digits query finds a row by: its id, or its position in a
/// file without MID1.
fn label(ids: Option<&[u16]>, index: usize) -> usize {
    match ids {
        Some(ids) => ids.get(index).copied().map_or(0, usize::from),
        None => index,
    }
}

const fn messages(count: usize) -> &'static str {
    if count == 1 { "message" } else { "messages" }
}

/// Each message's text as a line, with tags as chips named by `version`'s
/// tables.
fn lines(bmg: &Bmg, version: Option<Version>) -> Lines {
    let mut lines = Lines::default();
    for message in &bmg.messages {
        for segment in &message.text {
            match segment {
                TextSegment::Text(bytes) => lines.text(&bmg.encoding.decode(bytes)),
                &TextSegment::Tag {
                    group,
                    code,
                    ref args,
                } => lines.chip(|out| push_tag(out, version, group, code, args)),
            }
        }
        lines.end();
    }
    lines
}

/// A tag's chip label onto `out`: its name, then its code when the code is
/// its value, then its argument when that's a number.
fn push_tag(out: &mut String, version: Option<Version>, group: u8, code: u16, args: &[u8]) {
    let Some(tag) = version.and_then(|version| find_tag(version, group, code)) else {
        let _ = write!(out, "{{{group}:{code}}}");
        return;
    };
    out.push_str(tag.name);
    if tag.any_code {
        let _ = write!(out, " {code}");
    }
    let value = match (tag.args, args) {
        (Args::U8, &[a]) => Some(u32::from(a)),
        (Args::U16, &[a, b]) => Some(u32::from(u16::from_be_bytes([a, b]))),
        (Args::U32, &[a, b, c, d]) => Some(u32::from_be_bytes([a, b, c, d])),
        _ => None,
    };
    if let Some(value) = value {
        let _ = write!(out, " {value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::jsystem::jmessage::bmg::{Encoding, Message, MessageKey, Mid1Header};
    use lines::RunKind;

    fn bmg(texts: &[&[TextSegment]]) -> Bmg {
        Bmg {
            encoding: Encoding::Windows1252,
            record_len: 4,
            mid1: Some(Mid1Header::default()),
            messages: (0..)
                .zip(texts)
                .map(|(key, text)| Message {
                    id: u16::try_from(key).map_or(0, |key| 100 + key),
                    key: MessageKey(key),
                    attributes: Box::new([]),
                    text: text.to_vec(),
                })
                .collect(),
            flow: None,
            strings: None,
        }
    }

    fn text(s: &[u8]) -> TextSegment {
        TextSegment::Text(s.to_vec())
    }

    fn tag(group: u8, code: u16, args: &[u8]) -> TextSegment {
        TextSegment::Tag {
            group,
            code,
            args: args.to_vec(),
        }
    }

    #[test]
    fn the_tables_name_tags() {
        let file = bmg(&[&[
            text(b"Hi"),
            tag(0, 7, &[0, 30]),
            tag(0, 0, &[]),
            tag(250, 9, &[]),
        ]]);
        let lines = lines(&file, Some(Version::GcnUsa));
        assert_eq!(
            lines.get(0).collect::<Vec<_>>(),
            [
                (RunKind::Text, "Hi"),
                (RunKind::Chip, "Pause 30"),
                (RunKind::Chip, "Player name"),
                (RunKind::Chip, "{250:9}"),
            ]
        );
    }

    #[test]
    fn without_tables_tags_are_raw() {
        let lines = lines(&bmg(&[&[tag(0, 7, &[0, 30])]]), None);
        assert_eq!(lines.get(0).collect::<Vec<_>>(), [(RunKind::Chip, "{0:7}")]);
    }

    #[test]
    fn find_narrows_the_list_and_counts() {
        let file = bmg(&[&[text(b"a cat")], &[text(b"dog")], &[text(b"cat, cat")]]);
        let doc = MessagesDoc::new(file, None);
        let mut view = MessagesView::default();
        view.refind(&doc);
        assert_eq!(view.shown, [0, 1, 2]);
        assert_eq!(view.count, "3 messages");
        view.search.text = "cat".to_owned();
        view.refind(&doc);
        assert_eq!(view.shown, [0, 2]);
        assert_eq!(view.count, "3 in 2 messages");
    }

    #[test]
    fn find_by_digits_also_finds_that_id() {
        let file = bmg(&[&[text(b"101 cats")], &[text(b"dog")], &[text(b"cat")]]);
        let doc = MessagesDoc::new(file, None);
        let mut view = MessagesView::default();
        view.search.text = "101".to_owned();
        view.refind(&doc);
        assert_eq!(view.shown, [0, 1]);
        assert_eq!(view.count, "2 in 2 messages");
    }

    #[test]
    fn without_mid1_rows_go_by_position() {
        let mut file = bmg(&[&[text(b"a")], &[text(b"b")]]);
        file.mid1 = None;
        let doc = MessagesDoc::new(file, None);
        let mut view = MessagesView::default();
        view.search.text = "1".to_owned();
        view.refind(&doc);
        assert_eq!(view.shown, [1]);
    }
}
