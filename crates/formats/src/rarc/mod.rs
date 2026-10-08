//! RARC, `JKernel`'s resource archive (`.arc`): a directory tree and every
//! file's bytes, mounted and unloaded as one.
//!
//! ```text
//! 0x00  header       magic, sizes, where the info block is
//! 0x20  info         counts, and offsets counted from the info block
//!       nodes        one per directory, naming its run of entries
//!       entries      one per file and directory, `.` and `..` included
//!       names        Shift-JIS, null terminated, each beside a hash of it
//!       file data    each file 0x20 padded: main memory, then ARAM, then disc
//! ```
//!
//! Sections after the info block start 0x20 aligned. Nodes number depth
//! first, and a directory's run is its children, then `.` and `..`. File data
//! goes out in entry order, so the header's two preload sizes are runs of it.
//!
//! Other files reference a member (cross-reference) by its id rather than its path.
//! Ids equal to entry indices let `JKRArchive::findIdResource` skip its search,
//! which the info block's sync flag says.

mod decode;
mod encode;

use diag::Diagnostics;

use crate::{Be16, Be32, Decode, Encode, Flag, Result, Writer, record};

/// An archive taken apart into its root's name and every file under it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rarc {
    /// The name the game mounts the archive under, apart from its file name.
    pub root: String,
    /// Depth first, in entry order. A directory is the shared prefix of its
    /// files' paths, so an empty one doesn't survive a round trip.
    pub files: Vec<File>,
    /// The info block's next free id, `None` when it's the one
    /// [`encode`](Encode::encode) derives. Nothing reads it.
    pub next_id: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct File {
    /// `/`-separated under the root, every component a plain name.
    pub path: String,
    pub data: Vec<u8>,
    /// `None` takes the lowest id no other file holds.
    pub id: Option<u16>,
    pub preload: Preload,
}

/// Which memory a file loads into when its archive mounts.
/// A request the mounting code is free to ignore, and mostly does.
///
/// Declared in the order an archive stores its files.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Preload {
    #[default]
    /// Main RAM pool
    Mram,
    /// The auxiliary pool, reached over DMA.
    Aram,
    /// Read from the disc when asked for.
    Disc,
}

impl Rarc {
    pub const MAGIC: [u8; 4] = *b"RARC";
}

impl Decode for Rarc {
    fn detect(bytes: &[u8]) -> bool {
        bytes.starts_with(&Self::MAGIC)
    }

    fn decode(bytes: &[u8], _: &mut Diagnostics) -> Result<Self> {
        decode::decode(bytes)
    }
}

impl Encode for Rarc {
    /// Ids and paths must be unique, and files grouped by [`Preload`] in entry
    /// order. The entry flags' compression bits are set from each file's magic.
    fn encode(&self, out: &mut Writer) -> Result<()> {
        encode::encode(self, out)
    }
}

record! {
    /// What follows the magic. Offsets are counted from the info block.
    struct Header {
        size: Be32,
        info_at: Be32,
        data_at: Be32,
        data_len: Be32,
        /// The padded length of the file data's main memory run.
        mram: Be32,
        aram: Be32,
        pad: [u8; 4],
    }
}

record! {
    /// Offsets are counted from here.
    struct Info {
        node_count: Be32,
        nodes_at: Be32,
        entry_count: Be32,
        entries_at: Be32,
        names_len: Be32,
        names_at: Be32,
        next_id: Be16,
        synced: Flag,
        pad: [u8; 5],
    }
}

record! {
    /// One directory, naming its run of entries.
    struct Node {
        /// The name's first four bytes uppercased, `ROOT` for the root.
        tag: [u8; 4],
        name: Be32,
        hash: Be16,
        /// Counts `.` and `..` too.
        count: Be16,
        first: Be32,
    }
}

record! {
    /// One file or directory.
    struct Entry {
        id: Be16,
        hash: Be16,
        flags: u8,
        /// Into the name pool, 24-bit big-endian.
        name: [u8; 3],
        /// A file's offset into the data, or a directory's node.
        target: Be32,
        size: Be32,
        pad: [u8; 4],
    }
}

impl Entry {
    fn name(&self) -> u32 {
        let [a, b, c] = self.name;
        u32::from_be_bytes([0, a, b, c])
    }
}

/// Where the info block starts, right after the header. Retail archives all
/// put it here.
const INFO_AT: usize = Rarc::MAGIC.len() + size_of::<Header>();
const ALIGN: usize = 0x20;

/// The name pool opens with `.` and `..`, which every directory names its
/// own two by.
const DOT_AT: u32 = 0;
const DOT_DOT_AT: u32 = 2;

/// Directories share the id that is no id.
const NO_ID: u16 = 0xFFFF;
/// What a directory entry states as its size, the record's own on retail.
const DIR_SIZE: u32 = 0x10;
/// What the root's `..` points at.
const NO_NODE: u32 = u32::MAX;

/// Each flag is one bit of an entry's top byte.
mod flag {
    pub const FILE: u8 = 0x01;
    pub const DIR: u8 = 0x02;
    /// Yaz0 if `YAZ0` is set too, otherwise Yay0.
    pub const COMPRESSED: u8 = 0x04;
    pub const MRAM: u8 = 0x10;
    pub const ARAM: u8 = 0x20;
    pub const DISC: u8 = 0x40;
    pub const YAZ0: u8 = 0x80;
}

/// The hash stored beside every name: each byte added to three times the total.
fn name_hash(name: &[u8]) -> u16 {
    name.iter().fold(0, |hash: u16, &b| {
        hash.wrapping_mul(3).wrapping_add(u16::from(b))
    })
}

/// The info block's next free id. With synced ids every entry, directories
/// included, holds an id, so it's the entry count.
fn next_id(entry_count: usize, highest: Option<u16>, synced: bool) -> Result<u16> {
    let next = if synced {
        entry_count
    } else {
        highest.map_or(0, |id| usize::from(id) + 1)
    };
    u16::try_from(next).map_err(|_| crate::Error::TooLarge {
        what: "the next free id",
    })
}

/// Whether `name` can be a path component, on disk and in the name pool.
fn is_plain(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

#[cfg(test)]
mod tests;
