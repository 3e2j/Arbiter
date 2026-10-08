//! `base/`, the unpacked read-only editions a mod is made against. Never committed.
//!
//! ```text
//! base/
//! ├─ GZ2E01/          the disc's own tree, every file read-only
//! │  ├─ res/Msgus/bmgres.arc/
//! │  │  ├─ zel_00.bmg
//! │  │  └─ .rarc.toml  how the archive packs back, see `sidecar`
//! │  └─ sys/          what the disc keeps outside its file system, see `pack::disc`
//! ├─ GZ2E01.toml      each path, its hash, and what came off a disc file
//! ├─ GZ2P01/
//! └─ GZ2P01.toml
//! ```
//!
//! Files sit at their disc paths so any program can open them,
//! with their compression taken off. An archive is a directory of its
//! members, nested any depth.
//! Read-only keeps them from being edited by accident: changes go in `changes/`.

pub mod packing;
pub mod sidecar;

use std::{
    collections::{BTreeMap, btree_map},
    fmt::{self, Display},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    thread,
};

use game::edition::{self, Edition, Game, Verdict};
use pack::{
    disc::{self, Disc},
    unpack::{self, Piece},
};
use serde::{Deserialize, Serialize};
use toml_writer::{TomlWrite, WriteTomlKey};

use crate::{
    config::{Config, Country, Platform, Recorded, Region},
    hash::Hash,
};
use sidecar::Sidecar;

pub const DIR: &str = "base";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Disc(#[from] disc::Error),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error(transparent)]
    Unpack(#[from] unpack::Error),
    #[error("two files unpack to {0}")]
    Duplicate(String),
    #[error("hashing the disc panicked")]
    HashPanicked,
    #[error("{path}: {source}")]
    Toml {
        path: PathBuf,
        #[source]
        source: toml::ser::Error,
    },
}

/// One edition's files. Sorted by path, so the files digest doesn't depend on
/// disc order. Disc metadata is in `arbiter.toml`, not here.
///
/// The hashes are kept to catch a modified base. Read-only can be undone, so a
/// base file can still be edited, deleted or added to. Any file that no longer
/// matches its hash is a hard error until it's unpacked again from the disc.
///
/// TODO: verify and repair.
/// - Record the disc's last known location (absolute, so local only) and the
///   unpack time here.
/// - Verify on open by rehashing only files whose mtime is after the unpack,
///   and everything on `arbiter check`. A changed, missing or extra file is a
///   `base/modified` error that blocks builds.
/// - Its fix re-reads just those files from the disc's last known location. If
///   the disc moved, ask for `arbiter unpack <disc>`, which updates it.
///
/// Written by its `Display`, one line per file.
#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub files: BTreeMap<String, Entry>,
}

/// What an unpack made of one file, a disc file or an archive member.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Of the stored bytes, with everything taken off.
    pub hash: Hash,
    /// What came off a disc file. A member's is in its archive's sidecar.
    pub compression: Option<Compression>,
}

/// Mirrors `formats::compression::Compression`. The discriminants feed the
/// files digest, so they're fixed apart from the variant order. 0 is none.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Compression {
    Yaz0 = 1,
    Yay0 = 2,
}

impl Compression {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Yaz0 => "yaz0",
            Self::Yay0 => "yay0",
        }
    }
}

impl From<formats::compression::Compression> for Compression {
    fn from(compression: formats::compression::Compression) -> Self {
        match compression {
            formats::compression::Compression::Yaz0 => Self::Yaz0,
            formats::compression::Compression::Yay0 => Self::Yay0,
        }
    }
}

impl Manifest {
    /// Covers every field of every entry, so two bases that would build
    /// differently never share a digest.
    #[must_use]
    pub fn files_digest(&self) -> Hash {
        let mut bytes = Vec::new();
        for (path, entry) in &self.files {
            bytes.extend_from_slice(path.as_bytes());
            bytes.push(0);
            bytes.extend_from_slice(&entry.hash.0.to_be_bytes());
            bytes.push(entry.compression.map_or(0, |c| c as u8));
        }
        Hash::of(&bytes)
    }
}

impl Display for Manifest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.open_table_header()?;
        f.key("files")?;
        f.close_table_header()?;
        f.newline()?;
        for (path, entry) in &self.files {
            assign(f, path.as_str())?;
            f.open_inline_table()?;
            f.space()?;
            assign(f, "hash")?;
            f.value(entry.hash.to_string())?;
            if let Some(compression) = entry.compression {
                f.val_sep()?;
                f.space()?;
                assign(f, "compression")?;
                f.value(compression.name())?;
            }
            f.space()?;
            f.close_inline_table()?;
            f.newline()?;
        }
        Ok(())
    }
}

