//! Taking a disc file apart into the files a base stores.
//!
//! Compression comes off and archives open into their members, nested any
//! depth. A compressed archive holding a compressed message:
//!
//! ```text
//! bmgres.arc             Yaz0   peeled
//! bmgres.arc             RARC   opened
//! bmgres.arc/zel_00.bmg  Yaz0   peeled
//! bmgres.arc/zel_00.bmg  BMG    Piece::File
//! bmgres.arc                    Piece::Archive, after its members
//! ```
//!
//! Each compression comes out once: a disc file's on its own piece, an
//! archive's on its own [`Piece::Archive`], and a plain member's in the
//! `members` of the archive holding it.
//!
//! Only the magic decides what a file is, since some retail names extensions
//! lie about their contents.

use std::borrow::Cow;

use diag::Diagnostics;
use formats::{Decode, compression::Compression, rarc::Rarc};

use crate::disc::is_plain;

/// How deep archives open. A guard only: uncompressed nesting shrinks every
/// level, so only a compressed archive that holds itself would open forever.
/// The limit is picked freely, not derived.
// TODO: Report hitting it as a diagnostic instead of failing the unpack, and
// let the user raise it.
pub const MAX_DEPTH: usize = 8;

/// One thing a disc file became.
#[derive(Clone, Copy)]
pub enum Piece<'a> {
    /// Bytes to store at `path`. `compression` came off a disc file, and is
    /// `None` for a member, whose archive carries it instead.
    File {
        path: &'a str,
        bytes: &'a [u8],
        compression: Option<Compression>,
    },
    /// An archive opened into a directory at `path`, with `compression` taken
    /// off. Comes after its members' pieces.
    Archive {
        path: &'a str,
        archive: &'a Rarc,
        compression: Option<Compression>,
        /// What came off each of `archive.files`, `None` for one that's an
        /// archive itself.
        members: &'a [Option<Compression>],
    },
}

/// Names the innermost file that failed, not the disc file it came in.
#[derive(Debug, thiserror::Error)]
#[error("{path}: {source}")]
pub struct Error {
    pub path: String,
    #[source]
    pub source: formats::Error,
}

/// A disc file with its compression taken off.
pub struct Peeled<'a> {
    /// The wrapper that came off, to put back on a build.
    pub compression: Option<Compression>,
    pub bytes: Cow<'a, [u8]>,
}

