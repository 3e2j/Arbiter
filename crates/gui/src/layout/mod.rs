//! Boxes described by properties, not by position.
//! Each frame the app declares every box on screen again, a pass (see [`crate::host`]),
//! and the solver places them from the window down, so a resize can't leave a
//! box behind or make two overlap.
//!
//! - Boxes are kept flat in declaration order, each knowing where its subtree
//!   ends, so walking the tree is walking an array.
//! - Draw order is declaration order. Floating boxes draw after the whole
//!   tree, outside their parent's clip, in the order they were declared.
//! - A pass reads last pass's rects, since its own aren't solved yet.
//!   A box is the one from last pass with the same parent and the same [`Slot`].

mod emit;
mod solve;

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::panic::Location;

use crate::canvas::{Color, FontId, Glyphs, IconId, Line, Quad, Rect, Vertex};
use crate::input::Cursor;

/// How big a box is along one axis.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Size {
    Fixed(f32),
    /// Just big enough for its children and padding.
    Fit,
    /// Fits, then takes an equal share of what its parent has left along the
    /// parent's direction, or all of it across. A box that clips starts from
    /// its padding instead of fitting, so its children can't make it bigger.
    Grow,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    LeftToRight,
    TopToBottom,
}

/// Where children sit in the space their box has left.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Where a floating box goes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Anchor {
    /// A point on its parent's rect meets a point on its own, each picked per
    /// axis, then it shifts by `offset`. The parent's padding and scroll offset
    /// don't move it.
    Parent {
        parent: [Align; 2],
        own: [Align; 2],
        offset: [f32; 2],
    },
    /// Its top left at a point, such as the pointer.
    At([f32; 2]),
}

/// Drawn inside the box, so it never changes the box's size.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Border {
    pub width: f32,
    pub color: Color,
}

/// What a box is.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Element {
    /// Which way its children are stacked.
    pub direction: Direction,
    /// Width, then height.
    pub size: [Size; 2],
    pub min: [f32; 2],
    /// Left, top, right, bottom.
    pub padding: [f32; 4],
    /// Between children.
    pub gap: f32,
    /// Its children across, then down.
    pub align: [Align; 2],
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub radius: f32,
    /// Cuts its children off at its edge. A [`Size::Grow`] box that clips
    /// doesn't fit them.
    pub clip: bool,
    /// Shifts its children, for scrolling.
    pub offset: [f32; 2],
    /// Drawn after the whole tree, outside its parent's clip, and left out of
    /// its parent's size.
    pub float: Option<Anchor>,
    /// The pointer's shape over it, unless a box drawn later over the same spot
    /// sets its own.
    pub cursor: Option<Cursor>,
}

/// Which of its parent's children a box is. A box is the one last pass
/// declared under the same parent with an equal slot, compared exactly.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slot {
    /// The line that declared it.
    pub site: &'static Location<'static>,
    /// How many boxes that line declared before it under the same parent, or
    /// its key.
    pub n: u32,
    /// Among boxes sharing a key, how many came before it.
    pub dup: u32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TextStyle {
    pub font: FontId,
    /// In logical pixels.
    pub size: f32,
    pub color: Color,
}

/// One pass's boxes, and last pass's for their rects. Kept between passes,
/// so a pass allocates nothing once warm.
#[derive(Default)]
pub(crate) struct Layout {
    /// In declaration order. The first is the window.
    nodes: Vec<Node>,
    last: Vec<Node>,
    /// Indexed by [`Content::Text`].
    texts: Vec<Text>,
    /// Taken by the node that matches the one holding it.
    last_texts: Vec<Option<Text>>,
    /// Last pass's index for each parent and slot, built the first time a pass
    /// misses.
    last_slots: HashMap<(usize, Slot), usize>,
    mapped: bool,
    /// Whether a box isn't at its index last pass.
    shifted: bool,
    /// The boxes opened and not yet closed, with the node each matches last
    /// pass.
    open: Vec<(usize, Option<usize>)>,
    /// What custom boxes drew, indexed by [`Content::Custom`].
    paints: Vec<Paint>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    /// Scratch for [`Self::emit`].
    floats: Vec<usize>,
    clips: Vec<(usize, Rect)>,
}

struct Node {
    /// The window's box is its own.
    parent: usize,
    slot: Slot,
    element: Element,
    content: Content,
    /// One past its last descendant. Its first child is right after it, and
    /// each next one where the one before ends.
    end: usize,
    size: [f32; 2],
    /// How far its children reach along each axis, without padding.
    used: [f32; 2],
    rect: Rect,
}

