//! The project's files as the file tree shows them:
//! - The Game tree, the target editions with `changes/` laid over them
//! - The Project tree, everything on disk.
//!
//! A tree is one flat list of nodes in preorder, folders before files,
//! each level sorted by name, or by another [`SortOrder`] once [`Tree::sort`]ed.
//! Each node knows its parent, its depth, and where its subtree ends,
//! so a closed folder is skipped in one step and anything folders take from
//! their contents is one pass backwards.
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
    io, iter,
    ops::Index,
    path::{Path, PathBuf},
};

mod filter;
mod git;
mod read;
mod sort;
#[cfg(test)]
mod tests;

pub use filter::{Narrowed, TreeFilter};
pub use git::GitStatus;
pub use read::Trees;
pub use sort::{SortKey, SortOrder};

#[derive(Debug, thiserror::Error)]
pub enum TreeError {
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
pub struct TreeRow {
    pub first: NodeId,
    pub last: NodeId,
    /// How many rows it's inside, which is less than `first`'s depth once a
    /// row above it holds a run.
    pub depth: u16,
    /// How many of the rows it's inside end with it, innermost first.
    pub ends: u16,
    /// Whether it's the first row inside the row it's in, which a sibling
    /// before it being left out can make it.
    pub leads: bool,
}

impl TreeRow {
    /// Its nodes, `first` to `last`.
    pub fn nodes(self) -> impl Iterator<Item = NodeId> {
        (self.first.0..=self.last.0).map(NodeId)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    Folder,
    File,
}

/// What the mod does to a node. A folder is only ever `Unchanged` or `Inside`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum NodeStatus {
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
    pub kind: NodeKind,
    pub status: NodeStatus,
    /// Which of the tree's editions have it, by bit. A folder has every bit
    /// any node inside has, so it lacks one only when everything inside does.
    /// Empty in the Project tree.
    pub editions: u32,
    /// What git sees changed, or for a game path, in its files under
    /// `changes/`. Always clean when the project doesn't use git.
    pub git: GitStatus,
}

pub struct Tree {
    nodes: Vec<Node>,
    names: String,
    /// Each bit of `Node::editions`, lowest first.
    pub editions: Vec<String>,
}

/// A path relative to the tree's root, with `/` between components.
#[derive(Clone, Debug)]
struct Entry {
    path: String,
    kind: NodeKind,
    status: NodeStatus,
    editions: u32,
    git: GitStatus,
}

impl Entry {
    fn file(path: String) -> Self {
        Self {
            path,
            kind: NodeKind::File,
            status: NodeStatus::Unchanged,
            editions: 0,
            git: GitStatus::Clean,
        }
    }
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
        let l_folder = lefts.peek().is_some() || left.kind == NodeKind::Folder;
        let r_folder = rights.peek().is_some() || right.kind == NodeKind::Folder;
        let order = r_folder.cmp(&l_folder).then_with(|| name_order(l, r));
        if order != Ordering::Equal {
            return order;
        }
    }
}

