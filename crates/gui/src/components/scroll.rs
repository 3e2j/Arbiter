//! Boxes taller than the space they're given, moved by the wheel while the
//! pointer is over them, with a thumb along the right edge showing where.

use crate::cast::count;
use crate::input::Button;
use crate::ui::{Align, Anchor, Element, Size, Ui};

/// How far a [`scroll`] box is scrolled down, in logical pixels.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Scroll {
    pub offset: f32,
}

/// Where a [`list`] is scrolled to. It counts whole rows plus a remainder, so
/// nothing it does walks more rows than are in view or scrolled past, however
/// long the list.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ListScroll {
    /// The row at the top of the view.
    pub top: usize,
    /// How much of `top` is scrolled out of view, in logical pixels. Less
    /// than a row.
    pub offset: f32,
}

/// A box clipped to `element`'s size, its children moved up by `state`.
/// `element` should be [`Size::Grow`] or [`Size::Fixed`] down, since a box
/// that fits its children has nothing to scroll.
#[track_caller]
pub fn scroll<R>(
    ui: &mut Ui,
    state: &mut Scroll,
    element: Element,
    body: impl FnOnce(&mut Ui) -> R,
) -> R {
    let clipped = Element {
        clip: true,
        ..element
    };
    ui.element(clipped, |ui| {
        let padding = element.padding[1] + element.padding[3];
        let view = ui.rect().map(|rect| rect.h);
        let content = ui.content().map_or(0., |[_, h]| h + padding);
        let wheel = ui.wheel()[1];
        // The first pass has no size to stop at, so it keeps the offset.
        if let Some(view) = view {
            state.offset -= wheel;
            state.offset += thumb(ui, state.offset, content, view);
            state.offset = state.offset.clamp(0., (content - view).max(0.));
        }
        ui.style().offset[1] = element.offset[1] - state.offset;
        body(ui)
    })
}

/// Fills its parent, showing `items` as rows `height` tall, and declares only
/// the rows in view, each through `row` into a box of its own, keyed by its
/// index so it keeps its state as the list scrolls.
#[track_caller]
pub fn list<T>(
    ui: &mut Ui,
    state: &mut ListScroll,
    items: &[T],
    height: f32,
    mut row: impl FnMut(&mut Ui, &T),
) {
    let element = Element {
        clip: true,
        ..Element::column()
    };
    let slot = Element {
        size: [Size::Grow, Size::Fixed(height)],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let wheel = ui.wheel()[1];
        // Nothing to place rows against until the list has a size.
        let Some(view) = ui.rect() else { return };
        let len = items.len();
        let content = count(len) * height;
        let pixels = -wheel
            + thumb(
                ui,
                count(state.top) * height + state.offset,
                content,
                view.h,
            );
        state.by(view.h, height, pixels, len);
        ui.style().offset[1] = -state.offset;
        let mut top = -state.offset;
        for (key, item) in (0..).zip(items).skip(state.top) {
            if top >= view.h {
                break;
            }
            ui.keyed(key, slot, |ui| row(ui, item));
            top += height;
        }
    });
}

/// Along the right edge of the open box, as long as `view` is a share of
/// `content`, and as far down as `offset` is a share of how far it scrolls.
/// Dragged, it moves the content as far as it moves itself. Returns how far
/// that is in the content's pixels, and nothing when it all fits.
fn thumb(ui: &mut Ui, offset: f32, content: f32, view: f32) -> f32 {
    let scrolls = content - view;
    if scrolls <= 0. || view <= 0. {
        return 0.;
    }
    let theme = ui.theme();
    let width = theme.size.scroll_bar;
    let inset = width / 2.;
    let track = view - 2. * inset;
    // Never shorter than a row, however long the content.
    let length = (track * view / content).max(theme.size.row).min(track);
    let travel = track - length;
    let at = |offset: f32| [-inset, inset + travel * offset / scrolls];
    let element = Element {
        size: [Size::Fixed(width), Size::Fixed(length)],
        background: Some(theme.color.thumb),
        radius: width / 2.,
        float: Some(Anchor::Parent {
            parent: [Align::End, Align::Start],
            own: [Align::End, Align::Start],
            offset: at(offset),
            clipped: true,
        }),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let dragged = ui.dragged(Button::Left);
        if dragged.is_some() || ui.hovered() {
            ui.style().background = Some(theme.color.thumb_hover);
        }
        let Some([_, dy]) = dragged.filter(|_| travel > 0.) else {
            return 0.;
        };
        let by = dy * scrolls / travel;
        let moved = (offset + by).clamp(0., scrolls);
        if let Some(Anchor::Parent { offset, .. }) = &mut ui.style().float {
            *offset = at(moved);
        }
        moved - offset
    })
}

impl ListScroll {
    /// At the first row.
    pub const TOP: Self = Self { top: 0, offset: 0. };

    /// Scrolled as far down as `len` rows `height` tall go in a `view` that
    /// tall, with the last at its bottom. At the top when they don't fill it.
    #[must_use]
    pub fn end(view: f32, height: f32, len: usize) -> Self {
        let mut filled = 0.;
        for top in (0..len).rev() {
            filled += height;
            if filled >= view {
                return Self {
                    top,
                    offset: filled - view,
                };
            }
        }
        Self::TOP
    }

    /// Whether the last of `len` rows sits at the bottom of `view`, or they
    /// all fit in it.
    #[must_use]
    pub fn at_end(self, view: f32, height: f32, len: usize) -> bool {
        self == Self::end(view, height, len)
    }

    /// Scrolls `pixels` down, or up when negative, stopping at either end.
    fn by(&mut self, view: f32, height: f32, pixels: f32, len: usize) {
        self.offset += pixels;
        while self.offset >= height && self.top + 1 < len {
            self.top += 1;
            self.offset -= height;
        }
        while self.offset < 0. && self.top > 0 {
            self.top -= 1;
            self.offset += height;
        }
        self.offset = self.offset.clamp(0., height);
        let end = Self::end(view, height, len);
        if (self.top, self.offset) > (end.top, end.offset) {
            *self = end;
        }
    }
}
