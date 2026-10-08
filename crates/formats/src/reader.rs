use crate::{Error, Record, Result};

/// A cursor over a borrowed buffer.
///
/// Sequential reads advance the cursor. The `_at` reads take an absolute
/// position and leave it alone, which is what following an offset out of a
/// header amounts to. Both hand back slices of the original buffer.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    #[must_use]
    pub const fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.data.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    #[must_use]
    pub const fn pos(&self) -> usize {
        self.pos
    }

    /// Bytes after the cursor, zero once it's past the end.
    #[must_use]
    pub const fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Moves the cursor.
    /// Landing past the end is not an error until something is read from there.
    pub const fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    /// Borrows `len` bytes at an absolute position. Every read goes through
    /// this bounds check.
    ///
    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if `pos..pos + len` runs past the end.
    pub fn bytes_at(&self, pos: usize, len: usize) -> Result<&'a [u8]> {
        let out_of_bounds = || Error::OutOfBounds {
            pos,
            len,
            size: self.data.len(),
        };
        let end = pos.checked_add(len).ok_or_else(out_of_bounds)?;
        self.data.get(pos..end).ok_or_else(out_of_bounds)
    }

    /// Borrows `len` bytes at the cursor and steps over them.
    ///
    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if `len` bytes are not left.
    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let out = self.bytes_at(self.pos, len)?;
        // `bytes_at` proved `pos + len` fits in the buffer.
        self.pos = self.pos.saturating_add(len);
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let out = self.array_at(self.pos)?;
        self.pos = self.pos.saturating_add(N);
        Ok(out)
    }

    fn array_at<const N: usize>(&self, pos: usize) -> Result<[u8; N]> {
        let bytes = self.bytes_at(pos, N)?;
        bytes.try_into().map_err(|_| Error::OutOfBounds {
            pos,
            len: N,
            size: self.data.len(),
        })
    }

    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if a byte is not left.
    pub fn u8(&mut self) -> Result<u8> {
        let [byte] = self.array()?;
        Ok(byte)
    }

    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if 2 bytes are not left.
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if 4 bytes are not left.
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    /// Steps over `expected` at the cursor. A mismatch leaves the cursor
    /// where it was.
    ///
    /// # Errors
    ///
    /// [`Error::WrongMagic`] if the next 4 bytes differ, or fewer are left.
    pub fn magic(&mut self, expected: [u8; 4]) -> Result<()> {
        match self.array_at::<4>(self.pos) {
            Ok(found) if found == expected => {
                self.pos = self.pos.saturating_add(4);
                Ok(())
            }
            _ => Err(Error::WrongMagic { expected }),
        }
    }

    /// Borrows a record at the cursor and steps over it.
    ///
    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if the record runs past the end.
    pub fn record<T: Record>(&mut self) -> Result<&'a T> {
        const { assert!(align_of::<T>() == 1, "a record must be aligned to 1") };
        let bytes = self.bytes(T::LEN)?;
        // SAFETY: `bytes` is exactly one `T` long and borrowed for as long as
        // the result. `T` is aligned to 1 (checked above), so any address
        // suits it, and `Record` promises every bit pattern is a valid `T`.
        Ok(unsafe { &*bytes.as_ptr().cast::<T>() })
    }

    /// Borrows `count` records laid end to end at an absolute position. The
    /// bounds check comes before anything is made of `count`, which is
    /// usually from the file.
    ///
    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if the records run past the end.
    pub fn records_at<T: Record>(&self, pos: usize, count: usize) -> Result<&'a [T]> {
        const { assert!(align_of::<T>() == 1, "a record must be aligned to 1") };
        let len = count.checked_mul(T::LEN).ok_or(Error::OutOfBounds {
            pos,
            len: usize::MAX,
            size: self.data.len(),
        })?;
        let bytes = self.bytes_at(pos, len)?;
        // SAFETY: `bytes` is exactly `count` records long and borrowed for as
        // long as the result. `T` is aligned to 1 (checked above), so any
        // address suits it, and `Record` promises every bit pattern is a
        // valid `T`.
        Ok(unsafe { std::slice::from_raw_parts(bytes.as_ptr().cast::<T>(), count) })
    }

    /// Borrows the null-terminated bytes at an absolute position, terminator
    /// excluded. Their encoding is the caller's business.
    ///
    /// # Errors
    ///
    /// [`Error::OutOfBounds`] if `pos` is past the end, or
    /// [`Error::Unterminated`] if no null byte follows it.
    pub fn cstr_at(&self, pos: usize) -> Result<&'a [u8]> {
        let rest = self.data.get(pos..).ok_or(Error::OutOfBounds {
            pos,
            len: 1,
            size: self.data.len(),
        })?;
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(Error::Unterminated { pos })?;
        rest.get(..end).ok_or(Error::Unterminated { pos })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_advance_and_stay_in_bounds() {
        let mut reader = Reader::new(&[0x00, 0x01, 0x02, 0x03, 0x04]);
        assert_eq!(reader.u32().unwrap(), 0x0001_0203);
        assert_eq!(reader.pos(), 4);
        assert_eq!(reader.u8().unwrap(), 0x04);
        assert!(matches!(reader.u8(), Err(Error::OutOfBounds { .. })));
    }

    #[test]
    fn absolute_reads_leave_the_cursor_alone() {
        let reader = Reader::new(&[0x0D, 0xEF, 0xAC, 0xED]);
        assert_eq!(reader.bytes_at(2, 2).unwrap(), [0xAC, 0xED]);
        assert_eq!(reader.pos(), 0);
    }

    /// A length that overflows when added to the position has to read as out
    /// of bounds rather than wrapping into a range that happens to exist.
    #[test]
    fn absurd_lengths_do_not_wrap() {
        let reader = Reader::new(&[0u8; 8]);
        assert!(matches!(
            reader.bytes_at(4, usize::MAX),
            Err(Error::OutOfBounds { .. })
        ));
    }

    #[test]
    fn magic_steps_over_only_a_match() {
        let mut reader = Reader::new(b"Yaz0rest");
        assert_eq!(
            reader.magic(*b"RARC"),
            Err(Error::WrongMagic { expected: *b"RARC" })
        );
        assert_eq!(reader.pos(), 0);
        reader.magic(*b"Yaz0").unwrap();
        assert_eq!(reader.pos(), 4);

        let mut short = Reader::new(b"Ya");
        assert!(matches!(
            short.magic(*b"Yaz0"),
            Err(Error::WrongMagic { .. })
        ));
    }

    #[test]
    fn strings_stop_at_the_terminator() {
        let reader = Reader::new(b"name\0next\0");
        assert_eq!(reader.cstr_at(0).unwrap(), b"name");
        assert_eq!(reader.cstr_at(5).unwrap(), b"next");
        assert!(matches!(
            Reader::new(b"unterminated").cstr_at(0),
            Err(Error::Unterminated { .. })
        ));
    }
}
