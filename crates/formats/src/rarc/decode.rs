use std::ops::Range;

use encoding_rs::SHIFT_JIS;

use super::{Entry, File, Header, Info, Node, Preload, Rarc, flag, is_plain, name_hash, next_id};
use crate::{Be32, Error, Reader, Result};

/// An archive's tables, borrowed in place, every offset made absolute.
struct Tables<'a> {
    reader: Reader<'a>,
    nodes: &'a [Node],
    entries: &'a [Entry],
    names_at: usize,
    data_at: usize,
}

pub(super) fn decode(bytes: &[u8]) -> Result<Rarc> {
    let mut r = Reader::new(bytes);
    r.magic(Rarc::MAGIC)?;
    let header: &Header = r.record()?;
    if header.size.get() as usize != bytes.len() {
        return Err(malformed("the stated size isn't the file's"));
    }
    let info_at = header.info_at.get() as usize;
    // Saturating keeps a nonsense offset out of bounds rather than wrapping
    // it somewhere real.
    let from_info = |offset: Be32| info_at.saturating_add(offset.get() as usize);
    let data_at = from_info(header.data_at);
    let data_len = bytes.len().saturating_sub(data_at);
    if header.data_len.get() as usize != data_len {
        return Err(malformed(
            "the stated data size isn't what follows the data offset",
        ));
    }
    let mram = header.mram.get() as usize;
    let aram = header.aram.get() as usize;
    if mram
        .checked_add(aram)
        .is_none_or(|preloaded| preloaded > data_len)
    {
        return Err(malformed("the preload sizes are larger than the data"));
    }

    r.seek(info_at);
    let info: &Info = r.record()?;
    let tables = Tables {
        nodes: r.records_at(from_info(info.nodes_at), info.node_count.get() as usize)?,
        entries: r.records_at(from_info(info.entries_at), info.entry_count.get() as usize)?,
        names_at: from_info(info.names_at),
        data_at,
        reader: r,
    };
    let Some(root) = tables.nodes.first() else {
        return Err(malformed("there is no root directory"));
    };
    let root = tables.name(root.name.get(), root.hash.get())?;
    let (files, derived) = tables.walk()?;
    let stored = info.next_id.get();
    Ok(Rarc {
        root,
        files,
        next_id: (stored != derived).then_some(stored),
    })
}

impl Tables<'_> {
    /// Flattens the tree depth first from the root, directories into their
    /// files' paths. Returns the files and the next free id they derive.
    fn walk(&self) -> Result<(Vec<File>, u16)> {
        let mut files = Vec::new();
        let mut visited = vec![false; self.nodes.len()];
        let mut highest = None;
        let mut synced = true;
        // Each frame is a directory mid-walk: the entries left, and its path.
        let mut stack = vec![(self.open(0, &mut visited)?, String::new())];

        while let Some((run, dir)) = stack.last_mut() {
            let Some(at) = run.next() else {
                stack.pop();
                continue;
            };
            let entry = &self.entries[at];
            let name = self.name(entry.name(), entry.hash.get())?;
            let is_dir = entry.flags & flag::DIR != 0;
            // The walk keeps its own stack, so it needs neither.
            if is_dir && (name == "." || name == "..") {
                continue;
            }
            if !is_plain(&name) {
                return Err(Error::Name { name });
            }
            let path = if dir.is_empty() {
                name
            } else {
                format!("{dir}/{name}")
            };

            if is_dir {
                let run = self.open(entry.target.get() as usize, &mut visited)?;
                stack.push((run, path));
                continue;
            }
            let preload = if entry.flags & flag::MRAM != 0 {
                Preload::Mram
            } else if entry.flags & flag::ARAM != 0 {
                Preload::Aram
            } else if entry.flags & flag::DISC != 0 {
                Preload::Disc
            } else {
                return Err(malformed("a file is marked for no memory"));
            };
            let id = entry.id.get();
            highest = highest.max(Some(id));
            synced &= usize::from(id) == at;
            let data = self.reader.bytes_at(
                self.data_at.saturating_add(entry.target.get() as usize),
                entry.size.get() as usize,
            )?;
            files.push(File {
                path,
                data: data.to_vec(),
                id: Some(id),
                preload,
            });
        }

        Ok((files, next_id(self.entries.len(), highest, synced)?))
    }

    /// Marks a node visited and returns its run of entries.
    fn open(&self, node: usize, visited: &mut [bool]) -> Result<Range<usize>> {
        match visited.get_mut(node) {
            None => return Err(malformed("a directory points at a node that doesn't exist")),
            Some(true) => return Err(malformed("the directory tree loops")),
            Some(seen) => *seen = true,
        }
        let node = &self.nodes[node];
        let first = node.first.get() as usize;
        first
            .checked_add(usize::from(node.count.get()))
            .filter(|&end| end <= self.entries.len())
            .map(|end| first..end)
            .ok_or_else(|| malformed("a directory claims entries that don't exist"))
    }

    /// A name from the pool, checked against the hash stored beside its
    /// offset so an offset landing on some other string is caught.
    fn name(&self, offset: u32, hash: u16) -> Result<String> {
        let raw = self
            .reader
            .cstr_at(self.names_at.saturating_add(offset as usize))?;
        if name_hash(raw) != hash {
            return Err(malformed("a name doesn't match its hash"));
        }
        SHIFT_JIS
            .decode_without_bom_handling_and_without_replacement(raw)
            .map(String::from)
            .ok_or_else(|| malformed("a name isn't Shift-JIS"))
    }
}

const fn malformed(what: &'static str) -> Error {
    Error::Malformed { what }
}
