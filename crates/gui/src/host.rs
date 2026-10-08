//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself.
// TODO: For now a pass draws straight into the canvas. Context and input come later.

use std::error::Error;

use crate::canvas::{Canvas, Color, Rect};

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
    fn ui(&mut self, rect: Rect, canvas: &mut Canvas);
    /// After the last pass.
    fn on_close(&mut self) {}
}

/// What the app sets up before the first pass.
// TODO: `glyphs` once `gui::canvas` has them, and `wake` once something runs
// off the main thread, such as `app::watch`.
pub struct Startup {
    // TODO: becomes `theme: Theme` from `gui::context`.
    pub background: Color,
}

// TODO: input, layout and memory, once `gui::input`, `gui::layout` and
// `gui::context` exist.
pub struct Host<A> {
    app: A,
    background: Color,
}

impl<A: App> Host<A> {
    /// # Errors
    ///
    /// When [`App::new`] fails.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut start = Startup {
            background: Color([0., 0., 0., 1.]),
        };
        let app = A::new(&mut start)?;
        Ok(Self {
            app,
            background: start.background,
        })
    }

    /// Runs one pass over `rect`, the window in logical pixels.
    // TODO: return `Out`.
    pub fn draw(&mut self, rect: Rect, canvas: &mut Canvas) {
        canvas.clear(rect);
        canvas.background = self.background;
        self.app.ui(rect, canvas);
    }

    pub fn close(&mut self) {
        self.app.on_close();
    }
}
