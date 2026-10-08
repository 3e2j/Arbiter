//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself.
// TODO: For now a pass draws straight into the canvas. Context and input come later.

use std::error::Error;

use crate::canvas::{AtlasUpdate, Canvas, Color, Glyphs, Rect};

/// The app's side of the host.
pub trait App: Sized {
    /// The window's title.
    // TODO: an `Out` field too, so the app can change it after startup.
    const TITLE: &'static str;
    /// # Errors
    ///
    /// When the app can't start. The window doesn't open.
    fn new(start: &mut Startup) -> Result<Self, Box<dyn Error>>;
    /// Draws one pass into `canvas`, which covers `rect`.
    // TODO: take `&mut Context` once `gui::context` exists.
    fn ui(&mut self, rect: Rect, canvas: &mut Canvas, glyphs: &mut Glyphs);
    /// After the last pass.
    fn on_close(&mut self) {}
}

/// What the app sets up before the first pass.
// TODO: `wake` once something runs off the main thread, such as `app::watch`.
pub struct Startup<'a> {
    // TODO: becomes `theme: Theme` from `gui::context`.
    pub background: Color,
    /// Where the app adds its fonts and icons.
    pub glyphs: &'a mut Glyphs,
}

// TODO: input, layout and memory, once `gui::input`, `gui::layout` and
// `gui::context` exist.
pub struct Host<A> {
    app: A,
    background: Color,
    glyphs: Glyphs,
}

impl<A: App> Host<A> {
    /// # Errors
    ///
    /// When [`App::new`] fails.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut glyphs = Glyphs::default();
        let mut start = Startup {
            background: Color([0., 0., 0., 1.]),
            glyphs: &mut glyphs,
        };
        let app = A::new(&mut start)?;
        let background = start.background;
        Ok(Self {
            app,
            background,
            glyphs,
        })
    }

    /// Runs one pass over `rect`, the window in logical pixels, with `scale`
    /// physical pixels per logical one.
    // TODO: return `Out`.
    pub fn draw(&mut self, rect: Rect, scale: f32, canvas: &mut Canvas) {
        self.glyphs.set_scale(scale);
        self.glyphs.next_frame();
        canvas.clear(rect);
        canvas.background = self.background;
        self.app.ui(rect, canvas, &mut self.glyphs);
    }

    /// Makes the next [`Self::take_atlas_update`] hold the whole atlas, for a
    /// new GPU that has none of it.
    pub fn reupload_atlas(&mut self) {
        self.glyphs.reupload();
    }

    /// What the passes since the last call added to the atlas, for the GPU.
    pub fn take_atlas_update(&mut self) -> Option<AtlasUpdate<'_>> {
        self.glyphs.take_update()
    }

    pub fn close(&mut self) {
        self.app.on_close();
    }
}
