//! Runs an [`App`], keeping what it needs between passes.
//!
//! The window drives a [`Host`] and never calls the app itself.

use std::{error::Error, sync::Arc, time::Duration};

use crate::canvas::{AtlasUpdate, Canvas, Color, Glyphs, Rect};
use crate::clipboard::Clipboard;
use crate::input::{Event, Input, WindowRequest};
use crate::ui::layout::Layout;
use crate::ui::{Border, Memory, Slots, Ui};

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
    /// [`window_drag`](crate::ui::window::window_drag) and
    /// [`window_edges`](crate::ui::window::window_edges).
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
    /// What the window's own box looks like, under everything. Asked each
    /// pass.
    fn window_style(&self) -> WindowStyle;
    /// A zoom to draw at from now on, as [`Startup::zoom`] says. Asked once
    /// a frame, before [`App::input`], so the frame is drawn at it.
    fn take_zoom(&mut self) -> Option<f32> {
        None
    }
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

/// What the window's own box looks like, from [`App::window_style`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WindowStyle {
    /// What the window is cleared to.
    pub background: Color,
    /// Drawn only when the window is rounded.
    pub border: Border,
    pub radius: f32,
}

/// What the app sets up before the first pass.
pub struct Startup<'a> {
    /// Where the app adds its fonts and icons.
    pub glyphs: &'a mut Glyphs,
    /// For the app's threads to keep.
    pub wake: Wake,
    /// How much bigger than the system's scale everything is drawn, 1 for as
    /// is. The window keeps its minimum size in zoomed logical pixels.
    pub zoom: f32,
}

pub struct Host<A> {
    app: A,
    zoom: f32,
    glyphs: Glyphs,
    input: Input,
    layout: Layout,
    slots: Slots,
    memory: Memory,
    /// Whether the last frame asked for another, as
    /// [`WindowRequest::redraw_in`] says.
    timed: bool,
}

impl<A: App> Host<A> {
    /// # Errors
    ///
    /// When [`App::new`] fails.
    pub fn new(args: A::Args, wake: Wake) -> Result<Self, Box<dyn Error>> {
        let mut glyphs = Glyphs::default();
        let mut start = Startup {
            glyphs: &mut glyphs,
            wake,
            zoom: 1.,
        };
        let app = A::new(&mut start, args)?;
        let Startup { zoom, .. } = start;
        Ok(Self {
            app,
            zoom,
            glyphs,
            input: Input::default(),
            layout: Layout::default(),
            slots: Slots::default(),
            memory: Memory::new(Clipboard::system()),
            timed: false,
        })
    }

    pub const fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Picks up a zoom the app changed since the last call. Call before
    /// working out the scale for [`Self::draw`]. Returns whether it changed.
    pub fn update_zoom(&mut self) -> bool {
        let Some(zoom) = self.app.take_zoom() else {
            return false;
        };
        self.zoom = zoom;
        true
    }

    /// Holds `event` for the next pass.
    pub fn push(&mut self, event: Event) {
        self.input.push(event);
    }

    /// Draws a frame over `rect`, the window in logical pixels, with `scale`
    /// physical pixels per logical one, zoom included, from the events pushed since the last.
    /// `rounded` draws the page with the [`WindowStyle`]'s radius and border,
    /// clear past its corners. `elapsed` is the time since the last frame.
    pub fn draw(
        &mut self,
        rect: Rect,
        scale: f32,
        rounded: bool,
        elapsed: Duration,
        canvas: &mut Canvas,
    ) -> WindowRequest {
        self.glyphs.set_scale(scale);
        self.glyphs.next_frame();
        // After sitting idle, elapsed says nothing about what's moving, and
        // would finish at once whatever starts this frame.
        if self.timed {
            self.input.set_elapsed(elapsed.as_secs_f32());
        }

        // Act on input before any pass
        self.app.input(&self.input);

        // 1st pass
        let mut out = self.pass(rounded);

        let fresh = self.input.fresh();
        self.input.clear();
        let shifted = self.layout.solve(rect, scale);
        if shifted || self.memory.changed() {
            // The 1st pass answered from last frame's rects, stale now a box
            // moved or a button changed hands. Never a 3rd: what's still
            // stale waits for the next frame.
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
            self.app.window_style().background
        };
        self.layout.emit(canvas, &mut self.glyphs);
        if let Some(cursor) = self.layout.cursor(self.input.pointer()) {
            out.cursor = cursor;
        }
        self.timed = out.redraw_in.is_some();
        // Input can change boxes after they were declared, so show that
        // next frame rather than at the next input.
        out.again = self.input.trickle() || fresh;
        out
    }

    fn pass(&mut self, rounded: bool) -> WindowRequest {
        let mut out = WindowRequest::default();
        self.layout.clear();
        let mut ui = Ui::root(
            &mut self.layout,
            &mut self.glyphs,
            &self.input,
            &mut out,
            &mut self.slots,
            &mut self.memory,
        );
        if rounded {
            let style = self.app.window_style();
            let window = ui.style();
            window.background = Some(style.background);
            window.border = Some(style.border);
            window.radius = [style.radius; 4];
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
