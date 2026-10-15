//! The user's settings: a gear in the title bar, and the panel it opens over
//! the workspace. Pages down the side, the open one's rows on the right.
//! Each page lives in its own module and draws its rows from [`widgets`].

mod appearance;
mod editor;
mod file;
mod widgets;

use std::path::PathBuf;

use gui::{
    anim::{Tween, ease_out},
    canvas::Color,
    components::{IconState, Scroll, icon_button, notice, rule, scroll, spacer},
    input::{CursorIcon, Key, Modifiers, PointerButton},
    keymap::{AppCommand, Chord, ChordAction, KeyContext, Keymap},
    ui::{Align, Anchor, Direction, Element, Length, TextStyle, Ui},
};

use crate::assets::Icon;
use crate::theme::{HOVER_THUMB, Palette};
use appearance::Appearance;
pub use appearance::Zoom;
use editor::Editor;
use widgets::sized;

const TOGGLE: ChordAction = ChordAction::Command(AppCommand("settings.toggle"));
const ZOOM_IN: ChordAction = ChordAction::Command(AppCommand("zoom.in"));
const ZOOM_OUT: ChordAction = ChordAction::Command(AppCommand("zoom.out"));
const ZOOM_RESET: ChordAction = ChordAction::Command(AppCommand("zoom.reset"));

/// The panel's size, and how close it comes to the edges of the room under
/// the title bar before it shrinks, down to its least.
const MAX: [f32; 2] = [820., 580.];
const MIN: [f32; 2] = [560., 400.];
const MARGIN: f32 = 96.;
const SIDEBAR: f32 = 200.;
const ITEM: f32 = 34.;
const FOOTER: f32 = 44.;

/// How far the panel rises as it opens, and a page as it swaps in.
const RISE: f32 = 6.;
const OPEN_SECONDS: f32 = 0.2;
const SWAP_SECONDS: f32 = 0.16;
/// Over the workspace while open, at its darkest.
const SCRIM: Color = Color([4, 5, 7, 140]);

/// What the sidebar lists.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum Page {
    #[default]
    Appearance,
    Editor,
    Tools,
    Keybinds,
}

#[derive(Default)]
pub struct Settings {
    /// Where they're kept, unless there's nowhere to.
    path: Option<PathBuf>,
    /// As last read or written, to write again only once they change.
    written: file::Saved,
    open: bool,
    page: Page,
    appearance: Appearance,
    editor: Editor,
    /// The panel coming in.
    opening: Tween,
    /// The open page coming in.
    swap: Tween,
    /// Fades the scrim while a preview is worn, so it shows on the workspace.
    lift: Tween,
    /// The open page's rows, when they don't fit.
    scroll: Scroll,
}

impl Page {
    const ALL: [Self; 4] = [Self::Appearance, Self::Editor, Self::Tools, Self::Keybinds];

    const fn name(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Editor => "Editor",
            Self::Tools => "Tools",
            Self::Keybinds => "Keybinds",
        }
    }

    const fn icon(self) -> Icon {
        match self {
            Self::Appearance => Icon::Palette,
            Self::Editor => Icon::File,
            Self::Tools => Icon::Hammer,
            Self::Keybinds => Icon::Keyboard,
        }
    }
}

impl Settings {
    /// Ctrl+, opens and closes it, and Ctrl with +, - or 0 steps the zoom,
    /// from anywhere. Both + and = zoom in, for layouts that need Shift for +.
    pub fn bind(keymap: &mut Keymap) {
        let binds = [
            (',', TOGGLE),
            ('+', ZOOM_IN),
            ('=', ZOOM_IN),
            ('-', ZOOM_OUT),
            ('0', ZOOM_RESET),
        ];
        for (key, action) in binds {
            let chord = Chord::new(Key::Char(key), Modifiers::COMMAND);
            keymap.bind(chord, KeyContext::GLOBAL, Some(action));
        }
    }

    /// As the settings file left them.
    pub fn load() -> Self {
        let path = file::path();
        if path.is_none() {
            tracing::warn!("there's no folder for settings, so they won't be kept");
        }
        let written = path.as_deref().map(file::read).unwrap_or_default();
        Self {
            path,
            written,
            appearance: Appearance::new(written.appearance),
            editor: Editor::new(written.editor),
            ..Self::default()
        }
    }

    /// Writes the settings file, if anything changed since it was last.
    pub fn save(&mut self) {
        let saved = file::Saved {
            appearance: self.appearance.saved(),
            editor: self.editor.saved(),
        };
        if saved == self.written {
            return;
        }
        // Even if it fails, so it's tried once a change rather than a pass.
        self.written = saved;
        if let Some(path) = &self.path
            && let Err(err) = file::write(path, saved)
        {
            tracing::warn!("can't save the settings: {err}");
        }
    }

    pub const fn palette(&self) -> Palette {
        self.appearance.palette()
    }

