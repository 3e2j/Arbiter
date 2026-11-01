//! A [`Bmg`] stored as its delta against the base: one entry per edited
//! message, holding only the fields that differ. Messages match by key,
//! which is the base position, so one added or removed makes no entry.
//!
//! ```toml
//! [[message]]
//! key = 1
//! attributes = [[9, [1]]]
//! text = "Hello, {0:0}!"
//! ```

use std::collections::HashMap;

use diag::{Diagnostic, DiagnosticCode, Diagnostics, Location, Severity};
use serde::{Deserialize, Serialize};

use super::{Bmg, BmgEdit, Message, MessageKey, raw};
use crate::{Editable, Patch};

pub const MISSING_MESSAGE: DiagnosticCode = DiagnosticCode {
    id: "bmg/missing-message",
    severity: Severity::Warning,
    summary: "a delta names a message the base doesn't hold, or bytes past its attributes",
};

pub const UNREADABLE_TEXT: DiagnosticCode = DiagnosticCode {
    id: "bmg/unreadable-text",
    severity: Severity::Warning,
    summary: "a text delta isn't raw text the base's encoding can hold",
};

/// Every difference from a base message file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BmgDelta {
    #[serde(rename = "message", default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<MessageDelta>,
}

/// One message's differences from its base, each field whole when present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageDelta {
    pub key: MessageKey,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u16>,
    /// Each changed run as `[at, bytes]`, so no base bytes are stored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(usize, Vec<u8>)>,
    /// Written as [`raw`] text, so it reads in the patch file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Patch for Bmg {
    type Delta = BmgDelta;

    fn diff(base: &Self, edited: &Self) -> BmgDelta {
        let by_key: HashMap<MessageKey, &Message> = base
            .messages
            .iter()
            .map(|message| (message.key, message))
            .collect();
        let mut messages = Vec::new();
        for message in &edited.messages {
            let Some(old) = by_key.get(&message.key) else {
                continue;
            };
            let delta = MessageDelta {
                key: message.key,
                id: (old.id != message.id).then_some(message.id),
                attributes: changed_runs(&old.attributes, &message.attributes),
                text: (old.text != message.text).then(|| {
                    let mut text = String::new();
                    raw::write(&message.text, edited.encoding, &mut text);
                    text
                }),
            };
            if delta.id.is_some() || !delta.attributes.is_empty() || delta.text.is_some() {
                messages.push(delta);
            }
        }
        BmgDelta { messages }
    }

    fn patch(&mut self, delta: &BmgDelta, diag: &mut Diagnostics) {
        for delta in &delta.messages {
            let key = delta.key;
            let at = || Location::key("message", key.0);
            if !self.messages.iter().any(|message| message.key == key) {
                let text = format!(
                    "message {} isn't in this base, so its delta is skipped",
                    key.0
                );
                diag.push(Diagnostic::new(&MISSING_MESSAGE, text).at(at()));
                continue;
            }
            if let Some(id) = delta.id {
                self.apply(BmgEdit::Id {
                    message: key,
                    id,
                    copy: None,
                });
            }
            for (from, bytes) in &delta.attributes {
                let edit = BmgEdit::Attributes {
                    message: key,
                    at: *from,
                    bytes: bytes.as_slice().into(),
                };
                if self.apply(edit).1.is_empty() {
                    let text = format!(
                        "message {}'s attributes end before byte {from}, so it's skipped",
                        key.0
                    );
                    diag.push(Diagnostic::new(&MISSING_MESSAGE, text).at(at()));
                }
            }
            let Some(text) = &delta.text else {
                continue;
            };
            match raw::read(text, self.encoding) {
                Ok(text) => {
                    self.apply(BmgEdit::Text { message: key, text });
                }
                Err(err) => {
                    let text = format!("message {}'s text is skipped. {err}", key.0);
                    diag.push(Diagnostic::new(&UNREADABLE_TEXT, text).at(at()));
                }
            }
        }
    }
}