#[derive(Clone)]
enum Content {
    Box,
    Text(usize),
    Icon(IconId, f32, Color),
    /// A range of [`Layout::paints`].
    Custom(Range<usize>),
}

struct Text {
    line: Line,
    style: TextStyle,
    /// From its top down to the baseline.
    ascent: f32,
    size: [f32; 2],
}

#[derive(Clone)]
enum Paint {
    Quad(Quad),
    Triangles {
        vertices: Range<usize>,
        indices: Range<usize>,
    },
}

impl Anchor {
    /// Its top left at its parent's bottom left.
    pub const BELOW: Self = Self::Parent {
        parent: [Align::Start, Align::End],
        own: [Align::Start; 2],
        offset: [0.; 2],
    };
    /// Its top left at its parent's top right.
    pub const RIGHT: Self = Self::Parent {
        parent: [Align::End, Align::Start],
        own: [Align::Start; 2],
        offset: [0.; 2],
    };
}

impl Element {
    /// Fits its children, stacked down.
    pub const DEFAULT: Self = Self {
        direction: Direction::TopToBottom,
        size: [Size::Fit; 2],
        min: [0.; 2],
        padding: [0.; 4],
        gap: 0.,
        align: [Align::Start; 2],
        background: None,
        border: None,
        radius: 0.,
        clip: false,
        offset: [0.; 2],
        float: None,
        cursor: None,
    };

    /// Fills its parent and stacks its children left to right.
    #[must_use]
    pub const fn row() -> Self {
        Self {
            direction: Direction::LeftToRight,
            size: [Size::Grow; 2],
            ..Self::DEFAULT
        }
    }

    /// Fills its parent and stacks its children down.
    #[must_use]
    pub const fn column() -> Self {
        Self {
            size: [Size::Grow; 2],
            ..Self::DEFAULT
        }
    }

    /// The same padding on every side.
    #[must_use]
    pub const fn padded(mut self, by: f32) -> Self {
        self.padding = [by; 4];
        self
    }
}

impl Default for Element {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl PartialEq for Slot {
    fn eq(&self, other: &Self) -> bool {
        self.n == other.n
            && self.dup == other.dup
            && (std::ptr::eq(self.site, other.site) || self.site == other.site)
    }
}

impl Eq for Slot {}

impl Hash for Slot {
    /// Leaves out the file, which equal slots share anyway.
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.site.line(), self.site.column(), self.n, self.dup).hash(state);
    }
}

impl Layout {
    /// Starts a pass, keeping this one's boxes as the last.
    pub fn clear(&mut self) {
        std::mem::swap(&mut self.nodes, &mut self.last);
        self.nodes.clear();
        self.last_texts.clear();
        self.last_texts.extend(self.texts.drain(..).map(Some));
        self.last_slots.clear();
        self.mapped = false;
        self.shifted = false;
        self.open.clear();
        self.paints.clear();
        self.vertices.clear();
        self.indices.clear();
        let window = Slot {
            site: Location::caller(),
            n: 0,
            dup: 0,
        };
        self.open.push((0, (!self.last.is_empty()).then_some(0)));
        self.nodes
            .push(Node::new(0, 0, window, Element::DEFAULT, Content::Box));
    }

    /// Opens a box, whose children are declared until [`Self::close`].
    /// Returns its node and its rect last pass.
    pub fn open(&mut self, slot: Slot, element: Element) -> (usize, Option<Rect>) {
        let (index, last) = self.push(slot, element, Content::Box);
        self.open.push((index, last));
        (index, self.last_rect(last))
    }

    /// Closes the box opened last.
    pub fn close(&mut self) {
        // The window's box stays open until the pass is solved.
        if self.open.len() > 1
            && let Some((index, _)) = self.open.pop()
        {
            let end = self.nodes.len();
            if let Some(node) = self.nodes.get_mut(index) {
                node.end = end;
            }
        }
    }

    /// How far the open box's children reached last pass along each axis,
    /// without its padding.
    pub fn content(&self) -> Option<[f32; 2]> {
        let (_, last) = self.open.last()?;
        Some(self.last.get((*last)?)?.used)
    }

    /// What `node` was declared as, to change before the pass is solved.
    pub fn element_mut(&mut self, node: usize) -> &mut Element {
        &mut self.nodes[node].element
    }

