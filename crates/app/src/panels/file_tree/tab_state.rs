//! What a tab remembers of its tree, so switching back finds it as it was.

use project::tree::{Narrowed, NodeId, Tree, TreeFilter, TreeRow};

use crate::components::ListScroll;
use crate::theme::HOVER_THUMB;

/// The open folders, a bit per node by [`NodeId::index`].
#[derive(Default)]
struct Open(Vec<u64>);

#[derive(Default)]
pub struct TabState {
    open: Open,
    /// Folders clicked while searching or filtering, which flip how the
    /// results show them. Kept apart from `open`, and emptied when the results
    /// change.
    flipped: Open,
    narrowed: Narrowed,
    /// The rows were last built for a search or filter.
    narrowing: bool,
    pub selected: Option<NodeId>,
    pub scroll: ListScroll,
    /// The lines shown, rebuilt when `stale`.
    pub rows: Vec<TreeRow>,
    /// Something changed what's shown, so `rows` are rebuilt next pass.
    pub stale: bool,
}

impl Open {
    fn has(&self, id: NodeId) -> bool {
        let i = id.index();
        self.0
            .get(i / 64)
            .is_some_and(|word| word >> (i % 64) & 1 != 0)
    }

    fn toggle(&mut self, id: NodeId) {
        let i = id.index();
        if self.0.len() <= i / 64 {
            self.0.resize(i / 64 + 1, 0);
        }
        if let Some(word) = self.0.get_mut(i / 64) {
            *word ^= 1 << (i % 64);
        }
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    /// Itself with each node moved to its id in `map`, by old
    /// [`NodeId::index`], and those with none dropped.
    fn remapped(&self, map: &[Option<NodeId>]) -> Self {
        let mut open = Self::default();
        let words = map.chunks(64).zip(&self.0);
        for (ids, &word) in words {
            let set = ids.iter().zip(0..).filter(|&(_, bit)| word >> bit & 1 != 0);
            for new in set.filter_map(|(&new, _)| new) {
                open.toggle(new);
            }
        }
        open
    }
}

impl TabState {
    pub fn new() -> Self {
        Self {
            scroll: ListScroll::TOP.thumb(HOVER_THUMB),
            stale: true,
            ..Self::default()
        }
    }

    /// Moves what it remembers of each node to its id in `map`, by old
    /// [`NodeId::index`], and forgets those with none.
    pub fn remap(&mut self, map: &[Option<NodeId>]) {
        self.open = self.open.remapped(map);
        self.flipped = self.flipped.remapped(map);
        self.selected = self
            .selected
            .and_then(|id| map.get(id.index()).copied().flatten());
        self.stale = true;
    }

    /// Forgets folders flipped in the old results and goes back to the top.
    pub fn renarrow(&mut self) {
        self.flipped.clear();
        self.scroll.jump_to_top();
        self.stale = true;
    }

    /// Rebuilds `rows` from `tree` when `stale`, narrowed to `query` and
    /// `filter` when either is set.
    pub fn rebuild(&mut self, tree: &Tree, query: &str, filter: TreeFilter, compact: bool) {
        if !self.stale {
            return;
        }
        self.narrowing = !query.is_empty() || !filter.is_empty();
        if self.narrowing {
            tree.narrow(query, filter, &mut self.narrowed);
        }
        let mut rows = std::mem::take(&mut self.rows);
        rows.clear();
        rows.extend(tree.rows(
            |id| self.is_open(id),
            |id| !self.narrowing || self.narrowed.kept(id),
            compact,
        ));
        self.rows = rows;
        self.stale = false;
    }

    /// Whether `id` shows open, by `narrowed` while narrowing.
    pub fn is_open(&self, id: NodeId) -> bool {
        if self.narrowing {
            self.narrowed.revealed(id) != self.flipped.has(id)
        } else {
            self.open.has(id)
        }
    }

    /// Opens or closes the folder `id`, in the results while narrowing.
    pub fn toggle(&mut self, id: NodeId) {
        if self.narrowing {
            self.flipped.toggle(id);
        } else {
            self.open.toggle(id);
        }
        self.stale = true;
    }
}
