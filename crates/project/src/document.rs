//! An open file, decoded into its format's structs. Front ends read the
//! structs straight off it, through the format's own types.

use diag::Diagnostics;
use formats::{Decode, FormatError};

pub struct Document<T> {
    /// Relative to the project's root.
    path: String,
    data: T,
}

impl<T: Decode> Document<T> {
    /// Whether `bytes` look like `T`, by magic where it has one.
    #[must_use]
    pub fn detect(bytes: &[u8]) -> bool {
        T::detect(bytes)
    }

    /// `bytes` read from `path`, relative to the project's root, with the
    /// problems the format carried on past.
    ///
    /// # Errors
    ///
    /// When `bytes` aren't `T`, or are broken.
    pub fn decode(path: String, bytes: &[u8]) -> Result<(Self, Diagnostics), FormatError> {
        let mut diag = Diagnostics::default();
        let data = T::decode(bytes, &mut diag)?;
        Ok((Self { path, data }, diag))
    }
}

impl<T> Document<T> {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub const fn get(&self) -> &T {
        &self.data
    }
}