    /// A line of text, as wide as it's shaped. The line shaped last pass is
    /// kept while its text and style are the same.
    pub fn text(
        &mut self,
        glyphs: &mut Glyphs,
        slot: Slot,
        style: TextStyle,
        text: &str,
    ) -> Option<Rect> {
        let last = self.matches(slot);
        let held = last
            .and_then(|last| match self.last[last].content {
                Content::Text(at) => self.last_texts.get_mut(at).and_then(Option::take),
                _ => None,
            })
            .filter(|held| {
                held.style.size.to_bits() == style.size.to_bits()
                    && glyphs.holds_line(&held.line, style.font, style.size, text)
            });
        let text = if let Some(held) = held {
            Text { style, ..held }
        } else {
            let line = glyphs.shape_line(style.font, style.size, text);
            let metrics = glyphs.line_metrics(style.font, style.size);
            let size = [glyphs.line_width(&line), metrics.ascent + metrics.descent];
            Text {
                line,
                style,
                ascent: metrics.ascent,
                size,
            }
        };
        self.texts.push(text);
        let at = self.texts.len() - 1;
        let (_, last) = self.push(slot, Element::DEFAULT, Content::Text(at));
        self.last_rect(last)
    }

    /// `icon` in a square `size` logical pixels wide.
    pub fn icon(&mut self, slot: Slot, icon: IconId, size: f32, color: Color) -> Option<Rect> {
        let element = Element {
            size: [Size::Fixed(size); 2],
            ..Element::DEFAULT
        };
        let (_, last) = self.push(slot, element, Content::Icon(icon, size, color));
        self.last_rect(last)
    }

    /// A box painted into through [`Self::paint_quad`] and
    /// [`Self::paint_triangles`], until the next box is declared. Returns its
    /// node and its rect last pass.
    pub fn custom(&mut self, slot: Slot, element: Element) -> (usize, Option<Rect>) {
        let start = self.paints.len();
        let (index, last) = self.push(slot, element, Content::Custom(start..start));
        (index, self.last_rect(last))
    }

    pub fn paint_quad(&mut self, node: usize, quad: Quad) {
        self.paints.push(Paint::Quad(quad));
        self.grow_paints(node);
    }

    pub fn paint_triangles(&mut self, node: usize, vertices: &[Vertex], indices: &[u32]) {
        let v = self.vertices.len();
        let i = self.indices.len();
        self.vertices.extend_from_slice(vertices);
        self.indices.extend_from_slice(indices);
        self.paints.push(Paint::Triangles {
            vertices: v..self.vertices.len(),
            indices: i..self.indices.len(),
        });
        self.grow_paints(node);
    }

    fn grow_paints(&mut self, node: usize) {
        let end = self.paints.len();
        if let Some(Content::Custom(range)) = self.nodes.get_mut(node).map(|node| &mut node.content)
        {
            range.end = end;
        }
    }

    /// Adds a box under the open one. Returns its index, and the node it
    /// matches last pass.
    fn push(&mut self, slot: Slot, element: Element, content: Content) -> (usize, Option<usize>) {
        let index = self.nodes.len();
        let last = self.matches(slot);
        self.shifted |= last != Some(index);
        let parent = self.open.last().map_or(0, |&(parent, _)| parent);
        self.nodes
            .push(Node::new(index, parent, slot, element, content));
        (index, last)
    }

    /// The node last pass that a box declared next with `slot` is: the one at
    /// its index, or else wherever it was.
    fn matches(&mut self, slot: Slot) -> Option<usize> {
        let index = self.nodes.len();
        let parent = self.open.last().and_then(|&(_, last)| last)?;
        if self
            .last
            .get(index)
            .is_some_and(|node| node.parent == parent && node.slot == slot)
        {
            return Some(index);
        }
        if !self.mapped {
            self.mapped = true;
            // The window's box is matched by `clear`, and has no parent.
            let slots = self.last.iter().map(|node| (node.parent, node.slot));
            self.last_slots.extend(slots.zip(0..).skip(1));
        }
        self.last_slots.get(&(parent, slot)).copied()
    }

    fn last_rect(&self, last: Option<usize>) -> Option<Rect> {
        last.map(|last| self.last[last].rect)
    }
}

impl Node {
    /// A box at `index`, with no children yet.
    fn new(index: usize, parent: usize, slot: Slot, element: Element, content: Content) -> Self {
        Self {
            parent,
            slot,
            element,
            content,
            end: index + 1,
            size: [0.; 2],
            used: [0.; 2],
            rect: Rect::default(),
        }
    }
}

#[cfg(test)]
mod tests;
