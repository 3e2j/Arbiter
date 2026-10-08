//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

use gui::{
    canvas::{Canvas, Color, Quad, Rect},
    host::{App, Startup},
    platform::window,
};

// TODO: from the user's Settings, once the editor reads them.
const PAGE: u32 = 0x0e_0f_11;
const RAISED: u32 = 0x17_18_1b;
const LINE: u32 = 0x2a_2c_31;
const DIM: u32 = 0x9a_9b_98;
const ACCENT: u32 = 0xbe_95_ff;
const GAP: f32 = 8.;
const RADIUS: f32 = 8.;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Window(#[from] window::Error),
}

/// Opens the editor in a window and blocks until it's closed.
///
/// # Errors
///
/// When the editor can't start, or the window fails.
pub fn run() -> Result<(), Error> {
    window::run::<Editor>()?;
    Ok(())
}

/// The editor, as one [`App`]. Owns what only Arbiter knows about.
// TODO: owns `Workspace` and `Watch` once `app::workspace` and `app::watch` exist.
struct Editor;

impl App for Editor {
    const TITLE: &str = "Arbiter";

    fn new(start: &mut Startup) -> Result<Self, Box<dyn std::error::Error>> {
        start.background = Color::hex(PAGE);
        Ok(Self)
    }

    // TODO: everything below is a stand-in for the workspace, placed by hand until `gui::layout` exists.
    fn ui(&mut self, rect: Rect, canvas: &mut Canvas) {
        let area = rect.inset(GAP);
        let left = Rect::new(area.x, area.y, 240., area.h);
        let main_x = left.right() + GAP;
        let main = Rect::new(main_x, area.y, area.right() - main_x, area.h * 0.7);
        let bottom_y = main.bottom() + GAP;
        let bottom = Rect::new(main.x, bottom_y, main.w, area.bottom() - bottom_y);
        for dock in [left, main, bottom] {
            canvas.quad(
                Quad::new(dock, Color::hex(RAISED))
                    .rounded(RADIUS)
                    .bordered(1., Color::hex(LINE)),
            );
        }
        tabs(canvas, main);
        rows(canvas, left);
    }
}

fn tabs(canvas: &mut Canvas, dock: Rect) {
    let mut x = dock.x + GAP;
    for (i, width) in (0..).zip([96., 128., 80.]) {
        let tab = Rect::new(x, dock.y + GAP, width, 28.);
        let quad = Quad::new(tab, Color::hex(LINE)).rounded(6.);
        canvas.quad(if i == 0 {
            quad.bordered(1., Color::hex(ACCENT))
        } else {
            quad
        });
        x = tab.right() + 4.;
    }
}

/// More rows than fit, cut off at the dock's edge.
fn rows(canvas: &mut Canvas, dock: Rect) {
    let body = dock.inset(GAP);
    canvas.clip(body);
    let mut y = body.y;
    for depth in [0., 1., 1., 2., 2., 1., 0., 1., 2., 3., 3., 2., 0.]
        .iter()
        .cycle()
        .take(80)
    {
        let indent = depth * 14.;
        let row = Rect::new(body.x + indent, y, body.w - indent + 40., 18.);
        canvas.quad(Quad::new(row, Color::hex(DIM).alpha(0.25)).rounded(4.));
        y += 22.;
    }
}
