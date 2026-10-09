//! Declaring the interface. Every pass, the app describes its boxes through a
//! [`Ui`], and the layout places them.
//!
//! ```ignore
//! ui.element(Element::row(), |ui| {
//!     if ui.pressed() {
//!         // ...
//!     }
//!     ui.text(style, "Open");
//! });
//! ```
//!
//! Boxes are never named, but two things keep their state with them:
//! - Each item of a list that can insert or reorder goes in [`Ui::keyed`].
//! - A component called from several places can take `#[track_caller]`.

use std::collections::HashSet;
use std::hash::Hash;
use std::panic::Location;

use crate::canvas::{Color, Glyphs, IconId, Quad, Rect, Vertex};
use crate::input::{Button, Cursor, Input, Out};
use crate::layout::{Id, Layout};

pub use crate::layout::{Align, Anchor, Border, Direction, Element, Size, TextStyle};

pub struct Ui<'a> {
    open: Open,
    layout: &'a mut Layout,
    glyphs: &'a mut Glyphs,
    input: &'a Input,
    out: &'a mut Out,
    ids: &'a mut Ids,
}

/// What a pass needs to give each box its own id: its parent's, hashed with the
/// line that declared it and how many boxes that line has declared in the same
/// parent. Kept between passes, so a pass allocates nothing once warm.
#[derive(Default)]
pub(crate) struct Ids {
    /// How many boxes each line has declared in each open box, innermost last.
    sites: Vec<(&'static Location<'static>, u32)>,
    /// Every key scope this pass, so a key given twice still makes two.
    keyed: HashSet<Id>,
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

/// The box a [`Ui`] stands in.
#[derive(Clone, Copy)]
struct Open {
    id: Id,
    node: usize,
    /// Its rect last pass.
    rect: Option<Rect>,
    /// Where its children's lines start in [`Ids::sites`].
    sites: usize,
}

impl<'a> Ui<'a> {
    /// In the window's box, which `layout` has open.
    pub(crate) fn root(
        layout: &'a mut Layout,
        glyphs: &'a mut Glyphs,
        input: &'a Input,
        out: &'a mut Out,
        ids: &'a mut Ids,
    ) -> Self {
        ids.sites.clear();
        ids.keyed.clear();
        Self {
            open: Open {
                id: Id::ROOT,
                node: 0,
                rect: None,
                sites: 0,
            },
            layout,
            glyphs,
            input,
            out,
            ids,
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

    /// Whether the pointer is over the open box, by its rect last pass.
    #[must_use]
    pub fn hovered(&self) -> bool {
        self.open
            .rect
            .zip(self.input.pointer())
            .is_some_and(|(rect, at)| rect.contains(at))
    }

    /// Whether the left button went down over the open box.
    #[must_use]
    pub fn pressed(&self) -> bool {
        self.hovered() && self.input.pressed(Button::Left)
    }

    /// The open box, to change from what its queries say.
    pub fn style(&mut self) -> &mut Element {
        self.layout.element_mut(self.open.node)
    }

    /// A box, with what `body` declares as its children.
    #[track_caller]
    pub fn element<R>(&mut self, element: Element, body: impl FnOnce(&mut Self) -> R) -> R {
        let id = self.next_id(Location::caller());
        let (node, rect) = self.layout.open(id, element);
        let parent = self.enter(Open {
            id,
            node,
            rect,
            sites: 0,
        });
        let out = body(self);
        self.leave(parent);
        self.layout.close();
        out
    }

    /// Declares `body`'s boxes under `key` rather than by count, so they keep
    /// their ids when the list they're in inserts or reorders. Adds no box. A
    /// key given twice falls back to its order among the items sharing it.
    #[track_caller]
    pub fn keyed<R>(&mut self, key: impl Hash, body: impl FnOnce(&mut Self) -> R) -> R {
        let keyed = self.open.id.child((Location::caller(), key));
        let mut id = keyed;
        let mut seen = 0u32;
        while !self.ids.keyed.insert(id) {
            seen += 1;
            id = keyed.child(seen);
        }
        let parent = self.enter(Open { id, ..self.open });
        let out = body(self);
        self.leave(parent);
        out
    }

    /// A line of text, as wide as it's shaped. Returns its rect last pass.
    #[track_caller]
    pub fn text(&mut self, style: TextStyle, text: &str) -> Option<Rect> {
        let id = self.next_id(Location::caller());
        self.layout.text(self.glyphs, id, style, text)
    }

    /// `icon` in a square `size` logical pixels wide. Returns its rect last
    /// pass.
    #[track_caller]
    pub fn icon(&mut self, icon: IconId, size: f32, color: Color) -> Option<Rect> {
        let id = self.next_id(Location::caller());
        self.layout.icon(id, icon, size, color)
    }

    /// A box its owner paints into. Use elements first, as [`Painter`] says.
    #[track_caller]
    pub fn custom(&mut self, element: Element) -> Painter<'_> {
        let id = self.next_id(Location::caller());
        let (node, rect) = self.layout.custom(id, element);
        Painter {
            rect,
            layout: self.layout,
            node,
        }
    }

    fn next_id(&mut self, site: &'static Location<'static>) -> Id {
        let sites = &mut self.ids.sites;
        let start = self.open.sites.min(sites.len());
        let count = if let Some((_, count)) = sites
            .get_mut(start..)
            .and_then(|open| open.iter_mut().find(|(at, _)| *at == site))
        {
            *count += 1;
            *count
        } else {
            sites.push((site, 0));
            0
        };
        self.open.id.child((site, count))
    }

    /// Makes `open` the open box, returning the one it's under.
    fn enter(&mut self, open: Open) -> Open {
        let sites = self.ids.sites.len();
        std::mem::replace(&mut self.open, Open { sites, ..open })
    }

    fn leave(&mut self, parent: Open) {
        self.ids.sites.truncate(self.open.sites);
        self.open = parent;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Event;

    const WINDOW: Rect = Rect::new(0., 0., 300., 100.);

    fn fixed(w: f32, h: f32) -> Element {
        Element {
            size: [Size::Fixed(w), Size::Fixed(h)],
            ..Element::DEFAULT
        }
    }

    /// What the host keeps between passes.
    #[derive(Default)]
    struct Kept {
        layout: Layout,
        glyphs: Glyphs,
        input: Input,
        ids: Ids,
    }

    impl Kept {
        /// Runs one pass and solves it, returning what `body` did.
        fn pass<R>(&mut self, body: impl FnOnce(&mut Ui) -> R) -> R {
            self.layout.clear();
            let mut out = Out::default();
            let mut ui = Ui::root(
                &mut self.layout,
                &mut self.glyphs,
                &self.input,
                &mut out,
                &mut self.ids,
            );
            let r = body(&mut ui);
            self.layout.solve(WINDOW, 1.);
            r
        }
    }

    /// The open box's rect last pass.
    fn rect(ui: &mut Ui) -> Option<Rect> {
        ui.open.rect
    }

    #[test]
    fn a_box_keeps_its_id_when_one_is_inserted_before_it() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept, insert: bool| {
            kept.pass(|ui| {
                if insert {
                    ui.element(fixed(10., 30.), |_| ());
                }
                ui.element(fixed(10., 10.), rect)
            })
        };
        pass(&mut kept, false);
        assert_eq!(pass(&mut kept, true), Some(Rect::new(0., 0., 10., 10.)));
    }

    #[test]
    fn boxes_from_one_line_each_get_their_own_id() {
        let mut kept = Kept::default();
        let pass =
            |kept: &mut Kept| kept.pass(|ui| [10., 20.].map(|h| ui.element(fixed(10., h), rect)));
        pass(&mut kept);
        let rects = pass(&mut kept);
        assert_eq!(
            rects,
            [
                Some(Rect::new(0., 0., 10., 10.)),
                Some(Rect::new(0., 10., 10., 20.)),
            ]
        );
    }

    #[test]
    fn keyed_items_keep_their_ids_when_reordered() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept, keys: [(&str, f32); 2]| {
            kept.pass(|ui| keys.map(|(key, h)| ui.keyed(key, |ui| ui.element(fixed(10., h), rect))))
        };
        pass(&mut kept, [("a", 10.), ("b", 20.)]);
        let [b, a] = pass(&mut kept, [("b", 20.), ("a", 10.)]);
        assert_eq!(b, Some(Rect::new(0., 10., 10., 20.)));
        assert_eq!(a, Some(Rect::new(0., 0., 10., 10.)));
    }

