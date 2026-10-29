//! A message file's messages, one to a row: the id the game looks it up by,
//! then its text wrapped, its tags as chips, and the box it shows in when
//! that's not the ordinary one.
//!
//! The game's tables name the tags and boxes. Without them, a tag shows as
//! its raw `group.code` and no box is marked.
//!
//! Find above narrows the list to the messages whose text holds the query,
//! never counting a tag's. A query of only digits also finds the message
//! with that id.
//!
//! The Inspector shows a message as a card, built by [`Messages::inspect`].

use std::fmt::Write;

use formats::jsystem::jmessage::bmg::{Bmg, Message, TextSegment};
use formats::jsystem::jmessage::games::{Args, Layout, Tables, Tag};
use gui::canvas::Color;
use gui::ui::{Direction, Element, Length, TextStyle, Ui};

use crate::components::{
    self, ListScroll, TextField, fit_list, line_box, notice, text_field, toggle, toolbar,
};
use crate::theme;
use crate::views::{self, Find, Inspection, Lines};

pub struct Messages {
    /// The file's name, the root of the breadcrumb.
    file: String,
    bmg: Bmg,
    /// What each row shows as its id, by position. A file without MID1 is
    /// addressed by position, so that's its id.
    ids: Vec<usize>,
    /// The widest id's, so the text lines up after it.
    digits: usize,
    /// Each message's text, by position.
    lines: Lines,
    scroll: ListScroll,
    /// By position.
    selected: Option<usize>,
    search: TextField,
    /// Find matches case.
    case: bool,
    /// Find matches whole words only.
    word: bool,
    /// The positions find leaves in the list, in order.
    shown: Vec<usize>,
    /// What find found, or how many messages there are.
    count: String,
}

impl Messages {
    /// `bmg`, read from the file named `file`.
    pub fn new(file: &str, bmg: Bmg, tables: Option<Tables>) -> Self {
        let ids: Vec<usize> = match bmg.mid1 {
            Some(_) => bmg
                .messages
                .iter()
                .map(|m| usize::from(m.public_id))
                .collect(),
            None => (0..bmg.messages.len()).collect(),
        };
        let mut messages = Self {
            file: file.to_owned(),
            digits: ids.iter().max().map_or(1, |max| max.to_string().len()),
            ids,
            lines: lines(&bmg, tables),
            bmg,
            scroll: ListScroll::TOP,
            selected: None,
            search: TextField::default(),
            case: false,
            word: false,
            shown: Vec::new(),
            count: String::new(),
        };
        messages.refind();
        messages
    }

    /// Narrows the list to what find now finds, from the top.
    fn refind(&mut self) {
        let len = self.bmg.messages.len();
        let mut find = Find::new(&self.search.text, self.case, self.word);
        self.shown.clear();
        self.count.clear();
        self.scroll = ListScroll::TOP;
        if find.is_empty() {
            self.shown.extend(0..len);
            let _ = write!(self.count, "{len} messages");
            return;
        }
        let id = self.search.text.trim().parse::<usize>().ok();
        let mut hits = 0;
        for (index, &at) in self.ids.iter().enumerate() {
            let found = find.count(&self.lines, index) + usize::from(id == Some(at));
            if found > 0 {
                self.shown.push(index);
                hits += found;
            }
        }
        let rows = self.shown.len();
        let plural = if rows == 1 { "" } else { "s" };
        let _ = write!(self.count, "{hits} in {rows} message{plural}");
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let len = self.bmg.messages.len();
        if len == 0 {
            return notice(ui, "No messages");
        }
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let id_style = TextStyle {
            font: theme.font.buffer,
            ..theme.ui_text(c.dim)
        };
        let line = s.row - s.gap / 2.;
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
            self.refind();
        }
        if self.shown.is_empty() {
            return notice(ui, "Nothing found");
        }
        let digits = self.digits;
        let mut id = String::new();
        let mut clicked = None;
        let shown = &self.shown;
        fit_list(ui, &mut self.scroll, shown.len(), |ui, row| {
            let Some(&index) = shown.get(row) else {
                return;
            };
            let Some(at) = self.ids.get(index) else {
                return;
            };
            id.clear();
            let _ = write!(id, "{at:>digits$}");
            let selected = self.selected == Some(index);
            let (click, ()) = components::row(ui, element, selected, |ui| {
                ui.element(line_box(line), |ui| ui.text(id_style, &id));
                views::line(ui, &self.lines, index, line);
            });
            if click {
                clicked = Some(index);
            }
        });
        if clicked.is_some() {
            self.selected = clicked;
        }
    }

    /// The selected message's position.
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// The message at `index` as the Inspector shows it.
    pub fn inspect(&self, index: usize) -> Option<Inspection> {
        let message = self.bmg.messages.get(index)?;
        let id = self.ids.get(index)?;
        let mut inspection = Inspection::default();
        inspection.crumb(&self.file);
        inspection.crumb(format_args!("Message {id}"));
        inspection.card("Message", id);
        inspection.section("Entry");
        inspection.number("Position", index);
        inspection.bytes("Attributes", &message.attributes);
        Some(inspection)
    }
}

