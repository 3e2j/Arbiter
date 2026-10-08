//! The game's file formats as plain structures. Knows no game or project.
//!
//! Every format implements [`Decode`] and [`Encode`].
//! One that can be edited implements:
//! - [`Edit`], in either project kind.
//! - [`Patch`], which stores the edits as changes against the base (for mods).
//!
//! Data is big-endian, read through [`Reader`] and written through [`Writer`].
//! Fixed tables are [`record!`] structs, borrowed and copied whole. The types
//! their fields can be are listed on [`Record`].

pub mod compression;
pub mod rarc;

mod reader;
mod record;
mod writer;

use diag::{Address, Diagnostics};
use serde::{Serialize, de::DeserializeOwned};

pub use reader::Reader;
pub use record::{Be16, Be32, Flag, Record, bytes_of};
pub use writer::Writer;

/// Why bytes couldn't become a format, or a format couldn't become bytes.
///
/// Every offset and count comes out of a file somebody else wrote.
/// Problems a decode can carry on past go to `Diagnostics` instead.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("read of {len} bytes at {pos:#x} runs past the end of a {size:#x} byte buffer")]
    OutOfBounds { pos: usize, len: usize, size: usize },

    #[error("the string at {pos:#x} is not terminated before the end of the buffer")]
    Unterminated { pos: usize },

    #[error("expected magic `{}`", expected.escape_ascii())]
    WrongMagic { expected: [u8; 4] },

    #[error("{what} is too large for the format to store")]
    TooLarge { what: &'static str },

    #[error("malformed: {what}")]
    Malformed { what: &'static str },

    #[error("{name:?} can't be a file name")]
    Name { name: String },
}

pub type Result<T> = std::result::Result<T, Error>;

/// Bytes into a format. The decoded form owns its data.
pub trait Decode: Sized {
    /// Whether `bytes` look like this format, by magic where it has one.
    fn detect(bytes: &[u8]) -> bool;

    /// Takes the file apart. Problems it can carry on past go to `diag`.
    ///
    /// # Errors
    ///
    /// [`Error::WrongMagic`] when `bytes` aren't this format, any other
    /// [`Error`] when they are but are broken.
    fn decode(bytes: &[u8], diag: &mut Diagnostics) -> Result<Self>;
}

/// A format back into bytes, appended to `out` so a container can encode its
/// contents in place.
pub trait Encode {
    /// # Errors
    ///
    /// When the value doesn't fit the format, such as a size field
    /// overflowing ([`Error::TooLarge`]).
    fn encode(&self, out: &mut Writer) -> Result<()>;
}

/// A document changed through edits, with undo. An edit is an intent keyed by
/// stable ids, never a row index. It never fails for being invalid, only a
/// check does, so one naming something the document doesn't hold changes
/// nothing.
pub trait Edit {
    type Edit;
    type Item: Address;

    /// Returns the edit that undoes this one, and the items it touched for
    /// rechecking.
    fn apply(&mut self, edit: Self::Edit) -> (Self::Edit, Vec<Self::Item>);

    /// Whether `next` joins `edit`'s undo step, such as typing in one field.
    fn merges(_edit: &Self::Edit, _next: &Self::Edit) -> bool {
        false
    }
}

/// A document stored in a mod as its changes against the base.
pub trait Patch: Sized {
    /// Keyed by a stable id, as [`Edit::Edit`] is.
    type Change: Serialize + DeserializeOwned;

    /// What turns `base` into `edited`.
    fn diff(base: &Self, edited: &Self) -> Vec<Self::Change>;

    /// Applies `changes` to the base. One that no longer fits it, such as one
    /// naming something removed since, is skipped and reported to `diag`.
    fn patch(&mut self, changes: &[Self::Change], diag: &mut Diagnostics);
}
