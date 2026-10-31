//! BMG, `JMessage`'s message file (`.bmg`).
//!
//! ```text
//! 0x00  header      magic, kind, a size, the section count, the encoding
//! 0x20  sections    one after another, each naming itself and its own size
//! ```
//!
//! Every section states its own size padded to 0x20, so the table is walked
//! rather than indexed. The last one stops where its bytes do, though its
//! stated size still counts the padding. These six are the only sections
//! any retail file has:
//!
//! ```text
//! INF1  one fixed-width record per message: where its text starts, then the
//!       attribute bytes saying how it's displayed
//! DAT1  the text, every message null terminated
//! MID1  the ids the game looks messages up by, one per INF1 record
//! STR1  a pool of null terminated strings that attributes point into
//! FLW1  the flow graph: one record per node, and the edge table they index
//! FLI1  which node each flow id enters the graph at
//! ```
//!
//! Only INF1 and DAT1 are always there. A file addressed by position has no
//! MID1, and one with no conversations has neither flow section.
//!
//! What a tag means, what an event does, and which attribute byte holds the
//! speaker are the game's business, so text and attributes stay raw.

mod decode;
mod edit;
mod encode;
mod flow;
mod message;

use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;

use diag::Diagnostics;
use encoding_rs::{SHIFT_JIS, UTF_8, UTF_16BE, WINDOWS_1252};

use crate::{Be32, ByteWriter, Decode, Encode, FormatError, Result, record};

pub use edit::BmgEdit;
pub use flow::{Flow, Node, NodeId, Root};
pub use message::{Message, MessageKey, Mid1Header, TextSegment};

/// A message file taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bmg {
    pub encoding: Encoding,
    /// Every message's attributes width, INF1's record width less its text
    /// offset. Carried because a file with no messages still states one.
    pub attributes_len: u16,
    /// `None` for a file with no MID1, which is addressed by position.
    pub mid1: Option<Mid1Header>,
    pub messages: Vec<Message>,
    /// FLW1 and FLI1, which are both there or both absent.
    pub flow: Option<Flow>,
    /// STR1 split on its terminators. Trailing empty strings are kept, as
    /// they're how the pool's padding comes back.
    pub strings: Option<Vec<Vec<u8>>>,
}

/// What the text is encoded in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Encoding {
    /// Predates the byte, so whatever the reading game assumed.
    Legacy = 0x00,
    Windows1252 = 0x01,
    Utf16Be = 0x02,
    #[default]
    ShiftJis = 0x03,
    Utf8 = 0x04,
}

impl Encoding {
    /// An unknown byte reads as Shift-JIS, the guess the game makes.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        match byte {
            0x00 => Self::Legacy,
            0x01 => Self::Windows1252,
            0x02 => Self::Utf16Be,
            0x04 => Self::Utf8,
            _ => Self::ShiftJis,
        }
    }

    #[must_use]
    pub const fn byte(self) -> u8 {
        self as u8
    }

    /// `bytes` as text. Legacy reads as Shift-JIS, as an unknown byte does,
    /// and anything malformed as U+FFFD.
    #[must_use]
    pub fn decode(self, bytes: &[u8]) -> Cow<'_, str> {
        let encoding = match self {
            Self::Legacy | Self::ShiftJis => SHIFT_JIS,
            Self::Windows1252 => WINDOWS_1252,
            Self::Utf16Be => UTF_16BE,
            Self::Utf8 => UTF_8,
        };
        encoding.decode_without_bom_handling(bytes).0
    }
}

impl Bmg {
    /// `MESG`, then the kind `bmg1`.
    pub const MAGIC: [u8; 4] = *b"MESG";
    pub const KIND: [u8; 4] = *b"bmg1";
}

impl Decode for Bmg {
    fn detect(bytes: &[u8]) -> bool {
        bytes.starts_with(&Self::MAGIC) && bytes.get(4..8) == Some(&Self::KIND)
    }

    fn decode(bytes: &[u8], _: &mut Diagnostics) -> Result<Self> {
        decode::decode(bytes)
    }
}

impl Encode for Bmg {
    /// Message and node ids must be unique, every edge must name a node the
    /// flow holds, and every message's attributes must be `attributes_len`
    /// wide. A retail file comes back byte for byte.
    fn encode(&self, out: &mut ByteWriter) -> Result<()> {
        encode::encode(self, out)
    }
}

record! {
    /// What follows the magic and kind.
    struct Header {
        /// Where the sections end, the flow pair left out.
        size: Be32,
        section_count: Be32,
        /// An [`Encoding`] byte.
        encoding: u8,
        pad: [u8; 15],
    }
}

record! {
    /// What every section opens with.
    struct SectionHeader {
        magic: [u8; 4],
        /// The whole section, this header and trailing padding included.
        size: Be32,
    }
}

const HEADER_LEN: usize = Bmg::MAGIC.len() + Bmg::KIND.len() + size_of::<Header>();
const ALIGN: usize = 0x20;

/// Message information: one record per message.
const INF1: [u8; 4] = *b"INF1";
/// Message text, which INF1 records point into.
const DAT1: [u8; 4] = *b"DAT1";
/// Message ids, one per INF1 record.
const MID1: [u8; 4] = *b"MID1";
/// Strings attributes point into.
const STR1: [u8; 4] = *b"STR1";
/// Flow nodes and edges.
const FLW1: [u8; 4] = *b"FLW1";
/// Flow ids, the ways into the graph.
const FLI1: [u8; 4] = *b"FLI1";

/// Maps each handle to its position, which is how the file refers to a
/// message or a node. Two equal handles are refused, as a reference to
/// either could go both ways.
fn positions<Id: Copy + Eq + Hash>(
    ids: impl ExactSizeIterator<Item = Id>,
    duplicate: &'static str,
) -> Result<HashMap<Id, u16>> {
    let count = u16::try_from(ids.len()).map_err(|_| too_large("the message or node count"))?;
    let positions: HashMap<_, _> = ids.zip(0..count).collect();
    if positions.len() != usize::from(count) {
        return Err(malformed(duplicate));
    }
    Ok(positions)
}

const fn malformed(what: &'static str) -> FormatError {
    FormatError::Malformed { what }
}

const fn too_large(what: &'static str) -> FormatError {
    FormatError::TooLarge { what }
}

#[cfg(test)]
mod tests;
