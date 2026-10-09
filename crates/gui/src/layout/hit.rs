//! Which box is on top at a point, inside its clip, as [`emit`](super::emit)
//! draws them: the tree in declaration order, then floating boxes over it,
//! one inside another after both.

use super::{Layout, Node};
use crate::canvas::Rect;
use crate::input::Cursor;

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
    pub fn cursor(&mut self, pointer: Option<[f32; 2]>) -> Option<Cursor> {
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

/// The deepest in floating boxes wins, then the one declared last. `reach`
/// is scratch.
fn top(nodes: &[Node], reach: &mut Vec<(Rect, u32)>, at: [f32; 2]) -> Option<usize> {
    reach.clear();
    let window = nodes.first()?.rect;
    let mut top = None;
    for (index, node) in (0..).zip(nodes) {
        let (clip, floats) = if index == 0 {
            (window, 0)
        } else {
            let parent = &nodes[node.parent];
            let (clip, floats) = reach[node.parent];
            let clip = if parent.element.clip {
                clip.intersect(parent.rect)
            } else {
                clip
            };
            match node.element.float {
                Some(anchor) if anchor.clipped() => (clip, floats + 1),
                Some(_) => (window, floats + 1),
                None => (clip, floats),
            }
        };
        reach.push((clip, floats));
        if clip.contains(at) && node.rect.contains(at) && top.is_none_or(|(_, over)| floats >= over)
        {
            top = Some((index, floats));
        }
    }
    top.map(|(index, _)| index)
}
