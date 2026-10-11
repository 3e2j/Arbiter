//! The project's files as the file tree shows them:
//! - The Game tree, the target editions with `changes/` laid over them
//! - The Project tree, everything on disk.
//!
//! A tree is one flat list of nodes in preorder, folders before files, each
//! level sorted by name. Each node knows its parent, its depth, and where its
//! subtree ends, so a closed folder is skipped in one step and anything
//! folders take from their contents is one pass backwards.
//!
//! ```text
//! 0  (root)          end 6
//! 1  res/            end 5
//! 2    Msg/          end 4
//! 3      zel_00.bmg  end 4
//! 4    al.bmd        end 5
//! 5  sys/            end 6
//! ```

use std::{
    cmp::Ordering,
    fmt, fs, io, iter,
    ops::Index,
    path::{Path, PathBuf},
};

use serde::{
    Deserialize, Deserializer,
    de::{IgnoredAny, MapAccess, Visitor},
};

use crate::{
    Project, base, changes,
    config::{self, Config},
    git,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{0} target editions, but the file tree tells apart at most 32")]
    Editions(usize),
    #[error("more files than the file tree can hold")]
    Full,
}

/// A node's place in its tree.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct NodeId(u32);

impl NodeId {
    /// Every tree's first node, holding the rest.
    pub const ROOT: Self = Self(0);

    /// Its place in tree order, from 0 to the tree's node count, for keeping
    /// something per node beside the tree.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

/// One line of a tree as a reader sees it: a node, or a run of folders that
/// each hold only the next, down to the last, as in `Layout/title`. A run's
/// nodes are back to back, `first` to `last`, and `last` is the one that
/// opens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row {
    pub first: NodeId,
    pub last: NodeId,
    /// How many rows it's inside, which is less than `first`'s depth once a
    /// row above it holds a run.
    pub depth: u16,
    /// How many of the rows it's inside end with it, innermost first.
    pub ends: u16,
}

impl Row {
    /// Its nodes, `first` to `last`.
    pub fn nodes(self) -> impl Iterator<Item = NodeId> {
        (self.first.0..=self.last.0).map(NodeId)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Folder,
    File,
}

/// What the mod does to a node. A folder is only ever `Unchanged` or `Inside`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    Unchanged,
    /// The mod changes a file the target has.
    Changed,
    /// The mod adds a file no target edition has.
    Added,
    /// A folder with a changed or added file somewhere below.
    Inside,
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    name: u32,
    name_len: u32,
    pub parent: NodeId,
    /// One past the last node in its subtree.
    pub end: NodeId,
    /// The root is 0, the top level 1.
    pub depth: u16,
    pub kind: Kind,
    pub status: Status,
    /// Which of the tree's editions have it, by bit. A folder has every bit
    /// any node inside has, so it lacks one only when everything inside does.
    /// Empty in the Project tree.
    pub editions: u32,
    /// What git sees changed, or for a game path, in its files under
    /// `changes/`. Always clean when the project doesn't use git.
    pub git: git::Status,
}

pub struct Tree {
    nodes: Vec<Node>,
    names: String,
    /// Each bit of `Node::editions`, lowest first.
    pub editions: Vec<String>,
}

/// Both of a project's trees, read together since both need the target's paths.
pub struct Trees {
    pub game: Tree,
    pub project: Tree,
}

impl Project {
    /// Reads both trees from disk.
    ///
    /// # Errors
    ///
    /// If a directory or a target edition's manifest can't be read, or the
    /// target has more than 32 editions.
    pub fn trees(&self) -> Result<Trees, Error> {
        Trees::read(&self.root, &self.config)
    }
}

