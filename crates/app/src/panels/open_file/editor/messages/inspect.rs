//! A message as the inspector shows it: its position and id, then its
//! attributes. The TP tables split them into named fields, with a switch to
//! show them as bytes. Without tables for their width they're only bytes,
//! as many as the file's attributes are wide. Both write the same bytes.

use std::fmt::Write;

use formats::Editable;
use formats::games::tp::Version;
use formats::games::tp::jsystem::jmessage::{Layout, layout, values};
use formats::jsystem::jmessage::bmg::BmgEdit;
use gui::ui::Ui;

use super::MessagesDoc;
use crate::panels::inspector::{Fields, byte_grid, field, named, number, section, view_switch};

/// The field under the pointer's label and help.
type Over = Option<(&'static str, String)>;

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

    /// The message at `index`'s fields, setting its attributes as they're
    /// typed in. Returns the label and help of the field under the pointer,
    /// if it has help.
    pub fn inspect(&mut self, ui: &mut Ui, index: usize, fields: &mut Fields) -> Over {
        let message = self.bmg.messages.get(index)?;
        let key = message.key;
        let mut over = None;
        let mut value = String::new();
        section(ui, "File");
        let _ = write!(value, "{index}");
        field(ui, "Position", &value);
        if self.bmg.mid1.is_some() {
            value.clear();
            let _ = write!(value, "{}", message.id);
            if field(ui, "ID", &value) {
                over = Some(("ID", "From MID1, what the game looks it up by".to_owned()));
            }
        }
        section(ui, "Attributes");
        let tables = self
            .version
            .and_then(|version| Some((version, layout(version, self.bmg.attributes_len + 4)?)));
        if tables.is_some() {
            view_switch(ui, fields, &["Fields", "Bytes"]);
        }
        let attributes = &message.attributes;
        let set = if let Some((version, layout)) = tables.filter(|_| fields.view == 0) {
            attribute_fields(ui, fields, version, layout, attributes, &mut over)
        } else {
            let layout = tables.map(|(_, layout)| layout);
            attribute_bytes(ui, fields, layout, attributes, &mut over)
        };
        if let Some((at, bytes)) = set {
            // The undo edit is dropped until there's history to keep it in.
            self.bmg.apply(BmgEdit::Attributes {
                message: key,
                at,
                bytes,
            });
            self.edits = self.edits.wrapping_add(1);
        }
        over
    }
}

/// `attributes` as `layout`'s fields, their values named by `version`'s
/// tables. Returns the bytes typed and where they go.
fn attribute_fields(
    ui: &mut Ui,
    fields: &mut Fields,
    version: Version,
    layout: &Layout,
    attributes: &[u8],
    over: &mut Over,
) -> Option<(usize, Box<[u8]>)> {
    let mut set = None;
    for section_layout in layout.sections {
        section(ui, section_layout.title);
        for record_field in section_layout.fields {
            let (Some(start), Some(read)) = (record_field.start(), record_field.read(attributes))
            else {
                continue;
            };
            let max = if record_field.len == 1 {
                u16::from(u8::MAX)
            } else {
                u16::MAX
            };
            let name = record_field.name;
            let (hovered, typed) = match record_field.values {
                Some(table) => {
                    let names = values(version, table).map(|entry| (entry.value, entry.name));
                    named(ui, fields, name, read, max, names)
                }
                None => number(ui, fields, name, read, max),
            };
            if let Some(typed) = typed {
                let bytes = typed.to_be_bytes();
                let width = bytes.len().saturating_sub(record_field.len);
                set = bytes.get(width..).map(|bytes| (start, bytes.into()));
            }
            if !hovered {
                continue;
            }
            let mut help = record_field
                .notes
                .map_or_else(String::new, |notes| format!("{notes}. "));
            let entry = record_field
                .values
                .map(|table| values(version, table).find(|entry| entry.value == read));
            let _ = match entry {
                Some(Some(entry)) => match entry.notes {
                    Some(notes) => write!(help, "{}: {notes}. ", entry.name),
                    None => Ok(()),
                },
                Some(None) => write!(help, "{read} has no TP name. "),
                None => Ok(()),
            };
            let _ = write!(help, "Any value from 0 to {max}.");
            *over = Some((name, help));
        }
    }
    set
}

/// `attributes` as bytes, each naming the field of `layout` it's part of in
/// its help. Returns the byte typed and where it goes.
fn attribute_bytes(
    ui: &mut Ui,
    fields: &mut Fields,
    layout: Option<&Layout>,
    attributes: &[u8],
    over: &mut Over,
) -> Option<(usize, Box<[u8]>)> {
    let (hovered, typed) = byte_grid(ui, fields, attributes);
    if let Some(at) = hovered {
        let mut help = format!("At 0x{at:02X}, any value from 00 to FF.");
        let owner = layout
            .into_iter()
            .flat_map(|layout| layout.sections)
            .flat_map(|section_layout| section_layout.fields)
            .find(|record_field| {
                let start = record_field.start();
                start.is_some_and(|start| (start..start + record_field.len).contains(&at))
            });
        if let Some(owner) = owner {
            let _ = write!(help, " TP's tables read it as part of {}.", owner.name);
        }
        *over = Some(("Attribute byte", help));
    }
    typed.map(|(at, byte)| (at, Box::from([byte])))
}
