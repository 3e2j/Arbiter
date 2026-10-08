use encoding_rs::SHIFT_JIS;

use super::{
    ALIGN, DIR_SIZE, DOT_AT, DOT_DOT_AT, Entry, Header, INFO_AT, Info, NO_ID, NO_NODE, Node,
    Preload, Rarc, flag, is_plain, name_hash, next_id,
};
use crate::compression::Compression;
use crate::{Be16, Be32, Error, Flag, Record, Result, Writer};

struct Dir {
    name: Vec<u8>,
    parent: usize,
    children: Vec<Child>,
}

struct Child {
    name: Vec<u8>,
    kind: Kind,
}

#[derive(Clone, Copy)]
enum Kind {
    /// Into the directory list.
    Dir(usize),
    /// Into the archive's files.
    File(usize),
}

/// The directories, and the order the archive stores them in.
struct Tree {
    dirs: Vec<Dir>,
    /// Directory indices in node order.
    order: Vec<usize>,
    /// Each directory's node.
    node_of: Vec<u32>,
    /// Each node's first entry.
    first: Vec<u32>,
    entry_count: usize,
}

/// Where each file goes, by its index in the archive's files.
struct Placed {
    ids: Vec<u16>,
    offsets: Vec<u32>,
    synced: bool,
    data_len: usize,
    mram: usize,
    aram: usize,
}

struct Names {
    pool: Vec<u8>,
    /// By directory index.
    dirs: Vec<u32>,
    /// By file index.
    files: Vec<u32>,
}

pub(super) fn encode(rarc: &Rarc, out: &mut Writer) -> Result<()> {
    let tree = Tree::build(rarc)?;
    let placed = place(rarc, &tree)?;
    let next = match rarc.next_id {
        Some(id) => id,
        None => next_id(
            tree.entry_count,
            placed.ids.iter().copied().max(),
            placed.synced,
        )?,
    };
    let names = names(rarc, &tree)?;

    let nodes_at = INFO_AT + Info::LEN;
    let entries_at = (nodes_at + tree.order.len() * Node::LEN).next_multiple_of(ALIGN);
    let names_at = (entries_at + tree.entry_count * Entry::LEN).next_multiple_of(ALIGN);
    let names_len = names.pool.len().next_multiple_of(ALIGN);
    let data_at = names_at + names_len;
    let size = data_at + placed.data_len;

    let from_info = |at: usize| be32(at - INFO_AT);
    out.bytes(&Rarc::MAGIC);
    out.record(&Header {
        size: be32(size)?,
        info_at: be32(INFO_AT)?,
        data_at: from_info(data_at)?,
        data_len: be32(placed.data_len)?,
        mram: be32(placed.mram)?,
        aram: be32(placed.aram)?,
        pad: [0; 4],
    });
    out.record(&Info {
        node_count: be32(tree.order.len())?,
        nodes_at: from_info(nodes_at)?,
        entry_count: be32(tree.entry_count)?,
        entries_at: from_info(entries_at)?,
        names_len: be32(names_len)?,
        names_at: from_info(names_at)?,
        next_id: Be16::new(next),
        synced: Flag::new(placed.synced),
        pad: [0; 5],
    });

    for (node, &dir) in (0u32..).zip(&tree.order) {
        out.record(&tree.node(node, dir, &names)?);
    }
    out.align(ALIGN);

    for (node, &dir) in (0u32..).zip(&tree.order) {
        for child in &tree.dirs[dir].children {
            match child.kind {
                Kind::Dir(sub) => {
                    dir_entry(out, &child.name, names.dirs[sub], tree.node_of[sub])?;
                }
                Kind::File(file) => {
                    let f = &rarc.files[file];
                    let flags = flag::FILE
                        | match f.preload {
                            Preload::Mram => flag::MRAM,
                            Preload::Aram => flag::ARAM,
                            Preload::Disc => flag::DISC,
                        }
                        | match Compression::detect(&f.data) {
                            Some(Compression::Yaz0) => flag::COMPRESSED | flag::YAZ0,
                            Some(Compression::Yay0) => flag::COMPRESSED,
                            None => 0,
                        };
                    out.record(&Entry {
                        id: Be16::new(placed.ids[file]),
                        hash: Be16::new(name_hash(&child.name)),
                        flags,
                        name: name_offset(names.files[file])?,
                        target: Be32::new(placed.offsets[file]),
                        size: be32(f.data.len())?,
                        pad: [0; 4],
                    });
                }
            }
        }
        dir_entry(out, b".", DOT_AT, node)?;
        let parent = if node == 0 {
            NO_NODE
        } else {
            tree.node_of[tree.dirs[dir].parent]
        };
        dir_entry(out, b"..", DOT_DOT_AT, parent)?;
    }
    out.align(ALIGN);

    out.bytes(&names.pool);
    out.zeros(names_len - names.pool.len());

    for file in tree.files() {
        out.bytes(&rarc.files[file].data);
        out.align(ALIGN);
    }
    Ok(())
}

