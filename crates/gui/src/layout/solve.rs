//! Sizes, then places, every box. Each axis is sized on its own: fit sizes
//! from the leaves up, then grow sizes from the window down. Text doesn't
//! wrap yet, so a box's height never depends on its width.

use super::{Align, Anchor, Content, Direction, Layout, Length, Node};
use crate::canvas::Rect;

impl Layout {
    /// Places every box in `root`, the window in logical pixels, with box
    /// edges on whole physical pixels at `scale`. Returns whether any box
    /// moved since last pass, which makes what the pass read stale.
    pub fn solve(&mut self, root: Rect, scale: f32) -> bool {
        // A box the pass left open holds everything declared after it.
        while self.open.len() > 1 {
            self.close();
        }
        let len = self.nodes.len();
        let Some(window) = self.nodes.first_mut() else {
            return false;
        };
        window.end = len;
        window.element.size = [Length::Fixed(root.w), Length::Fixed(root.h)];
        for axis in [0, 1] {
            self.fit(axis);
            self.grow(axis);
        }
        self.nodes[0].rect = root;
        self.place(scale);
        self.shifted
            || self.nodes.len() != self.last.len()
            || self
                .nodes
                .iter()
                .zip(&self.last)
                .any(|(node, last)| node.rect != last.rect)
    }

    /// Leaves first, so each box's children are sized before it is.
    fn fit(&mut self, axis: usize) {
        for index in (0..self.nodes.len()).rev() {
            let node = &self.nodes[index];
            let intrinsic = match node.content {
                Content::Text(at) => Some(self.texts[at].size[axis]),
                Content::Icon(_, size, _) => Some(size),
                Content::Box | Content::Custom(_) => None,
            };
            let used = intrinsic.unwrap_or_else(|| {
                let sizes = flow(&self.nodes, index).map(|child| self.nodes[child].size[axis]);
                if main(node) == axis {
                    let (sum, count) =
                        sizes.fold((0., 0u16), |(sum, count), size| (sum + size, count + 1));
                    sum + node.element.gap * f32::from(count.saturating_sub(1))
                } else {
                    sizes.fold(0., f32::max)
                }
            });
            let node = &mut self.nodes[index];
            let size = match node.element.size[axis] {
                Length::Fixed(size) => size,
                // Cut off at its edge anyway, so it takes only what it's
                // given and what's past that can scroll.
                Length::Grow if node.element.clip => padding(node, axis),
                Length::Fit | Length::Grow => used + padding(node, axis),
            };
            node.size[axis] = size.max(node.element.min[axis]);
        }
    }

    /// Parents first, so a box has its final size before its children share
    /// it out.
    fn grow(&mut self, axis: usize) {
        for index in 0..self.nodes.len() {
            let node = &self.nodes[index];
            let inner = node.size[axis] - padding(node, axis);
            if main(node) == axis {
                let (mut used, mut growing) = (0., 0u16);
                let mut count = 0u16;
                for child in flow(&self.nodes, index) {
                    let child = &self.nodes[child];
                    used += child.size[axis];
                    count += 1;
                    growing += u16::from(child.element.size[axis] == Length::Grow);
                }
                used += node.element.gap * f32::from(count.saturating_sub(1));
                let free = inner - used;
                if growing == 0 || free <= 0. {
                    continue;
                }
                let share = free / f32::from(growing);
                self.each_child(index, |child| {
                    if child.element.float.is_none() && child.element.size[axis] == Length::Grow {
                        child.size[axis] += share;
                    }
                });
            } else {
                self.each_child(index, |child| {
                    if child.element.float.is_none() && child.element.size[axis] == Length::Grow {
                        child.size[axis] = child.size[axis].max(inner);
                    }
                });
            }
        }
    }

