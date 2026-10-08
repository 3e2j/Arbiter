//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself. For now a pass
//! only yields a background color. The context, input and canvas come later.

use std::error::Error;

/// The app's side of the host.
pub trait App: Sized {
    /// The window's title.
    // TODO: an `Out` field too, so the app can change it after startup.
    const TITLE: &'static str;
    /// # Errors
    ///
    /// When the app can't start. The window doesn't open.
    fn new(start: &mut Startup) -> Result<Self, Box<dyn Error>>;
    // TODO: take `&mut Context` once `gui::context` exists.
    fn ui(&mut self);
    /// After the last pass.
    fn on_close(&mut self) {}
}

/// What the app sets up before the first pass.
// TODO: `glyphs` once `gui::canvas` exists, and `wake` once something runs off
// the main thread, such as `app::watch`.
pub struct Startup {
    /// Linear RGBA.
    // TODO: becomes `theme: Theme` from `gui::context`.
    pub background: [f64; 4],
}

// TODO: input, layout and memory, once `gui::input`, `gui::layout` and
// `gui::context` exist.
pub struct Host<A> {
    app: A,
    background: [f64; 4],
}

impl<A: App> Host<A> {
    /// # Errors
    ///
    /// When [`App::new`] fails.
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut start = Startup {
            background: [0., 0., 0., 1.],
        };
        let app = A::new(&mut start)?;
        Ok(Self {
            app,
            background: start.background,
        })
    }

    /// Runs one pass and returns the color to clear to.
    // TODO: take the window's rect and a `&mut Canvas`, and return `Out`.
    pub fn draw(&mut self) -> [f64; 4] {
        self.app.ui();
        self.background
    }

    pub fn close(&mut self) {
        self.app.on_close();
    }
}