impl Trees {
    fn read(root: &Path, config: &Config) -> Result<Self, Error> {
        let git = git::status(root).unwrap_or_default();
        // TODO: a game project's Game tree, once game projects have a layout.
        if config.kind == config::Kind::Game {
            let mut project = Vec::new();
            walk(root, None, &mut String::new(), &mut project)?;
            mark(&mut project, &git);
            return Ok(Self {
                game: Tree::build(Vec::new(), Vec::new())?,
                project: Tree::build(project, Vec::new())?,
            });
        }
        let target = Target::read(root, config)?;

        // The Game tree shows what's in `base/`.
        let mut project = Vec::new();
        walk(root, Some(base::DIR), &mut String::new(), &mut project)?;
        let prefix = format!("{}/", changes::DIR);
        for entry in project.iter_mut().filter(|e| e.kind == Kind::File) {
            if let Some(rel) = entry.path.strip_prefix(&prefix) {
                let (path, _) = target.game_path(rel, config);
                entry.status = if target.has(&path) {
                    Status::Changed
                } else {
                    Status::Added
                };
            }
        }
        mark(&mut project, &git);

        let mut changed = Vec::new();
        walk(
            &root.join(changes::DIR),
            None,
            &mut String::new(),
            &mut changed,
        )?;
        let changed = changed.into_iter().filter(|e| e.kind == Kind::File);
        let changed: Vec<_> = changed
            .map(|entry| {
                let git = git::find(&git, &format!("{prefix}{}", entry.path));
                let (path, editions) = target.game_path(&entry.path, config);
                Entry {
                    status: Status::Added,
                    editions,
                    git,
                    ..Entry::file(path)
                }
            })
            .collect();
        // A deleted change file colours its game path, and stands in for it
        // when only the change had it.
        let deleted = git
            .iter()
            .filter(|(_, status)| *status == git::Status::Deleted);
        let deleted = deleted.filter_map(|(path, _)| path.strip_prefix(&prefix));
        let deleted: Vec<_> = deleted
            .filter(|rel| !rel.split('/').any(is_hidden))
            .map(|rel| {
                let (path, editions) = target.game_path(rel, config);
                Entry {
                    editions,
                    git: git::Status::Deleted,
                    ..Entry::file(path)
                }
            })
            .collect();
        let (all, editions) = (target.all, target.editions);
        let mut game = target.paths;
        game.extend(changed);
        game.sort_unstable_by(|a, b| tree_order(a, b).then(a.status.cmp(&b.status)));
        // A base path sorts before the changes at it, and several change files
        // can share one game path, so each run collapses into its first entry.
        game.dedup_by(|change, first| {
            if change.path != first.path || change.kind != first.kind {
                return false;
            }
            if first.status == Status::Unchanged {
                first.status = Status::Changed;
            }
            first.editions |= change.editions;
            first.git = first.git.merge(change.git);
            true
        });
        for ghost in deleted {
            match game.binary_search_by(|e| tree_order(e, &ghost)) {
                Ok(i) => game[i].git = game[i].git.merge(ghost.git),
                Err(i) => game.insert(i, ghost),
            }
        }
        // Only what isn't in the target has no editions.
        for entry in game.iter_mut().filter(|e| e.editions == 0) {
            entry.editions = all;
        }

        Ok(Self {
            game: Tree::build(game, editions)?,
            project: Tree::build(project, Vec::new())?,
        })
    }
}

/// The target editions' files that are unpacked, from their manifests.
struct Target {
    /// Sorted by path, each base file once with the editions that have it.
    paths: Vec<Entry>,
    editions: Vec<String>,
    /// A bit for each of `editions`.
    all: u32,
}

/// A manifest's paths alone, so the hashes aren't parsed.
#[derive(Deserialize)]
struct Paths {
    files: Keys,
}

/// A table's keys, in its order.
struct Keys(Vec<String>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> Visitor<'de> for Visit {
            type Value = Keys;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a table of paths")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut keys = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((key, IgnoredAny)) = map.next_entry()? {
                    keys.push(key);
                }
                Ok(Keys(keys))
            }
        }
        d.deserialize_map(Visit)
    }
}

