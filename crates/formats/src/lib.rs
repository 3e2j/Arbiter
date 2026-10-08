//! The game's file formats as plain structures. Knows no game or project.
//!
//! Every format implements [`Decode`] and [`Encode`].
//! Data is big-endian, read through [`Reader`] and written through [`Writer`].

mod reader;
mod writer;

use diag::Diagnostics;

pub use reader::Reader;
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
