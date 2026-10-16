//! Which box is on top at a point, inside its clip, as [`emit`](super::emit)
//! draws them: the tree in declaration order, then floating boxes over it by
//! [`Layer`].

use super::{Layer, Layout, Node, layer};
use crate::canvas::Rect;
use crate::input::CursorIcon;

impl Layout {
    /// Finds the box last pass drew on top at `pointer`, for [`Self::under`].
    pub fn hit_test(&mut self, pointer: Option<[f32; 2]>) {
        self.hit = pointer.and_then(|at| top(&self.last, &mut self.reach, at));
    }

    /// Whether `last` is the box [`Self::hit_test`] found, or holds it and is
    /// under the pointer too. A floating box's parent doesn't count where only
    /// the floating box is.
    pub fn under(&self, last: usize, pointer: [f32; 2]) -> bool {
        let node = &self.last[last];
        self.hit
            .is_some_and(|hit| (last..node.end).contains(&hit) && node.rect.contains(pointer))
    }

    /// The cursor of the top box at `pointer` this pass, or of the nearest box
    /// holding it that has one.
    pub fn cursor(&mut self, pointer: Option<[f32; 2]>) -> Option<CursorIcon> {
        let mut index = top(&self.nodes, &mut self.reach, pointer?)?;
        loop {
            let node = &self.nodes[index];
            if node.element.cursor.is_some() || index == 0 {
                return node.element.cursor;
            }
            index = node.parent;
        }
    }
}

/// The highest [`Layer`] wins, then the one declared last. `reach` is scratch.
fn top(nodes: &[Node], reach: &mut Vec<(Rect, Layer)>, at: [f32; 2]) -> Option<usize> {
    reach.clear();
    let window = nodes.first()?.rect;
    let mut top = None;
    for (index, node) in (0..).zip(nodes) {
        let (clip, own) = if index == 0 {
            (window, [0; 2])
        } else {
            let parent = &nodes[node.parent];
            let (clip, under) = reach[node.parent];
            let clip = if parent.element.clip {
                clip.intersect(parent.rect)
            } else {
                clip
            };
            let float = node.element.float;
            let escapes = float.is_some_and(|anchor| !anchor.clipped());
            (if escapes { window } else { clip }, layer(under, float))
        };
        reach.push((clip, own));
        if clip.contains(at) && node.rect.contains(at) && top.is_none_or(|(_, over)| own >= over) {
            top = Some((index, own));
        }
    }
    top.map(|(index, _)| index)
}