impl Target {
    fn read(root: &Path, config: &Config) -> Result<Self, Error> {
        let dir = root.join(base::DIR);
        let mut paths = Vec::new();
        let mut editions = Vec::new();
        let ids: Vec<_> = config
            .editions
            .keys()
            .filter(|id| config.is_target(id))
            .collect();
        if ids.len() > 32 {
            return Err(Error::Editions(ids.len()));
        }
        for id in ids {
            let path = dir.join(format!("{id}.toml"));
            // Not unpacked yet, as after a clone. Opening asks for it.
            let text = match fs::read_to_string(&path) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                text => text.map_err(io_err(&path))?,
            };
            let manifest: Paths =
                toml::from_str(&text).map_err(|source| Error::Manifest { path, source })?;
            let bit = 1u32 << editions.len();
            editions.push(id.clone());
            let files = manifest.files.0.into_iter();
            paths.extend(
                files
                    .filter(|p| !p.split('/').any(is_hidden))
                    .map(|path| Entry {
                        editions: bit,
                        ..Entry::file(path)
                    }),
            );
        }
        paths.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        paths.dedup_by(|next, first| {
            let same = next.path == first.path;
            if same {
                first.editions |= next.editions;
            }
            same
        });
        let all = editions.iter().fold(0, |all, _| all << 1 | 1);
        Ok(Self {
            paths,
            editions,
            all,
        })
    }

    fn has(&self, path: &str) -> bool {
        self.paths
            .binary_search_by(|e| e.path.as_str().cmp(path))
            .is_ok()
    }

    /// Where a file under `changes/` lands in the game, and the editions its
    /// name scopes it to, if any. A stored change sits beside its working
    /// file as `.toml` or `.patch`, and an edition-scoped one names the
    /// edition just before its extension, as in `common.GZ2P01.jpc`.
    fn game_path(&self, path: &str, config: &Config) -> (String, u32) {
        let working = path
            .strip_suffix(".toml")
            .or_else(|| path.strip_suffix(".patch"))
            .unwrap_or(path);
        let (dir, name) = working
            .rsplit_once('/')
            .map_or(("", working), |(d, n)| (d, n));
        let mut parts = name.rsplitn(3, '.');
        let (Some(ext), Some(id), Some(stem)) = (parts.next(), parts.next(), parts.next()) else {
            return (working.to_owned(), 0);
        };
        if !config.editions.contains_key(id) {
            return (working.to_owned(), 0);
        }
        let bit = self
            .editions
            .iter()
            .zip(0u32..)
            .find_map(|(e, i)| (e == id).then(|| 1 << i))
            .unwrap_or(0);
        let path = if dir.is_empty() {
            format!("{stem}.{ext}")
        } else {
            format!("{dir}/{stem}.{ext}")
        };
        (path, bit)
    }
}

/// A dotfile or dot-folder, such as `.git` or an archive's `.rarc.toml`,
/// left out of both trees with everything inside it.
// TODO: a toggle to show them.
fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// A path relative to the tree's root, with `/` between components.
#[derive(Clone, Debug)]
struct Entry {
    path: String,
    kind: Kind,
    status: Status,
    editions: u32,
    git: git::Status,
}

impl Entry {
    fn file(path: String) -> Self {
        Self {
            path,
            kind: Kind::File,
            status: Status::Unchanged,
            editions: 0,
            git: git::Status::Clean,
        }
    }
}

/// Gives each of `entries` its state in `git`, and adds an entry for each
/// file deleted from disk that git still has.
fn mark(entries: &mut Vec<Entry>, git: &[(String, git::Status)]) {
    for entry in entries.iter_mut().filter(|e| e.kind == Kind::File) {
        entry.git = git::find(git, &entry.path);
    }
    let deleted = git
        .iter()
        .filter(|(_, status)| *status == git::Status::Deleted);
    let deleted = deleted.filter(|(path, _)| !path.split('/').any(is_hidden));
    entries.extend(deleted.map(|(path, status)| Entry {
        git: *status,
        ..Entry::file(path.clone())
    }));
}

