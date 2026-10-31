//! A button showing the picked choice, opening a list of them all under it.
//! Picking one closes it, as does a press anywhere else or Escape. While it's
//! open it has the keyboard: Up and Down mark a choice, Enter picks it. The
//! list scrolls once it's longer than [`ROWS`], and [`choice_list`] opens it
//! anywhere, without the button.
//!
//! With a [`Search`], what's typed while it's open narrows the choices to
//! those whose label or note holds it, labels starting with it first.
//! Backspace past its start closes it.

use crate::components::{ListScroll, indexed_list};
use crate::theme::{self, Theme};
use gui::canvas::FontId;
use gui::input::{CursorIcon, Key, PointerButton, TypedInput};
use gui::ui::{Align, Anchor, Border, Direction, Element, Length, TextCaret, TextStyle, Ui};

/// Whether a [`dropdown`] is open, owned by the caller.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Dropdown {
    open: bool,
    /// Moved by Up and Down, from the picked choice, as a place among the
    /// choices the query finds. `None` until they're pressed.
    marked: Option<usize>,
    /// Moves the list from where it hangs, such as to slide it in as it
    /// opens.
    pub offset: [f32; 2],
    list: ListScroll,
    /// What's typed to narrow a list with a [`Search`].
    query: String,
    /// The pointer was over the list last pass.
    over: bool,
}

/// One line of a [`dropdown`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Choice<'a> {
    pub label: &'a str,
    /// Its label's font, such as the font it names. The UI font if `None`.
    pub font: Option<FontId>,
    /// Dim and in code at the end of the line, such as its raw value.
    pub detail: Option<&'a str>,
    /// A smaller line under it. Every row is two lines tall if any has one.
    pub note: Option<&'a str>,
}

/// What a searched list shows over its choices: a title, then a box with
/// the query in it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Search<'a> {
    pub title: &'a str,
    /// In the box while the query is empty.
    pub placeholder: &'a str,
}

/// What the user did to a [`dropdown`] this pass, as places in its choices.
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
/// Choices shown at once.
const ROWS: usize = 8;
/// The narrowest a list gets, however narrow what it opens from.
const MIN_WIDTH: f32 = 200.;

impl Dropdown {
    pub const CLOSED: Self = Self {
        open: false,
        marked: None,
        offset: [0.; 2],
        list: ListScroll::TOP,
        query: String::new(),
        over: false,
    };
    pub const OPEN: Self = Self {
        open: true,
        marked: None,
        offset: [0.; 2],
        list: ListScroll::TOP,
        query: String::new(),
        over: false,
    };

    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.open
    }

    /// What's typed to narrow a searched list.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Whether the pointer was over the open list last pass, such as to
    /// leave a press there to it.
    #[must_use]
    pub const fn over(&self) -> bool {
        self.open && self.over
    }

    pub const fn close(&mut self) {
        self.open = false;
        self.marked = None;
    }

    fn show(&mut self, ui: &mut Ui, selected: usize) {
        *self = Self {
            offset: self.offset,
            ..Self::OPEN
        };
        self.reveal(selected);
        ui.focus();
    }

    /// Up and Down step the mark through `len` choices from `from`, wrapping,
    /// and typing narrows a `search`ed list. Returns whether Enter or Tab went
    /// down.
    fn keys(&mut self, ui: &Ui, from: Option<usize>, len: usize, search: bool) -> bool {
        let mut enter = false;
        for typed in ui.input().typed() {
            let press = match typed {
                TypedInput::Key(press) => press,
                TypedInput::Text(text) if search => {
                    self.query.push_str(text);
                    self.mark_first();
                    continue;
                }
                TypedInput::Text(_) => continue,
            };
            let at = self.marked.or(from);
            match press.key {
                Key::Down if len > 0 => self.marked = Some(at.map_or(0, |at| (at + 1) % len)),
                Key::Up if len > 0 => {
                    self.marked = Some(at.map_or(len - 1, |at| (at + len - 1) % len));
                }
                Key::Enter | Key::Tab => enter = true,
                Key::Escape => self.close(),
                Key::Backspace if search => match self.query.pop() {
                    Some(_) => self.mark_first(),
                    None => self.close(),
                },
                _ => {}
            }
            if let Some(marked) = self.marked {
                self.reveal(marked);
            }
        }
        enter
    }

    /// Marks the best match, as the query changes.
    const fn mark_first(&mut self) {
        self.marked = Some(0);
        self.list = ListScroll::TOP;
    }

    /// Scrolls the list just far enough to show choice `index`.
    fn reveal(&mut self, index: usize) {
        if index < self.list.top {
            self.list = ListScroll::at(index);
        } else if index >= self.list.top + ROWS {
            self.list = ListScroll::at(index + 1 - ROWS);
        }
    }
}

