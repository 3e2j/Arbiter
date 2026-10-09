//! Declaring the interface. Every pass, the app describes its boxes through a
//! [`Ui`], and the layout places them.
//!
//! ```ignore
//! ui.element(Element::row(), |ui| {
//!     if ui.pressed(Button::Left) {
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
pub use crate::theme::{Colors, Fonts, Sizes, Theme};

pub struct Ui<'a> {
    open: Open,
    theme: &'a Theme,
    layout: &'a mut Layout,
    glyphs: &'a mut Glyphs,
    input: &'a Input,
    out: &'a mut Out,
    slots: &'a mut Slots,
    memory: &'a mut Memory,
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

/// Which box holds each button, kept between passes. A box takes a button by
/// asking about it in the pass it goes down over the box, and keeps it until
/// it comes up, wherever the pointer goes. Each button is held on its own.
#[derive(Default)]
pub(crate) struct Memory {
    /// Indexed by [`Button::index`].
    // TODO: a short `Vec` keyed by a `Source` enum once keys, touch, gamepad
    // buttons, MIDI can be held too, those found by focus rather than position.
    // Sticks and other axes stay in `Input`, read by the holder or the focus.
    holds: [Hold; Button::ALL.len()],
    /// The box that takes the wheel, by its index last pass: the innermost
    /// under the pointer to ask for it.
    wheel: Option<usize>,
    /// The last declared box this pass to ask for the wheel while hovered.
    wheel_claim: Option<usize>,
    /// Where the pointer was at the end of last pass.
    pointer: Option<[f32; 2]>,
    /// Whether a button changed hands or was let go of.
    changed: bool,
}

#[derive(Clone, Copy, Default)]
struct Hold {
    /// The box holding the button, by its index last pass.
    held: Option<usize>,
    /// The holder's index this pass.
    found: Option<usize>,
    /// The last declared box this pass to ask about a press that went down
    /// over it.
    claim: Option<usize>,
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
    /// The node it matches last pass.
    last: Option<usize>,
    /// Its rect last pass.
    rect: Option<Rect>,
    /// Where its children's lines start in [`Slots::sites`].
    sites: usize,
}

impl<'a> Ui<'a> {
    /// In the window's box, which `layout` has open.
    pub(crate) fn root(
        theme: &'a Theme,
        layout: &'a mut Layout,
        glyphs: &'a mut Glyphs,
        input: &'a Input,
        out: &'a mut Out,
        slots: &'a mut Slots,
        memory: &'a mut Memory,
    ) -> Self {
        slots.sites.clear();
        slots.keys.clear();
        memory.begin();
        layout.hit_test(input.pointer());
        Self {
            open: Open {
                node: 0,
                last: None,
                rect: None,
                sites: 0,
            },
            theme,
            layout,
            glyphs,
            input,
            out,
            slots,
            memory,
        }
    }

    /// What everything is drawn with.
    #[must_use]
    pub const fn theme(&self) -> &'a Theme {
        self.theme
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

    /// Whether the pointer is over the open box, or one of its children,
    /// where last pass drew it on top and inside its clip.
    #[must_use]
    pub fn hovered(&self) -> bool {
        self.open
            .last
            .zip(self.input.pointer())
            .is_some_and(|(last, at)| self.layout.under(last, at))
    }

    /// How far the wheel scrolled over the open box since last pass. Nothing
    /// unless it was the innermost box under the pointer to ask last pass, so
    /// nested scrolling boxes don't all move.
    pub fn wheel(&mut self) -> [f32; 2] {
        if self.hovered() {
            let claim = &mut self.memory.wheel_claim;
            *claim = (*claim).max(Some(self.open.node));
        }
        if self.open.last.is_some() && self.open.last == self.memory.wheel {
            self.input.scroll()
        } else {
            [0.; 2]
        }
    }

    /// Whether `button` went down over the open box.
    #[must_use]
    pub fn pressed(&self, button: Button) -> bool {
        self.hovered() && self.input.pressed(button)
    }

    /// Whether the open box holds `button`: it went down over it and hasn't
    /// come up. Starts the pass after the press, since a box declared later
    /// in the press pass, such as one inside it, takes the button instead.
    pub fn held(&mut self, button: Button) -> bool {
        self.holds(button) && self.input.held(button)
    }

    /// Whether `button` came up over the open box after going down over it,
    /// in the pass it comes up, so the pass run again before drawing shows
    /// what the click changed.
    pub fn clicked(&mut self, button: Button) -> bool {
        self.holds(button) && self.input.released(button) && self.hovered()
    }

