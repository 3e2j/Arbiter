//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself.

use std::error::Error;

use crate::canvas::{AtlasUpdate, Canvas, Color, Glyphs, Rect};
use crate::input::{Event, Input, Out};
use crate::layout::Layout;
use crate::ui::Ui;

/// The app's side of the host.
pub trait App: Sized {
    /// The window's title.
    // TODO: an `Out` field too, so the app can change it after startup.
    const TITLE: &'static str;
    /// # Errors
    ///
    /// When the app can't start. The window doesn't open.
    fn new(start: &mut Startup) -> Result<Self, Box<dyn Error>>;
    /// Declares one pass's boxes through `ui`, which starts in the window's
    /// box. Runs twice when a box moved, the second time with no new input.
    fn ui(&mut self, ui: &mut Ui);
    /// After the last pass.
    fn on_close(&mut self) {}
}

/// What the app sets up before the first pass.
// TODO: `wake` once something runs off the main thread, such as `app::watch`.
pub struct Startup<'a> {
    // TODO: becomes `theme: Theme` from `gui::ui`.
    pub background: Color,
    /// Where the app adds its fonts and icons.
    pub glyphs: &'a mut Glyphs,
}

// TODO: memory, once responses track presses.
pub struct Host<A> {
    app: A,
    background: Color,
    glyphs: Glyphs,
    input: Input,
    layout: Layout,
}

impl<A: App> Host<A> {
    /// # Errors
    ///
    /// When [`App::new`] fails.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut glyphs = Glyphs::default();
        let mut start = Startup {
            background: Color::hex(0),
            glyphs: &mut glyphs,
        };
        let app = A::new(&mut start)?;
        let background = start.background;
        Ok(Self {
            app,
            background,
            glyphs,
            input: Input::default(),
            layout: Layout::default(),
        })
    }

    /// Holds `event` for the next pass.
    pub fn push(&mut self, event: Event) {
        self.input.push(event);
    }

    /// Runs one pass over `rect`, the window in logical pixels, with `scale`
    /// physical pixels per logical one, using up the events pushed since the
    /// last.
    pub fn draw(&mut self, rect: Rect, scale: f32, canvas: &mut Canvas) -> Out {
        self.glyphs.set_scale(scale);
        self.glyphs.next_frame();
        let mut out = self.pass();
        self.input.clear();
        // The pass read rects that have since moved. Running it again with the
        // input used up keeps a click from landing twice.
        if self.layout.solve(rect, scale) {
            out = self.pass();
            self.layout.solve(rect, scale);
        }
        canvas.clear(rect);
        canvas.background = self.background;
        self.layout.emit(canvas, &mut self.glyphs);
        out
    }

    fn pass(&mut self) -> Out {
        let mut out = Out::default();
        self.layout.clear();
        let mut ui = Ui::root(&mut self.layout, &mut self.glyphs, &self.input, &mut out);
        self.app.ui(&mut ui);
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
