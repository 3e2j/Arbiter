//! A button showing the picked choice, opening a list of them all under it.
//! Picking one closes it, as does a press anywhere else or Escape. While it's
//! open it has the keyboard: Up and Down mark a choice, Enter picks it.

use crate::theme::{self, Theme};
use gui::canvas::FontId;
use gui::input::{CursorIcon, Key, PointerButton};
use gui::ui::{Align, Anchor, Border, Direction, Element, Length, TextStyle, Ui};

/// Whether a [`dropdown`] is open, owned by the caller.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Dropdown {
    open: bool,
    /// Moved by Up and Down, from the picked choice. `None` until they're
    /// pressed.
    marked: Option<usize>,
    /// Moves the list from where it hangs, such as to slide it in as it
    /// opens.
    pub offset: [f32; 2],
}

/// One line of a [`dropdown`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice<'a> {
    pub label: &'a str,
    /// Its label's font, such as the font it names. The UI font if `None`.
    pub font: Option<FontId>,
}

/// What the user did to a [`dropdown`] this pass.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Chosen {
    /// The choice under the pointer, or else the one the keys marked, while
    /// it's open. For trying a choice on before picking it.
    pub marked: Option<usize>,
    /// Clicked, or marked when Enter went down. Closes it.
    pub picked: Option<usize>,
}

/// Between the button and its list, and around the list's choices.
const DROP: f32 = 4.;

impl Dropdown {
    pub const CLOSED: Self = Self {
        open: false,
        marked: None,
        offset: [0.; 2],
    };

    #[must_use]
    pub const fn is_open(self) -> bool {
        self.open
    }

    pub const fn close(&mut self) {
        self.open = false;
        self.marked = None;
    }

    fn show(&mut self, ui: &mut Ui) {
        self.open = true;
        self.marked = None;
        ui.focus();
        // So the list shows this frame, not the next.
        ui.stale();
    }

    /// Up and Down step the mark through `len` choices from `from`, wrapping.
    /// Returns whether Enter went down.
    fn keys(&mut self, ui: &Ui, from: usize, len: usize) -> bool {
        let mut enter = false;
        for press in ui.input().keys() {
            let at = self.marked.unwrap_or(from);
            match press.key {
                Key::Down => self.marked = Some((at + 1) % len),
                Key::Up => self.marked = Some((at + len - 1) % len),
                Key::Enter => enter = true,
                Key::Escape => self.close(),
                _ => {}
            }
        }
        enter
    }
}

impl<'a> Choice<'a> {
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self { label, font: None }
    }

    /// Its label drawn in `font`.
    #[must_use]
    pub const fn font(self, font: FontId) -> Self {
        Self {
            font: Some(font),
            ..self
        }
    }
}

/// Fills its parent across, sunken, showing `choices[selected]` with a
/// chevron at the end. Pressing it opens `choices` in a raised box under it,
/// over everything else, with a check by the selected one.
#[track_caller]
pub fn dropdown(ui: &mut Ui, state: &mut Dropdown, choices: &[Choice], selected: usize) -> Chosen {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        background: Some(c.sunken),
        radius: [s.radius; 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    let label = Element {
        size: [Length::Grow, Length::Fit],
        clip: true,
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        if let Some(choice) = choices.get(selected) {
            ui.element(label, |ui| ui.text(text(&theme, choice), choice.label));
        }
        ui.icon(theme.icon.down, s.icon, c.dim);
        let mut chosen = Chosen::default();
        let mut over_list = false;
        if state.open {
            let enter = ui.focused() && state.keys(ui, selected, choices.len());
            if enter {
                chosen.picked = Some(state.marked.unwrap_or(selected));
            }
            if state.open
                && let Some(rect) = ui.rect()
            {
                let (over, hovered, clicked) =
                    list(ui, choices, selected, state.marked, rect.w, state.offset);
                over_list = over;
                chosen.marked = hovered.or(state.marked);
                chosen.picked = chosen.picked.or(clicked);
            }
        }
        let pressed = ui.input().pressed(PointerButton::Left);
        let over_button = ui.hovered() && !over_list;
        if over_button && pressed {
            if state.open {
                state.close();
            } else {
                state.show(ui);
            }
        } else if (pressed && !over_list) || chosen.picked.is_some() {
            state.close();
        }
        if !state.open {
            chosen.marked = None;
            ui.unfocus();
        }
        let color = if state.open {
            c.accent
        } else if over_button {
            c.dim
        } else {
            c.line
        };
        ui.style().border = Some(Border {
            color,
            ..theme.border()
        });
        chosen
    })
}

/// `choices` in a raised box `width` across, hanging under the open box,
/// moved by `offset`. Returns whether the pointer is over it,
/// the choice it's over, and the choice clicked.
#[track_caller]
fn list(
    ui: &mut Ui,
    choices: &[Choice],
    selected: usize,
    marked: Option<usize>,
    width: f32,
    offset: [f32; 2],
) -> (bool, Option<usize>, Option<usize>) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    // Its anchor ignores the parent's padding, so it's offset by it.
    let left = ui.style().padding[0];
    let element = Element {
        size: [Length::Fixed(width), Length::Fit],
        padding: [DROP; 4],
        background: Some(c.surface),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        float: Some(Anchor::Parent {
            parent: [Align::Start, Align::End],
            own: [Align::Start; 2],
            offset: [offset[0] - left, offset[1] + DROP],
            clipped: false,
        }),
        ..Element::DEFAULT
    };
    let line = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow, Length::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        radius: [s.radius; 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    let label = Element {
        size: [Length::Grow, Length::Fit],
        clip: true,
        ..Element::DEFAULT
    };
    let tick = Element {
        size: [Length::Fixed(s.icon); 2],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let (mut hovered, mut clicked) = (None, None);
        for (index, choice) in (0..).zip(choices) {
            ui.element(line, |ui| {
                let over = ui.hovered();
                if over {
                    hovered = Some(index);
                }
                ui.style().background = (over || marked == Some(index)).then_some(c.hover);
                ui.element(label, |ui| ui.text(text(&theme, choice), choice.label));
                ui.element(tick, |ui| {
                    if index == selected {
                        ui.icon(theme.icon.check, s.icon, c.accent);
                    }
                });
                if ui.clicked(PointerButton::Left) {
                    clicked = Some(index);
                }
            });
        }
        (ui.hovered(), hovered, clicked)
    })
}

/// `choice`'s label in its own font.
fn text(theme: &Theme, choice: &Choice) -> TextStyle {
    TextStyle {
        font: choice.font.unwrap_or(theme.font.ui),
        ..theme.ui_text(theme.color.text)
    }
}
