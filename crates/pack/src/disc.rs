//! GameCube and Wii discs, in any container nod reads (ISO, CISO, RVZ, WIA,
//! WBFS, GCZ, TGC). Files stream one at a time, so a disc never loads whole.

use std::{
    io::{self, BufRead},
    path::Path,
};

use nod::{
    common::PartitionKind,
    disc::fst::Node,
    read::{DiscOptions, DiscReader, PartitionOptions, PartitionReader},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Nod(#[from] nod::Error),
    #[error("the disc's file system table is invalid: {0}")]
    Fst(&'static str),
    #[error("game id {0:?} isn't six ASCII letters and digits")]
    Id([u8; 6]),
}

/// The data partition of an opened disc. A Wii disc's other partitions
/// (update, channel) hold nothing of the game's.
pub struct Disc {
    /// The edition id, such as `GZ2E01`. Checked to be ASCII alphanumeric, so
    /// it's safe in a file name.
    pub id: String,
    pub revision: u8,
    /// Files only, in file system order.
    pub files: Vec<File>,
    pub reader: Reader,
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
    /// If nod can't read the image or its data partition, or the game id or
    /// file system table is malformed.
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
        let meta = partition.meta()?;
        let fst = meta.fst().map_err(Error::Fst)?;
        let files = fst
            .iter()
            .filter(|(_, node, _)| node.is_file())
            .map(|(_, node, path)| File { path, node })
            .collect();

        Ok(Self {
            id,
            revision,
            files,
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
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    #[test]
    #[ignore = "needs a retail disc at dev/fixtures/NA.ciso"]
    fn reads_the_retail_disc() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dev/fixtures/NA.ciso");
        let mut disc = Disc::open(&path).unwrap();
        assert_eq!(disc.id, "GZ2E01");
        assert_eq!(disc.revision, 0);

        let file = disc
            .files
            .iter()
            .find(|f| f.path == "res/Msgus/bmgres.arc")
            .unwrap();
        let mut bytes = Vec::new();
        disc.reader
            .open(file)
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        assert_eq!(bytes.len(), usize::try_from(file.size()).unwrap());
        assert_eq!(&bytes[..4], b"Yaz0");
    }
}