    #[test]
    fn items_sharing_a_key_each_get_their_own_id() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept, insert: bool| {
            kept.pass(|ui| {
                // Moves the items off their last index, so they're found by id.
                if insert {
                    ui.element(fixed(10., 30.), |_| ());
                }
                [10., 20.].map(|h| ui.keyed("same", |ui| ui.element(fixed(10., h), rect)))
            })
        };
        pass(&mut kept, false);
        let rects = pass(&mut kept, true);
        assert_eq!(
            rects,
            [
                Some(Rect::new(0., 0., 10., 10.)),
                Some(Rect::new(0., 10., 10., 20.)),
            ]
        );
    }

    #[test]
    fn style_follows_the_pointer_over_last_pass_rect() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept| {
            kept.pass(|ui| {
                ui.element(fixed(10., 10.), |ui| {
                    let hovered = ui.hovered();
                    ui.style().background = hovered.then_some(Color::hex(0xff_ff_ff));
                    hovered
                })
            })
        };
        kept.input.push(Event::Pointer(Some([5., 5.])));
        // No rect yet, so nothing is under the pointer.
        assert!(!pass(&mut kept));
        assert!(pass(&mut kept));
        assert_eq!(
            kept.layout.element_mut(1).background,
            Some(Color::hex(0xff_ff_ff))
        );
    }
}
