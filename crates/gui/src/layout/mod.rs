//! Boxes described by properties, not by position.
//! Each frame the app declares every box on screen again, a pass (see [`crate::host`]),
//! and the solver places them from the window down, so a resize can't leave a
//! box behind or make two overlap.
//!
//! - Boxes are kept flat in declaration order, each knowing where its subtree
//!   ends, so walking the tree is walking an array.
//! - Draw order is declaration order. Floating boxes draw after the whole
//!   tree, outside their parent's clip, in the order they were declared.
//! - A pass reads last pass's rects, since its own aren't solved yet. Each
//!   node is matched to last pass's node at the same index, or by id when
//!   that index holds a different id, such as after an insert.

mod emit;
mod solve;

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use crate::canvas::{Color, FontId, Glyphs, IconId, Line, Quad, Rect, Vertex};

/// How big a box is along one axis.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Size {
    Fixed(f32),
    /// Just big enough for its children and padding.
    Fit,
    /// Fits, then takes an equal share of what its parent has left along the
    /// parent's direction, or all of it across.
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

/// Where a floating box's top left corner goes.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Anchor {
    /// At its parent's bottom left.
    Below,
    /// At its parent's top right.
    Right,
    /// At a point, such as the pointer.
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
    /// Cuts its children off at its edge.
    pub clip: bool,
    /// Shifts its children, for scrolling.
    pub offset: [f32; 2],
    /// Drawn after the whole tree, outside its parent's clip, and left out of
    /// its parent's size.
    pub float: Option<Anchor>,
}

/// Which box an element is, from its parent's id and a salt the parent
/// gives it, so it stays the same between passes.
// TODO: a `Ui` derives ids from call sites, so apps don't write salts they
// never read. Data lists that reorder still pass a key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Id(u64);

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
pub struct Layout {
    /// In declaration order. The first is the window.
    nodes: Vec<Node>,
    last: Vec<Node>,
    /// Indexed by [`Content::Text`].
    texts: Vec<Text>,
    /// Taken by the node that matches the one holding it.
    last_texts: Vec<Option<Text>>,
    /// Last pass's index for each id, built the first time a pass misses.
    last_ids: HashMap<Id, usize>,
    mapped: bool,
    /// The boxes opened and not yet closed.
    open: Vec<usize>,
    /// What custom boxes drew, indexed by [`Content::Custom`].
    paints: Vec<Paint>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    /// Scratch for [`Self::emit`].
    floats: Vec<usize>,
    clips: Vec<(usize, Rect)>,
}

/// Puts shapes into a custom box, at last pass's rect, from [`Layout::custom`].
///
/// Only for content whose positions come from data rather than from layout,
/// such as music notes on a timeline or nodes in a graph.
///
/// Anything that could be a row of text and boxes should be elements, which get
/// layout, clipping and hit testing for free.
pub struct Painter<'a> {
    /// `None` the first pass it's declared.
    pub rect: Option<Rect>,
    layout: &'a mut Layout,
    node: usize,
}

struct Node {
    id: Id,
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

impl Id {
    /// The window's box, which every pass starts in.
    pub const ROOT: Self = Self(0);

    #[must_use]
    pub fn child(self, salt: impl Hash) -> Self {
        let mut hasher = DefaultHasher::new();
        (self.0, salt).hash(&mut hasher);
        Self(hasher.finish())
    }
}

impl Layout {
    /// Starts a pass, keeping this one's boxes as the last.
    pub fn clear(&mut self) {
        std::mem::swap(&mut self.nodes, &mut self.last);
        self.nodes.clear();
        self.last_texts.clear();
        self.last_texts.extend(self.texts.drain(..).map(Some));
        self.last_ids.clear();
        self.mapped = false;
        self.open.clear();
        self.paints.clear();
        self.vertices.clear();
        self.indices.clear();
        self.open.push(0);
        self.push(Id::ROOT, Element::DEFAULT, Content::Box);
    }

