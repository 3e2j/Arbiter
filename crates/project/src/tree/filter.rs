//! Narrowing a tree to what a search and a [`TreeFilter`] keep.

use super::{Node, NodeId, NodeKind, NodeStatus, Tree};

/// Which files a tree keeps: those every set field keeps. One with nothing
/// set keeps them all.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct TreeFilter {
    /// Changed or added by the mod.
    pub modded: bool,
    /// In any of the tree's editions with a bit set here, as in
    /// [`Node::editions`], but not in every edition. 0 keeps every file.
    pub editions: u32,
}

impl TreeFilter {
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.modded && self.editions == 0
    }

    /// Whether it keeps `node`, of a tree whose editions are the bits in `all`.
    fn keeps(self, node: &Node, all: u32) -> bool {
        let differs = node.editions & self.editions != 0 && node.editions != all;
        (!self.modded || matches!(node.status, NodeStatus::Changed | NodeStatus::Added))
            && (self.editions == 0 || differs)
    }
}

/// What a search and a [`TreeFilter`] leave of a tree, from [`Tree::narrow`],
/// as bits by node.
#[derive(Default)]
pub struct Narrowed(Vec<u8>);

impl Narrowed {
    /// Its name has the search in it.
    const NAMED: u8 = 1;
    /// It or a folder above it is [`Self::NAMED`].
    const MATCHED: u8 = 1 << 1;
    const KEPT: u8 = 1 << 2;
    /// A folder with a hit inside, to show open.
    const REVEALED: u8 = 1 << 3;

    fn has(&self, id: NodeId, bit: u8) -> bool {
        self.0.get(id.index()).is_some_and(|bits| bits & bit != 0)
    }

    #[must_use]
    pub fn kept(&self, id: NodeId) -> bool {
        self.has(id, Self::KEPT)
    }

    /// Whether it's a folder with a hit inside, meant to show open.
    #[must_use]
    pub fn revealed(&self, id: NodeId) -> bool {
        self.has(id, Self::REVEALED)
    }
}

impl Tree {
    /// Keeps each node whose name has `query` in it, ignoring ASCII case,
    /// with everything inside it and every folder above it. An empty `query`
    /// keeps every node. Of those, only files `filter` keeps stay, with the
    /// folders above them, and a folder stays only when one does. Folders
    /// above a hit are revealed: a hit is a node kept for its own name, or
    /// any file kept while `filter` keeps only what the mod changed. Editions
    /// alone reveal nothing, so folders stay as they were opened.
    pub fn narrow(&self, query: &str, filter: TreeFilter, out: &mut Narrowed) {
        let bits = &mut out.0;
        bits.clear();
        bits.resize(self.nodes.len(), 0);
        let query = query.as_bytes();
        if query.is_empty() {
            bits.fill(Narrowed::MATCHED);
        } else {
            // Parents come before their children, so each takes its parent's
            // match as it goes.
            for (index, node) in (0..).zip(&self.nodes).skip(1) {
                let id = NodeId(index);
                let name = self.name(id).as_bytes();
                let named = name
                    .windows(query.len())
                    .any(|w| w.eq_ignore_ascii_case(query));
                let matched = bits[node.parent.index()] & Narrowed::MATCHED != 0;
                bits[id.index()] = match (named, matched) {
                    (true, _) => Narrowed::NAMED | Narrowed::MATCHED,
                    (false, true) => Narrowed::MATCHED,
                    (false, false) => 0,
                };
            }
        }
        let filtering = !filter.is_empty();
        let all = self.every_edition();
        // Children come after their parents, so each has had all its
        // children's say by the time it's reached.
        for (node, index) in self.nodes.iter().zip(0..bits.len()).skip(1).rev() {
            let file = node.kind == NodeKind::File;
            let passes = if file {
                filter.keeps(node, all)
            } else {
                !filtering
            };
            let mut own = bits[index];
            if own & Narrowed::MATCHED != 0 && passes {
                own |= Narrowed::KEPT;
            }
            bits[index] = own;
            let kept = own & Narrowed::KEPT != 0;
            let hit = kept && (own & Narrowed::NAMED != 0 || filter.modded && file);
            let parent = &mut bits[node.parent.index()];
            if kept {
                *parent |= Narrowed::KEPT;
            }
            if hit || own & Narrowed::REVEALED != 0 {
                *parent |= Narrowed::REVEALED;
            }
        }
    }
}
