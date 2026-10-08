//! File compression. Yaz0 and Yay0.
//!
//! Instead of storing repeated bytes, both store a short back-reference to
//! bytes already written. The tokens are shared, see `token`, and so is the
//! match search. Each token has one bit, read top bit first: 1 for a literal,
//! 0 for a back-reference. The formats differ only in where the bits and
//! tokens go: Yaz0 interleaves them, and Yay0 splits them into three streams.
//!
//! A loader that reads only part of a file can't read Yay0.
//! Its three streams are decoded together, so it needs the whole file at once,
//! whereas Yaz0 decodes front to back.
//!
//! The output doubles as the dictionary a back-reference reads from, and a
//! run's source and destination can overlap.

mod search;
mod token;
mod yay0;
mod yaz0;

pub use yay0::Yay0;
pub use yaz0::Yaz0;

use crate::{Be32, Error, Reader, Record, Result, Writer};

/// A compression wrapper, told apart by its magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Compression {
    Yaz0,
    Yay0,
}

/// How the encoder searches for back-references.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    /// Nintendo's own search, so a retail file comes back byte for byte.
    /// What decoding sets, since it's what made retail files.
    Parity,
    /// Chases longer back-references for a smaller file. Slower, and no
    /// longer byte for byte with retail.
    Extensive,
}

impl Compression {
    pub const ALL: [Self; 2] = [Self::Yaz0, Self::Yay0];

    #[must_use]
    pub const fn magic(self) -> [u8; 4] {
        match self {
            Self::Yaz0 => Yaz0::MAGIC,
            Self::Yay0 => Yay0::MAGIC,
        }
    }

    /// The wrapper `bytes` open with, if any.
    #[must_use]
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|compression| bytes.starts_with(&compression.magic()))
    }

    /// Unwraps `bytes`, which open with this wrapper's magic.
    ///
    /// # Errors
    ///
    /// [`Error::WrongMagic`] for another wrapper, or any other [`Error`] for a
    /// broken one.
    pub fn decompress(self, bytes: &[u8]) -> Result<Vec<u8>> {
        match self {
            Self::Yaz0 => yaz0::decompress(bytes),
            Self::Yay0 => yay0::decompress(bytes),
        }
    }

    /// Wraps `data` and appends it to `out`.
    ///
    /// # Errors
    ///
    /// [`Error::TooLarge`] if `data` doesn't fit a 32-bit size.
    pub fn compress(self, data: &[u8], strategy: Strategy, out: &mut Writer) -> Result<()> {
        match self {
            Self::Yaz0 => yaz0::compress(data, strategy, out),
            Self::Yay0 => yay0::compress(data, strategy, out),
        }
    }
}

/// The most output one input byte can stand for: a full extended
/// back-reference is 3 bytes for `MAX_LENGTH`. Flag and mask bits only lower
/// it.
const MAX_EXPANSION: usize = token::backref::MAX_LENGTH as usize / 3;

/// Steps over `magic` and borrows the header after it.
fn header<'a, T: Record>(reader: &mut Reader<'a>, magic: [u8; 4]) -> Result<&'a T> {
    reader.magic(magic)?;
    reader.record()
}

/// A header's decompressed size, which comes from the file, so it's held
/// against what `input_len` bytes could expand to before anything is
/// allocated for it.
fn decompressed_size(size: Be32, input_len: usize) -> Result<usize> {
    let size = size.get() as usize;
    if size > input_len.saturating_mul(MAX_EXPANSION) {
        return Err(Error::Malformed {
            what: "the decompressed size is more than the data can expand to",
        });
    }
    Ok(size)
}

/// `input`'s length, for a header's 32-bit size.
fn size_of(input: &[u8]) -> Result<u32> {
    u32::try_from(input.len()).map_err(|_| Error::TooLarge {
        what: "the decompressed data",
    })
}

#[cfg(test)]
mod tests {
    use super::token::backref::{MAX_LENGTH, MIN_LENGTH};
    use super::*;

    /// Deterministic noise, so a failure repeats.
    fn noise(len: usize) -> Vec<u8> {
        let mut state = 0x1234_5678u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
                state.to_be_bytes()[1]
            })
            .collect()
    }

    fn round_trip(input: &[u8]) {
        for compression in Compression::ALL {
            for strategy in [Strategy::Parity, Strategy::Extensive] {
                let mut out = Writer::new();
                compression.compress(input, strategy, &mut out).unwrap();
                let encoded = out.finish();
                assert_eq!(Compression::detect(&encoded), Some(compression));
                assert_eq!(
                    compression.decompress(&encoded).unwrap(),
                    input,
                    "on {} bytes, {compression:?} {strategy:?}",
                    input.len()
                );
            }
        }
    }

    /// Literals, back-references, and a run overlapping its own output.
    #[test]
    fn round_trips_literals_and_runs() {
        let mut input = b"the quick brown fox jumps over the quick brown dog".to_vec();
        input.extend(std::iter::repeat_n(b'!', 300));
        round_trip(&input);
    }

    /// Both length encodings and the boundary where the nibble runs out.
    #[test]
    fn round_trips_every_match_length() {
        for length in MIN_LENGTH as usize..=MAX_LENGTH as usize + 8 {
            let mut input = noise(length);
            input.extend_from_within(..);
            round_trip(&input);
        }
    }

    /// Lengths either side of a full Yaz0 group of 8 and a Yay0 mask word of
    /// 32, empty included. Noise is all literals, one token per byte.
    #[test]
    fn round_trips_short_buffers() {
        for length in 0..72 {
            round_trip(&b"abc".repeat(24)[..length]);
            round_trip(&noise(length));
        }
    }

    /// The densest possible input still decodes under the expansion cap.
    #[test]
    fn the_longest_runs_fit_the_expansion_cap() {
        round_trip(&vec![0; 0x1_0000]);
    }

    #[test]
    fn a_huge_declared_size_is_refused_before_allocating() {
        for compression in Compression::ALL {
            let mut data = compression.magic().to_vec();
            data.extend_from_slice(&u32::MAX.to_be_bytes());
            data.extend_from_slice(&[0; 8]);
            assert!(matches!(
                compression.decompress(&data),
                Err(Error::Malformed { .. })
            ));
        }
    }
}
