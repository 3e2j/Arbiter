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
use xxhash_rust::xxh3::Xxh3;

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
    #[error("reading the disc: {0}")]
    Read(#[from] io::Error),
    #[error("the disc ended at {read} of {size} bytes")]
    Truncated { read: u64, size: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    GameCube,
    Wii,
}

/// The console region a disc boots on. Several countries share one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    NtscJ,
    NtscU,
    Pal,
    /// Wii only, Korean GameCube discs are `NtscJ`.
    NtscK,
}

/// The market a disc was released in, from the id's fourth character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Country {
    World,
    Usa,
    Japan,
    Korea,
    Taiwan,
    Europe,
    Germany,
    France,
    Italy,
    Netherlands,
    Russia,
    Spain,
    Australia,
}

/// The data partition of an opened disc.
// A Wii disc's other partitions (update, channel) hold nothing of the game's.
pub struct Disc {
    /// The edition id, such as `GZ2E01`.
    /// Checked to be ASCII alphanumeric, so it's safe in a file name.
    pub id: String,
    pub revision: u8,
    pub platform: Platform,
    /// `None` if the disc's region word isn't one of the four.
    pub region: Option<Region>,
    /// `None` for a country code nobody has catalogued.
    pub country: Option<Country>,
    /// Files only, in file system order.
    /// Every path is plain and relative, so joining one to a directory can't escape it.
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
        let wii_region = disc.region().and_then(|r| region(*r.first_chunk()?));

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
        let region = match platform {
            Platform::Wii => wii_region,
            Platform::GameCube => raw_bi2
                .get(BI2_REGION..)
                .and_then(|r| region(*r.first_chunk()?)),
        };
        let country = country(raw_id[3], platform, region);

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
            region,
            country,
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

/// XXH3-128 over every byte of the disc.
/// The same disc gives the same hash in any container.
///
/// # Errors
///
/// If nod can't open the image, reading fails, or the disc ends before its size.
// Read the way Dusklight's `borealis::disc::verify` does, so it matches the hashes in its catalogue.
pub fn hash(path: &Path) -> Result<u128, Error> {
    let mut disc = DiscReader::new(path, &DiscOptions::default())?;
    let size = disc.disc_size();
    let mut hasher = Xxh3::new();
    let mut read = 0;
    loop {
        let buf = disc.fill_buf()?;
        let len = buf.len();
        if len == 0 {
            break;
        }
        hasher.update(buf);
        disc.consume(len);
        read += len as u64;
    }
    if read != size {
        return Err(Error::Truncated { read, size });
    }
    Ok(hasher.digest128())
}

/// The region word's offset in `bi2.bin`. A Wii disc has its own region data
/// outside the partition, which nod reads.
const BI2_REGION: usize = 0x18;

/// The region word, numbered as in Dolphin's `DiscIO::Region`.
fn region(word: [u8; 4]) -> Option<Region> {
    match u32::from_be_bytes(word) {
        0 => Some(Region::NtscJ),
        1 => Some(Region::NtscU),
        2 => Some(Region::Pal),
        4 => Some(Region::NtscK),
        _ => None,
    }
}

/// After Dolphin's `CountryCodeToCountry`. Some codes only resolve with the region.
fn country(code: u8, platform: Platform, region: Option<Region>) -> Option<Country> {
    let gc = platform == Platform::GameCube;
    Some(match code {
        // Codes shared across markets, told apart by the region. English
        // GameCube discs sold in Korea use `E` or `W` with an NTSC-J region.
        b'E' if gc && region == Some(Region::NtscJ) => Country::Korea,
        b'W' if gc => Country::Korea,
        b'W' if region != Some(Region::Pal) => Country::Taiwan,
        b'X' | b'Y' | b'Z' if region == Some(Region::NtscU) => Country::Usa,

        b'A' => Country::World,
        b'E' | b'B' | b'N' => Country::Usa,
        b'J' => Country::Japan,
        b'K' | b'Q' | b'T' => Country::Korea,
        b'P' | b'L' | b'M' | b'V' | b'W' | b'X' | b'Y' | b'Z' => Country::Europe,
        b'D' => Country::Germany,
        b'F' => Country::France,
        b'I' => Country::Italy,
        b'H' => Country::Netherlands,
        b'R' => Country::Russia,
        b'S' => Country::Spain,
        b'U' => Country::Australia,
        _ => return None,
    })
}

// Here to avoid any discs from writing where they shouldn't be.
pub(crate) fn is_plain(path: &str) -> bool {
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
    fn country_needs_the_region_for_some_codes() {
        let gc = Platform::GameCube;
        assert_eq!(country(b'E', gc, Some(Region::NtscU)), Some(Country::Usa));
        assert_eq!(country(b'E', gc, Some(Region::NtscJ)), Some(Country::Korea));
        assert_eq!(
            country(b'E', Platform::Wii, Some(Region::NtscJ)),
            Some(Country::Usa)
        );
        assert_eq!(country(b'X', gc, Some(Region::NtscU)), Some(Country::Usa));
        assert_eq!(country(b'X', gc, Some(Region::Pal)), Some(Country::Europe));
        assert_eq!(country(b'W', gc, Some(Region::NtscJ)), Some(Country::Korea));
        assert_eq!(
            country(b'W', Platform::Wii, Some(Region::NtscJ)),
            Some(Country::Taiwan)
        );
        assert_eq!(country(b'G', gc, None), None);
    }

    #[test]
    fn region_words() {
        assert_eq!(region([0, 0, 0, 2]), Some(Region::Pal));
        assert_eq!(region([0, 0, 0, 4]), Some(Region::NtscK));
        assert_eq!(region([0, 0, 0, 3]), None);
    }
}