    /// The palette to wear, if it changed since the last call.
    pub fn take_changed(&mut self) -> Option<Palette> {
        self.appearance.take_changed()
    }

    pub const fn zoom(&self) -> Zoom {
        self.appearance.zoom()
    }

    /// The zoom to draw at, if it changed since the last call.
    pub fn take_zoom(&mut self) -> Option<Zoom> {
        self.appearance.take_zoom()
    }

    fn show(&mut self, open: bool) {
        self.open = open;
        self.appearance.leave();
        if open {
            self.opening = Tween::START;
            self.opening.go(true);
            self.swap = Tween::END;
            self.lift = Tween::START;
            self.scroll.offset = 0.;
            self.scroll.thumb = HOVER_THUMB;
        }
    }

    fn pick(&mut self, page: Page) {
        if page == self.page {
            return;
        }
        self.page = page;
        self.appearance.leave();
        self.swap = Tween::START;
        self.swap.go(true);
        self.scroll.offset = 0.;
    }

    /// The gear, lit while the panel is open. Where its keys are acted on,
    /// and what gui keeps is handed to it, before the boxes that read it.
    pub fn gear(&mut self, ui: &mut Ui) {
        self.editor.apply(ui);
        let mut toggled = false;
        for &press in ui.input().keys() {
            match ui.keymap().resolve(press, &[KeyContext::GLOBAL]) {
                Some(TOGGLE) => toggled = true,
                Some(ZOOM_IN) => self.appearance.step_zoom(true),
                Some(ZOOM_OUT) => self.appearance.step_zoom(false),
                Some(ZOOM_RESET) => self.appearance.reset_zoom(),
                _ => {}
            }
        }
        let state = if self.open {
            IconState::Lit
        } else {
            IconState::Normal
        };
        if icon_button(ui, Icon::Settings, state).pressed || toggled {
            self.show(!self.open);
        }
    }

    /// The panel over the open box while it's open. Declare it last in the
    /// box it covers, so it draws over what's there.
    pub fn panel(&mut self, ui: &mut Ui) {
        // Before the closed check, so it catches the pass that closed it.
        self.save();
        if !self.open {
            return;
        }
        // Not while a box in it has the keyboard, which takes Escape itself.
        if !ui.typing()
            && ui
                .input()
                .keys()
                .iter()
                .any(|press| press.key == Key::Escape)
        {
            self.show(false);
            return;
        }
        let Some(room) = ui.rect() else { return };
        let theme = ui.theme();
        let (c, s) = (theme.color, theme.size);
        let shown = ease_out(self.opening.step(ui, OPEN_SECONDS));
        self.lift.go(self.appearance.previewing());
        let lifted = self.lift.step(ui, SWAP_SECONDS);
        let scrim = Element {
            size: [Length::Fixed(room.w), Length::Fixed(room.h)],
            align: [Align::Center; 2],
            background: Some(SCRIM.faded(shown * (1. - lifted))),
            offset: [0., RISE * (1. - shown)],
            float: Some(Anchor::At([room.x, room.y])),
            ..Element::DEFAULT
        };
        let [w, h] = [0, 1].map(|axis| {
            let room = [room.w, room.h][axis];
            (room - MARGIN)
                .max(MIN[axis])
                .min(MAX[axis])
                .min(room - 2. * s.gap)
        });
        // Inside its border, so what's in it doesn't paint over it.
        let inner = s.window_radius - s.line;
        let panel = Element {
            size: [Length::Fixed(w), Length::Fixed(h)],
            padding: [s.line; 4],
            background: Some(c.surface),
            border: Some(theme.border()),
            radius: [s.window_radius; 4],
            clip: true,
            ..Element::DEFAULT
        };
        let close = ui.element(scrim, |ui| {
            let (over, close) = ui.element(panel, |ui| {
                let close = ui.element(Element::row(), |ui| {
                    self.sidebar(ui, inner);
                    rule(ui);
                    self.main(ui)
                });
                rule(ui);
                footer(ui, inner);
                (ui.hovered(), close)
            });
            close || (ui.pressed(PointerButton::Left) && !over)
        });
        if close {
            self.show(false);
        }
    }

