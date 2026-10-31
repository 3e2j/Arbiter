//! The open project's files, as the Game or the Project tree, one at a time.
//! See [`project::tree`].
//!
//! Includes Search, Filter, and View options.

mod row;
mod tab_state;

use crate::components::{
    self, IconState, TextField, icon_button, list, notice, rule, text_field, toolbar,
};
use crate::keymap::{AppCommand, Chord, KeyContext};
use gui::{
    input::{CursorIcon, Key, Modifiers, PointerButton},
    ui::{Align, Direction, Element, Length, Ui},
};
use serde::{Deserialize, Serialize};

use project::tree::{NodeKind, SortKey, SortOrder, Tree, TreeFilter, Trees};

use crate::assets::Icon;
use crate::commands::Command;

use super::{Context, Event, Reply};
use row::{Columns, row_ui};
use tab_state::TabState;

use crate::components::popover::{WIDTH as POPOVER, check, heading};
use crate::theme;

/// While the panel has focus.
pub const FILES: KeyContext = KeyContext("files");
/// Focuses the search, Ctrl+F by default.
const SEARCH: AppCommand = AppCommand("files.search");

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Tab {
    Project,
    Game,
}

impl Tab {
    // In display order.
    const ALL: [Self; 2] = [Self::Project, Self::Game];

    const fn title(self) -> &'static str {
        match self {
            Self::Game => "Game",
            Self::Project => "Project",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Popover {
    /// Which rows show.
    Filter,
    /// Their order, and how they're shown.
    View,
}

/// How the panel shows its trees, as the session keeps it. Anything left
/// out is as a new panel has it.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct FilesView {
    tab: Tab,
    /// [`TreeFilter::modded`]. Its editions are bits in one read's edition
    /// list, so they aren't kept.
    modded: bool,
    compact: bool,
    letters: bool,
    order: SortOrder,
}

impl Default for FilesView {
    fn default() -> Self {
        Self {
            tab: Tab::Project,
            modded: false,
            compact: true,
            letters: true,
            order: SortOrder::default(),
        }
    }
}

/// An open project's trees, and what each tab remembers of its own.
struct Opened {
    trees: Trees,
    /// By [`Tab`].
    states: [TabState; Tab::ALL.len()],
}

pub struct FileTree {
    /// `None` until a project is open.
    opened: Option<Box<Opened>>,
    tab: Tab,
    search: TextField,
    /// The Game tree's, as everything in the Project tree is a change.
    filter: TreeFilter,
    /// Folders holding only one folder share its line.
    compact: bool,
    /// Each file's edition letters, when it isn't in every edition.
    letters: bool,
    /// Both trees'.
    order: SortOrder,
    popover: Option<Popover>,
}

impl FileTree {
    /// Once for every file tree.
    pub const COMMANDS: [Command; 1] = [Command {
        id: SEARCH,
        name: "Search files",
        context: FILES,
        chords: &[Chord::new(Key::Char('f'), Modifiers::COMMAND)],
    }];

    pub(super) fn new(view: FilesView) -> Self {
        Self {
            opened: None,
            tab: view.tab,
            search: TextField::default(),
            filter: TreeFilter {
                modded: view.modded,
                ..TreeFilter::default()
            },
            compact: view.compact,
            letters: view.letters,
            order: view.order,
            popover: None,
        }
    }

    pub(super) const fn view(&self) -> FilesView {
        FilesView {
            tab: self.tab,
            modded: self.filter.modded,
            compact: self.compact,
            letters: self.letters,
            order: self.order,
        }
    }

    /// Shows `trees` in place of what was shown, every folder closed.
    fn set(&mut self, mut trees: Trees) {
        if self.order != SortOrder::default() {
            trees.game.sort(self.order);
            trees.project.sort(self.order);
        }
        let states = [TabState::new(), TabState::new()];
        self.opened = Some(Box::new(Opened { trees, states }));
        self.filter.editions = 0;
    }

    /// Shows `trees`, read again, in place of what was shown. Each tab keeps
    /// what it had open and selected wherever the same path still is.
    fn refresh(&mut self, mut trees: Trees) {
        let Some(opened) = self.opened.as_deref_mut() else {
            return self.set(trees);
        };
        if self.order != SortOrder::default() {
            trees.game.sort(self.order);
            trees.project.sort(self.order);
        }
        if trees.game.editions != opened.trees.game.editions {
            self.filter.editions = 0;
        }
        let [project, game] = &mut opened.states;
        project.remap(&opened.trees.project.ids_in(&trees.project));
        game.remap(&opened.trees.game.ids_in(&trees.game));
        opened.trees = trees;
    }

