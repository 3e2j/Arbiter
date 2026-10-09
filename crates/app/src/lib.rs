//! The editor: everything the user sees that knows Arbiter. `gui` gives the
//! mechanisms, and this crate decides the policy.
//!
//! A panel moves out to its own crate when it brings heavy dependencies or
//! slows the build.

mod assets;
mod showcase;

use assets::{Fonts, Icons};
use gui::{
    canvas::Color,
    host::{App, Startup},
    platform::window,
    ui::{Border, Element, Ui},
};
use showcase::Showcase;

// TODO: from the user's Settings, once the editor reads them.
const PAGE: Color = Color::hex(0x0e_0f_11);
const RAISED: Color = Color::hex(0x17_18_1b);
const LINE: Color = Color::hex(0x2a_2c_31);
const TEXT: Color = Color::hex(0xe6_e6_e4);
const DIM: Color = Color::hex(0x9a_9b_98);
const HOVER: Color = Color::hex(0x24_26_2b);
const SELECTED: Color = Color::hex(0x33_36_3d);
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
    // TODO: becomes a panel in the workspace.
    showcase: Showcase,
}

impl App for Editor {
    const TITLE: &str = "Arbiter";

    fn new(start: &mut Startup) -> Result<Self, Box<dyn std::error::Error>> {
        start.background = PAGE;
        let fonts = Fonts::load(start.glyphs)?;
        let icons = Icons::load(start.glyphs)?;
        Ok(Self {
            showcase: Showcase::new(fonts, icons),
        })
    }

    fn ui(&mut self, ui: &mut Ui) {
        // TODO: a stand-in for the workspace and one dock, until
        // `app::workspace` exists.
        let dock = Element {
            gap: GAP,
            background: Some(RAISED),
            border: Some(Border {
                width: 1.,
                color: LINE,
            }),
            radius: RADIUS,
            clip: true,
            ..Element::column().padded(GAP * 2.)
        };
        ui.element(Element::column().padded(GAP), |ui| {
            ui.element(dock, |ui| self.showcase.ui(ui));
        });
    }
}
