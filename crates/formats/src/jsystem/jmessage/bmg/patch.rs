//! A [`Bmg`] stored as its deltas against the base. Messages match by key,
//! which is the base position, so one added or removed makes no delta.

use std::collections::HashMap;

use diag::{Diagnostic, DiagnosticCode, Diagnostics, Location, Severity};
use serde::{Deserialize, Serialize};

use super::{Bmg, BmgEdit, Message, MessageKey, TextSegment};
use crate::{Editable, Patch};

pub const MISSING_MESSAGE: DiagnosticCode = DiagnosticCode {
    id: "bmg/missing-message",
    severity: Severity::Warning,
    summary: "a delta names a message the base doesn't hold, or bytes past its attributes",
};

/// One difference from a base message file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum BmgDelta {
    Id {
        message: MessageKey,
        id: u16,
    },
    /// Only the changed run, so no base bytes are stored.
    Attributes {
        message: MessageKey,
        at: usize,
        bytes: Vec<u8>,
    },
    Text {
        message: MessageKey,
        text: Vec<TextSegment>,
    },
}

impl Patch for Bmg {
    type Delta = BmgDelta;

    fn diff(base: &Self, edited: &Self) -> Vec<BmgDelta> {
        let by_key: HashMap<MessageKey, &Message> = base
            .messages
            .iter()
            .map(|message| (message.key, message))
            .collect();
        let mut deltas = Vec::new();
        for message in &edited.messages {
            let Some(old) = by_key.get(&message.key) else {
                continue;
            };
            if old.id != message.id {
                deltas.push(BmgDelta::Id {
                    message: message.key,
                    id: message.id,
                });
            }
            for (at, bytes) in changed_runs(&old.attributes, &message.attributes) {
                deltas.push(BmgDelta::Attributes {
                    message: message.key,
                    at,
                    bytes: bytes.to_vec(),
                });
            }
            if old.text != message.text {
                deltas.push(BmgDelta::Text {
                    message: message.key,
                    text: message.text.clone(),
                });
            }
        }
        deltas
    }

    fn patch(&mut self, deltas: &[BmgDelta], diag: &mut Diagnostics) {
        for delta in deltas {
            let (message, edit) = match delta.clone() {
                BmgDelta::Id { message, id } => (
                    message,
                    BmgEdit::Id {
                        message,
                        id,
                        copy: None,
                    },
                ),
                BmgDelta::Attributes { message, at, bytes } => (
                    message,
                    BmgEdit::Attributes {
                        message,
                        at,
                        bytes: bytes.into(),
                    },
                ),
                BmgDelta::Text { message, text } => (message, BmgEdit::Text { message, text }),
            };
            let (_, touched) = self.apply(edit);
            if touched.is_empty() {
                diag.push(
                    Diagnostic::new(
                        &MISSING_MESSAGE,
                        format!(
                            "message {} doesn't fit this base, so its delta is skipped",
                            message.0
                        ),
                    )
                    .at(Location::key("message", message.0)),
                );
            }
        }
    }
}

/// Each run of `edited` that differs from `base`, over the bytes both hold.
fn changed_runs<'a>(base: &[u8], edited: &'a [u8]) -> Vec<(usize, &'a [u8])> {
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
        runs.push((from, &edited[from..at]));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsystem::jmessage::bmg::Encoding;

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

    fn attributes(message: u32, at: usize, bytes: &[u8]) -> BmgDelta {
        BmgDelta::Attributes {
            message: MessageKey(message),
            at,
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn a_diff_patches_the_base_into_the_edit() {
        let base = base();
        assert_eq!(Bmg::diff(&base, &base), []);

        let mut edited = base.clone();
        edited.messages[0].id = 9;
        edited.messages[1].attributes[2..4].copy_from_slice(&[1, 2]);
        edited.messages[1].text = Vec::new();
        let deltas = Bmg::diff(&base, &edited);
        assert_eq!(
            deltas,
            [
                BmgDelta::Id {
                    message: MessageKey(0),
                    id: 9,
                },
                attributes(1, 2, &[1, 2]),
                BmgDelta::Text {
                    message: MessageKey(1),
                    text: Vec::new(),
                },
            ]
        );

        let mut patched = base;
        let mut diag = Diagnostics::default();
        patched.patch(&deltas, &mut diag);
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
        assert_eq!(
            Bmg::diff(&base, &edited),
            [
                attributes(0, 1, &[1]),
                attributes(0, 3, &[3]),
                attributes(0, 7, &[7])
            ]
        );
    }

    #[test]
    fn a_message_only_one_side_holds_makes_no_delta() {
        let base = base();
        let mut edited = base.clone();
        edited.messages.remove(0);
        edited.messages[0].key = MessageKey(2);
        assert_eq!(Bmg::diff(&base, &edited), []);
    }

    #[test]
    fn a_delta_that_doesnt_fit_is_skipped_and_reported() {
        let mut patched = base();
        let mut diag = Diagnostics::default();
        patched.patch(
            &[attributes(1, 7, &[1, 2]), attributes(4, 0, &[1])],
            &mut diag,
        );
        assert_eq!(patched, base());
        assert_eq!(diag.items.len(), 2);
        assert!(diag.items.iter().all(|item| item.code == &MISSING_MESSAGE));
    }
}