/// Each message's text as a line: tags as chips, named by `tables`, and
/// the box it shows in as the badge when that's not the ordinary one.
fn lines(bmg: &Bmg, tables: Option<Tables>) -> Lines {
    let layout = tables.and_then(|tables| tables.layout(bmg.record_len));
    let mut lines = Lines::default();
    for message in &bmg.messages {
        let box_kind = box_kind(layout, message);
        for segment in &message.text {
            match segment {
                TextSegment::Text(bytes) => lines.text(&bmg.encoding.decode(bytes)),
                &TextSegment::Tag {
                    group,
                    code,
                    ref args,
                } => match tables.and_then(|tables| Some((tables, tables.tag(group, code)?))) {
                    Some((tables, tag)) => {
                        let swatch = tables.is_color(tag).then(|| {
                            let color = args.first().copied().unwrap_or(0);
                            let [r, g, b] = tables.rgb(color, box_kind.unwrap_or(0));
                            Color([r, g, b, u8::MAX])
                        });
                        lines.chip(swatch, |out| push_tag(out, tables, tag, code, args));
                    }
                    None => lines.chip(None, |out| {
                        let _ = write!(out, "{group}.{code}");
                    }),
                },
            }
        }
        if let Some(kind) = box_kind.filter(|&kind| kind != 0)
            && let Some(tables) = tables
        {
            let values = layout.and_then(|layout| layout.box_kind?.values);
            let named = values.and_then(|values| tables.values(values).get(u16::from(kind)));
            lines.badge(|out| {
                let _ = match named {
                    Some(entry) => out.write_str(entry.name),
                    None => write!(out, "Box {kind}"),
                };
            });
        }
        lines.end();
    }
    lines
}

/// The box kind `message` shows in, when the layout has one.
fn box_kind(layout: Option<&Layout>, message: &Message) -> Option<u8> {
    let value = layout?.box_kind?.read(&message.attributes)?;
    u8::try_from(value).ok()
}

/// `tag`'s chip label onto `out`: its name, then what its code or argument
/// says, named when its table names it. A color is just its name.
fn push_tag(out: &mut String, tables: Tables, tag: &Tag, code: u16, args: &[u8]) {
    let value = match (tag.args, args) {
        (Args::U8, &[a]) => Some(u32::from(a)),
        (Args::U16, &[a, b]) => Some(u32::from(u16::from_be_bytes([a, b]))),
        (Args::U32, &[a, b, c, d]) => Some(u32::from_be_bytes([a, b, c, d])),
        _ => None,
    };
    let named = value.and_then(|value| {
        let value = u16::try_from(value).ok()?;
        tables.values(tag.values?).get(value)
    });
    if let Some(entry) = named
        && tables.is_color(tag)
    {
        return out.push_str(entry.name);
    }
    out.push_str(tag.name);
    if tag.any_code {
        let _ = write!(out, " {code}");
    }
    let _ = match (named, value) {
        (Some(entry), _) => write!(out, " {}", entry.name),
        (None, Some(value)) => write!(out, " {value}"),
        (None, None) => Ok(()),
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::views::RunKind;
    use formats::games::tp::Version;
    use formats::jsystem::jmessage::bmg::{Encoding, MessageId};

    fn bmg(texts: &[&[TextSegment]]) -> Bmg {
        Bmg {
            encoding: Encoding::Windows1252,
            record_len: 4,
            mid1: None,
            messages: (0..)
                .zip(texts)
                .map(|(id, text)| Message {
                    public_id: 0,
                    id: MessageId(id),
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

    const TAG: RunKind = RunKind::Chip(None);

    #[test]
    fn the_tables_name_tags_and_boxes() {
        let mut file = bmg(&[
            &[
                tag(255, 0, &[1]),
                text(b"Hi"),
                tag(0, 7, &[0, 30]),
                tag(1, 12, &[]),
            ],
            &[tag(9, 9, &[])],
        ]);
        file.record_len = 20;
        for (message, kind) in file.messages.iter_mut().zip([13, 0]) {
            let mut attributes = [0; 16];
            attributes[5] = kind;
            message.attributes = Box::new(attributes);
        }
        let lines = lines(&file, Some(Tables::tp(Version::GcnUsa)));
        let red = RunKind::Chip(Some(Color::hex(0xF0_78_78)));
        assert_eq!(
            lines.get(0).collect::<Vec<_>>(),
            [
                (red, "Red"),
                (RunKind::Text, "Hi"),
                (TAG, "Pause 30"),
                (TAG, "Sound 12"),
                (RunKind::Badge, "Midna"),
            ]
        );
        assert_eq!(lines.get(1).collect::<Vec<_>>(), [(TAG, "9.9")]);
    }

    #[test]
    fn without_tables_tags_are_raw() {
        let lines = lines(&bmg(&[&[tag(255, 0, &[1])]]), None);
        assert_eq!(lines.get(0).collect::<Vec<_>>(), [(TAG, "255.0")]);
    }

    #[test]
    fn find_narrows_the_list_and_counts() {
        let file = bmg(&[&[text(b"a cat")], &[text(b"dog")], &[text(b"cat, cat")]]);
        let mut messages = Messages::new("a.bmg", file, None);
        assert_eq!(messages.shown, [0, 1, 2]);
        assert_eq!(messages.count, "3 messages");
        messages.search.text = "cat".to_owned();
        messages.refind();
        assert_eq!(messages.shown, [0, 2]);
        assert_eq!(messages.count, "3 in 2 messages");
    }

    #[test]
    fn find_by_digits_also_finds_that_id() {
        let file = bmg(&[&[text(b"2 cats")], &[text(b"dog")], &[text(b"cat")]]);
        let mut messages = Messages::new("a.bmg", file, None);
        messages.search.text = "2".to_owned();
        messages.refind();
        assert_eq!(messages.shown, [0, 2]);
        assert_eq!(messages.count, "2 in 2 messages");
    }
}