    /// Parents first, each placing its children from its own rect.
    /// A float that escapes its clips moves as little as keeps it inside the
    /// window, so a menu by the window's edge stays readable.
    fn place(&mut self, scale: f32) {
        let window = self.nodes[0].rect;
        for index in 0..self.nodes.len() {
            let node = &self.nodes[index];
            if node.end == index + 1 {
                continue;
            }
            let parent = node.rect;
            let element = node.element;
            let main = main(node);
            let cross = 1 - main;
            let [left, top, right, bottom] = element.padding;
            let origin = [
                parent.x + left + element.offset[0],
                parent.y + top + element.offset[1],
            ];
            let inner = [parent.w - left - right, parent.h - top - bottom];
            let mut used = 0.;
            let mut count = 0u16;
            for child in flow(&self.nodes, index) {
                used += self.nodes[child].size[main];
                count += 1;
            }
            used += element.gap * f32::from(count.saturating_sub(1));
            let mut pen = share(element.align[main], inner[main] - used);
            let mut across = 0f32;
            self.each_child(index, |child| {
                let size = child.size;
                let at = match child.element.float {
                    Some(Anchor::Parent {
                        parent: on,
                        own,
                        offset,
                        clipped,
                    }) => {
                        let start = [parent.x, parent.y];
                        let extent = [parent.w, parent.h];
                        let at = std::array::from_fn(|axis| {
                            start[axis] + point(on[axis], extent[axis])
                                - point(own[axis], size[axis])
                                + offset[axis]
                        });
                        if clipped {
                            at
                        } else {
                            inside(at, size, window)
                        }
                    }
                    Some(Anchor::At(at)) => inside(at, size, window),
                    None => {
                        let mut at = [0.; 2];
                        at[main] = origin[main] + pen;
                        at[cross] =
                            origin[cross] + share(element.align[cross], inner[cross] - size[cross]);
                        pen += size[main] + element.gap;
                        across = across.max(size[cross]);
                        at
                    }
                };
                child.rect = snap(at, size, scale);
            });
            let node = &mut self.nodes[index];
            node.used[main] = used;
            node.used[cross] = across;
        }
    }
}

impl Layout {
    /// Runs `f` on each of `parent`'s children in order.
    fn each_child(&mut self, parent: usize, mut f: impl FnMut(&mut Node)) {
        let end = self.nodes[parent].end;
        let mut child = parent + 1;
        while child < end {
            let node = &mut self.nodes[child];
            f(node);
            child = node.end;
        }
    }
}

/// The axis `node` stacks its children along.
const fn main(node: &Node) -> usize {
    match node.element.direction {
        Direction::LeftToRight => 0,
        Direction::TopToBottom => 1,
    }
}

fn padding(node: &Node, axis: usize) -> f32 {
    node.element.padding[axis] + node.element.padding[axis + 2]
}

/// How far into `free` space the children start. None when they overflow,
/// so the start stays in view.
fn share(align: Align, free: f32) -> f32 {
    match align {
        Align::Start => 0.,
        Align::Center => (free / 2.).max(0.),
        Align::End => free.max(0.),
    }
}

/// How far along a `length` an attach point is.
fn point(align: Align, length: f32) -> f32 {
    match align {
        Align::Start => 0.,
        Align::Center => length / 2.,
        Align::End => length,
    }
}

/// The box `at` and `size`, its edges moved to whole physical pixels. Boxes
/// that share an edge round it the same way, so no gap opens between them.
/// `at` moved as little as keeps a box of `size` there inside `window`, its
/// top left kept in when it's too big to fit.
fn inside(at: [f32; 2], size: [f32; 2], window: Rect) -> [f32; 2] {
    let start = [window.x, window.y];
    let end = [window.x + window.w, window.y + window.h];
    std::array::from_fn(|axis| at[axis].min(end[axis] - size[axis]).max(start[axis]))
}

fn snap(at: [f32; 2], size: [f32; 2], scale: f32) -> Rect {
    let edge = |value: f32| (value * scale).round() / scale;
    let [x, y] = at.map(edge);
    Rect::new(x, y, edge(at[0] + size[0]) - x, edge(at[1] + size[1]) - y)
}

/// The indices of `parent`'s children.
pub(super) fn children(nodes: &[Node], parent: usize) -> impl Iterator<Item = usize> + '_ {
    let end = nodes[parent].end;
    let mut next = parent + 1;
    std::iter::from_fn(move || {
        let child = next;
        (child < end).then(|| {
            next = nodes[child].end;
            child
        })
    })
}

/// The children that take space in `parent`, leaving out floating ones.
fn flow(nodes: &[Node], parent: usize) -> impl Iterator<Item = usize> + '_ {
    children(nodes, parent).filter(|&child| nodes[child].element.float.is_none())
}
