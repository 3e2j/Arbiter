//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself.

use std::error::Error;

use crate::canvas::{AtlasUpdate, Canvas, Glyphs, Rect};
use crate::input::{Event, Input, Out};
use crate::layout::Layout;
use crate::ui::{Memory, Slots, Theme, Ui};

/// The app's side of the host.
pub trait App: Sized {
    /// The window's title.
    // TODO: an `Out` field too, so the app can change it after startup.
    const TITLE: &'static str;
    /// The smallest the window can be, in logical pixels. It opens at this
    /// size.
    const MIN_SIZE: [f32; 2];
    /// # Errors
    ///
    /// When the app can't start. The window doesn't open.
    fn new(start: &mut Startup) -> Result<Self, Box<dyn Error>>;
    /// Acts on this frame's input once, before any pass, so what it changes
    /// is already in place when the boxes are declared. Where shortcuts that
    /// aren't asked of a box go, such as Escape closing a menu.
    fn input(&mut self, _input: &Input) {}
    /// Declares one pass's boxes through `ui`, which starts in the window's
    /// box. Can run twice in a frame, as [`Host::draw`] says, and only the
    /// second is drawn.
    ///
    /// Act on input before declaring the boxes it changes. Nothing runs the
    /// pass again for a change made after, so it shows a frame late.
    fn ui(&mut self, ui: &mut Ui);
    /// After the last pass.
    fn on_close(&mut self) {}
}

/// What the app sets up before the first pass.
// TODO: `wake` once something runs off the main thread, such as `app::watch`.
pub struct Startup<'a> {
    /// What everything is drawn with, and the window cleared to. Black and
    /// zero sized until the app fills it in.
    pub theme: Theme,
    /// Where the app adds its fonts and icons.
    pub glyphs: &'a mut Glyphs,
}

pub struct Host<A> {
    app: A,
    theme: Theme,
    glyphs: Glyphs,
    input: Input,
    layout: Layout,
    slots: Slots,
    memory: Memory,
}

impl<A: App> Host<A> {
    /// # Errors
    ///
    /// When [`App::new`] fails.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut glyphs = Glyphs::default();
        let mut start = Startup {
            theme: Theme::default(),
            glyphs: &mut glyphs,
        };
        let app = A::new(&mut start)?;
        let theme = start.theme;
        Ok(Self {
            app,
            theme,
            glyphs,
            input: Input::default(),
            layout: Layout::default(),
            slots: Slots::default(),
            memory: Memory::default(),
        })
    }

    /// Holds `event` for the next pass.
    pub fn push(&mut self, event: Event) {
        self.input.push(event);
    }

    /// Runs one pass over `rect`, the window in logical pixels, with `scale`
    /// physical pixels per logical one, using up the events pushed since the
    /// last.
    ///
    /// A pass answers from last frame's rects. If a box moved or a button
    /// changed hands, those answers are stale, such as a row that landed under
    /// the pointer without its hover (which is incorrect), so the pass runs again
    /// with no new input and that one is drawn instead. So a frame never runs
    /// more than two passes, a button that changes twice since the last waits
    /// for the next frame, which [`Out::again`] asks for.
    pub fn draw(&mut self, rect: Rect, scale: f32, canvas: &mut Canvas) -> Out {
        self.glyphs.set_scale(scale);
        self.glyphs.next_frame();

        // Act on input before any pass
        self.app.input(&self.input);

        // 1st pass
        let mut out = self.pass();

        self.input.clear();
        let shifted = self.layout.solve(rect, scale);
        if shifted || self.memory.changed() {
            // 2nd pass, corrects any UI that would otherwise appear incorrectly
            out = self.pass();
            self.layout.solve(rect, scale);
        }

        canvas.clear(rect);
        canvas.background = self.theme.color.page;
        self.layout.emit(canvas, &mut self.glyphs);
        if let Some(cursor) = self.layout.cursor(self.input.pointer()) {
            out.cursor = cursor;
        }
        out.again = self.input.trickle();
        out
    }

    fn pass(&mut self) -> Out {
        let mut out = Out::default();
        self.layout.clear();
        let mut ui = Ui::root(
            &self.theme,
            &mut self.layout,
            &mut self.glyphs,
            &self.input,
            &mut out,
            &mut self.slots,
            &mut self.memory,
        );
        self.app.ui(&mut ui);
        self.memory.end(&self.input);
        out
    }

    /// Makes the next [`Self::take_atlas_updates`] hold the whole atlases, for a
    /// new GPU that has none of them.
    pub fn reupload_atlas(&mut self) {
        self.glyphs.reupload();
    }

    /// What the passes since the last call added to the atlases, for the GPU.
    pub fn take_atlas_updates(&mut self) -> impl Iterator<Item = AtlasUpdate<'_>> {
        self.glyphs.take_updates()
    }

    pub fn close(&mut self) {
        self.app.on_close();
    }
}
