//! Changing a [`Bmg`] in place. Attributes change as byte ranges, so an edit
//! knows nothing of which byte is which, and the file's attributes width
//! stays as it is.

use diag::{Address, AddressStep};

use super::{Bmg, MessageKey};
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
        let BmgEdit::Attributes {
            message,
            at,
            mut bytes,
        } = edit;
        let range = at..at.saturating_add(bytes.len());
        let held = self
            .messages
            .iter_mut()
            .find(|held| held.key == message)
            .and_then(|held| held.attributes.get_mut(range));
        let Some(held) = held else {
            return (BmgEdit::Attributes { message, at, bytes }, Vec::new());
        };
        held.swap_with_slice(&mut bytes);
        (BmgEdit::Attributes { message, at, bytes }, vec![message])
    }

    /// Typing over the same bytes is one step.
    fn merges(edit: &BmgEdit, next: &BmgEdit) -> bool {
        let (
            BmgEdit::Attributes { message, at, bytes },
            BmgEdit::Attributes {
                message: next_message,
                at: next_at,
                bytes: next_bytes,
            },
        ) = (edit, next);
        message == next_message && at == next_at && bytes.len() == next_bytes.len()
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
}
