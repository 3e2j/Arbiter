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

use std::collections::HashMap;
use std::panic::Location;

use crate::canvas::{Color, Glyphs, IconId, Quad, Rect, Vertex};
use crate::input::{Button, Cursor, Input, Out};
use crate::layout::{Layout, Slot};

pub use crate::layout::{Align, Anchor, Border, Direction, Element, Size, TextStyle};

pub struct Ui<'a> {
    open: Open,
    layout: &'a mut Layout,
    glyphs: &'a mut Glyphs,
    input: &'a Input,
    out: &'a mut Out,
    slots: &'a mut Slots,
}

/// What a pass needs to give each box its own [`Slot`]: the line that declared
/// it, and how many boxes that line has declared in the same parent or its key.
/// Kept between passes, so a pass allocates nothing once warm.
#[derive(Default)]
pub(crate) struct Slots {
    /// How many boxes each line has declared in each open box, innermost last.
    sites: Vec<(&'static Location<'static>, u32)>,
    /// How many boxes each key has had in each parent, so a key given twice
    /// still makes two.
    keys: HashMap<(usize, Slot), u32>,
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
    node: usize,
    /// Its rect last pass.
    rect: Option<Rect>,
    /// Where its children's lines start in [`Slots::sites`].
    sites: usize,
}

impl<'a> Ui<'a> {
    /// In the window's box, which `layout` has open.
    pub(crate) fn root(
        layout: &'a mut Layout,
        glyphs: &'a mut Glyphs,
        input: &'a Input,
        out: &'a mut Out,
        slots: &'a mut Slots,
    ) -> Self {
        slots.sites.clear();
        slots.keys.clear();
        Self {
            open: Open {
                node: 0,
                rect: None,
                sites: 0,
            },
            layout,
            glyphs,
            input,
            out,
            slots,
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

    /// The open box's rect last pass. `None` the first pass it's declared.
    #[must_use]
    pub const fn rect(&self) -> Option<Rect> {
        self.open.rect
    }

    /// How far the open box's children reached last pass along each axis,
    /// without its padding.
    #[must_use]
    pub fn content(&self) -> Option<[f32; 2]> {
        self.layout.content()
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
        let slot = self.next_slot(Location::caller());
        self.open(slot, element, body)
    }

    /// A box found by `key` rather than by count, so its state follows it when
    /// the list it's in inserts or reorders. A key given twice falls back to
    /// its order among the boxes sharing it.
    #[track_caller]
    pub fn keyed<R>(&mut self, key: u32, element: Element, body: impl FnOnce(&mut Self) -> R) -> R {
        let slot = Slot {
            site: Location::caller(),
            n: key,
            dup: 0,
        };
        let dups = self.slots.keys.entry((self.open.node, slot)).or_insert(0);
        let slot = Slot { dup: *dups, ..slot };
        *dups += 1;
        self.open(slot, element, body)
    }

    /// A line of text, as wide as it's shaped. Returns its rect last pass.
    #[track_caller]
    pub fn text(&mut self, style: TextStyle, text: &str) -> Option<Rect> {
        let slot = self.next_slot(Location::caller());
        self.layout.text(self.glyphs, slot, style, text)
    }

    /// `icon` in a square `size` logical pixels wide. Returns its rect last
    /// pass.
    #[track_caller]
    pub fn icon(&mut self, icon: IconId, size: f32, color: Color) -> Option<Rect> {
        let slot = self.next_slot(Location::caller());
        self.layout.icon(slot, icon, size, color)
    }

    /// A box its owner paints into. Use elements first, as [`Painter`] says.
    #[track_caller]
    pub fn custom(&mut self, element: Element) -> Painter<'_> {
        let slot = self.next_slot(Location::caller());
        let (node, rect) = self.layout.custom(slot, element);
        Painter {
            rect,
            layout: self.layout,
            node,
        }
    }

    fn open<R>(&mut self, slot: Slot, element: Element, body: impl FnOnce(&mut Self) -> R) -> R {
        let (node, rect) = self.layout.open(slot, element);
        let open = Open {
            node,
            rect,
            sites: self.slots.sites.len(),
        };
        let parent = std::mem::replace(&mut self.open, open);
        let out = body(self);
        self.leave(parent);
        self.layout.close();
        out
    }

    fn next_slot(&mut self, site: &'static Location<'static>) -> Slot {
        let sites = &mut self.slots.sites;
        let start = self.open.sites.min(sites.len());
        let n = if let Some((_, count)) = sites
            .get_mut(start..)
            .and_then(|open| open.iter_mut().find(|(at, _)| *at == site))
        {
            *count += 1;
            *count
        } else {
            sites.push((site, 0));
            0
        };
        Slot { site, n, dup: 0 }
    }

    fn leave(&mut self, parent: Open) {
        self.slots.sites.truncate(self.open.sites);
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
        slots: Slots,
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
                &mut self.slots,
            );
            let r = body(&mut ui);
            self.layout.solve(WINDOW, 1.);
            r
        }
    }

    /// The open box's rect last pass.
    fn rect(ui: &mut Ui) -> Option<Rect> {
        ui.rect()
    }

    #[test]
    fn a_box_keeps_its_slot_when_one_is_inserted_before_it() {
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
    fn boxes_from_one_line_each_get_their_own_slot() {
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
    fn keyed_boxes_keep_their_slots_when_reordered() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept, keys: [(u32, f32); 2]| {
            kept.pass(|ui| keys.map(|(key, h)| ui.keyed(key, fixed(10., h), rect)))
        };
        pass(&mut kept, [(0, 10.), (1, 20.)]);
        let [b, a] = pass(&mut kept, [(1, 20.), (0, 10.)]);
        assert_eq!(b, Some(Rect::new(0., 10., 10., 20.)));
        assert_eq!(a, Some(Rect::new(0., 0., 10., 10.)));
    }

    #[test]
    fn boxes_sharing_a_key_each_get_their_own_slot() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept, insert: bool| {
            kept.pass(|ui| {
                // Moves the items off their last index, so they're found by slot.
                if insert {
                    ui.element(fixed(10., 30.), |_| ());
                }
                [10., 20.].map(|h| ui.keyed(7, fixed(10., h), rect))
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
