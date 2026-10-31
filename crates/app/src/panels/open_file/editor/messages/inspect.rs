//! A message as the inspector shows it: its position and id, then its INF1
//! record's fields named by the TP tables, or its bytes without them.

use std::fmt::Write;

use formats::games::tp::jsystem::jmessage::{layout, values};
use gui::ui::Ui;

use super::MessagesDoc;
use crate::panels::inspector::{FieldKind, field, section};

impl MessagesDoc {
    /// The message at `index`'s name onto `out`, by its id when the file has
    /// MID1.
    pub fn item_name(&self, index: usize, out: &mut String) {
        let Some(message) = self.bmg.messages.get(index) else {
            return;
        };
        let _ = if self.bmg.mid1.is_some() {
            write!(out, "Message {}", message.id)
        } else {
            write!(out, "Message at {index}")
        };
    }

    /// The message at `index`'s fields, its record read with the TP tables
    /// when the file has a version. Returns the label and help of the field
    /// under the pointer, if it has help.
    pub fn inspect(&self, ui: &mut Ui, index: usize) -> Option<(&'static str, String)> {
        let message = self.bmg.messages.get(index)?;
        let mut over = None;
        let mut value = String::new();
        let mut help = String::new();
        let mut show = |ui: &mut Ui, label, value: &str, help: &str, kind| {
            if field(ui, label, value, kind) && !help.is_empty() {
                over = Some((label, help.to_owned()));
            }
        };
        section(ui, "File");
        let _ = write!(value, "{index}");
        show(ui, "Position", &value, "", FieldKind::Value);
        if self.bmg.mid1.is_some() {
            value.clear();
            let _ = write!(value, "{}", message.id);
            let help = "From MID1, what the game looks it up by";
            show(ui, "ID", &value, help, FieldKind::Value);
        }
        let record = self
            .version
            .and_then(|version| Some((version, layout(version, self.bmg.attributes_len + 4)?)));
        let Some((version, layout)) = record else {
            section(ui, "Record");
            value.clear();
            for (i, byte) in (0..).zip(&message.attributes) {
                let gap = if i > 0 { " " } else { "" };
                let _ = write!(value, "{gap}{byte:02X}");
            }
            show(ui, "Attributes", &value, "", FieldKind::Bytes);
            return over;
        };
        for section_layout in layout.sections {
            section(ui, section_layout.title);
            for record_field in section_layout.fields {
                let Some(read) = record_field.read(&message.attributes) else {
                    continue;
                };
                let entry = record_field
                    .values
                    .and_then(|table| values(version, table).find(|entry| entry.value == read));
                value.clear();
                help.clear();
                help.push_str(record_field.notes.unwrap_or_default());
                let _ = write!(value, "{read}");
                if let Some(entry) = entry {
                    let _ = write!(value, " · {}", entry.name);
                    if let Some(notes) = entry.notes {
                        let gap = if help.is_empty() { "" } else { ". " };
                        let _ = write!(help, "{gap}{}: {notes}", entry.name);
                    }
                }
                show(ui, record_field.name, &value, &help, FieldKind::Value);
            }
        }
        over
    }
}