    /// The filter as `tab`'s tree takes it.
    fn filter(&self, tab: Tab) -> TreeFilter {
        match tab {
            Tab::Game => self.filter,
            Tab::Project => TreeFilter::default(),
        }
    }

    /// Sorts both trees by `order`, keeping what each tab has open.
    fn sort(&mut self, order: SortOrder) {
        self.order = order;
        let Some(opened) = self.opened.as_deref_mut() else {
            return;
        };
        let sort = |tree: &mut Tree| -> Vec<_> { tree.sort(order).into_iter().map(Some).collect() };
        let [project, game] = &mut opened.states;
        project.remap(&sort(&mut opened.trees.project));
        game.remap(&sort(&mut opened.trees.game));
    }

    /// Rebuilds every tab's rows.
    fn restale(&mut self) {
        for state in self.opened.iter_mut().flat_map(|opened| &mut opened.states) {
            state.stale = true;
        }
    }

    /// Rebuilds every tab's rows from the top for a new search or filter.
    fn renarrow(&mut self) {
        for state in self.opened.iter_mut().flat_map(|opened| &mut opened.states) {
            state.renarrow();
        }
    }
    /// The search across, then Filter on the Game tab, then View.
    fn toolbar(&mut self, ui: &mut Ui) {
        toolbar(ui, |ui| {
            let icon = Some(Icon::Search.into());
            if text_field(ui, &mut self.search, icon, "Search") {
                self.renarrow();
            }
            if self.tab == Tab::Game {
                let set = !self.filter.is_empty();
                self.popover_button(ui, Popover::Filter, Icon::Filter, set);
            }
            self.popover_button(ui, Popover::View, Icon::View, false);
        });
    }

    /// A button with `icon` that toggles `popover` under the right end of
    /// the box `ui` has open, lit while it's open or while `set` says what it
    /// holds narrows the tree.
    fn popover_button(&mut self, ui: &mut Ui, popover: Popover, icon: Icon, set: bool) {
        let open = self.popover == Some(popover);
        let state = if open || set {
            IconState::Lit
        } else {
            IconState::Normal
        };
        if icon_button(ui, icon, state).pressed {
            self.popover = (!open).then_some(popover);
        }
        if self.popover != Some(popover) {
            return;
        }
        let ((), outside) = components::popover(ui, POPOVER, |ui| match popover {
            Popover::Filter => self.filter_ui(ui),
            Popover::View => self.view_ui(ui),
        });
        if outside {
            self.popover = None;
        }
    }

    /// What the mod changed and each edition, ticked to show only what it
    /// has that another doesn't.
    fn filter_ui(&mut self, ui: &mut Ui) {
        let mut filter = self.filter;
        heading(ui, "Show only");
        if check(ui, "Changed", filter.modded) {
            filter.modded = !filter.modded;
        }
        let editions = self
            .opened
            .as_ref()
            .map_or(&[][..], |opened| &opened.trees.game.editions);
        if editions.len() > 1 {
            heading(ui, "Differing in");
            for (bit, id) in (0..).zip(editions) {
                if check(ui, id, filter.editions >> bit & 1 != 0) {
                    filter.editions ^= 1 << bit;
                }
            }
        }
        if filter != self.filter {
            self.filter = filter;
            self.renarrow();
        }
    }

    /// Each [`SortKey`], then each direction, the one in use ticked. Then how
    /// rows are shown.
    fn view_ui(&mut self, ui: &mut Ui) {
        let mut order = self.order;
        heading(ui, "Sort by");
        for (key, label) in [(SortKey::Name, "Name"), (SortKey::Type, "Type")] {
            if check(ui, label, order.key == key) {
                order.key = key;
            }
        }
        heading(ui, "Order");
        for (descending, label) in [(false, "Ascending"), (true, "Descending")] {
            if check(ui, label, order.descending == descending) {
                order.descending = descending;
            }
        }
        if order != self.order {
            self.sort(order);
        }
        heading(ui, "Display");
        if check(ui, "Compact folders", self.compact) {
            self.compact = !self.compact;
            self.restale();
        }
        if check(ui, "Edition letters", self.letters) {
            self.letters = !self.letters;
        }
    }

