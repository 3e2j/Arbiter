//! Sorting a tree's siblings by a [`SortOrder`].

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{Node, NodeId, NodeKind, Tree, extension, name_order};

/// What a tree's siblings are sorted by, after folders, which always come
/// first. The default is how a tree is read.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
pub struct SortOrder {
    pub key: SortKey,
    pub descending: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SortKey {
    /// Ignoring ASCII case.
    #[default]
    Name,
    /// The extension, so archives and messages each sit together, then the
    /// name.
    Type,
}

impl Tree {
    /// Sorts each folder's children by `order`, keeping every node's subtree
    /// right after it. Returns each node's new id, by its old
    /// [`NodeId::index`].
    pub fn sort(&mut self, order: SortOrder) -> Vec<NodeId> {
        let mut map = vec![NodeId::ROOT; self.nodes.len()];
        let mut nodes = Vec::with_capacity(self.nodes.len());
        // Parents are taken before their children, so a node's parent is
        // always mapped by the time it's placed.
        let mut stack = vec![NodeId::ROOT];
        let mut children = Vec::new();
        for new in (0..).map(NodeId) {
            let Some(old) = stack.pop() else {
                break;
            };
            let node = self.nodes[old.index()];
            map[old.index()] = new;
            nodes.push(Node {
                parent: map[node.parent.index()],
                end: NodeId(new.0 + (node.end.0 - old.0)),
                ..node
            });
            children.clear();
            let mut child = NodeId(old.0 + 1);
            while child < node.end {
                children.push(child);
                child = self.nodes[child.index()].end;
            }
            children.sort_by(|&a, &b| self.sibling_order(order, a, b));
            stack.extend(children.iter().rev());
        }
        self.nodes = nodes;
        map
    }

    fn sibling_order(&self, order: SortOrder, a: NodeId, b: NodeId) -> Ordering {
        let (a_name, b_name) = (self.name(a), self.name(b));
        let by = match order.key {
            SortKey::Name => name_order(a_name, b_name),
            SortKey::Type => name_order(extension(a_name), extension(b_name))
                .then_with(|| name_order(a_name, b_name)),
        };
        let by = if order.descending { by.reverse() } else { by };
        let folder = |id: NodeId| self[id].kind == NodeKind::Folder;
        folder(b).cmp(&folder(a)).then(by)
    }
}
