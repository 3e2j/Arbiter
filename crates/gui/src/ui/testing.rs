//! Running passes without a window, for tests of what's declared through a
//! [`Ui`].

use super::layout::Layout;
use super::{Memory, Slots, Ui};
use crate::canvas::{Glyphs, Rect};
use crate::input::{Input, WindowRequest};

pub const WINDOW: Rect = Rect::new(0., 0., 300., 100.);

/// What the host keeps between passes.
#[derive(Default)]
pub struct Kept {
    pub(crate) layout: Layout,
    pub glyphs: Glyphs,
    pub input: Input,
    slots: Slots,
    memory: Memory,
    /// What the last pass asked of the window.
    pub out: WindowRequest,
}

impl Kept {
    /// Seconds since the last frame, as the host sets it.
    pub const fn set_elapsed(&mut self, seconds: f32) {
        self.input.set_elapsed(seconds);
    }

    /// Runs one pass and solves it, returning what `body` did. The input
    /// is used up and what waited comes in, as between the host's frames.
    pub fn pass<R>(&mut self, body: impl FnOnce(&mut Ui) -> R) -> R {
        self.layout.clear();
        self.out = WindowRequest::default();
        let mut ui = Ui::root(
            &mut self.layout,
            &mut self.glyphs,
            &self.input,
            &mut self.out,
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