    /// A tab for each tree along the top, the shown one underlined, over a
    /// line across the panel.
    fn tabs(&mut self, ui: &mut Ui) {
        let theme = theme::current();
        let (c, s) = (theme.color, theme.size);
        let strip = Element {
            direction: Direction::LeftToRight,
            size: [Length::Grow, Length::Fit],
            padding: [s.gap, 0., s.gap / 2., 0.],
            align: [Align::Start, Align::Center],
            ..Element::DEFAULT
        };
        let tab = Element {
            size: [Length::Fit; 2],
            cursor: Some(CursorIcon::Pointer),
            ..Element::column()
        };
        let label = Element {
            size: [Length::Fit, Length::Fixed(s.row + s.gap)],
            padding: [s.gap, 0., s.gap, 0.],
            align: [Align::Center; 2],
            ..Element::DEFAULT
        };
        let underline = Element {
            size: [Length::Grow, Length::Fixed(2. * s.line)],
            ..Element::DEFAULT
        };
        let top = Element {
            size: [Length::Grow, Length::Fit],
            ..Element::column()
        };
        ui.element(top, |ui| {
            ui.element(strip, |ui| {
                for each in Tab::ALL {
                    let on = each == self.tab;
                    let clicked = ui.element(tab, |ui| {
                        let color = if on || ui.hovered() { c.text } else { c.dim };
                        ui.element(label, |ui| ui.text(theme.ui_text(color), each.title()));
                        let underline = Element {
                            background: on.then_some(c.text),
                            ..underline
                        };
                        ui.element(underline, |_| ());
                        ui.clicked(PointerButton::Left)
                    });
                    if clicked && each != self.tab {
                        self.tab = each;
                        self.popover = None;
                    }
                }
            });
            rule(ui);
        });
    }
}

impl FileTree {
    /// Takes each tree read again, as its own copy to sort.
    pub(super) fn receive(&mut self, event: &mut Event) -> Reply {
        if let Event::Read(trees) = event {
            self.refresh(Trees::clone(trees));
        }
        Reply::Pass
    }

    /// Does `commands`, the [`FILES`] ones pressed while it has focus.
    pub(super) fn act(&mut self, commands: &[AppCommand]) {
        if self.opened.is_some() && commands.contains(&SEARCH) {
            self.search.focus = true;
        }
    }

    pub(super) fn ui(&mut self, ui: &mut Ui, cx: &mut Context) {
        self.tabs(ui);
        if self.opened.is_none() {
            return notice(ui, "No project open");
        }
        self.toolbar(ui);
        let query = self.search.text.trim();
        let filter = self.filter(self.tab);
        let Some(opened) = self.opened.as_deref_mut() else {
            return;
        };
        let tree = match self.tab {
            Tab::Game => &opened.trees.game,
            Tab::Project => &opened.trees.project,
        };
        let state = &mut opened.states[self.tab as usize];
        if tree.is_empty() {
            return notice(ui, "Nothing here yet");
        }
        state.rebuild(tree, query, filter, self.compact);
        if state.rows.is_empty() {
            return notice(ui, "No matches");
        }
        let height = theme::current().size.row;
        let columns = Columns {
            gutter: self.tab == Tab::Game,
            letters: self.letters,
        };
        let mut clicked = None;
        let mut scroll = state.scroll;
        list(ui, &mut scroll, &state.rows, height, |ui, &row| {
            let open = state.is_open(row.last);
            let selected = state.selected == Some(row.last);
            if row_ui(ui, tree, row, open, selected, columns) {
                clicked = Some(row.last);
            }
        });
        state.scroll = scroll;
        if let Some(id) = clicked {
            state.selected = Some(id);
            if tree[id].kind == NodeKind::File && ui.input().pressed_again(PointerButton::Left) {
                let mut path = String::new();
                let found = match self.tab {
                    Tab::Game => opened.trees.game_file(id, &mut path),
                    Tab::Project => {
                        tree.path(id, &mut path);
                        true
                    }
                };
                if found {
                    cx.post(Event::Pick(path));
                } else {
                    tracing::info!(
                        "{} is only a change, with nothing to show yet",
                        tree.name(id)
                    );
                }
            }
            if tree[id].kind == NodeKind::Folder {
                state.toggle(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use project::tree::SortKey;

    use super::*;

    #[test]
    fn a_files_view_comes_back_from_its_state() {
        let text = r#"
            tab = "game"
            modded = true
            compact = false
            order = { key = "type", descending = true }
        "#;
        let view: FilesView = toml::from_str(text).unwrap();
        let again = FileTree::new(view).view();
        assert_eq!(again, view);
        let FilesView {
            tab,
            modded,
            compact,
            letters,
            order,
        } = again;
        assert_eq!(tab, Tab::Game);
        assert!(modded && !compact && letters);
        assert_eq!(
            order,
            SortOrder {
                key: SortKey::Type,
                descending: true
            }
        );
    }
}