/// Appends every file and folder under `dir`, at paths after `rel`, but for
/// a top-level entry named `skip`. A missing `dir` adds nothing.
fn walk(
    dir: &Path,
    skip: Option<&str>,
    rel: &mut String,
    out: &mut Vec<Entry>,
) -> Result<(), Error> {
    let entries = match fs::read_dir(dir) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        entries => entries.map_err(io_err(dir))?,
    };
    for entry in entries {
        let entry = entry.map_err(io_err(dir))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_hidden(&name) || rel.is_empty() && skip == Some(&*name) {
            continue;
        }
        let len = rel.len();
        if !rel.is_empty() {
            rel.push('/');
        }
        rel.push_str(&name);
        let path = entry.path();
        let folder = entry.file_type().map_err(io_err(&path))?.is_dir();
        out.push(Entry {
            kind: if folder { Kind::Folder } else { Kind::File },
            ..Entry::file(rel.clone())
        });
        if folder {
            walk(&path, None, rel, out)?;
        }
        rel.truncate(len);
    }
    Ok(())
}

/// Folders before files at each level, then by name ignoring ASCII case, so
/// a folder's contents follow it.
fn tree_order(left: &Entry, right: &Entry) -> Ordering {
    let mut lefts = left.path.split('/').peekable();
    let mut rights = right.path.split('/').peekable();
    loop {
        let (l, r) = match (lefts.next(), rights.next()) {
            (Some(l), Some(r)) => (l, r),
            (l, r) => return l.is_some().cmp(&r.is_some()),
        };
        let l_folder = lefts.peek().is_some() || left.kind == Kind::Folder;
        let r_folder = rights.peek().is_some() || right.kind == Kind::Folder;
        let order = r_folder.cmp(&l_folder).then_with(|| name_order(l, r));
        if order != Ordering::Equal {
            return order;
        }
    }
}

fn name_order(a: &str, b: &str) -> Ordering {
    fn fold(s: &str) -> impl Iterator<Item = u8> {
        s.bytes().map(|b| b.to_ascii_lowercase())
    }
    fold(a).cmp(fold(b)).then_with(|| a.cmp(b))
}

impl Tree {
    /// Folders an entry sits in are made as needed, so only empty folders
    /// need their own entry.
    fn build(mut entries: Vec<Entry>, editions: Vec<String>) -> Result<Self, Error> {
        entries.sort_by(tree_order);
        let mut tree = Self {
            nodes: Vec::with_capacity(entries.len() + 1),
            names: String::new(),
            editions,
        };
        tree.push("", NodeId::ROOT, 0, Kind::Folder)?;
        // The folders open above the next entry, each with its name.
        let mut open: Vec<(NodeId, &str)> = Vec::new();
        for entry in &entries {
            let parts: Vec<&str> = entry.path.split('/').collect();
            let Some((last, folders)) = parts.split_last() else {
                continue;
            };
            let kept = open
                .iter()
                .zip(folders)
                .take_while(|((_, open), part)| open == *part)
                .count();
            tree.close(&mut open, kept)?;
            for &part in &folders[kept..] {
                let parent = open.last().map_or(NodeId::ROOT, |&(id, _)| id);
                let id = tree.push(part, parent, open.len() + 1, Kind::Folder)?;
                open.push((id, part));
            }
            let parent = open.last().map_or(NodeId::ROOT, |&(id, _)| id);
            let id = tree.push(last, parent, open.len() + 1, entry.kind)?;
            let node = &mut tree.nodes[id.0 as usize];
            node.status = entry.status;
            node.editions = entry.editions;
            node.git = entry.git;
            if entry.kind == Kind::Folder {
                open.push((id, last));
            }
        }
        tree.close(&mut open, 0)?;
        tree.nodes[0].end = tree.next()?;

        for i in (1..tree.nodes.len()).rev() {
            let Node {
                parent,
                status,
                editions,
                git,
                ..
            } = tree.nodes[i];
            let parent = &mut tree.nodes[parent.0 as usize];
            parent.editions |= editions;
            parent.git = parent.git.merge(git);
            if status != Status::Unchanged {
                parent.status = Status::Inside;
            }
        }
        Ok(tree)
    }

