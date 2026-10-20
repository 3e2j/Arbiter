//! The shape of an INF1 record: which attribute bytes are which.

use crate::games::{Entry, Versions};

/// A record struct the game reads INF1 with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Record width, text offset included.
    pub len: u16,
    /// Every field after the 4-byte text offset, in record order.
    pub fields: &'static [Field],
    /// The field that repeats the MID1 id, in a layout that has one.
    pub id: Option<Field>,
    pub versions: Versions,
}

/// One field of an INF1 record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Byte offset into the whole record, where 0x00 is the text offset.
    pub offset: usize,
    /// 1 or 2 bytes, big-endian.
    pub len: usize,
    pub name: &'static str,
    pub notes: Option<&'static str>,
    /// The table naming the field's values, read through
    /// [`super::Tables::values`].
    pub values: Option<&'static [Entry]>,
    /// The value is a byte offset into STR1, and the string there is what the
    /// game uses.
    pub string: bool,
}

pub(crate) const fn field(offset: usize, len: usize, name: &'static str) -> Field {
    Field {
        offset,
        len,
        name,
        notes: None,
        values: None,
        string: false,
    }
}

impl Field {
    pub(crate) const fn notes(self, notes: &'static str) -> Self {
        Self {
            notes: Some(notes),
            ..self
        }
    }

    pub(crate) const fn values(self, values: &'static [Entry]) -> Self {
        Self {
            values: Some(values),
            ..self
        }
    }

    pub(crate) const fn string(self) -> Self {
        Self {
            string: true,
            ..self
        }
    }
}