/// Takes `bytes` apart and hands `sink` each piece, members ahead of their
/// archive.
///
/// # Errors
///
/// If a wrapper or archive is broken, archives nest past [`MAX_DEPTH`], a
/// member's path isn't plain, or `sink` fails.
pub fn unpack<E: From<Error>>(
    path: &str,
    bytes: &[u8],
    sink: &mut impl FnMut(Piece<'_>) -> Result<(), E>,
) -> Result<(), E> {
    unpack_at(path, bytes, 0, sink).map(drop)
}

/// Returns what came off `bytes` if it stays a file, for its archive to carry.
fn unpack_at<E: From<Error>>(
    path: &str,
    bytes: &[u8],
    depth: usize,
    sink: &mut impl FnMut(Piece<'_>) -> Result<(), E>,
) -> Result<Option<Compression>, E> {
    let at = |source| Error {
        path: path.to_owned(),
        source,
    };
    let Peeled { compression, bytes } = peel(bytes).map_err(at)?;
    if !Rarc::detect(&bytes) {
        sink(Piece::File {
            path,
            bytes: &bytes,
            compression: compression.filter(|_| depth == 0),
        })?;
        return Ok(compression);
    }
    if depth == MAX_DEPTH {
        return Err(at(formats::Error::Malformed {
            what: "archives nest too deep",
        })
        .into());
    }

    // RARC reports nothing here yet. Per-file diagnostics wait on the store.
    let archive = Rarc::decode(&bytes, &mut Diagnostics::default()).map_err(at)?;
    let mut members = Vec::with_capacity(archive.files.len());
    for file in &archive.files {
        let member = format!("{path}/{}", file.path);
        if !is_plain(&member) {
            return Err(Error {
                path: member,
                source: formats::Error::Name {
                    name: file.path.clone(),
                },
            }
            .into());
        }
        members.push(unpack_at(&member, &file.data, depth + 1, sink)?);
    }
    sink(Piece::Archive {
        path,
        archive: &archive,
        compression,
        members: &members,
    })?;
    Ok(None)
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
    use formats::compression::Strategy;
    use formats::rarc::File;
    use formats::{Encode, Writer};

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
            let wrapped = wrap(compression, b"RARC RARC RARC");
            let peeled = peel(&wrapped).unwrap();
            assert_eq!(peeled.compression, Some(compression));
            assert_eq!(*peeled.bytes, *b"RARC RARC RARC");
        }
    }

    fn wrap(compression: Compression, data: &[u8]) -> Vec<u8> {
        let mut out = Writer::new();
        compression
            .compress(data, Strategy::Parity, &mut out)
            .unwrap();
        out.finish()
    }

    fn archive(root: &str, files: &[(&str, &[u8])]) -> Vec<u8> {
        let rarc = Rarc {
            root: root.to_owned(),
            files: files
                .iter()
                .map(|&(path, data)| File {
                    path: path.to_owned(),
                    data: data.to_vec(),
                    ..File::default()
                })
                .collect(),
            next_id: None,
        };
        let mut out = Writer::new();
        rarc.encode(&mut out).unwrap();
        out.finish()
    }

    /// What one piece was, owned.
    #[derive(Debug, PartialEq, Eq)]
    enum Seen {
        File(String, Vec<u8>, Option<Compression>),
        Archive(
            String,
            String,
            Option<Compression>,
            Vec<Option<Compression>>,
        ),
    }

    fn unpack_all(path: &str, bytes: &[u8]) -> Result<Vec<Seen>, Error> {
        let mut seen = Vec::new();
        unpack(path, bytes, &mut |piece| {
            seen.push(match piece {
                Piece::File {
                    path,
                    bytes,
                    compression,
                } => Seen::File(path.to_owned(), bytes.to_vec(), compression),
                Piece::Archive {
                    path,
                    archive,
                    compression,
                    members,
                } => Seen::Archive(
                    path.to_owned(),
                    archive.root.clone(),
                    compression,
                    members.to_vec(),
                ),
            });
            Ok::<_, Error>(())
        })?;
        Ok(seen)
    }

    /// Every wrapper comes off, at every depth, and is named once.
    #[test]
    fn archives_open_nested_and_wrapped() {
        let inner = wrap(
            Compression::Yaz0,
            &archive("inner", &[("deep.bin", b"deep")]),
        );
        let wrapped = wrap(Compression::Yay0, b"member");
        let outer = wrap(
            Compression::Yaz0,
            &archive(
                "outer",
                &[
                    ("inner.arc", &inner),
                    ("dir/wrapped.bin", &wrapped),
                    ("plain.bin", b"plain"),
                ],
            ),
        );

        let yaz0 = Some(Compression::Yaz0);
        assert_eq!(
            unpack_all("res/outer.arc", &outer).unwrap(),
            [
                Seen::File(
                    "res/outer.arc/inner.arc/deep.bin".into(),
                    b"deep".into(),
                    None
                ),
                Seen::Archive(
                    "res/outer.arc/inner.arc".into(),
                    "inner".into(),
                    yaz0,
                    vec![None]
                ),
                Seen::File(
                    "res/outer.arc/dir/wrapped.bin".into(),
                    b"member".into(),
                    None
                ),
                Seen::File("res/outer.arc/plain.bin".into(), b"plain".into(), None),
                Seen::Archive(
                    "res/outer.arc".into(),
                    "outer".into(),
                    yaz0,
                    vec![None, Some(Compression::Yay0), None]
                ),
            ]
        );
    }

    #[test]
    fn a_disc_file_names_its_own_compression() {
        assert_eq!(
            unpack_all("a.bin", &wrap(Compression::Yaz0, b"plain")).unwrap(),
            [Seen::File(
                "a.bin".into(),
                b"plain".into(),
                Some(Compression::Yaz0)
            )]
        );
    }

    #[test]
    fn errors_name_the_innermost_file() {
        let mut broken = wrap(Compression::Yaz0, b"member bytes cut short");
        broken.truncate(12);
        let outer = archive("outer", &[("bad.bin", &broken)]);
        let err = unpack_all("res/outer.arc", &outer).unwrap_err();
        assert_eq!(err.path, "res/outer.arc/bad.bin");
    }

    /// An archive holding itself compressed would open forever.
    #[test]
    fn nesting_stops_at_the_limit() {
        let mut nested = archive("leaf", &[]);
        for _ in 0..=MAX_DEPTH {
            nested = archive("nest", &[("n.arc", &nested)]);
        }
        let err = unpack_all("n.arc", &nested).unwrap_err();
        assert!(matches!(err.source, formats::Error::Malformed { .. }));
    }
}