    /// The pages, under the panel's title.
    fn sidebar(&mut self, ui: &mut Ui, radius: f32) {
        let theme = ui.theme();
        let (c, s) = (theme.color, theme.size);
        let element = Element {
            size: [Length::Fixed(SIDEBAR), Length::Grow],
            padding: [10., 14., 10., 10.],
            gap: 2.,
            background: Some(c.page),
            radius: [radius, 0., 0., 0.],
            ..Element::DEFAULT
        };
        // Its gear lines up with the pages' icons.
        let title = Element {
            direction: Direction::LeftToRight,
            padding: [12., 0., 0., 14.],
            gap: 10.,
            align: [Align::Start, Align::Center],
            ..Element::DEFAULT
        };
        let item = Element {
            padding: [12., 0., 10., 0.],
            gap: 10.,
            radius: [6.; 4],
            ..Element::line(ITEM)
        };
        // Grows from its middle as its page swaps in.
        let grown = ease_out(self.swap.progress());
        let bar = ITEM - 18.;
        let marker = Element {
            size: [Length::Fixed(3.), Length::Fixed(bar * grown)],
            background: Some(c.accent),
            radius: [1.5; 4],
            float: Some(Anchor::Parent {
                parent: [Align::Start; 2],
                own: [Align::Start; 2],
                offset: [3., 9. + bar * (1. - grown) / 2.],
                clipped: true,
            }),
            ..Element::DEFAULT
        };
        ui.element(element, |ui| {
            ui.element(title, |ui| {
                ui.icon(Icon::Settings, s.icon, c.dim);
                ui.text(sized(theme, 3., c.text), "Settings");
            });
            for page in Page::ALL {
                let on = page == self.page;
                let element = Element {
                    cursor: (!on).then_some(CursorIcon::Pointer),
                    ..item
                };
                let clicked = ui.element(element, |ui| {
                    let hovered = ui.hovered();
                    let (background, color) = if on {
                        (Some(c.selected), c.accent)
                    } else if hovered {
                        (Some(c.hover), c.text)
                    } else {
                        (None, c.dim)
                    };
                    ui.style().background = background;
                    if on {
                        ui.element(marker, |_| ());
                    }
                    ui.icon(page.icon(), s.icon, color);
                    ui.text(theme.ui_text(color), page.name());
                    ui.clicked(PointerButton::Left)
                });
                if clicked {
                    self.pick(page);
                }
            }
        });
    }

    /// The open page: what it is, then its rows. Returns whether its close
    /// button was clicked.
    fn main(&mut self, ui: &mut Ui) -> bool {
        let theme = ui.theme();
        let (c, s) = (theme.color, theme.size);
        let head = Element {
            direction: Direction::LeftToRight,
            size: [Length::Grow, Length::Fit],
            padding: [22., 16., 12., 12.],
            gap: 12.,
            align: [Align::Start, Align::Center],
            ..Element::DEFAULT
        };
        let badge = Element {
            size: [Length::Fixed(32.); 2],
            align: [Align::Center; 2],
            background: Some(c.selected),
            radius: [8.; 4],
            ..Element::DEFAULT
        };
        let swapped = ease_out(self.swap.step(ui, SWAP_SECONDS));
        let body = Element {
            padding: [14., 10., 14., 18.],
            offset: [0., RISE * (1. - swapped)],
            ..Element::column()
        };
        let page = self.page;
        ui.element(Element::column(), |ui| {
            let close = ui.element(head, |ui| {
                ui.element(badge, |ui| ui.icon(page.icon(), s.icon, c.accent));
                ui.text(sized(theme, 3., c.text), page.name());
                spacer(ui);
                icon_button(ui, Icon::Close, IconState::Normal).clicked
            });
            rule(ui);
            scroll(ui, &mut self.scroll, body, |ui| {
                match page {
                    Page::Appearance => self.appearance.ui(ui),
                    Page::Editor => self.editor.ui(ui),
                    Page::Tools | Page::Keybinds => {
                        notice(ui, "Nothing to set here yet.");
                    }
                }
                fade_in(ui, swapped);
            });
            close
        })
    }
}

/// The keys that work, along the panel's bottom.
fn footer(ui: &mut Ui, radius: f32) {
    let theme = ui.theme();
    let c = theme.color;
    let element = Element {
        padding: [10., 0., 10., 0.],
        gap: 4.,
        background: Some(c.page),
        radius: [0., 0., radius, radius],
        ..Element::line(FOOTER)
    };
    let key = Element {
        min: [20., 20.],
        padding: [5., 0., 5., 0.],
        align: [Align::Center; 2],
        background: Some(c.surface),
        border: Some(theme.border()),
        radius: [4.; 4],
        ..Element::DEFAULT
    };
    let mono = TextStyle {
        font: theme.font.buffer,
        ..sized(theme, -3., c.text)
    };
    ui.element(element, |ui| {
        spacer(ui);
        ui.element(key, |ui| ui.text(mono, "Esc"));
        ui.text(sized(theme, -1., c.dim), "Close");
    });
}

/// Fades the open box in as `shown` goes from 0 to 1, by covering it in the
/// colour it sits on, since gui can't draw a box translucent as a whole.
fn fade_in(ui: &mut Ui, shown: f32) {
    let Some(rect) = ui.rect().filter(|_| shown < 1.) else {
        return;
    };
    let cover = Element {
        size: [Length::Fixed(rect.w), Length::Fixed(rect.h)],
        background: Some(ui.theme().color.surface.faded(1. - shown)),
        float: Some(Anchor::Parent {
            parent: [Align::Start; 2],
            own: [Align::Start; 2],
            offset: [0.; 2],
            clipped: true,
        }),
        ..Element::DEFAULT
    };
    ui.element(cover, |_| ());
}