    fn next(&self) -> Result<NodeId, Error> {
        u32::try_from(self.nodes.len())
            .map(NodeId)
            .map_err(|_| Error::Full)
    }

    fn push(
        &mut self,
        name: &str,
        parent: NodeId,
        depth: usize,
        kind: Kind,
    ) -> Result<NodeId, Error> {
        let id = self.next()?;
        let end = id.0.checked_add(1).map(NodeId).ok_or(Error::Full)?;
        let start = u32::try_from(self.names.len()).map_err(|_| Error::Full)?;
        let len = u32::try_from(name.len()).map_err(|_| Error::Full)?;
        self.names.push_str(name);
        self.nodes.push(Node {
            name: start,
            name_len: len,
            parent,
            end,
            depth: u16::try_from(depth).map_err(|_| Error::Full)?,
            kind,
            status: Status::Unchanged,
            editions: 0,
            git: git::Status::Clean,
        });
        Ok(id)
    }

    /// Ends the subtrees of every open folder past the first `keep`.
    fn close(&mut self, open: &mut Vec<(NodeId, &str)>, keep: usize) -> Result<(), Error> {
        let end = self.next()?;
        for (id, _) in open.drain(keep..) {
            self.nodes[id.0 as usize].end = end;
        }
        Ok(())
    }

    #[must_use]
    pub fn name(&self, id: NodeId) -> &str {
        let node = &self[id];
        let start = node.name as usize;
        self.names
            .get(start..start + node.name_len as usize)
            .unwrap_or_default()
    }

    /// Every node but the root.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len() - 1
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The lines a reader sees, in order: everything below the root that
    /// isn't inside a folder for which `open` is false. With `compact`, a
    /// folder holding only one node shares its line, see [`Row`].
    pub fn rows(&self, open: impl Fn(NodeId) -> bool, compact: bool) -> impl Iterator<Item = Row> {
        let mut at = NodeId(1);
        // The ends of the open rows `at` is inside, innermost last.
        let mut inside: Vec<NodeId> = Vec::new();
        iter::from_fn(move || {
            let first = at;
            while inside.last().is_some_and(|&end| end <= first) {
                inside.pop();
            }
            let depth = u16::try_from(inside.len()).ok()?;
            let mut last = first;
            let mut node = self.nodes.get(last.index())?;
            while compact
                && node.kind == Kind::Folder
                && let Some(child) = self.nodes.get(last.index() + 1)
                && last.0 + 1 < node.end.0
                && child.end == node.end
            {
                last = NodeId(last.0 + 1);
                node = child;
            }
            at = if node.kind == Kind::Folder && !open(last) {
                node.end
            } else {
                NodeId(last.0 + 1)
            };
            let ending = inside.iter().rev().take_while(|&&end| end <= at).count();
            let ends = u16::try_from(ending).ok()?;
            if node.kind == Kind::Folder && at != node.end {
                inside.push(node.end);
            }
            Some(Row {
                first,
                last,
                depth,
                ends,
            })
        })
    }

    /// Writes `row`'s name into `out`, its nodes' names joined by `/`.
    pub fn row_name(&self, row: Row, out: &mut String) {
        out.clear();
        for id in row.nodes() {
            if id != row.first {
                out.push('/');
            }
            out.push_str(self.name(id));
        }
    }
}

impl Index<NodeId> for Tree {
    type Output = Node;

