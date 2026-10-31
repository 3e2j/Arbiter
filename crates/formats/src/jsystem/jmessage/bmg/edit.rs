//! Changing a [`Bmg`] in place. Attributes change as byte ranges, so an edit
//! knows nothing of which byte is which, and the file's attributes width
//! stays as it is. Text changes whole, as its segments.

use diag::{Address, AddressStep};

use super::{Bmg, MessageKey, TextSegment};
use crate::Editable;

/// One change to a message file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BmgEdit {
    /// `bytes` over `message`'s attributes from `at`.
    Attributes {
        message: MessageKey,
        at: usize,
        bytes: Box<[u8]>,
    },
    /// `text` in place of `message`'s.
    Text {
        message: MessageKey,
        text: Vec<TextSegment>,
    },
}

impl Address for MessageKey {
    fn key(&self) -> Vec<AddressStep> {
        vec![("message", self.0.to_string())]
    }
}

impl Editable for Bmg {
    type Edit = BmgEdit;
    type Item = MessageKey;

    /// A range past the message's attributes, or a message the file doesn't
    /// hold, changes nothing.
    fn apply(&mut self, edit: BmgEdit) -> (BmgEdit, Vec<MessageKey>) {
        let key = match &edit {
            BmgEdit::Attributes { message, .. } | BmgEdit::Text { message, .. } => *message,
        };
        let Some(held) = self.messages.iter_mut().find(|held| held.key == key) else {
            return (edit, Vec::new());
        };
        let undo = match edit {
            BmgEdit::Attributes {
                message,
                at,
                mut bytes,
            } => {
                let range = at..at.saturating_add(bytes.len());
                let Some(held) = held.attributes.get_mut(range) else {
                    return (BmgEdit::Attributes { message, at, bytes }, Vec::new());
                };
                held.swap_with_slice(&mut bytes);
                BmgEdit::Attributes { message, at, bytes }
            }
            BmgEdit::Text { message, mut text } => {
                std::mem::swap(&mut held.text, &mut text);
                BmgEdit::Text { message, text }
            }
        };
        (undo, vec![key])
    }

    /// Typing over the same bytes, or in the same message's text, is one
    /// step.
    fn merges(edit: &BmgEdit, next: &BmgEdit) -> bool {
        match (edit, next) {
            (
                BmgEdit::Attributes { message, at, bytes },
                BmgEdit::Attributes {
                    message: next_message,
                    at: next_at,
                    bytes: next_bytes,
                },
            ) => message == next_message && at == next_at && bytes.len() == next_bytes.len(),
            (BmgEdit::Text { message, .. }, BmgEdit::Text { message: next, .. }) => message == next,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jsystem::jmessage::bmg::{Encoding, Message};

    fn bmg(attributes: &[u8]) -> Bmg {
        Bmg {
            encoding: Encoding::ShiftJis,
            attributes_len: u16::try_from(attributes.len()).unwrap(),
            mid1: None,
            messages: vec![Message {
                id: 0,
                key: MessageKey(7),
                attributes: attributes.into(),
                text: Vec::new(),
            }],
            flow: None,
            strings: None,
        }
    }

    fn set(message: u32, at: usize, bytes: &[u8]) -> BmgEdit {
        BmgEdit::Attributes {
            message: MessageKey(message),
            at,
            bytes: bytes.into(),
        }
    }

    #[test]
    fn an_attribute_edit_undoes_with_the_bytes_it_replaced() {
        let mut file = bmg(&[0; 24]);
        let (undo, touched) = file.apply(set(7, 22, &[1, 2]));
        assert_eq!(file.messages[0].attributes[22..], [1, 2]);
        assert_eq!((undo, touched), (set(7, 22, &[0, 0]), vec![MessageKey(7)]));
    }

    #[test]
    fn an_edit_past_the_attributes_changes_nothing() {
        let mut file = bmg(&[0; 16]);
        for edit in [set(7, 15, &[1, 2]), set(8, 0, &[1])] {
            let (undo, touched) = file.apply(edit.clone());
            assert_eq!((undo, touched), (edit, Vec::new()));
        }
        assert_eq!(*file.messages[0].attributes, [0; 16]);
    }

    #[test]
    fn a_text_edit_undoes_with_the_text_it_replaced() {
        let mut file = bmg(&[]);
        let text = vec![TextSegment::Text(b"Hi".to_vec())];
        let edit = BmgEdit::Text {
            message: MessageKey(7),
            text: text.clone(),
        };
        let (undo, touched) = file.apply(edit.clone());
        assert_eq!(file.messages[0].text, text);
        let empty = BmgEdit::Text {
            message: MessageKey(7),
            text: Vec::new(),
        };
        assert_eq!((undo, touched), (empty, vec![MessageKey(7)]));
        assert!(Bmg::merges(&edit, &edit));
    }
}
