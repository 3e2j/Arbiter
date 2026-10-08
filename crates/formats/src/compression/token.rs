/// One step of the output: a literal byte, or a back-reference.
pub enum Token {
    Literal(u8),
    BackReference(backref::Backreference),
}

pub mod backref {
    use crate::{Error, Result};

    /// A 4-bit length nibble and a 12-bit distance. An extra byte follows for
    /// lengths the nibble can't hold.
    pub type Pair = u16;
    const PAIR_SIZE: u16 = 2;
    const _: () = assert!(PAIR_SIZE as usize == size_of::<Pair>());

    /// One more than a pair, or a literal would have been as cheap.
    pub const MIN_LENGTH: u16 = PAIR_SIZE + 1;

    /// Too long for the nibble, so an extra byte follows. That byte holds how
    /// far past this the length reaches, not the length itself.
    pub const MIN_EXTENDED_LENGTH: u16 = MIN_LENGTH + 0xF;

    /// A full extra byte on top of `MIN_EXTENDED_LENGTH`.
    pub const MAX_LENGTH: u16 = 0xFF + MIN_EXTENDED_LENGTH;

    pub const DISTANCE_MASK: u16 = 0xFFF;
    /// A stored distance of 0 still jumps back one, so the real distance is
    /// the stored value plus one.
    pub const MAX_DISTANCE: u16 = DISTANCE_MASK + 1;

    /// A back-reference's distance and length, apart from the pair and extra
    /// byte it packs into.
    ///
    /// The packing lives only in [`from_pair`](Self::from_pair),
    /// [`pair`](Self::pair) and [`extended`](Self::extended), so reading and
    /// writing can't drift apart. Where the pair and extra byte go is up to
    /// each format.
    #[derive(Clone, Copy)]
    pub struct Backreference {
        distance: u16,
        length: u16,
    }

    impl Backreference {
        /// A match of `length` bytes found `distance` bytes back, or `None`
        /// if the format has no room for it.
        pub fn new(distance: usize, length: usize) -> Option<Self> {
            let distance = u16::try_from(distance)
                .ok()
                .filter(|distance| (1..=MAX_DISTANCE).contains(distance))?;
            let length = u16::try_from(length)
                .ok()
                .filter(|length| (MIN_LENGTH..=MAX_LENGTH).contains(length))?;
            Some(Self { distance, length })
        }

        pub const fn length(self) -> u16 {
            self.length
        }

        /// Unpacks a pair, calling `extended` for the byte that follows it
        /// when the nibble is zero.
        pub fn from_pair(pair: Pair, extended: impl FnOnce() -> Result<u8>) -> Result<Self> {
            let distance = (pair & DISTANCE_MASK) + 1;
            let length = match pair >> 12 {
                0 => u16::from(extended()?) + MIN_EXTENDED_LENGTH,
                nibble => nibble - 1 + MIN_LENGTH,
            };
            Ok(Self { distance, length })
        }

        /// The inverse of [`from_pair`](Self::from_pair).
        pub const fn pair(self) -> Pair {
            let distance = self.distance - 1;
            if self.length < MIN_EXTENDED_LENGTH {
                // Never 0, which marks an extra byte.
                let nibble = self.length - (MIN_LENGTH - 1);
                nibble << 12 | distance
            } else {
                distance
            }
        }

        /// The byte that follows the pair, for a length the nibble can't hold.
        pub fn extended(self) -> Option<u8> {
            u8::try_from(self.length.checked_sub(MIN_EXTENDED_LENGTH)?).ok()
        }

        /// Copies this run into `out` at `pos` from the bytes already behind
        /// it, and returns where the run ends.
        ///
        /// # Errors
        ///
        /// [`Error::Malformed`] if it reaches before the start of `out`, or
        /// runs past its end.
        pub fn copy(self, out: &mut [u8], pos: usize) -> Result<usize> {
            let (distance, length) = (usize::from(self.distance), usize::from(self.length));
            let start = pos.checked_sub(distance).ok_or(Error::Malformed {
                what: "a back-reference reaches before the start of the output",
            })?;
            let end = pos + length;
            if end > out.len() {
                return Err(Error::Malformed {
                    what: "a back-reference runs past the decompressed size",
                });
            }

            // A run longer than its distance repeats the bytes behind it, so
            // each pass copies everything written since `start`, doubling the
            // chunk.
            let mut pos = pos;
            while pos < end {
                let chunk = (pos - start).min(end - pos);
                out.copy_within(start..start + chunk, pos);
                pos += chunk;
            }
            Ok(end)
        }
    }
}