    fn index(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        base::{Entry as BaseEntry, Manifest},
        config::{Edition, Platform},
        hash::Hash,
    };

    fn config(kind: config::Kind, ids: &[&str], target: Option<&[&str]>) -> Config {
        let edition = || Edition {
            platform: Platform::GameCube,
            region: None,
            country: None,
            revision: 0,
            disc_hash: Hash(0),
            files_digest: Hash(0),
        };
        Config {
            kind,
            target: target.map(|t| t.iter().map(|&id| id.to_owned()).collect()),
            editions: ids.iter().map(|&id| (id.to_owned(), edition())).collect(),
        }
    }

    fn touch(root: &Path, paths: &[&str]) {
        for path in paths {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"").unwrap();
        }
    }

    fn manifest(root: &Path, id: &str, paths: &[&str]) {
        let manifest = Manifest {
            files: paths
                .iter()
                .map(|&p| {
                    let entry = BaseEntry {
                        hash: Hash(0),
                        compression: None,
                    };
                    (p.to_owned(), entry)
                })
                .collect(),
        };
        fs::create_dir_all(root.join(base::DIR)).unwrap();
        let path = root.join(base::DIR).join(format!("{id}.toml"));
        fs::write(path, toml::to_string(&manifest).unwrap()).unwrap();
    }

    /// One line per node shown when `open`, indented by depth: the name, `/`
    /// for a folder, its status, its editions' region letters when it isn't
    /// in all of them, and its git state.
    fn show(tree: &Tree, open: impl Fn(NodeId) -> bool) -> Vec<String> {
        let all = tree.editions.iter().fold(0, |all, _| all << 1 | 1);
        tree.rows(open, false)
            .map(|Row { last: id, .. }| {
                let node = tree[id];
                let mut line = "  ".repeat(usize::from(node.depth - 1));
                line.push_str(tree.name(id));
                if node.kind == Kind::Folder {
                    line.push('/');
                }
                line.push_str(match node.status {
                    Status::Unchanged => "",
                    Status::Changed => " M",
                    Status::Added => " A",
                    Status::Inside => " •",
                });
                if node.editions != all && node.editions != 0 {
                    line.push(' ');
                    for (id, i) in tree.editions.iter().zip(0..) {
                        if node.editions & 1 << i != 0 {
                            line.push_str(id.get(3..4).unwrap());
                        }
                    }
                }
                line.push_str(match node.git {
                    git::Status::Clean => "",
                    git::Status::Added => " +",
                    git::Status::Modified => " ~",
                    git::Status::Deleted => " -",
                });
                line
            })
            .collect()
    }

    fn mod_project() -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let shared = [
            "sys/main.dol",
            "res/Layout/logo.bti",
            "res/Particle/common.jpc",
            "res/Object/al.arc/.rarc.toml",
            "res/Object/al.arc/al.bmd",
        ];
        manifest(
            root,
            "GZ2E01",
            &[&shared[..], &["res/Msgus/zel_00.bmg"]].concat(),
        );
        manifest(
            root,
            "GZ2P01",
            &[&shared[..], &["res/Msgus/zel_00.bmg"]].concat(),
        );
        manifest(
            root,
            "GZ2J01",
            &[&shared[..], &["res/Msgjp/zel_00.bmg"]].concat(),
        );
        // A reference, which the Game tree leaves out.
        manifest(root, "GZLE01", &["res/Object/Link.arc/link.bmd"]);
        touch(
            root,
            &[
                "arbiter.toml",
                ".gitignore",
                ".git/HEAD",
                ".arbiter/log",
                "code/mod.cpp",
                "changes/res/Layout/logo.bti",
                "changes/res/Layout/logo.bti.toml",
                "changes/res/Particle/common.GZ2P01.jpc.patch",
                "changes/res/Msgus/zel_50.bmg.toml",
            ],
        );
        fs::create_dir_all(root.join("assets")).unwrap();
        let ids = ["GZ2E01", "GZ2J01", "GZ2P01", "GZLE01"];
        let target = ["GZ2E01", "GZ2J01", "GZ2P01"];
        (dir, config(config::Kind::Mod, &ids, Some(&target)))
    }

    #[test]
    fn game_tree_merges_the_target_and_lays_changes_over_it() {
        let (dir, config) = mod_project();
        let trees = Trees::read(dir.path(), &config).unwrap();
        assert_eq!(trees.game.editions, ["GZ2E01", "GZ2J01", "GZ2P01"]);
        assert_eq!(
            show(&trees.game, |_| true),
            [
                "res/ •",
                "  Layout/ •",
                "    logo.bti M",
                "  Msgjp/ J",
                "    zel_00.bmg J",
                "  Msgus/ •",
                "    zel_00.bmg EP",
                "    zel_50.bmg A",
                "  Object/",
                "    al.arc/",
                "      al.bmd",
                "  Particle/ •",
                "    common.jpc M",
                "sys/",
                "  main.dol",
            ]
        );
    }

    #[test]
    fn an_added_file_scoped_by_name_has_only_its_edition() {
        let (dir, config) = mod_project();
        touch(dir.path(), &["changes/res/new.GZ2J01.bmg.toml"]);
        let trees = Trees::read(dir.path(), &config).unwrap();
        let lines = show(&trees.game, |_| true);
        assert!(lines.contains(&"  new.bmg A J".to_owned()), "{lines:#?}");
    }

    #[test]
    fn project_tree_marks_change_files_as_the_game_tree_does() {
        let (dir, config) = mod_project();
        let trees = Trees::read(dir.path(), &config).unwrap();
        assert_eq!(
            show(&trees.project, |_| true),
            [
                "assets/",
                "changes/ •",
                "  res/ •",
                "    Layout/ •",
                "      logo.bti M",
                "      logo.bti.toml M",
                "    Msgus/ •",
                "      zel_50.bmg.toml A",
                "    Particle/ •",
                "      common.GZ2P01.jpc.patch M",
                "code/",
                "  mod.cpp",
                "arbiter.toml",
            ]
        );
    }

    /// Commits everything in `root` to a new repository.
    fn commit(root: &Path) {
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
        };
        git(&["init", "--quiet"]);
        git(&["add", "--all"]);
        git(&["commit", "--quiet", "--message", "base"]);
    }

    #[test]
    fn git_colours_names_and_folders_merge_what_is_inside() {
        let (dir, config) = mod_project();
        let root = dir.path();
        fs::remove_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".gitignore"), "base/\n").unwrap();
        touch(root, &["changes/res/Msgus/zel_00.bmg.toml"]);
        commit(root);
        fs::write(root.join("code/mod.cpp"), b"edit").unwrap();
        touch(root, &["code/new.cpp"]);
        fs::remove_file(root.join("changes/res/Layout/logo.bti")).unwrap();
        fs::remove_file(root.join("changes/res/Layout/logo.bti.toml")).unwrap();
        fs::remove_file(root.join("changes/res/Msgus/zel_50.bmg.toml")).unwrap();
        fs::remove_file(root.join(".gitignore")).unwrap();

        let trees = Trees::read(root, &config).unwrap();
        assert_eq!(
            show(&trees.project, |_| true),
            [
                "assets/",
                "changes/ • -",
                "  res/ • -",
                "    Layout/ -",
                "      logo.bti -",
                "      logo.bti.toml -",
                "    Msgus/ • -",
                "      zel_00.bmg.toml M",
                "      zel_50.bmg.toml -",
                "    Particle/ •",
                "      common.GZ2P01.jpc.patch M",
                "code/ ~",
                "  mod.cpp ~",
                "  new.cpp +",
                "arbiter.toml",
            ]
        );
        let game = show(&trees.game, |_| true);
        assert!(game.contains(&"    logo.bti -".to_owned()), "{game:#?}");
        assert!(game.contains(&"    zel_50.bmg -".to_owned()), "{game:#?}");
        assert!(
            game.contains(&"    zel_00.bmg M EP".to_owned()),
            "{game:#?}"
        );
    }

    #[test]
    fn closed_folders_hide_their_subtree() {
        let (dir, config) = mod_project();
        let game = Trees::read(dir.path(), &config).unwrap().game;
        let res = game.rows(|_| false, false).next().unwrap().last;
        assert_eq!(game.name(res), "res");
        assert_eq!(show(&game, |id| id == res).len(), 7);
        assert_eq!(show(&game, |_| false), ["res/ •", "sys/"]);
        assert_eq!(game.len(), 15);
    }

    #[test]
    fn a_folder_holding_one_node_shares_its_line() {
        let (dir, config) = mod_project();
        let game = Trees::read(dir.path(), &config).unwrap().game;
        let project = Trees::read(dir.path(), &config).unwrap().project;
        let lines = |tree: &Tree| {
            let mut name = String::new();
            let rows = tree.rows(|_| true, true);
            rows.map(|row| {
                tree.row_name(row, &mut name);
                format!("{}{name}", "  ".repeat(usize::from(row.depth)))
            })
            .collect::<Vec<_>>()
        };
        assert_eq!(
            lines(&game),
            [
                "res",
                "  Layout/logo.bti",
                "  Msgjp/zel_00.bmg",
                "  Msgus",
                "    zel_00.bmg",
                "    zel_50.bmg",
                "  Object/al.arc/al.bmd",
                "  Particle/common.jpc",
                "sys/main.dol",
            ]
        );
        assert_eq!(
            lines(&project),
            [
                "assets",
                "changes/res",
                "  Layout",
                "    logo.bti",
                "    logo.bti.toml",
                "  Msgus/zel_50.bmg.toml",
                "  Particle/common.GZ2P01.jpc.patch",
                "code/mod.cpp",
                "arbiter.toml",
            ]
        );
        let mut name = String::new();
        let ends: Vec<_> = (project.rows(|_| true, true))
            .filter(|row| row.ends > 0)
            .map(|row| {
                project.row_name(row, &mut name);
                (name.clone(), row.ends)
            })
            .collect();
        assert_eq!(
            ends,
            [
                ("logo.bti.toml".into(), 1),
                ("Particle/common.GZ2P01.jpc.patch".into(), 1),
            ]
        );
        let closed = game.rows(|_| false, true).count();
        assert_eq!(closed, 2);
    }

    #[test]
    fn an_edition_not_unpacked_is_left_out() {
        let (dir, config) = mod_project();
        fs::remove_file(dir.path().join("base/GZ2J01.toml")).unwrap();
        let game = Trees::read(dir.path(), &config).unwrap().game;
        assert_eq!(game.editions, ["GZ2E01", "GZ2P01"]);
        assert!(!show(&game, |_| true).iter().any(|l| l.contains("Msgjp")));
    }

    #[test]
    fn a_game_project_shows_only_its_files() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), &["arbiter.toml", "res/a.bmg"]);
        let config = config(config::Kind::Game, &[], None);
        let trees = Trees::read(dir.path(), &config).unwrap();
        assert!(trees.game.is_empty());
        assert_eq!(
            show(&trees.project, |_| true),
            ["res/", "  a.bmg", "arbiter.toml"]
        );
    }

    #[test]
    fn folders_sort_first_then_by_name_ignoring_case() {
        let entry = |path: &str, kind| Entry {
            kind,
            ..Entry::file(path.to_owned())
        };
        let mut entries = [
            entry("b", Kind::File),
            entry("A", Kind::File),
            entry("z/y", Kind::File),
            entry("c", Kind::Folder),
            entry("a", Kind::File),
        ];
        entries.sort_by(tree_order);
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["c", "z/y", "A", "a", "b"]);
    }
}