impl Tree {
    fn build(rarc: &Rarc) -> Result<Self> {
        let dirs = grow(rarc)?;

        // Depth first, children in sibling order.
        let mut order = Vec::with_capacity(dirs.len());
        let mut node_of = vec![0; dirs.len()];
        let mut stack = vec![0];
        while let Some(dir) = stack.pop() {
            node_of[dir] = u32_of(order.len())?;
            order.push(dir);
            stack.extend(
                dirs[dir]
                    .children
                    .iter()
                    .rev()
                    .filter_map(|c| match c.kind {
                        Kind::Dir(sub) => Some(sub),
                        Kind::File(_) => None,
                    }),
            );
        }

        let mut first = Vec::with_capacity(order.len());
        let mut entry_count = 0;
        for &dir in &order {
            first.push(u32_of(entry_count)?);
            entry_count += dirs[dir].children.len() + 2;
        }
        Ok(Self {
            dirs,
            order,
            node_of,
            first,
            entry_count,
        })
    }

    fn node(&self, node: u32, dir: usize, names: &Names) -> Result<Node> {
        let at = &self.dirs[dir];
        let mut tag = *b"ROOT";
        if node != 0 {
            tag = *b"    ";
            for (t, b) in tag.iter_mut().zip(&at.name) {
                *t = b.to_ascii_uppercase();
            }
        }
        let count = u16::try_from(at.children.len() + 2).map_err(|_| too_large("a directory"))?;
        Ok(Node {
            tag,
            name: Be32::new(names.dirs[dir]),
            hash: Be16::new(name_hash(&at.name)),
            count: Be16::new(count),
            first: Be32::new(self.first[node as usize]),
        })
    }

    /// Every file as `(entry, file)`, in entry order, which is data order.
    fn entries(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.order
            .iter()
            .zip(&self.first)
            .flat_map(move |(&dir, &first)| {
                (first as usize..)
                    .zip(&self.dirs[dir].children)
                    .filter_map(|(entry, child)| match child.kind {
                        Kind::File(file) => Some((entry, file)),
                        Kind::Dir(_) => None,
                    })
            })
    }

    fn files(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries().map(|(_, file)| file)
    }
}

/// The directory tree from the paths. A directory's place among its siblings
/// is where its first file appears, which for a decoded archive is where its
/// entry was.
fn grow(rarc: &Rarc) -> Result<Vec<Dir>> {
    let mut dirs = vec![Dir {
        name: sjis(&rarc.root)?,
        parent: 0,
        children: Vec::new(),
    }];
    for (file, f) in rarc.files.iter().enumerate() {
        let bad = || Error::Name {
            name: f.path.clone(),
        };
        let mut at = 0;
        let mut parts = f.path.split('/').peekable();
        while let Some(part) = parts.next() {
            if !is_plain(part) {
                return Err(bad());
            }
            let name = sjis(part)?;
            let found = dirs[at].children.iter().find(|c| c.name == name);
            if parts.peek().is_none() {
                if found.is_some() {
                    return Err(bad());
                }
                dirs[at].children.push(Child {
                    name,
                    kind: Kind::File(file),
                });
                break;
            }
            at = match found.map(|c| c.kind) {
                Some(Kind::Dir(dir)) => dir,
                Some(Kind::File(_)) => return Err(bad()),
                None => {
                    let dir = dirs.len();
                    dirs.push(Dir {
                        name: name.clone(),
                        parent: at,
                        children: Vec::new(),
                    });
                    dirs[at].children.push(Child {
                        name,
                        kind: Kind::Dir(dir),
                    });
                    dir
                }
            };
        }
    }
    Ok(dirs)
}

