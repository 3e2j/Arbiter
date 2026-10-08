//! GameCube and Wii discs, in any container nod reads (ISO, CISO, RVZ, WIA,
//! WBFS, GCZ, TGC). Files stream one at a time, so a disc never loads whole.
//!
//! Files outside the file system, such as `main.dol`, go under `sys/` as in
//! Dolphin and nodtool.

use std::{
    io::{self, BufRead, Read},
    path::{Component, Path},
    sync::Arc,
};

use nod::{
    common::PartitionKind,
    disc::fst::Node,
    read::{DiscOptions, DiscReader, PartitionMeta, PartitionOptions, PartitionReader},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Nod(#[from] nod::Error),
    #[error("the disc's file system table is invalid: {0}")]
    Fst(&'static str),
    #[error("game id {0:?} isn't six ASCII letters and digits")]
    Id([u8; 6]),
    #[error("the disc names a file {0:?}, which isn't a plain relative path")]
    Path(String),
    #[error("the disc has its own file {0:?}, where its system files would go")]
    Sys(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    GameCube,
    Wii,
}

/// The data partition of an opened disc. A Wii disc's other partitions
/// (update, channel) hold nothing of the game's.
pub struct Disc {
    /// The edition id, such as `GZ2E01`. Checked to be ASCII alphanumeric, so
    /// it's safe in a file name.
    pub id: String,
    pub revision: u8,
    pub platform: Platform,
    /// Files only, in file system order. Every path is plain and relative, so
    /// joining one to a directory can't escape it.
    pub files: Vec<File>,
    /// Read whole on open, they're small besides `main.dol`.
    pub sys: Vec<SysFile>,
    pub reader: Reader,
}

pub struct SysFile {
    /// Such as `sys/main.dol`. Never the path of a file in `Disc::files`.
    pub path: &'static str,
    pub bytes: Arc<[u8]>,
}

pub struct File {
    /// Relative and `/`-separated, such as `res/Msg/bmgres.arc`.
    pub path: String,
    node: Node,
}

/// Separate from `Disc::files` so a caller can read while iterating them.
pub struct Reader(Box<dyn PartitionReader>);

impl Disc {
    /// # Errors
    ///
    /// If nod can't read the image or its data partition, the game id or file
    /// system table is malformed, or a file's path isn't plain and relative.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let disc = DiscReader::new(path, &DiscOptions::default())?;
        let header = disc.header();
        let raw_id = header.game_id;
        if !raw_id.iter().all(u8::is_ascii_alphanumeric) {
            return Err(Error::Id(raw_id));
        }
        let id = raw_id.iter().copied().map(char::from).collect();
        let revision = header.disc_version;

        let mut partition =
            disc.open_partition_kind(PartitionKind::Data, &PartitionOptions::default())?;
        let platform = if partition.is_wii() {
            Platform::Wii
        } else {
            Platform::GameCube
        };
        let meta = partition.meta()?;
        let fst = meta.fst().map_err(Error::Fst)?;
        let files: Vec<File> = fst
            .iter()
            .filter(|(_, node, _)| node.is_file())
            .map(|(_, node, path)| File { path, node })
            .collect();
        if let Some(file) = files.iter().find(|f| !is_plain(&f.path)) {
            return Err(Error::Path(file.path.clone()));
        }
        if let Some(file) = files.iter().find(|f| is_sys(&f.path)) {
            return Err(Error::Sys(file.path.clone()));
        }

        // Exhaustive, so a field added in a nod update doesn't go unnoticed.
        let PartitionMeta {
            raw_boot,
            raw_bi2,
            raw_apploader,
            raw_dol,
            raw_fst,
            raw_ticket,
            raw_tmd,
            raw_cert_chain,
            raw_h3_table,
        } = meta;
        let sys = [
            ("sys/boot.bin", Some(raw_boot as Arc<[u8]>)),
            ("sys/bi2.bin", Some(raw_bi2)),
            ("sys/apploader.img", Some(raw_apploader)),
            ("sys/fst.bin", Some(raw_fst)),
            ("sys/main.dol", Some(raw_dol)),
            ("sys/ticket.bin", raw_ticket),
            ("sys/tmd.bin", raw_tmd),
            ("sys/cert.bin", raw_cert_chain),
            ("sys/h3.bin", raw_h3_table.map(|h3| h3 as Arc<[u8]>)),
        ]
        .into_iter()
        .filter_map(|(path, bytes)| {
            Some(SysFile {
                path,
                bytes: bytes?,
            })
        })
        .collect();

        Ok(Self {
            id,
            revision,
            platform,
            files,
            sys,
            reader: Reader(partition),
        })
    }
}

impl File {
    #[must_use]
    pub fn size(&self) -> u32 {
        self.node.length()
    }
}

impl Reader {
    /// # Errors
    ///
    /// If seeking to the file fails.
    pub fn open(&mut self, file: &File) -> io::Result<impl BufRead + '_> {
        self.0.open_file(file.node)
    }

    /// Replaces `buf` with the file's bytes, reusing its allocation.
    ///
    /// # Errors
    ///
    /// If seeking to or reading the file fails.
    pub fn read(&mut self, file: &File, buf: &mut Vec<u8>) -> io::Result<()> {
        buf.clear();
        self.open(file)?.read_to_end(buf)?;
        Ok(())
    }
}

// Here to avoid any discs from writing where they shouldn't be.
fn is_plain(path: &str) -> bool {
    let path = Path::new(path);
    path.components().next().is_some()
        && path.components().all(|c| matches!(c, Component::Normal(_)))
}

fn is_sys(path: &str) -> bool {
    path == "sys" || path.starts_with("sys/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_relative_paths() {
        assert!(is_plain("res/Msgus/bmgres.arc"));
        assert!(!is_plain(""));
        assert!(!is_plain("/etc/passwd"));
        assert!(!is_plain("res/../../x"));
        assert!(!is_plain("./res"));
    }

    #[test]
    fn sys_is_reserved() {
        assert!(is_sys("sys"));
        assert!(is_sys("sys/main.dol"));
        assert!(!is_sys("system/a.arc"));
        assert!(!is_sys("res/sys/a.arc"));
    }

    #[test]
    #[ignore = "needs a retail disc at dev/fixtures/NA.ciso"]
    fn reads_the_retail_disc() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/fixtures/NA.ciso");
        let mut disc = Disc::open(&path).unwrap();
        assert_eq!(disc.id, "GZ2E01");
        assert_eq!(disc.revision, 0);
        assert_eq!(disc.platform, Platform::GameCube);

        let file = disc
            .files
            .iter()
            .find(|f| f.path == "res/Msgus/bmgres.arc")
            .unwrap();
        let mut bytes = Vec::new();
        disc.reader.read(file, &mut bytes).unwrap();
        assert_eq!(bytes.len(), usize::try_from(file.size()).unwrap());
        assert_eq!(&bytes[..4], b"Yaz0");
    }
}
