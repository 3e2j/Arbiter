//! A message as the inspector shows it: its position and id, then its INF1
//! record's fields named by the TP tables, or its bytes without them.

use std::fmt::Write;

use formats::games::tp::Version;
use formats::games::tp::jsystem::jmessage::{layout, values};
use formats::jsystem::jmessage::bmg::Bmg;

use crate::panels::inspector::Page;

/// The message at `index` in `bmg`, under `file`'s name, its record read
/// with `version`'s tables when given.
pub fn page(bmg: &Bmg, version: Option<Version>, index: usize, file: &str) -> Option<Page> {
    let message = bmg.messages.get(index)?;
    let mut page = Page::default();
    let mid1 = bmg.mid1.is_some();
    page.crumb(file);
    if mid1 {
        page.crumb(format_args!("Message {}", message.id));
    } else {
        page.crumb(format_args!("Message at {index}"));
    }
    page.section("File");
    page.field("Position", index, "");
    if mid1 {
        page.field("ID", message.id, "From MID1, what the game looks it up by");
    }
    let Some((version, layout)) =
        version.and_then(|version| Some((version, layout(version, bmg.record_len)?)))
    else {
        page.section("Record");
        page.bytes("Attributes", &message.attributes, "");
        return Some(page);
    };
    let mut value_text = String::new();
    let mut help = String::new();
    for section in layout.sections {
        page.section(section.title);
        for field in section.fields {
            let Some(value) = field.read(&message.attributes) else {
                continue;
            };
            let entry = field
                .values
                .and_then(|table| values(version, table).find(|entry| entry.value == value));
            value_text.clear();
            help.clear();
            help.push_str(field.notes.unwrap_or_default());
            let _ = write!(value_text, "{value}");
            if let Some(entry) = entry {
                let _ = write!(value_text, " · {}", entry.name);
                if let Some(notes) = entry.notes {
                    let gap = if help.is_empty() { "" } else { ". " };
                    let _ = write!(help, "{gap}{}: {notes}", entry.name);
                }
            }
            page.field(field.name, &value_text, &help);
        }
    }
    Some(page)
}