/// Writes `key = `.
pub(crate) fn assign(f: &mut fmt::Formatter<'_>, key: impl WriteTomlKey) -> fmt::Result {
    f.key(key)?;
    f.space()?;
    f.keyval_sep()?;
    f.space()
}

/// A disc unpacked to `base/.<id>.partial/`, not yet in place. Either
/// `commit` it or `discard` it.
#[derive(Debug)]
pub struct Staged {
    pub id: String,
    pub revision: u8,
    pub platform: Platform,
    pub region: Option<Region>,
    pub country: Option<Country>,
    pub manifest: Manifest,
    /// See `config::Edition::disc_hash`.
    pub disc_hash: Hash,
    /// `None` for an id with no tables, which shows raw values.
    pub edition: Option<&'static Edition>,
    /// For an unknown id, the game its game code suggests.
    pub guess: Option<Game>,
    pub bytes: u64,
    /// Decided against `arbiter.toml` as it was when staged.
    pub outcome: Recorded,
    pub(crate) tree: PathBuf,
}

/// A disc's header, read without unpacking it.
#[derive(Debug)]
pub struct Peeked {
    pub id: String,
    pub revision: u8,
    /// `None` for an unknown id.
    pub game: Option<Game>,
}

/// Reads a disc's header and file system table alone.
///
/// # Errors
///
/// If the disc can't be read.
pub(crate) fn peek(path: &Path) -> Result<Peeked, Error> {
    let disc = Disc::open(path)?;
    let game = edition::edition(&disc.id).map(|e| e.game);
    Ok(Peeked {
        id: disc.id,
        revision: disc.revision,
        game,
    })
}

/// Hashes whole discs, one thread each, and checks each against its edition's
/// clean dump.
///
/// # Errors
///
/// If a disc can't be read.
pub(crate) fn verify(discs: &[(&Path, &Peeked)]) -> Result<Vec<(Hash, Verdict)>, Error> {
    thread::scope(|s| {
        let hashing: Vec<_> = discs
            .iter()
            .map(|&(path, _)| s.spawn(|| disc::hash(path)))
            .collect();
        hashing
            .into_iter()
            .zip(discs)
            .map(|(hashing, (_, peeked))| {
                let hash = Hash(hashing.join().map_err(|_| Error::HashPanicked)??);
                Ok((hash, check(&peeked.id, peeked.revision, hash)))
            })
            .collect()
    })
}

fn check(id: &str, revision: u8, hash: Hash) -> Verdict {
    edition::edition(id).map_or(Verdict::Uncatalogued, |e| e.check(revision, hash.0))
}

/// Unpacks a disc beside the edition's current tree, so the current one stays
/// whole until `Staged::commit` swaps them. `disc_hash` is from `verify`.
///
/// # Errors
///
/// If the disc can't be read or a file can't be written.
pub(crate) fn stage(
    base: &Path,
    path: &Path,
    disc_hash: Hash,
    config: &Config,
) -> Result<Staged, Error> {
    let mut disc = Disc::open(path)?;
    let tree = base.join(format!(".{}.partial", disc.id));
    remove_tree(&tree)?;

    let (manifest, bytes) = unpack(&tree, &mut disc)?;

    let edition = edition::edition(&disc.id);
    let guess = edition
        .is_none()
        .then(|| edition::guess(&disc.id))
        .flatten();
    let outcome = config.record(&disc.id, disc.revision, manifest.files_digest());
    Ok(Staged {
        id: disc.id,
        revision: disc.revision,
        platform: disc.platform.into(),
        region: disc.region.map(Into::into),
        country: disc.country.map(Into::into),
        manifest,
        disc_hash,
        edition,
        guess,
        bytes,
        outcome,
        tree,
    })
}

/// Writes every file to `tree` with its packaging unpacked (compression, archives).
/// Returns their manifest and total size.
fn unpack(tree: &Path, disc: &mut Disc) -> Result<(Manifest, u64), Error> {
    let mut out = Out {
        tree,
        files: BTreeMap::new(),
        bytes: 0,
        made_dir: None,
    };
    let mut buf = Vec::new();
    for file in &disc.files {
        disc.reader
            .read(file, &mut buf)
            .map_err(io_err(Path::new(&file.path)))?;
        unpack::unpack(&file.path, &buf, &mut |piece| out.store(piece))?;
    }

    for file in &disc.sys {
        out.write(file.path, &file.bytes, None)?;
    }

    Ok((Manifest { files: out.files }, out.bytes))
}

/// An unpack in progress.
struct Out<'a> {
    tree: &'a Path,
    files: BTreeMap<String, Entry>,
    bytes: u64,
    /// The last directory made. Files come in tree order, so siblings share it.
    made_dir: Option<PathBuf>,
}