/// After the last `.`, or empty.
fn extension(name: &str) -> &str {
    name.rsplit_once('.').map_or("", |(_, ext)| ext)
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
    fn build(mut entries: Vec<Entry>, editions: Vec<String>) -> Result<Self, TreeError> {
        entries.sort_by(tree_order);
        let mut tree = Self {
            nodes: Vec::with_capacity(entries.len() + 1),
            names: String::new(),
            editions,
        };
        tree.push("", NodeId::ROOT, 0, NodeKind::Folder)?;
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
                let id = tree.push(part, parent, open.len() + 1, NodeKind::Folder)?;
                open.push((id, part));
            }
            let parent = open.last().map_or(NodeId::ROOT, |&(id, _)| id);
            let id = tree.push(last, parent, open.len() + 1, entry.kind)?;
            let node = &mut tree.nodes[id.0 as usize];
            node.status = entry.status;
            node.editions = entry.editions;
            node.git = entry.git;
            if entry.kind == NodeKind::Folder {
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
            if status != NodeStatus::Unchanged {
                parent.status = NodeStatus::Inside;
            }
        }
        Ok(tree)
    }

    fn next(&self) -> Result<NodeId, TreeError> {
        u32::try_from(self.nodes.len())
            .map(NodeId)
            .map_err(|_| TreeError::Full)
    }

    fn push(
        &mut self,
        name: &str,
        parent: NodeId,
        depth: usize,
        kind: NodeKind,
    ) -> Result<NodeId, TreeError> {
        let id = self.next()?;
        let end = id.0.checked_add(1).map(NodeId).ok_or(TreeError::Full)?;
        let start = u32::try_from(self.names.len()).map_err(|_| TreeError::Full)?;
        let len = u32::try_from(name.len()).map_err(|_| TreeError::Full)?;
        self.names.push_str(name);
        self.nodes.push(Node {
            name: start,
            name_len: len,
            parent,
            end,
            depth: u16::try_from(depth).map_err(|_| TreeError::Full)?,
            kind,
            status: NodeStatus::Unchanged,
            editions: 0,
            git: GitStatus::Clean,
        });
        Ok(id)
    }

    /// Ends the subtrees of every open folder past the first `keep`.
    fn close(&mut self, open: &mut Vec<(NodeId, &str)>, keep: usize) -> Result<(), TreeError> {
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

    /// Each node's id in `other`, by its [`NodeId::index`] here: the node of
    /// the same kind at the same path, if `other` has one. So what's kept per
    /// node follows it into a tree read again.
    #[must_use]
    pub fn ids_in(&self, other: &Self) -> Vec<Option<NodeId>> {
        let mut map = vec![None; self.nodes.len()];
        map[0] = Some(NodeId::ROOT);
        let mut theirs = Vec::new();
        // Parents come before their children, so each is mapped by the time
        // its children are.
        for (id, node) in (0..).map(NodeId).zip(&self.nodes) {
            let Some(at) = map[id.index()] else {
                continue;
            };
            if node.kind != NodeKind::Folder {
                continue;
            }
            theirs.clear();
            theirs.extend(other.children(at).map(|child| (other.name(child), child)));
            theirs.sort_unstable_by(|a, b| a.0.cmp(b.0));
            for child in self.children(id) {
                let name = self.name(child);
                let found = theirs.binary_search_by(|&(theirs, _)| theirs.cmp(name));
                map[child.index()] = found
                    .ok()
                    .map(|i| theirs[i].1)
                    .filter(|&theirs| other[theirs].kind == self[child].kind);
            }
        }
        map
    }

    /// The nodes directly inside `id`, in order.
    fn children(&self, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let end = self[id].end;
        let first = Some(NodeId(id.0 + 1)).filter(|&child| child < end);
        iter::successors(first, move |&child| {
            Some(self[child].end).filter(|&next| next < end)
        })
    }

    /// Whether it's an unpacked `.arc`, a folder that's a file in the game.
    #[must_use]
    pub fn is_archive(&self, id: NodeId) -> bool {
        self[id].kind == NodeKind::Folder && extension(self.name(id)).eq_ignore_ascii_case("arc")
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
    /// `kept` keeps and isn't inside a folder for which `open` is false. A
    /// node left out takes its subtree with it. With `compact`, a folder
    /// holding only one folder shares its line, see [`TreeRow`], unless either
    /// is an archive.
    pub fn rows(
        &self,
        open: impl Fn(NodeId) -> bool,
        kept: impl Fn(NodeId) -> bool,
        compact: bool,
    ) -> impl Iterator<Item = TreeRow> {
        // The first node from `at` on that's kept, past each subtree that isn't.
        let skip = move |mut at: NodeId| {
            while let Some(node) = self.nodes.get(at.index())
                && !kept(at)
            {
                at = node.end;
            }
            at
        };
        let mut at = skip(NodeId(1));
        // The ends of the open rows `at` is inside, innermost last.
        let mut inside: Vec<NodeId> = Vec::new();
        // The last node of the row before.
        let mut above = NodeId::ROOT;
        iter::from_fn(move || {
            let first = at;
            while inside.last().is_some_and(|&end| end <= first) {
                inside.pop();
            }
            let depth = u16::try_from(inside.len()).ok()?;
            let mut last = first;
            let mut node = self.nodes.get(last.index())?;
            let leads = node.parent == above;
            while compact
                && node.kind == NodeKind::Folder
                && let Some(child) = self.nodes.get(last.index() + 1)
                && last.0 + 1 < node.end.0
                && child.end == node.end
                && child.kind == NodeKind::Folder
                && !self.is_archive(last)
                && !self.is_archive(NodeId(last.0 + 1))
            {
                last = NodeId(last.0 + 1);
                node = child;
            }
            at = skip(if node.kind == NodeKind::Folder && !open(last) {
                node.end
            } else {
                NodeId(last.0 + 1)
            });
            let ending = inside.iter().rev().take_while(|&&end| end <= at).count();
            let ends = u16::try_from(ending).ok()?;
            if node.kind == NodeKind::Folder && at < node.end {
                inside.push(node.end);
            }
            above = last;
            Some(TreeRow {
                first,
                last,
                depth,
                ends,
                leads,
            })
        })
    }

    /// A bit for each of its editions, as a node in all of them has.
    #[must_use]
    pub fn every_edition(&self) -> u32 {
        self.editions.iter().fold(0, |all, _| all << 1 | 1)
    }

    /// Writes `id`'s path from the root into `out`, its folders' names and
    /// its own joined by `/`.
    pub fn path(&self, id: NodeId, out: &mut String) {
        out.clear();
        let mut at = id;
        while at != NodeId::ROOT {
            if !out.is_empty() {
                out.insert(0, '/');
            }
            out.insert_str(0, self.name(at));
            at = self[at].parent;
        }
    }

    /// Writes `row`'s name into `out`, its nodes' names joined by `/`.
    pub fn row_name(&self, row: TreeRow, out: &mut String) {
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

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> TreeError + '_ {
    |source| TreeError::Io {
        path: path.to_path_buf(),
        source,
    }
}