impl<'a> Choice<'a> {
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            label,
            font: None,
            detail: None,
            note: None,
        }
    }

    /// With `detail` at the end of its line.
    #[must_use]
    pub const fn detail(self, detail: &'a str) -> Self {
        Self {
            detail: Some(detail),
            ..self
        }
    }

    /// With `note` under it.
    #[must_use]
    pub const fn note(self, note: Option<&'a str>) -> Self {
        Self { note, ..self }
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
/// over everything else, with a check by the selected one, and `search` over
/// them once there are more than fit.
#[track_caller]
pub fn dropdown(
    ui: &mut Ui,
    state: &mut Dropdown,
    choices: &[Choice],
    selected: usize,
    search: Option<Search>,
) -> Chosen {
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
        if state.open
            && let Some(rect) = ui.rect()
        {
            // Its anchor ignores the parent's padding, so it's offset by it.
            let left = ui.style().padding[0];
            let float = Anchor::Parent {
                parent: [Align::Start, Align::End],
                own: [Align::Start; 2],
                offset: [state.offset[0] - left, state.offset[1] + DROP],
                clipped: false,
            };
            let search = search.filter(|_| choices.len() > ROWS);
            chosen = choice_list(ui, state, choices, Some(selected), search, rect.w, float);
        }
        let over_list = state.over();
        let pressed = ui.input().pressed(PointerButton::Left);
        let over_button = ui.hovered() && !over_list;
        if over_button && pressed {
            if state.open {
                state.close();
            } else {
                state.show(ui, selected);
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

/// An open `state`'s `choices` in a raised box `width` across, or
/// [`MIN_WIDTH`] if that's wider, floating at
/// `float`, with a check by `selected` and `search` over them. Up and Down
/// mark a choice, and typing searches, while the box it's in has the
/// keyboard.
#[track_caller]
pub fn choice_list(
    ui: &mut Ui,
    state: &mut Dropdown,
    choices: &[Choice],
    selected: Option<usize>,
    search: Option<Search>,
    width: f32,
    float: Anchor,
) -> Chosen {
    // With nothing picked yet, the first is marked.
    if selected.is_none() {
        state.marked = state.marked.or(Some(0));
    }
    let mut enter = false;
    if ui.focused() {
        let len = found(choices, &state.query).len();
        enter = state.keys(ui, selected, len, search.is_some());
    }
    state.over = false;
    if !state.open {
        return Chosen::default();
    }
    let found = found(choices, &state.query);
    state.marked = state.marked.filter(|&at| at < found.len());
    let at = |place: Option<usize>| place.and_then(|place| found.get(place).copied());
    let width = width.max(MIN_WIDTH);
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let noted = choices.iter().any(|choice| choice.note.is_some());
    let row = if noted { s.row + s.text } else { s.row };
    let height: f32 = found.iter().take(ROWS).map(|_| row).sum();
    let element = Element {
        size: [Length::Fixed(width), Length::Fit],
        padding: [DROP; 4],
        gap: DROP,
        background: Some(c.surface),
        border: Some(theme.border()),
        radius: [s.radius; 4],
        float: Some(float),
        ..Element::DEFAULT
    };
    let rows = Element {
        size: [Length::Grow, Length::Fixed(height)],
        ..Element::DEFAULT
    };
    let marked = state.marked;
    let (over, hovered, clicked) = ui.element(element, |ui| {
        if let Some(search) = search {
            search_box(ui, search, &state.query, found.is_empty());
        }
        let (mut hovered, mut clicked) = (None, None);
        ui.element(rows, |ui| {
            indexed_list(ui, &mut state.list, found.len(), row, |ui, place| {
                let Some(index) = at(Some(place)) else {
                    return;
                };
                let Some(choice) = choices.get(index) else {
                    return;
                };
                let lit = marked == Some(place);
                let size = [width - 2. * DROP, row];
                let (over, click) = choice_line(ui, choice, size, lit, selected == Some(index));
                if over {
                    hovered = Some(index);
                }
                if click {
                    clicked = Some(index);
                }
            });
        });
        (ui.hovered(), hovered, clicked)
    });
    state.over = over;
    let unsearched = selected.filter(|_| state.query.is_empty());
    let picked = enter.then(|| at(state.marked).or(unsearched)).flatten();
    Chosen {
        marked: hovered.or(at(state.marked)),
        picked: picked.or(clicked),
    }
}

/// Where the choices holding `query` are in `choices`: those whose label
/// starts with it, then whose label holds it, then whose note does. All of
/// them, in order, for an empty one.
fn found(choices: &[Choice], query: &str) -> Vec<usize> {
    if query.is_empty() {
        return (0..choices.len()).collect();
    }
    let query = query.to_lowercase();
    let mut found: Vec<(u8, usize)> = (0..)
        .zip(choices)
        .filter_map(|(index, choice)| {
            let label = choice.label.to_lowercase();
            let rank = if label.starts_with(&query) {
                0
            } else if label.contains(&query) {
                1
            } else if choice
                .note
                .is_some_and(|note| note.to_lowercase().contains(&query))
            {
                2
            } else {
                return None;
            };
            Some((rank, index))
        })
        .collect();
    found.sort_by_key(|&(rank, _)| rank);
    found.into_iter().map(|(_, index)| index).collect()
}

/// `search`'s title and a box with `query` in it, then a line saying
/// nothing matched if `none` did.
fn search_box(ui: &mut Ui, search: Search, query: &str, none: bool) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let small = |color| TextStyle {
        size: s.text - 1.,
        ..theme.ui_text(color)
    };
    let title = Element {
        padding: [s.gap / 2., DROP, s.gap / 2., 0.],
        ..Element::DEFAULT
    };
    let field = Element {
        background: Some(c.sunken),
        border: Some(Border {
            color: c.accent,
            ..theme.border()
        }),
        radius: [s.radius; 4],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        ..Element::line(s.row)
    };
    ui.element(title, |ui| ui.text(small(c.dim), search.title));
    ui.element(field, |ui| {
        let caret = TextCaret {
            index: query.len(),
            width: 2.,
            color: c.text,
        };
        if query.is_empty() {
            ui.text_layered(theme.ui_text(c.text), "", &[], Some(caret));
            ui.text(theme.ui_text(c.dim), search.placeholder);
        } else {
            ui.text_layered(theme.ui_text(c.text), query, &[], Some(caret));
        }
    });
    if none {
        ui.element(title, |ui| ui.text(small(c.dim), "Nothing matches"));
    }
}

/// One of a [`choice_list`]'s choices, `size` big, lit while `marked`
/// and checked while `selected`. Returns whether it's under the pointer, and
/// whether it was clicked.
#[track_caller]
fn choice_line(
    ui: &mut Ui,
    choice: &Choice,
    [width, height]: [f32; 2],
    marked: bool,
    selected: bool,
) -> (bool, bool) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let line = Element {
        // Its list's slots fit their rows, so it can't grow into one.
        size: [Length::Fixed(width), Length::Fixed(height)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        align: [Align::Start, Align::Center],
        radius: [s.radius; 4],
        cursor: Some(CursorIcon::Pointer),
        ..Element::DEFAULT
    };
    let top = Element {
        gap: s.icon_gap,
        ..Element::line(s.text + 4.)
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
    let small = |font| TextStyle {
        font,
        size: s.text - 2.,
        ..theme.ui_text(c.dim)
    };
    ui.element(line, |ui| {
        let over = ui.hovered();
        ui.style().background = (over || marked).then_some(c.hover);
        ui.element(top, |ui| {
            ui.element(label, |ui| ui.text(text(&theme, choice), choice.label));
            if let Some(detail) = choice.detail {
                ui.text(small(theme.font.buffer), detail);
            }
            if selected {
                ui.element(tick, |ui| ui.icon(theme.icon.check, s.icon, c.accent));
            }
        });
        if let Some(note) = choice.note {
            ui.element(label, |ui| ui.text(small(theme.font.ui), note));
        }
        (over, ui.clicked(PointerButton::Left))
    })
}

/// `choice`'s label in its own font.
fn text(theme: &Theme, choice: &Choice) -> TextStyle {
    TextStyle {
        font: choice.font.unwrap_or(theme.font.ui),
        ..theme.ui_text(theme.color.text)
    }
}