impl Out<'_> {
    fn store(&mut self, piece: Piece<'_>) -> Result<(), Error> {
        match piece {
            Piece::File {
                path,
                bytes,
                compression,
            } => self.write(path, bytes, compression.map(Into::into)),
            Piece::Archive {
                path,
                archive,
                compression,
                members,
            } => {
                let sidecar = Sidecar::new(
                    archive,
                    compression.map(Into::into),
                    members.iter().map(|c| c.map(Into::into)),
                )
                .to_string();
                self.write(
                    &format!("{path}/{}", sidecar::NAME),
                    sidecar.as_bytes(),
                    None,
                )
            }
        }
    }

    fn write(
        &mut self,
        path: &str,
        bytes: &[u8],
        compression: Option<Compression>,
    ) -> Result<(), Error> {
        let btree_map::Entry::Vacant(slot) = self.files.entry(path.to_owned()) else {
            return Err(Error::Duplicate(path.to_owned()));
        };
        let dest = self.tree.join(path);
        let parent = dest.parent().unwrap_or(self.tree);
        if self.made_dir.as_deref() != Some(parent) {
            fs::create_dir_all(parent).map_err(io_err(parent))?;
            self.made_dir = Some(parent.to_path_buf());
        }
        write_readonly(&dest, bytes)?;
        self.bytes += bytes.len() as u64;
        slot.insert(Entry {
            hash: Hash::of(bytes),
            compression,
        });
        Ok(())
    }
}

impl Staged {
    /// The unpacked files, for checks that need the new base before it's in place.
    #[must_use]
    pub fn tree(&self) -> &Path {
        &self.tree
    }

    /// Replaces `base/<id>/` with the staged tree, then writes the manifest.
    pub(crate) fn commit(&self, base: &Path) -> Result<(), Error> {
        let dest = base.join(&self.id);
        remove_tree(&dest)?;
        fs::rename(&self.tree, &dest).map_err(io_err(&dest))?;

        let path = base.join(format!("{}.toml", self.id));
        write_atomic(&path, self.manifest.to_string().as_bytes())
    }

    /// # Errors
    ///
    /// If the staged tree can't be removed.
    pub fn discard(self) -> Result<(), Error> {
        remove_tree(&self.tree)
    }
}

fn write_readonly(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let err = io_err(path);
    let mut file = File::create(path).map_err(err)?;
    file.write_all(bytes).map_err(io_err(path))?;
    let mut perms = file.metadata().map_err(io_err(path))?.permissions();
    perms.set_readonly(true);
    file.set_permissions(perms).map_err(io_err(path))
}

/// Removes a tree of read-only files. A missing tree is fine.
pub(crate) fn remove_tree(path: &Path) -> Result<(), Error> {
    // Unix only needs the directories writable. Windows refuses to delete a
    // read-only file, so the flag comes off first.
    #[cfg(windows)]
    clear_readonly(path)?;
    match fs::remove_dir_all(path) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(io_err(path)(err)),
        _ => Ok(()),
    }
}

#[cfg(windows)]
fn clear_readonly(path: &Path) -> Result<(), Error> {
    let entries = match fs::read_dir(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        entries => entries.map_err(io_err(path))?,
    };
    for entry in entries {
        let path = entry.map_err(io_err(path))?.path();
        let meta = fs::symlink_metadata(&path).map_err(io_err(&path))?;
        if meta.is_dir() {
            clear_readonly(&path)?;
        } else {
            let mut perms = meta.permissions();
            perms.set_readonly(false);
            fs::set_permissions(&path, perms).map_err(io_err(&path))?;
        }
    }
    Ok(())
}

/// Written beside its destination and renamed over it, so the path never
/// holds half a file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    fs::rename(&tmp, path).map_err(io_err(path))
}

