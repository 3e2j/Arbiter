//! Taking a disc file apart into the file a base stores.
//!
//! Only the magic decides what a file is, since some retail names extensions
//! lie about their contents.
// TODO: Archives open here next, nested any depth.

use std::borrow::Cow;

use formats::compression::Compression;

/// A disc file with its compression taken off.
pub struct Peeled<'a> {
    /// The wrapper that came off, to put back on a build.
    pub compression: Option<Compression>,
    pub bytes: Cow<'a, [u8]>,
}

/// Takes off `bytes`' compression, if any.
///
/// # Errors
///
/// If the wrapper is broken.
pub fn peel(bytes: &[u8]) -> formats::Result<Peeled<'_>> {
    let compression = Compression::detect(bytes);
    let bytes = match compression {
        Some(compression) => Cow::Owned(compression.decompress(bytes)?),
        None => Cow::Borrowed(bytes),
    };
    Ok(Peeled { compression, bytes })
}

#[cfg(test)]
mod tests {
    use formats::Writer;
    use formats::compression::Strategy;

    use super::*;

    #[test]
    fn plain_bytes_pass_through_borrowed() {
        let peeled = peel(b"RARC....").unwrap();
        assert_eq!(peeled.compression, None);
        assert!(matches!(peeled.bytes, Cow::Borrowed(b"RARC....")));
    }

    #[test]
    fn compression_comes_off_and_is_named() {
        for compression in Compression::ALL {
            let mut out = Writer::new();
            compression
                .compress(b"RARC RARC RARC", Strategy::Parity, &mut out)
                .unwrap();
            let wrapped = out.finish();
            let peeled = peel(&wrapped).unwrap();
            assert_eq!(peeled.compression, Some(compression));
            assert_eq!(*peeled.bytes, *b"RARC RARC RARC");
        }
    }
}
