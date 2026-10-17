//! Inline text tags: what each group and code does, and what arguments it
//! takes. The group and code are the ones `formats::bmg::TextSegment::Tag`
//! holds.

use crate::{Entry, Versions};

/// One kind of tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub group: u8,
    /// 0 and meaningless when `any_code` is set.
    pub code: u16,
    /// Every code in the group is this tag, and the code is its value, like
    /// a sound effect id.
    pub any_code: bool,
    pub name: &'static str,
    pub args: Args,
    pub notes: Option<&'static str>,
    /// The table naming the argument's values, for a 1-byte argument that
    /// has one. Read through [`super::Tables::values`].
    pub values: Option<&'static [Entry]>,
    /// As [`Entry::versions`].
    pub versions: Versions,
}

/// What a tag's argument bytes hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Args {
    None,
    /// One byte, which some tags carry but no handler reads.
    U8,
    U16,
    U32,
    /// A u8, then the reading as 2-byte characters. The renderer keeps the
    /// u8, likely the base character count. The reading can end on half a
    /// character.
    Ruby,
}

impl Args {
    /// How many argument bytes a tag with these carries, or `None` for a
    /// length that varies.
    #[must_use]
    pub const fn fixed_len(self) -> Option<usize> {
        match self {
            Self::None => Some(0),
            Self::U8 => Some(1),
            Self::U16 => Some(2),
            Self::U32 => Some(4),
            Self::Ruby => None,
        }
    }
}

pub(crate) const fn tag(group: u8, code: u16, name: &'static str, args: Args) -> Tag {
    Tag {
        group,
        code,
        any_code: false,
        name,
        args,
        notes: None,
        values: None,
        versions: Versions::ANY,
    }
}

impl Tag {
    pub(crate) const fn any_code(self) -> Self {
        Self {
            any_code: true,
            ..self
        }
    }

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

    pub(crate) const fn only(self, versions: Versions) -> Self {
        Self { versions, ..self }
    }
}