pub(crate) fn io_err(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_files_are_readonly_and_removable() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("GZ2E01");
        fs::create_dir_all(tree.join("res")).unwrap();
        let file = tree.join("res/al.bmd");
        write_readonly(&file, b"al.bmd").unwrap();

        assert!(fs::metadata(&file).unwrap().permissions().readonly());
        assert!(fs::write(&file, b"edited").is_err());

        remove_tree(&tree).unwrap();
        assert!(!tree.exists());
        remove_tree(&tree).unwrap();
    }

    fn archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        use formats::{Encode, Writer, rarc};
        let rarc = rarc::Rarc {
            root: "archive".to_owned(),
            files: files
                .iter()
                .map(|&(path, data)| rarc::File {
                    path: path.to_owned(),
                    data: data.to_vec(),
                    ..rarc::File::default()
                })
                .collect(),
            next_id: None,
        };
        let mut out = Writer::new();
        rarc.encode(&mut out).unwrap();
        out.finish()
    }

    fn yaz0(data: &[u8]) -> Vec<u8> {
        use formats::{Writer, compression::Strategy};
        let mut out = Writer::new();
        formats::compression::Compression::Yaz0
            .compress(data, Strategy::Parity, &mut out)
            .unwrap();
        out.finish()
    }

    fn store<'a>(tree: &'a Path, path: &str, bytes: &[u8]) -> Result<Out<'a>, Error> {
        let mut out = Out {
            tree,
            files: BTreeMap::new(),
            bytes: 0,
            made_dir: None,
        };
        unpack::unpack(path, bytes, &mut |piece| out.store(piece))?;
        Ok(out)
    }

    #[test]
    fn archives_unpack_into_directories_with_a_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let inner = archive(&[("deep.bin", b"deep")]);
        let outer = yaz0(&archive(&[
            ("inner.arc", &inner),
            ("sub/a.bmg", &yaz0(b"message")),
        ]));
        let out = store(dir.path(), "res/a.arc", &outer).unwrap();

        let stored: Vec<_> = out
            .files
            .iter()
            .map(|(path, e)| (path.as_str(), e.compression))
            .collect();
        assert_eq!(
            stored,
            [
                ("res/a.arc/.rarc.toml", None),
                ("res/a.arc/inner.arc/.rarc.toml", None),
                ("res/a.arc/inner.arc/deep.bin", None),
                ("res/a.arc/sub/a.bmg", None),
            ]
        );
        let read = |path: &str| fs::read_to_string(dir.path().join(path)).unwrap();
        assert_eq!(read("res/a.arc/sub/a.bmg"), "message");
        assert_eq!(read("res/a.arc/inner.arc/deep.bin"), "deep");

        let sidecar: Sidecar = toml::from_str(&read("res/a.arc/.rarc.toml")).unwrap();
        assert_eq!(sidecar.root, "archive");
        assert_eq!(sidecar.compression, Some(Compression::Yaz0));
        let members: Vec<_> = sidecar
            .members
            .iter()
            .map(|m| (m.path.as_str(), m.id, m.compression))
            .collect();
        assert_eq!(
            members,
            [
                ("inner.arc", Some(0), None),
                ("sub/a.bmg", Some(1), Some(Compression::Yaz0))
            ]
        );
        let inner: Sidecar = toml::from_str(&read("res/a.arc/inner.arc/.rarc.toml")).unwrap();
        assert_eq!(inner.compression, None);
    }

    #[test]
    fn a_member_over_the_sidecar_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let outer = archive(&[(sidecar::NAME, b"not a sidecar")]);
        assert!(matches!(
            store(dir.path(), "a.arc", &outer),
            Err(Error::Duplicate(path)) if path == "a.arc/.rarc.toml"
        ));
    }

    fn entry(hash: u128, compression: Option<Compression>) -> Entry {
        Entry {
            hash: Hash(hash),
            compression,
        }
    }

    #[test]
    fn manifest_writes_a_line_per_file_and_reads_back() {
        let manifest = Manifest {
            files: [
                ("res/a.arc".to_owned(), entry(1, Some(Compression::Yaz0))),
                ("sys/main.dol".to_owned(), entry(2, None)),
            ]
            .into(),
        };
        let text = manifest.to_string();
        assert_eq!(
            text,
            "[files]\n\
             \"res/a.arc\" = { hash = \"xxh3:00000000000000000000000000000001\", compression = \"yaz0\" }\n\
             \"sys/main.dol\" = { hash = \"xxh3:00000000000000000000000000000002\" }\n"
        );
        assert_eq!(toml::from_str::<Manifest>(&text).unwrap(), manifest);
    }

    #[test]
    fn files_digest_follows_compression() {
        let manifest = |compression| Manifest {
            files: [("a.arc".to_owned(), entry(1, compression))].into(),
        };
        assert_ne!(
            manifest(None).files_digest(),
            manifest(Some(Compression::Yaz0)).files_digest()
        );
        assert_ne!(
            manifest(Some(Compression::Yaz0)).files_digest(),
            manifest(Some(Compression::Yay0)).files_digest()
        );
    }

    #[test]
    fn files_digest_follows_paths_and_contents() {
        let manifest = |files: &[(&str, u128)]| Manifest {
            files: files
                .iter()
                .map(|&(p, h)| (p.to_owned(), entry(h, None)))
                .collect(),
        };
        let a = manifest(&[("a", 1), ("b", 2)]);
        assert_eq!(
            a.files_digest(),
            manifest(&[("b", 2), ("a", 1)]).files_digest()
        );
        assert_ne!(
            a.files_digest(),
            manifest(&[("a", 2), ("b", 1)]).files_digest()
        );
        assert_ne!(
            a.files_digest(),
            manifest(&[("ab", 1), ("", 2)]).files_digest()
        );
    }
}
