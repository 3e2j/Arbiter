//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;

use assets::{Fonts, Icon, Icons};
use gui::{
    canvas::{Canvas, Color, FontId, Glyphs, Quad, Rect},
    host::{App, Startup},
    platform::window,
};

// TODO: from the user's Settings, once the editor reads them.
const PAGE: u32 = 0x0e_0f_11;
const RAISED: u32 = 0x17_18_1b;
const LINE: u32 = 0x2a_2c_31;
const TEXT: u32 = 0xe6_e6_e4;
const DIM: u32 = 0x9a_9b_98;
const GAP: f32 = 8.;
const RADIUS: f32 = 8.;
const TEXT_SIZE: f32 = 13.;
const ICON_SIZE: f32 = 16.;
const ROW: f32 = 22.;

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
struct Editor {
    fonts: Fonts,
    icons: Icons,
}

impl App for Editor {
    const TITLE: &str = "Arbiter";

    fn new(start: &mut Startup) -> Result<Self, Box<dyn std::error::Error>> {
        start.background = Color::hex(PAGE);
        Ok(Self {
            fonts: Fonts::load(start.glyphs)?,
            icons: Icons::load(start.glyphs)?,
        })
    }

    // TODO: everything below is a stand-in for the workspace, placed by hand until `gui::layout` exists.
    fn ui(&mut self, rect: Rect, canvas: &mut Canvas, glyphs: &mut Glyphs) {
        let panel = rect.inset(GAP);
        canvas.quad(
            Quad::new(panel, Color::hex(RAISED))
                .rounded(RADIUS)
                .bordered(1., Color::hex(LINE)),
        );
        let body = panel.inset(GAP * 2.);
        canvas.clip(body);
        let mut x = body.x;
        for icon in Icon::ALL {
            glyphs.icon(
                canvas,
                self.icons.get(icon),
                [x, body.y],
                ICON_SIZE,
                Color::hex(DIM),
            );
            x += ICON_SIZE + GAP;
        }
        let lines = [
            (self.fonts.ui, "The quick brown fox jumps over the lazy dog"),
            (self.fonts.ui, "いろはにほへと ちりぬるを わかよたれそ"),
            (self.fonts.buffer, "0O 1lI {}[]() => != 0x1f4"),
            (self.fonts.ui, "From a system font: 한국어"),
            (
                self.fonts.ui,
                "Colour emoji, which only has bitmaps, so a box: 🦀",
            ),
        ];
        let mut y = body.y + ICON_SIZE + GAP;
        for (font, line) in lines {
            let row = Rect::new(body.x, y, body.w, ROW);
            glyphs.text(
                canvas,
                font,
                [row.x, baseline(glyphs, font, row)],
                TEXT_SIZE,
                line,
                Color::hex(TEXT),
            );
            y += ROW;
        }
    }
}

/// The baseline that centres a line of `font` down `rect`.
fn baseline(glyphs: &Glyphs, font: FontId, rect: Rect) -> f32 {
    let line = glyphs.line_metrics(font, TEXT_SIZE);
    rect.y + (rect.h + line.ascent - line.descent) / 2.
}
