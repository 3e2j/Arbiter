//! A message file's messages, one to a row: the id the game looks it up by,
//! then its text on one line.

use std::fmt::Write;

use gui::ui::{Element, TextStyle, Ui};
use project::Document;
use project::jsystem::jmessage::{Bmg, TextSegment};

use crate::components::{self, ListScroll, indexed_list, notice};
use crate::theme;

pub struct Messages {
    doc: Document<Bmg>,
    /// What each row shows as its id, by position. A file without MID1 is
    /// addressed by position, so that's its id.
    ids: Vec<usize>,
    /// The widest id's, so the text lines up after it.
    digits: usize,
    lines: Lines,
    scroll: ListScroll,
    /// By position.
    selected: Option<usize>,
}

/// Every message's text on one line, end to end, so a row slices it rather
/// than decoding each pass.
#[derive(Default)]
struct Lines {
    text: String,
    /// Where each message's line ends in `text`, by position.
    ends: Vec<usize>,
}

impl Messages {
    pub fn new(doc: Document<Bmg>) -> Self {
        let bmg = doc.get();
        let ids: Vec<usize> = match bmg.mid1 {
            Some(_) => bmg
                .messages
                .iter()
                .map(|m| usize::from(m.public_id))
                .collect(),
            None => (0..bmg.messages.len()).collect(),
        };
        Self {
            digits: ids.iter().max().map_or(1, |max| max.to_string().len()),
            ids,
            lines: Lines::of(bmg),
            doc,
            scroll: ListScroll::TOP,
            selected: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let len = self.doc.get().messages.len();
        if len == 0 {
            return notice(ui, "No messages");
        }
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let id_style = TextStyle {
            font: theme.font.buffer,
            ..theme.ui_text(c.dim)
        };
        let element = Element {
            padding: [s.gap / 2., 0., s.gap / 2., 0.],
            gap: 2. * s.gap,
            clip: true,
            ..Element::line(s.row)
        };
        let digits = self.digits;
        let mut id = String::new();
        let mut clicked = None;
        indexed_list(ui, &mut self.scroll, len, s.row, |ui, index| {
            let Some(at) = self.ids.get(index) else {
                return;
            };
            id.clear();
            let _ = write!(id, "{at:>digits$}");
            let line = self.lines.get(index);
            let selected = self.selected == Some(index);
            let (click, ()) = components::row(ui, element, selected, |ui| {
                ui.text(id_style, &id);
                if line.is_empty() {
                    ui.text(theme.ui_text(c.dim), "empty");
                } else {
                    ui.text(theme.ui_text(c.text), line);
                }
            });
            if click {
                clicked = Some(index);
            }
        });
        if clicked.is_some() && clicked != self.selected {
            self.selected = clicked;
            // The rows were lit before the click moved it.
            ui.stale();
        }
    }
}

impl Lines {
    fn of(bmg: &Bmg) -> Self {
        let mut lines = Self::default();
        for message in &bmg.messages {
            let start = lines.text.len();
            for segment in &message.text {
                if let TextSegment::Text(bytes) = segment {
                    push_flat(&mut lines.text, start, &bmg.encoding.decode(bytes));
                }
            }
            if lines.text.ends_with(' ') {
                lines.text.pop();
            }
            lines.ends.push(lines.text.len());
        }
        lines
    }

    /// The line at `index`, empty past the last.
    fn get(&self, index: usize) -> &str {
        let start = index
            .checked_sub(1)
            .and_then(|before| self.ends.get(before))
            .copied()
            .unwrap_or(0);
        let end = self.ends.get(index).copied().unwrap_or(start);
        self.text.get(start..end).unwrap_or_default()
    }
}

/// `text` onto the line in `out` from `start`, each run of whitespace one
/// space, none leading.
fn push_flat(out: &mut String, start: usize, text: &str) {
    for ch in text.chars() {
        if !ch.is_whitespace() {
            out.push(ch);
        } else if out.len() > start && !out.ends_with(' ') {
            out.push(' ');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project::jsystem::jmessage::{Encoding, Message, MessageId};

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

    #[test]
    fn each_message_flattens_to_one_line() {
        let color = TextSegment::Tag {
            group: 255,
            code: 0,
            args: vec![1],
        };
        let lines = Lines::of(&bmg(&[
            &[text(b"  Hey,\nyou!  ")],
            &[],
            &[text(b"a "), color, text(b"\n caf\xe9")],
        ]));
        assert_eq!(lines.get(0), "Hey, you!");
        assert_eq!(lines.get(1), "");
        assert_eq!(lines.get(2), "a café");
        assert_eq!(lines.get(3), "");
    }
}
