//! What the app declares its boxes through, one pass at a time.
//!
//! A [`Ui`] borrows what the host keeps between passes, and stands in one open box.
//!
//! Each child's id is its salt hashed with that box's id, so a salt only has to be
//! unique among its siblings.

use std::hash::Hash;

use crate::canvas::{Color, Glyphs, IconId, Quad, Rect, Vertex};
use crate::input::{Cursor, Input, Out};
use crate::layout::{Element, Id, Layout, TextStyle};

pub struct Ui<'a> {
    /// The open box's.
    id: Id,
    layout: &'a mut Layout,
    glyphs: &'a mut Glyphs,
    input: &'a Input,
    out: &'a mut Out,
}

/// Puts shapes into a custom box, at last pass's rect, from [`Ui::custom`].
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

impl<'a> Ui<'a> {
    /// In the window's box, which `layout` has open.
    pub(crate) fn root(
        layout: &'a mut Layout,
        glyphs: &'a mut Glyphs,
        input: &'a Input,
        out: &'a mut Out,
    ) -> Self {
        Self {
            id: Id::ROOT,
            layout,
            glyphs,
            input,
            out,
        }
    }

    /// What the user did since the last pass.
    #[must_use]
    pub fn input(&self) -> &'a Input {
        self.input
    }

    pub fn cursor(&mut self, cursor: Cursor) {
        self.out.cursor = cursor;
    }

    /// The rect last pass gave the box declared next, if it has `salt`.
    pub fn peek(&mut self, salt: impl Hash) -> Option<Rect> {
        self.layout.peek(self.id.child(salt))
    }

    /// A box, with what `body` declares as its children. Returns its rect last
    /// pass.
    pub fn element(
        &mut self,
        salt: impl Hash,
        element: Element,
        body: impl FnOnce(&mut Self),
    ) -> Option<Rect> {
        let id = self.id.child(salt);
        let rect = self.layout.open(id, element);
        let parent = std::mem::replace(&mut self.id, id);
        body(self);
        self.id = parent;
        self.layout.close();
        rect
    }

    /// A line of text, as wide as it's shaped.
    pub fn text(&mut self, salt: impl Hash, style: TextStyle, text: &str) -> Option<Rect> {
        self.layout
            .text(self.glyphs, self.id.child(salt), style, text)
    }

    /// `icon` in a square `size` logical pixels wide.
    pub fn icon(&mut self, salt: impl Hash, icon: IconId, size: f32, color: Color) -> Option<Rect> {
        self.layout.icon(self.id.child(salt), icon, size, color)
    }

    /// A box its owner paints into. Use elements first, as [`Painter`] says.
    pub fn custom(&mut self, salt: impl Hash, element: Element) -> Painter<'_> {
        let (node, rect) = self.layout.custom(self.id.child(salt), element);
        Painter {
            rect,
            layout: self.layout,
            node,
        }
    }
}

impl Painter<'_> {
    pub fn quad(&mut self, quad: Quad) {
        self.layout.paint_quad(self.node, quad);
    }

    /// As [`Canvas::triangles`](crate::canvas::Canvas::triangles).
    pub fn triangles(&mut self, vertices: &[Vertex], indices: &[u32]) {
        self.layout.paint_triangles(self.node, vertices, indices);
    }
}
