//! Structs laid out in memory exactly as a file lays them out, so a table is
//! read by borrowing it and written by copying it.

use std::fmt;

/// A type whose memory layout is its file layout.
///
/// [`Reader::record`](crate::Reader::record) borrows one straight out of the
/// buffer, and [`Writer::record`](crate::Writer::record) appends one as it
/// stands.
///
/// Every record is built from these, and only these:
///
/// - `u8`
/// - [`Be16`], a big-endian `u16`
/// - [`Be32`], a big-endian `u32`
/// - [`Flag`], a byte read as a `bool`
/// - `[T; N]` of any of the above
/// - another struct declared with [`record!`](crate::record!)
///
/// Define records with [`record!`](crate::record!), which checks every field
/// and writes the impl, rather than implementing this by hand.
///
/// # Safety
///
/// The implementor **must**:
///
/// - be `#[repr(C)]` or `#[repr(transparent)]`, so fields keep declaration
///   order and add no padding;
/// - hold only `Record` fields, so it has alignment 1 and every bit pattern
///   is valid.
pub unsafe trait Record: Sized {
    /// How many bytes the record takes in the file.
    const LEN: usize = size_of::<Self>();

    /// The record exactly as the file stores it.
    fn as_bytes(&self) -> &[u8] {
        bytes_of(std::slice::from_ref(self))
    }
}

// SAFETY: one byte, and every value is valid.
unsafe impl Record for u8 {}
// SAFETY: an array of align-1 elements with no padding has none either.
unsafe impl<T: Record, const N: usize> Record for [T; N] {}

/// Defines a `#[repr(C)]` struct and implements [`Record`] for it. A field
/// whose type is not `Record`, such as a `bool` or a native `u32`, fails to
/// compile.
///
/// ```
/// formats::record! {
///     pub struct Header {
///         pub magic: [u8; 4],
///         pub size: formats::Be32,
///     }
/// }
/// ```
///
/// ```compile_fail
/// formats::record! {
///     struct Flagged {
///         set: bool,
///     }
/// }
/// ```
#[macro_export]
macro_rules! record {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $($(#[$field_meta:meta])* $field_vis:vis $field:ident: $ty:ty),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[repr(C)]
        $vis struct $name {
            $($(#[$field_meta])* $field_vis $field: $ty),*
        }

        const _: () = {
            const fn field<T: $crate::Record>() {}
            $(field::<$ty>();)*
        };

        // SAFETY: repr(C), and every field is `Record`, checked above.
        unsafe impl $crate::Record for $name {}
    };
}

/// A run of records exactly as the file stores it.
#[must_use]
pub const fn bytes_of<T: Record>(records: &[T]) -> &[u8] {
    const { assert!(align_of::<T>() == 1, "a record must be aligned to 1") };
    // SAFETY: `Record` rules out padding, so every byte behind `records` is
    // initialized, and a `u8` slice needs no alignment.
    unsafe { std::slice::from_raw_parts(records.as_ptr().cast::<u8>(), size_of_val(records)) }
}

/// A big-endian `u16` as it sits in a file: two bytes, aligned to one.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Be16([u8; 2]);

impl Be16 {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value.to_be_bytes())
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        u16::from_be_bytes(self.0)
    }
}

impl fmt::Debug for Be16 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#X}", self.get())
    }
}

// SAFETY: repr(transparent) over a byte array.
unsafe impl Record for Be16 {}

/// A big-endian `u32` as it sits in a file: four bytes, aligned to one.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Be32([u8; 4]);

impl Be32 {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value.to_be_bytes())
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        u32::from_be_bytes(self.0)
    }
}

impl fmt::Debug for Be32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#X}", self.get())
    }
}

// SAFETY: repr(transparent) over a byte array.
unsafe impl Record for Be32 {}

/// A one-byte flag as it sits in a file. Any byte is a valid `Flag`, unlike
/// a `bool`, and any nonzero byte reads as set.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
#[repr(transparent)]
pub struct Flag(u8);

impl Flag {
    #[must_use]
    pub const fn new(value: bool) -> Self {
        Self(value as u8)
    }

    #[must_use]
    pub const fn get(self) -> bool {
        self.0 != 0
    }
}

impl fmt::Debug for Flag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.get())
    }
}

// SAFETY: repr(transparent) over a byte, and `Flag` gives every value a
// meaning.
unsafe impl Record for Flag {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, Reader, Writer};

    record! {
        /// Fields of every width, so one at an odd position proves nothing
        /// needed alignment.
        #[derive(Debug, PartialEq, Eq)]
        struct Sample {
            tag: u8,
            wide: Be32,
            narrow: Be16,
            set: Flag,
        }
    }

    const SAMPLE: [u8; 8] = [0x0D, 0x00, 0x01, 0x02, 0x03, 0xAC, 0xED, 0x01];

    fn sample() -> Sample {
        Sample {
            tag: 0x0D,
            wide: Be32::new(0x0001_0203),
            narrow: Be16::new(0xACED),
            set: Flag::new(true),
        }
    }

    #[test]
    fn a_record_is_its_bytes() {
        assert_eq!(Sample::LEN, SAMPLE.len());
        assert_eq!(sample().as_bytes(), SAMPLE);

        let mut out = Writer::new();
        out.record(&sample());
        assert_eq!(out.finish(), SAMPLE);
    }

    #[test]
    fn any_nonzero_flag_is_set() {
        assert!(!Flag(0).get());
        assert!(Flag(1).get());
        assert!(Flag(0x80).get());
    }

    /// Starting at an odd position proves the record never needed alignment.
    #[test]
    fn records_borrow_from_any_position() {
        let mut bytes = vec![0xFF];
        bytes.extend_from_slice(&SAMPLE);
        bytes.extend_from_slice(&SAMPLE);

        let mut reader = Reader::new(&bytes);
        reader.seek(1);
        assert_eq!(*reader.record::<Sample>().unwrap(), sample());
        assert_eq!(reader.pos(), 1 + SAMPLE.len());

        let both = reader.records_at::<Sample>(1, 2).unwrap();
        assert_eq!(both, [sample(), sample()]);
        assert!(matches!(
            reader.records_at::<Sample>(1, 3),
            Err(Error::OutOfBounds { .. })
        ));
        assert!(reader.records_at::<Sample>(0, usize::MAX).is_err());
    }
}