/// Each run of `edited` that differs from `base`, over the bytes both hold.
fn changed_runs(base: &[u8], edited: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let len = base.len().min(edited.len());
    let mut runs = Vec::new();
    let mut at = 0;
    while at < len {
        if base[at] == edited[at] {
            at += 1;
            continue;
        }
        let from = at;
        while at < len && base[at] != edited[at] {
            at += 1;
        }
        runs.push((from, edited[from..at].to_vec()));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsystem::jmessage::bmg::{Encoding, TextSegment};

    fn base() -> Bmg {
        let message = |key, id| Message {
            id,
            key: MessageKey(key),
            attributes: Box::new([0; 8]),
            text: vec![TextSegment::Text(b"Hi".to_vec())],
        };
        Bmg {
            encoding: Encoding::ShiftJis,
            attributes_len: 8,
            mid1: None,
            messages: vec![message(0, 5), message(1, 6)],
            flow: None,
            strings: None,
        }
    }

    fn message(key: u32) -> MessageDelta {
        MessageDelta {
            key: MessageKey(key),
            id: None,
            attributes: Vec::new(),
            text: None,
        }
    }

    fn patched(messages: Vec<MessageDelta>) -> (Bmg, Diagnostics) {
        let mut bmg = base();
        let mut diag = Diagnostics::default();
        bmg.patch(&BmgDelta { messages }, &mut diag);
        (bmg, diag)
    }

    #[test]
    fn a_diff_patches_the_base_into_the_edit() {
        let base = base();
        assert_eq!(Bmg::diff(&base, &base), BmgDelta::default());

        let mut edited = base.clone();
        edited.messages[0].id = 9;
        edited.messages[1].attributes[2..4].copy_from_slice(&[1, 2]);
        edited.messages[1].text = vec![
            TextSegment::Text(b"{".to_vec()),
            TextSegment::Tag {
                group: 0,
                code: 0,
                args: Vec::new(),
            },
        ];
        let delta = Bmg::diff(&base, &edited);
        let messages = vec![
            MessageDelta {
                id: Some(9),
                ..message(0)
            },
            MessageDelta {
                attributes: vec![(2, vec![1, 2])],
                text: Some("{{{0:0}".to_owned()),
                ..message(1)
            },
        ];
        assert_eq!(delta.messages, messages);

        let (patched, diag) = patched(messages);
        assert_eq!(patched, edited);
        assert_eq!(diag.items, []);
    }

    /// Bytes set back to their base value between two changes stay out.
    #[test]
    fn attribute_deltas_hold_only_the_changed_bytes() {
        let base = base();
        let mut edited = base.clone();
        edited.messages[0].attributes[1] = 1;
        edited.messages[0].attributes[3] = 3;
        edited.messages[0].attributes[7] = 7;
        let runs = [(1, vec![1]), (3, vec![3]), (7, vec![7])];
        assert_eq!(Bmg::diff(&base, &edited).messages[0].attributes, runs);
    }

    #[test]
    fn a_message_only_one_side_holds_makes_no_delta() {
        let base = base();
        let mut edited = base.clone();
        edited.messages.remove(0);
        edited.messages[0].key = MessageKey(2);
        assert_eq!(Bmg::diff(&base, &edited), BmgDelta::default());
    }

    #[test]
    fn a_delta_that_doesnt_fit_is_skipped_and_reported() {
        let (patched, diag) = patched(vec![
            MessageDelta {
                attributes: vec![(7, vec![1, 2])],
                ..message(1)
            },
            MessageDelta {
                id: Some(1),
                text: Some("Bye".to_owned()),
                ..message(4)
            },
        ]);
        assert_eq!(patched, base());
        assert_eq!(diag.items.len(), 2);
        assert!(diag.items.iter().all(|item| item.code == &MISSING_MESSAGE));
    }

    #[test]
    fn text_that_doesnt_read_is_skipped_and_reported() {
        let text = |text: &str| MessageDelta {
            text: Some(text.to_owned()),
            ..message(0)
        };
        let (patched, diag) = patched(vec![text("a {x}"), text("\u{1f600}")]);
        assert_eq!(patched, base());
        assert_eq!(diag.items.len(), 2);
        assert!(diag.items.iter().all(|item| item.code == &UNREADABLE_TEXT));
    }
}