    /// Opens a box, whose children are declared until [`Self::close`].
    /// Returns its rect last pass.
    pub fn open(&mut self, id: Id, element: Element) -> Option<Rect> {
        let (index, rect) = self.push(id, element, Content::Box);
        self.open.push(index);
        rect
    }

    /// Closes the box opened last.
    pub fn close(&mut self) {
        // The window's box stays open until the pass is solved.
        if self.open.len() > 1
            && let Some(index) = self.open.pop()
        {
            let end = self.nodes.len();
            if let Some(node) = self.nodes.get_mut(index) {
                node.end = end;
            }
        }
    }

    /// The rect last pass gave the box declared next, if it has `id`.
    pub fn peek(&mut self, id: Id) -> Option<Rect> {
        let index = self.nodes.len();
        self.last_index(index, id).map(|last| self.last[last].rect)
    }

    /// A line of text, as wide as it's shaped. The line shaped last pass is
    /// kept while its text and style are the same.
    pub fn text(
        &mut self,
        glyphs: &mut Glyphs,
        id: Id,
        style: TextStyle,
        text: &str,
    ) -> Option<Rect> {
        let index = self.nodes.len();
        let held = self
            .last_index(index, id)
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
        self.push(id, Element::DEFAULT, Content::Text(at)).1
    }

    /// `icon` in a square `size` logical pixels wide.
    pub fn icon(&mut self, id: Id, icon: IconId, size: f32, color: Color) -> Option<Rect> {
        let element = Element {
            size: [Size::Fixed(size); 2],
            ..Element::DEFAULT
        };
        self.push(id, element, Content::Icon(icon, size, color)).1
    }

    /// A box its owner paints into with a [`Painter`]. Use elements first, as
    /// the painter says.
    pub fn custom(&mut self, id: Id, element: Element) -> Painter<'_> {
        let start = self.paints.len();
        let (node, rect) = self.push(id, element, Content::Custom(start..start));
        Painter {
            rect,
            layout: self,
            node,
        }
    }

    fn push(&mut self, id: Id, element: Element, content: Content) -> (usize, Option<Rect>) {
        let index = self.nodes.len();
        let rect = self.last_index(index, id).map(|last| self.last[last].rect);
        self.nodes.push(Node {
            id,
            element,
            content,
            end: index + 1,
            size: [0.; 2],
            used: [0.; 2],
            rect: Rect::default(),
        });
        (index, rect)
    }

    /// The node last pass that `id` at `index` matches.
    fn last_index(&mut self, index: usize, id: Id) -> Option<usize> {
        if self.last.get(index).is_some_and(|node| node.id == id) {
            return Some(index);
        }
        if !self.mapped {
            self.mapped = true;
            self.last_ids
                .extend(self.last.iter().map(|node| node.id).zip(0..));
        }
        self.last_ids.get(&id).copied()
    }
}

impl Painter<'_> {
    pub fn quad(&mut self, quad: Quad) {
        self.layout.paints.push(Paint::Quad(quad));
        self.grow();
    }

    /// As [`Canvas::triangles`](crate::canvas::Canvas::triangles).
    pub fn triangles(&mut self, vertices: &[Vertex], indices: &[u32]) {
        let layout = &mut *self.layout;
        let v = layout.vertices.len();
        let i = layout.indices.len();
        layout.vertices.extend_from_slice(vertices);
        layout.indices.extend_from_slice(indices);
        layout.paints.push(Paint::Triangles {
            vertices: v..layout.vertices.len(),
            indices: i..layout.indices.len(),
        });
        self.grow();
    }

    fn grow(&mut self) {
        let end = self.layout.paints.len();
        if let Some(Content::Custom(range)) = self
            .layout
            .nodes
            .get_mut(self.node)
            .map(|node| &mut node.content)
        {
            range.end = end;
        }
    }
}

#[cfg(test)]
mod tests;