    /// How far the pointer moved since last pass while the open box holds
    /// `button`, including the pass it comes up.
    pub fn dragged(&mut self, button: Button) -> Option<[f32; 2]> {
        if !self.holds(button) {
            return None;
        }
        let ([x, y], [from_x, from_y]) = (self.input.pointer()?, self.memory.pointer?);
        Some([x - from_x, y - from_y])
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

    /// How big [`Self::text`] would make `text`, without declaring it. The
    /// line is kept for the frame, so measuring it again is free.
    pub fn measure(&mut self, style: TextStyle, text: &str) -> [f32; 2] {
        let width = self.glyphs.text_width(style.font, style.size, text);
        let metrics = self.glyphs.line_metrics(style.font, style.size);
        [width, metrics.ascent + metrics.descent]
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
        let (node, last) = self.layout.open(slot, element);
        if last.is_some() {
            for hold in &mut self.memory.holds {
                if hold.held == last {
                    hold.found = Some(node);
                }
            }
        }
        let open = Open {
            node,
            last,
            rect: self.layout.last_rect(last),
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

    /// Whether the open box holds `button`, or let go of it this pass. Asks
    /// for it if the button went down over it.
    fn holds(&mut self, button: Button) -> bool {
        let pressed = self.pressed(button);
        let hold = &mut self.memory.holds[button.index()];
        if pressed {
            hold.claim = hold.claim.max(Some(self.open.node));
        }
        self.open.last.is_some() && self.open.last == hold.held
    }

    fn leave(&mut self, parent: Open) {
        self.slots.sites.truncate(self.open.sites);
        self.open = parent;
    }
}

impl Memory {
    fn begin(&mut self) {
        for hold in &mut self.holds {
            hold.found = None;
            hold.claim = None;
        }
        self.wheel_claim = None;
        self.changed = false;
    }

    /// After a pass, hands each button to the box that asked for it, and lets
    /// go once it's up. Losing focus lets go without a click.
    pub fn end(&mut self, input: &Input) {
        for (hold, button) in self.holds.iter_mut().zip(Button::ALL) {
            self.changed |= hold.end(input, button);
        }
        self.wheel = self.wheel_claim;
        self.pointer = input.pointer();
    }

    /// Whether the last pass gave a button to a box or let go of one, so it
    /// should run again before drawing.
    pub const fn changed(&self) -> bool {
        self.changed
    }
}

impl Hold {
    /// Returns whether it changed hands or was let go of.
    fn end(&mut self, input: &Input, button: Button) -> bool {
        let holder = self.claim.or(self.found);
        self.held = holder.filter(|_| input.held(button));
        self.claim.is_some() || (holder.is_some() && input.released(button))
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
pub(crate) mod tests {
    use super::*;
    use crate::input::Event;

    pub const WINDOW: Rect = Rect::new(0., 0., 300., 100.);

    fn fixed(w: f32, h: f32) -> Element {
        Element {
            size: [Size::Fixed(w), Size::Fixed(h)],
            ..Element::DEFAULT
        }
    }

    /// What the host keeps between passes.
    #[derive(Default)]
    pub(crate) struct Kept {
        pub theme: Theme,
        pub layout: Layout,
        pub glyphs: Glyphs,
        pub input: Input,
        slots: Slots,
        memory: Memory,
    }

    impl Kept {
        /// Runs one pass and solves it, returning what `body` did. The input
        /// is used up and what waited comes in, as between the host's frames.
        pub fn pass<R>(&mut self, body: impl FnOnce(&mut Ui) -> R) -> R {
            self.layout.clear();
            let mut out = Out::default();
            let mut ui = Ui::root(
                &self.theme,
                &mut self.layout,
                &mut self.glyphs,
                &self.input,
                &mut out,
                &mut self.slots,
                &mut self.memory,
            );
            let r = body(&mut ui);
            self.memory.end(&self.input);
            self.input.clear();
            self.input.trickle();
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
    /// What a box ten wide asked of the left button in one pass.
    #[derive(PartialEq, Debug, Default)]
    struct Asked {
        held: bool,
        clicked: bool,
        dragged: Option<[f32; 2]>,
    }

    fn ask(kept: &mut Kept) -> Asked {
        kept.pass(|ui| {
            ui.element(fixed(10., 10.), |ui| Asked {
                held: ui.held(Button::Left),
                clicked: ui.clicked(Button::Left),
                dragged: ui.dragged(Button::Left),
            })
        })
    }

    const HELD: Asked = Asked {
        held: true,
        clicked: false,
        dragged: Some([0.; 2]),
    };

    /// Still dragged, so the last move before the release counts.
    const CLICKED: Asked = Asked {
        held: false,
        clicked: true,
        dragged: Some([0.; 2]),
    };

    /// Gives the box a rect, then presses over it.
    fn press(kept: &mut Kept) {
        kept.input.push(Event::Pointer(Some([5., 5.])));
        ask(kept);
        kept.input.push(Event::Pressed(Button::Left));
        assert_eq!(ask(kept), Asked::default());
    }

    #[test]
    fn a_box_holds_the_button_until_it_comes_up_over_it() {
        let mut kept = Kept::default();
        press(&mut kept);
        assert_eq!(ask(&mut kept), HELD);
        kept.input.push(Event::Released(Button::Left));
        assert_eq!(ask(&mut kept), CLICKED);
        assert_eq!(ask(&mut kept), Asked::default());
    }

    #[test]
    fn a_press_and_release_in_one_pass_click_the_pass_after() {
        let mut kept = Kept::default();
        kept.input.push(Event::Pointer(Some([5., 5.])));
        ask(&mut kept);
        kept.input.push(Event::Pressed(Button::Left));
        kept.input.push(Event::Released(Button::Left));
        assert_eq!(ask(&mut kept), Asked::default());
        assert_eq!(ask(&mut kept), CLICKED);
        assert_eq!(ask(&mut kept), Asked::default());
    }

    #[test]
    fn letting_go_away_from_the_box_is_not_a_click() {
        let mut kept = Kept::default();
        press(&mut kept);
        kept.input.push(Event::Pointer(Some([50., 50.])));
        kept.input.push(Event::Released(Button::Left));
        ask(&mut kept);
        assert_eq!(ask(&mut kept), Asked::default());
    }

    #[test]
    fn losing_focus_lets_go_without_a_click() {
        let mut kept = Kept::default();
        press(&mut kept);
        kept.input.push(Event::Unfocused);
        ask(&mut kept);
        assert_eq!(ask(&mut kept), Asked::default());
    }

    #[test]
    fn a_drag_follows_the_pointer_outside_the_box() {
        let mut kept = Kept::default();
        press(&mut kept);
        kept.input.push(Event::Pointer(Some([40., 25.])));
        assert_eq!(ask(&mut kept).dragged, Some([35., 20.]));
        // A pass run again with no new input moved nothing.
        assert_eq!(ask(&mut kept).dragged, Some([0.; 2]));
    }

    #[test]
    fn the_box_declared_last_takes_the_press() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept| {
            kept.pass(|ui| {
                ui.element(fixed(20., 20.), |ui| {
                    let inner = ui.element(fixed(10., 10.), |ui| ui.held(Button::Left));
                    // Asking after what's inside it still loses to it.
                    (ui.held(Button::Left), inner)
                })
            })
        };
        kept.input.push(Event::Pointer(Some([5., 5.])));
        pass(&mut kept);
        kept.input.push(Event::Pressed(Button::Left));
        pass(&mut kept);
        assert_eq!(pass(&mut kept), (false, true));
    }
    #[test]
    fn each_button_is_held_on_its_own() {
        let mut kept = Kept::default();
        let pass = |kept: &mut Kept| {
            kept.pass(|ui| {
                [10., 20.].map(|h| {
                    ui.element(fixed(10., h), |ui| {
                        [ui.held(Button::Left), ui.held(Button::Middle)]
                    })
                })
            })
        };
        kept.input.push(Event::Pointer(Some([5., 5.])));
        pass(&mut kept);
        kept.input.push(Event::Pressed(Button::Left));
        pass(&mut kept);
        kept.input.push(Event::Pointer(Some([5., 15.])));
        kept.input.push(Event::Pressed(Button::Middle));
        pass(&mut kept);
        assert_eq!(pass(&mut kept), [[true, false], [false, true]]);
    }

    #[test]
    fn a_floating_box_hides_what_it_covers_from_the_pointer() {
        let mut kept = Kept::default();
        let cover = Element {
            float: Some(Anchor::Parent {
                parent: [Align::Start; 2],
                own: [Align::Start; 2],
                offset: [0.; 2],
                clipped: false,
            }),
            ..fixed(10., 10.)
        };
        let pass = |kept: &mut Kept| {
            kept.pass(|ui| {
                ui.element(fixed(20., 20.), |ui| {
                    let row = ui.element(fixed(20., 20.), |ui| ui.hovered());
                    let float = ui.element(cover, |ui| ui.hovered());
                    (ui.hovered(), row, float)
                })
            })
        };
        kept.input.push(Event::Pointer(Some([5., 5.])));
        pass(&mut kept);
        // The parent holds the floating box, so it's still under the pointer.
        assert_eq!(pass(&mut kept), (true, false, true));
        kept.input.push(Event::Pointer(Some([15., 15.])));
        assert_eq!(pass(&mut kept), (true, true, false));
    }

    #[test]
    fn a_box_past_its_clip_is_not_under_the_pointer() {
        let mut kept = Kept::default();
        let clipped = Element {
            clip: true,
            ..fixed(20., 10.)
        };
        let pass = |kept: &mut Kept| {
            kept.pass(|ui| ui.element(clipped, |ui| ui.element(fixed(20., 30.), |ui| ui.hovered())))
        };
        kept.input.push(Event::Pointer(Some([5., 20.])));
        pass(&mut kept);
        assert!(!pass(&mut kept));
        kept.input.push(Event::Pointer(Some([5., 5.])));
        assert!(pass(&mut kept));
    }
}