/// Ids, data offsets and preload sizes, in entry order.
///
/// A file with no id takes the lowest one unclaimed. The format's own way is
/// the next free id, which never reuses one, but filling gaps keeps ids
/// bounded and drifts toward the synced fast path.
fn place(rarc: &Rarc, tree: &Tree) -> Result<Placed> {
    let mut claimed: Vec<u16> = rarc.files.iter().filter_map(|f| f.id).collect();
    claimed.sort_unstable();
    if claimed.windows(2).any(|w| w[0] == w[1]) {
        return Err(Error::Malformed {
            what: "two files share an id",
        });
    }
    let mut unclaimed = (0..NO_ID).filter(|id| claimed.binary_search(id).is_err());

    let mut placed = Placed {
        ids: vec![NO_ID; rarc.files.len()],
        offsets: vec![0; rarc.files.len()],
        synced: true,
        data_len: 0,
        mram: 0,
        aram: 0,
    };
    let mut memory = Preload::Mram;
    for (entry, file) in tree.entries() {
        let f = &rarc.files[file];
        // The header's sizes can only describe one run per memory.
        if f.preload < memory {
            return Err(Error::Malformed {
                what: "files aren't grouped by memory: main, then ARAM, then disc",
            });
        }
        memory = f.preload;

        let id = match f.id {
            Some(id) => id,
            None => unclaimed
                .next()
                .ok_or_else(|| too_large("the file count"))?,
        };
        placed.ids[file] = id;
        placed.synced &= usize::from(id) == entry;
        placed.offsets[file] = u32_of(placed.data_len)?;
        let padded = f.data.len().next_multiple_of(ALIGN);
        placed.data_len += padded;
        match f.preload {
            Preload::Mram => placed.mram += padded,
            Preload::Aram => placed.aram += padded,
            Preload::Disc => {}
        }
    }
    Ok(placed)
}

/// `.` and `..` once, then in node order each directory's name followed by
/// its files'. Repeats are stored again, as on retail.
fn names(rarc: &Rarc, tree: &Tree) -> Result<Names> {
    let mut names = Names {
        pool: b".\0..\0".to_vec(),
        dirs: vec![0; tree.dirs.len()],
        files: vec![0; rarc.files.len()],
    };
    let push = |pool: &mut Vec<u8>, name: &[u8]| {
        let at = u32_of(pool.len())?;
        pool.extend_from_slice(name);
        pool.push(0);
        Ok(at)
    };
    for &dir in &tree.order {
        names.dirs[dir] = push(&mut names.pool, &tree.dirs[dir].name)?;
        for child in &tree.dirs[dir].children {
            if let Kind::File(file) = child.kind {
                names.files[file] = push(&mut names.pool, &child.name)?;
            }
        }
    }
    Ok(names)
}

fn dir_entry(out: &mut Writer, name: &[u8], name_at: u32, node: u32) -> Result<()> {
    out.record(&Entry {
        id: Be16::new(NO_ID),
        hash: Be16::new(name_hash(name)),
        flags: flag::DIR,
        name: name_offset(name_at)?,
        target: Be32::new(node),
        size: Be32::new(DIR_SIZE),
        pad: [0; 4],
    });
    Ok(())
}

/// A name pool offset as an entry's 24 bits.
fn name_offset(name_at: u32) -> Result<[u8; 3]> {
    match name_at.to_be_bytes() {
        [0, a, b, c] => Ok([a, b, c]),
        _ => Err(too_large("the name pool")),
    }
}

/// A name as the pool stores it.
fn sjis(name: &str) -> Result<Vec<u8>> {
    let (bytes, _, unmappable) = SHIFT_JIS.encode(name);
    if unmappable || name.is_empty() || name.contains('\0') {
        return Err(Error::Name {
            name: name.to_owned(),
        });
    }
    Ok(bytes.into_owned())
}

fn u32_of(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| too_large("the archive"))
}

fn be32(value: usize) -> Result<Be32> {
    u32_of(value).map(Be32::new)
}

const fn too_large(what: &'static str) -> Error {
    Error::TooLarge { what }
}
