//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself.

use std::{error::Error, sync::Arc};

use crate::canvas::{AtlasUpdate, Canvas, Color, Glyphs, Rect};
use crate::clipboard::Clipboard;
use crate::input::{Event, Input, WindowRequest};
use crate::keymap::Keymap;
use crate::layout::Layout;
use crate::ui::{Memory, Slots, Theme, Ui};

/// The app's side of the host.
pub trait App: Sized {
    /// The window's title.
    // TODO: a `WindowRequest` field too, so the app can change it after startup.
    const TITLE: &'static str;
    /// The smallest the window can be, in logical pixels. It opens at this
    /// size.
    const MIN_SIZE: [f32; 2];
    /// Whether the system draws the window's frame and title bar. Without
    /// it, the app draws its own and asks for what the system's would do
    /// through [`Ui::window`], such as with
    /// [`title_bar`](crate::components::title_bar) and
    /// [`window_edges`](crate::components::window_edges).
    const DECORATED: bool = true;
    /// What [`run`](crate::platform::window::run) hands to [`App::new`].
    type Args;
    /// # Errors
    ///
    /// When the app can't start. The window doesn't open.
    fn new(start: &mut Startup, args: Self::Args) -> Result<Self, Box<dyn Error>>;
    /// Acts on this frame's input once, before any pass, so what it changes
    /// is already in place when the boxes are declared. Where shortcuts that
    /// aren't asked of a box go, such as Escape closing a menu.
    fn input(&mut self, _input: &Input) {}
    /// Declares one pass's boxes through `ui`, which starts in the window's
    /// box. Can run twice in a frame, when the first pass's answers went
    /// stale, and only the second is drawn.
    ///
    /// Act on input before declaring the boxes it changes. A change made after
    /// shows a frame late unless [`Ui::stale`] asks for the pass again.
    fn ui(&mut self, ui: &mut Ui);
    /// After the last pass.
    fn on_close(&mut self) {}
}

/// Wakes the window from any thread, so a frame runs without input and the
/// app picks up what the thread made, such as in [`App::input`].
#[derive(Clone)]
pub struct Wake(Arc<dyn Fn() + Send + Sync>);

impl Wake {
    /// Calling `wake`, which the window makes its own, or a test does.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self(Arc::new(wake))
    }

    pub fn wake(&self) {
        (self.0)();
    }
}

/// What the app sets up before the first pass.
pub struct Startup<'a> {
    /// What everything is drawn with, and the window cleared to. Black and
    /// zero sized until the app fills it in.
    pub theme: Theme,
    /// Where the app adds its fonts and icons.
    pub glyphs: &'a mut Glyphs,
    /// The default keys, for the app to bind its own over.
    pub keymap: Keymap,
    /// For the app's threads to keep.
    pub wake: Wake,
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
    pub fn new(args: A::Args, wake: Wake) -> Result<Self, Box<dyn Error>> {
        let mut glyphs = Glyphs::default();
        let mut start = Startup {
            theme: Theme::default(),
            glyphs: &mut glyphs,
            keymap: Keymap::default(),
            wake,
        };
        let app = A::new(&mut start, args)?;
        let Startup { theme, keymap, .. } = start;
        Ok(Self {
            app,
            theme,
            glyphs,
            input: Input::default(),
            layout: Layout::default(),
            slots: Slots::default(),
            memory: Memory::new(Clipboard::system(), keymap),
        })
    }

    /// Holds `event` for the next pass.
    pub fn push(&mut self, event: Event) {
        self.input.push(event);
    }

    /// Runs one pass over `rect`, the window in logical pixels, with `scale`
    /// physical pixels per logical one, using up the events pushed since the
    /// last. With `rounded`, the page is drawn with the theme's
    /// [`window_radius`](crate::ui::ThemeSizes::window_radius) and a border,
    /// and what's past its corners left clear.
    ///
    /// A pass answers from last frame's rects. If a box moved or a button
    /// changed hands, those answers are stale, such as a row that landed under
    /// the pointer without its hover (which is incorrect), so the pass runs again
    /// with no new input and that one is drawn instead. So a frame never runs
    /// more than two passes, a button that changes twice since the last waits
    /// for the next frame, which [`WindowRequest::again`] asks for.
    ///
    /// A frame with input also asks for the next, so whatever it changed after
    /// the boxes it affects were declared, or a box new in the second pass
    /// with no size yet, is drawn a frame late rather than left until the
    /// next input.
    pub fn draw(
        &mut self,
        rect: Rect,
        scale: f32,
        rounded: bool,
        canvas: &mut Canvas,
    ) -> WindowRequest {
        self.glyphs.set_scale(scale);
        self.glyphs.next_frame();

        // Act on input before any pass
        self.app.input(&self.input);

        // 1st pass
        let mut out = self.pass(rounded);

        let fresh = self.input.fresh();
        self.input.clear();
        let shifted = self.layout.solve(rect, scale);
        if shifted || self.memory.changed() {
            // 2nd pass, corrects any UI that would otherwise appear incorrectly
            let first = out;
            out = self.pass(rounded);
            // It has no input to act on, so it can't ask again.
            out.action = out.action.or(first.action);
            self.layout.solve(rect, scale);
        }

        canvas.clear(rect);
        canvas.background = if rounded {
            Color::TRANSPARENT
        } else {
            self.theme.color.page
        };
        self.layout.emit(canvas, &mut self.glyphs);
        if let Some(cursor) = self.layout.cursor(self.input.pointer()) {
            out.cursor = cursor;
        }
        out.again = self.input.trickle() || fresh;
        out
    }

    fn pass(&mut self, rounded: bool) -> WindowRequest {
        let mut out = WindowRequest::default();
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
        if rounded {
            let window = ui.style();
            window.background = Some(self.theme.color.page);
            window.border = Some(self.theme.border());
            window.radius = [self.theme.size.window_radius; 4];
        }
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
