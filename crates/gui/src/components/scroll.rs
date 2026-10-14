//! Boxes taller than the space they're given, moved by the wheel while the
//! pointer is over them, with a thumb along the right edge showing where.

use crate::cast::count;
use crate::input::PointerButton;
use crate::ui::{Align, Anchor, Element, Length, Ui};

/// When a scroll box's thumb shows, while there's more than fits.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum ThumbShow {
    #[default]
    Always,
    /// While the pointer is over the box or the thumb is dragged, then for
    /// `hold` seconds after, then fading out over `fade` seconds.
    Hover { hold: f32, fade: f32 },
}

/// What a [`scroll`] box keeps: how far it's scrolled down, in logical
/// pixels, and its thumb.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Scroll {
    pub offset: f32,
    pub thumb: ThumbShow,
    /// Seconds a [`ThumbShow::Hover`] thumb has left to show, its hold and
    /// fade together.
    shown: f32,
}

/// What a [`list`] keeps: where it's scrolled to, and its thumb. It counts
/// whole rows plus a remainder, so nothing it does walks more rows than are
/// in view or scrolled past, however long the list.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct ListScroll {
    /// The row at the top of the view.
    pub top: usize,
    /// How much of `top` is scrolled out of view, in logical pixels. Less
    /// than a row.
    pub offset: f32,
    pub thumb: ThumbShow,
    /// Seconds a [`ThumbShow::Hover`] thumb has left to show, its hold and
    /// fade together.
    shown: f32,
}

/// A box clipped to `element`'s size, its children moved up by `state`.
/// `element` should be [`Length::Grow`] or [`Length::Fixed`] down, since a box
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
            state.offset += thumb(
                ui,
                state.thumb,
                &mut state.shown,
                state.offset,
                content,
                view,
            );
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
    indexed_list(ui, state, items.len(), height, |ui, index| {
        if let Some(item) = items.get(index) {
            row(ui, item);
        }
    });
}

/// [`list`] over `len` rows with nothing behind them, each declared by
/// `row` from its index, for rows made from their place, such as a hex
/// view's.
#[track_caller]
pub fn indexed_list(
    ui: &mut Ui,
    state: &mut ListScroll,
    len: usize,
    height: f32,
    mut row: impl FnMut(&mut Ui, usize),
) {
    let element = Element {
        clip: true,
        ..Element::column()
    };
    let slot = Element {
        size: [Length::Grow, Length::Fixed(height)],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let wheel = ui.wheel()[1];
        // Nothing to place rows against until the list has a size.
        let Some(view) = ui.rect() else { return };
        let content = count(len) * height;
        let pixels = -wheel
            + thumb(
                ui,
                state.thumb,
                &mut state.shown,
                count(state.top) * height + state.offset,
                content,
                view.h,
            );
        state.by(view.h, height, pixels, len);
        ui.style().offset[1] = -state.offset;
        let mut top = -state.offset;
        for index in state.top..len {
            // Past `u32::MAX` rows they'd need keys of their own.
            let Ok(key) = u32::try_from(index) else { break };
            if top >= view.h {
                break;
            }
            ui.keyed(key, slot, |ui| row(ui, index));
            top += height;
        }
    });
}

/// Along the right edge of the open box, as long as `view` is a share of
/// `content`, and as far down as `offset` is a share of how far it scrolls.
/// Dragged, it moves the content as far as it moves itself. Returns how far
/// that is in the content's pixels, and nothing when it all fits.
fn thumb(
    ui: &mut Ui,
    show: ThumbShow,
    shown: &mut f32,
    offset: f32,
    content: f32,
    view: f32,
) -> f32 {
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
    let over = ui.hovered();
    let at = |offset: f32| [-inset, inset + travel * offset / scrolls];
    let element = Element {
        size: [Length::Fixed(width), Length::Fixed(length)],
        radius: [width / 2.; 4],
        float: Some(Anchor::Parent {
            parent: [Align::End, Align::Start],
            own: [Align::End, Align::Start],
            offset: at(offset),
            clipped: true,
        }),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let dragged = ui.dragged(PointerButton::Left);
        let opacity = match show {
            ThumbShow::Hover { hold, fade } => {
                hover_opacity(ui, shown, over || dragged.is_some(), hold, fade)
            }
            ThumbShow::Always => 1.,
        };
        let color = if dragged.is_some() || ui.hovered() {
            theme.color.thumb_hover
        } else {
            theme.color.thumb
        };
        ui.style().background = Some(color.faded(opacity));
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

/// How shown a [`ThumbShow::Hover`] thumb is: fully while `in_use` and for
/// `hold` after, then less over `fade`. The hold asks for one frame at its
/// end rather than one each refresh.
fn hover_opacity(ui: &mut Ui, shown: &mut f32, in_use: bool, hold: f32, fade: f32) -> f32 {
    if in_use {
        *shown = hold + fade;
    } else {
        *shown = (*shown - ui.elapsed()).max(0.);
        if *shown > fade {
            ui.redraw_in(*shown - fade);
        } else if *shown > 0. {
            ui.animate();
        }
    }
    if fade > 0. {
        (*shown / fade).min(1.)
    } else {
        f32::from(*shown > 0.)
    }
}

impl ListScroll {
    /// At the first row, its thumb always shown.
    pub const TOP: Self = Self::at(0);

    /// With row `top` at the top.
    #[must_use]
    pub const fn at(top: usize) -> Self {
        Self {
            top,
            offset: 0.,
            thumb: ThumbShow::Always,
            shown: 0.,
        }
    }

    /// Showing its thumb as `thumb` says.
    #[must_use]
    pub const fn thumb(self, thumb: ThumbShow) -> Self {
        Self { thumb, ..self }
    }

    pub const fn jump_to_top(&mut self) {
        self.top = 0;
        self.offset = 0.;
    }

    /// Scrolls as far down as `len` rows `height` tall go in a `view` that
    /// tall, with the last at its bottom. To the top when they don't fill it.
    pub fn jump_to_end(&mut self, view: f32, height: f32, len: usize) {
        (self.top, self.offset) = end(view, height, len);
    }

    /// Whether the last of `len` rows sits at the bottom of `view`, or they
    /// all fit in it.
    #[must_use]
    pub fn at_end(self, view: f32, height: f32, len: usize) -> bool {
        (self.top, self.offset) == end(view, height, len)
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
        let end = end(view, height, len);
        if (self.top, self.offset) > end {
            (self.top, self.offset) = end;
        }
    }
}

/// The top row and offset scrolled as far down as `len` rows `height` tall go
/// in a `view` that tall.
fn end(view: f32, height: f32, len: usize) -> (usize, f32) {
    let mut filled = 0.;
    for top in (0..len).rev() {
        filled += height;
        if filled >= view {
            return (top, filled - view);
        }
    }
    (0, 0.)
}
