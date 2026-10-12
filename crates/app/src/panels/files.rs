//! The open project's files, as the Game or the Project tree, one at a time.
//! See [`project::tree`].
//!
//! Includes Search, Filter, and View options.

use gui::{
    components::{
        self, IconState, ListScroll, TextField, icon_button, list, notice, rule, text_field,
        toolbar,
    },
    input::{CursorIcon, Key, Modifiers, PointerButton},
    keymap::{AppCommand, Chord, ChordAction, KeyContext, Keymap},
    ui::{Align, Border, Direction, Element, Length, TextStyle, Ui},
};
use project::{
    git::GitStatus,
    tree::{
        Narrowed, NodeId, NodeKind, NodeStatus, SortKey, SortOrder, Tree, TreeFilter, TreeRow,
        Trees,
    },
};

use crate::assets::Icon;
use crate::theme;
use crate::workspace::Places;

/// Width of the gutter's bar.
const BAR: f32 = 3.;
/// How strong a folder's gutter bar is beside a file's.
// Low alpha so the color is noticeably different
const INSIDE: u8 = 0x39;
/// Width of the Filter and View popovers.
const POPOVER: f32 = 220.;

/// While the panel is active, see [`Files::active`].
const FILES: KeyContext = KeyContext("files");
/// Focuses the search, Ctrl+F by default.
const SEARCH: ChordAction = ChordAction::Command(AppCommand("files.search"));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tab {
    Project,
    Game,
}

impl Tab {
    // In order of appearance
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

/// The open folders, a bit per node by [`NodeId::index`].
#[derive(Default)]
struct Open(Vec<u64>);

/// What a tab remembers of its tree, so switching back finds it as it was.
#[derive(Default)]
struct View {
    open: Open,
    /// Folders clicked while searching or filtering, which flip how the
    /// results show them. Kept apart from `open`, and emptied when the results
    /// change.
    flipped: Open,
    narrowed: Narrowed,
    selected: Option<NodeId>,
    scroll: ListScroll,
    /// The lines shown, rebuilt when `stale`.
    rows: Vec<TreeRow>,
    /// Something changed what's shown, so `rows` are rebuilt next pass.
    stale: bool,
}

/// An open project's trees, and what each tab remembers of its own.
struct Opened {
    trees: Trees,
    /// By [`Tab`].
    views: [View; Tab::ALL.len()],
}

pub struct Files {
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
    /// The last press landed in the panel, so its [`FILES`] keys apply.
    active: bool,
}

impl Open {
    fn has(&self, id: NodeId) -> bool {
        let i = id.index();
        self.0
            .get(i / 64)
            .is_some_and(|word| word >> (i % 64) & 1 != 0)
    }

    fn toggle(&mut self, id: NodeId) {
        let i = id.index();
        if self.0.len() <= i / 64 {
            self.0.resize(i / 64 + 1, 0);
        }
        if let Some(word) = self.0.get_mut(i / 64) {
            *word ^= 1 << (i % 64);
        }
    }

    fn clear(&mut self) {
        self.0.clear();
    }

    /// Itself with each node moved to its id in `map`, by old
    /// [`NodeId::index`], and those with none dropped.
    fn remapped(&self, map: &[Option<NodeId>]) -> Self {
        let mut open = Self::default();
        let words = map.chunks(64).zip(&self.0);
        for (ids, &word) in words {
            let set = ids.iter().zip(0..).filter(|&(_, bit)| word >> bit & 1 != 0);
            for new in set.filter_map(|(&new, _)| new) {
                open.toggle(new);
            }
        }
        open
    }
}

impl View {
    fn new() -> Self {
        Self {
            stale: true,
            ..Self::default()
        }
    }

    /// Moves what it remembers of each node to its id in `map`, by old
    /// [`NodeId::index`], and forgets those with none.
    fn remap(&mut self, map: &[Option<NodeId>]) {
        self.open = self.open.remapped(map);
        self.flipped = self.flipped.remapped(map);
        self.selected = self
            .selected
            .and_then(|id| map.get(id.index()).copied().flatten());
        self.stale = true;
    }

    /// Whether `id` shows open, by `narrowed` while `narrowing`.
    fn is_open(&self, id: NodeId, narrowing: bool) -> bool {
        if narrowing {
            self.narrowed.revealed(id) != self.flipped.has(id)
        } else {
            self.open.has(id)
        }
    }
}

impl Files {
    pub const TITLE: &str = "Files";
    pub const PLACES: Places = Places::BANDS;

    /// Binds Ctrl+F (Command+F on macOS) in `keymap` to focus the search.
    pub fn new(keymap: &mut Keymap) -> Self {
        let find = Chord::new(Key::Char('f'), Modifiers::COMMAND);
        keymap.bind(find, FILES, Some(SEARCH));
        Self {
            opened: None,
            tab: Tab::Project,
            search: TextField::default(),
            filter: TreeFilter::default(),
            compact: true,
            letters: true,
            order: SortOrder::default(),
            popover: None,
            active: false,
        }
    }

    /// Shows `trees` in place of what was shown, every folder closed.
    fn set(&mut self, mut trees: Trees) {
        if self.order != SortOrder::default() {
            trees.game.sort(self.order);
            trees.project.sort(self.order);
        }
        let views = [View::new(), View::new()];
        self.opened = Some(Box::new(Opened { trees, views }));
        self.filter.editions = 0;
    }

    /// Shows no project, until the next [`Self::refresh`].
    pub fn close(&mut self) {
        self.opened = None;
    }

    /// Shows `trees`, read again, in place of what was shown. Each tab keeps
    /// what it had open and selected wherever the same path still is.
    pub fn refresh(&mut self, mut trees: Trees) {
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
        let [project, game] = &mut opened.views;
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
        let [project, game] = &mut opened.views;
        project.remap(&sort(&mut opened.trees.project));
        game.remap(&sort(&mut opened.trees.game));
    }

    /// Rebuilds every tab's rows.
    fn restale(&mut self) {
        for view in self.opened.iter_mut().flat_map(|opened| &mut opened.views) {
            view.stale = true;
        }
    }

    /// Rebuilds every tab's rows from the top for a new search or filter.
    fn renarrow(&mut self) {
        for view in self.opened.iter_mut().flat_map(|opened| &mut opened.views) {
            view.flipped.clear();
            view.scroll = ListScroll::TOP;
            view.stale = true;
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        if ui.input().pressed(PointerButton::Left) {
            self.active = ui.hovered();
        }
        self.tabs(ui);
        if self.opened.is_none() {
            return notice(ui, "No project open");
        }
        self.toolbar(ui);
        let query = self.search.text.trim();
        let filter = self.filter(self.tab);
        let narrowing = !query.is_empty() || !filter.is_empty();
        let Some(opened) = self.opened.as_deref_mut() else {
            return;
        };
        let tree = match self.tab {
            Tab::Game => &opened.trees.game,
            Tab::Project => &opened.trees.project,
        };
        let view = &mut opened.views[self.tab as usize];
        if tree.is_empty() {
            return notice(ui, "Nothing here yet");
        }
        if view.stale {
            if narrowing {
                tree.narrow(query, filter, &mut view.narrowed);
            }
            let mut rows = std::mem::take(&mut view.rows);
            rows.clear();
            rows.extend(tree.rows(
                |id| view.is_open(id, narrowing),
                |id| !narrowing || view.narrowed.kept(id),
                self.compact,
            ));
            view.rows = rows;
            view.stale = false;
        }
        if view.rows.is_empty() {
            return notice(ui, "No matches");
        }
        let height = ui.theme().size.row;
        let columns = Columns {
            gutter: self.tab == Tab::Game,
            letters: self.letters,
        };
        let mut clicked = None;
        let mut scroll = view.scroll;
        list(ui, &mut scroll, &view.rows, height, |ui, &row| {
            let open = view.is_open(row.last, narrowing);
            let selected = view.selected == Some(row.last);
            if row_ui(ui, tree, row, open, selected, columns) {
                clicked = Some(row.last);
            }
        });
        view.scroll = scroll;
        if let Some(id) = clicked {
            view.selected = Some(id);
            if tree[id].kind == NodeKind::Folder {
                if narrowing {
                    view.flipped.toggle(id);
                } else {
                    view.open.toggle(id);
                }
                view.stale = true;
            }
        }
    }

    /// The search across, then Filter on the Game tab, then View.
    fn toolbar(&mut self, ui: &mut Ui) {
        let input = ui.input();
        let keymap = ui.keymap();
        let focus = input
            .keys()
            .iter()
            .any(|&press| keymap.resolve(press, &[FILES]) == Some(SEARCH));
        if self.active && focus && !ui.typing() {
            self.search.focus = true;
        }
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
        let theme = ui.theme();
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

/// Which of a row's optional parts it shows.
#[derive(Clone, Copy)]
struct Columns {
    /// A [`gutter`], which in the Project tree would mark every file.
    gutter: bool,
    /// Its [`regions`].
    letters: bool,
}

/// With [`Columns::gutter`] a [`gutter`],
/// then an icon for its kind, open or not,
/// then its nodes' names split by `/`, each coloured by its git state, behind
/// a guide for each row it's inside,
/// then with [`Columns::letters`] its [`regions`].
/// Returns whether it was clicked this pass.
fn row_ui(
    ui: &mut Ui,
    tree: &Tree,
    row: TreeRow,
    open: bool,
    selected: bool,
    columns: Columns,
) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    // The gutter stands in for the left padding when there's one.
    let left = if columns.gutter { 0. } else { s.gap / 2. };
    let element = Element {
        padding: [left, 0., s.gap / 2., 0.],
        clip: true,
        ..Element::line(s.row)
    };
    // Under the middle of the parent's icon, kept on a whole pixel.
    let guide = Element {
        direction: Direction::LeftToRight,
        size: [Length::Fixed(s.indent), Length::Grow],
        padding: [((s.icon - s.line) / 2.).floor(), 0., 0., 0.],
        ..Element::DEFAULT
    };
    // A guide starts a little below its parent's row and ends a little
    // above the bottom of its last, as far as the icon sits from an edge.
    let inset = (s.row - s.icon) / 2.;
    let content = Element {
        direction: Direction::LeftToRight,
        size: [Length::Grow; 2],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        // Cut long names rather than push the regions out of the row.
        clip: true,
        ..Element::DEFAULT
    };
    let node = tree[row.last];
    let (clicked, ()) = components::row(ui, element, selected, |ui| {
        if columns.gutter {
            gutter(ui, node.status);
        }
        for level in 1..=row.depth {
            let mut guide = guide;
            if row.leads && level == row.depth {
                guide.padding[1] = inset;
            }
            if level > row.depth - row.ends {
                guide.padding[3] = inset;
            }
            ui.element(guide, rule);
        }
        ui.element(content, |ui| {
            let icon = match (node.kind, tree.is_archive(row.last), open) {
                (NodeKind::Folder, true, true) => Icon::ContainerOpen,
                (NodeKind::Folder, true, false) => Icon::Container,
                (NodeKind::Folder, false, true) => Icon::FolderOpen,
                (NodeKind::Folder, false, false) => Icon::Folder,
                (NodeKind::File, ..) => Icon::File,
            };
            ui.icon(icon, s.icon, c.dim);
            // Each folder in a run is its own text, to be picked on its own.
            for id in row.nodes() {
                if id != row.first {
                    ui.text(theme.ui_text(c.dim), "/");
                }
                let color = match tree[id].git {
                    GitStatus::Clean => c.text,
                    GitStatus::Added => theme::GIT_ADDED,
                    GitStatus::Modified => theme::GIT_MODIFIED,
                    GitStatus::Deleted => theme::GIT_DELETED,
                };
                ui.text(theme.ui_text(color), tree.name(id));
            }
        });
        if columns.letters {
            regions(ui, tree, node.editions);
        }
    });
    clicked
}

/// A dim line naming the checks below it in a popover.
fn heading(ui: &mut Ui, text: &str) {
    let theme = ui.theme();
    let s = theme.size;
    let element = Element {
        size: [Length::Grow, Length::Fixed(s.row)],
        padding: [s.icon_gap, 0., s.icon_gap, 0.],
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.text(theme.ui_text(theme.color.dim), text);
    });
}

/// A line in a popover: a tick while `on`, then `label`. Returns whether it
/// was clicked this pass.
fn check(ui: &mut Ui, label: &str, on: bool) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        padding: [s.icon_gap, 0., s.icon_gap, 0.],
        gap: s.icon_gap,
        radius: [s.radius; 4],
        ..Element::line(s.row)
    };
    let tick = Element {
        size: [Length::Fixed(s.icon); 2],
        ..Element::DEFAULT
    };
    let (clicked, ()) = components::row(ui, element, false, |ui| {
        ui.element(tick, |ui| {
            if on {
                ui.icon(Icon::Check, s.icon, c.accent);
            }
        });
        ui.text(theme.ui_text(c.text), label);
    });
    clicked
}

/// A bar for what the mod does to a node: solid where it changes a file,
/// outlined where it adds one, faint on a folder with either inside.
fn gutter(ui: &mut Ui, status: NodeStatus) {
    let theme = ui.theme();
    let (s, color) = (theme.size, theme.color.accent);
    let pad = [(s.gap - BAR) / 2., (s.row - s.icon) / 2.];
    let gutter = Element {
        size: [Length::Fixed(s.gap), Length::Grow],
        padding: [pad[0], pad[1], pad[0], pad[1]],
        ..Element::DEFAULT
    };
    let bar = Element {
        size: [Length::Grow; 2],
        radius: [BAR / 2.; 4],
        ..Element::DEFAULT
    };
    let bar = match status {
        NodeStatus::Unchanged => None,
        NodeStatus::Changed => Some(Element {
            background: Some(color),
            ..bar
        }),
        NodeStatus::Added => Some(Element {
            border: Some(Border {
                width: s.line,
                color,
            }),
            ..bar
        }),
        NodeStatus::Inside => Some(Element {
            background: Some(color.alpha(INSIDE)),
            ..bar
        }),
    };
    ui.element(gutter, |ui| {
        if let Some(bar) = bar {
            ui.element(bar, |_| ());
        }
    });
}

/// The region of each of `tree`'s editions in `editions`, or nothing when
/// that's every edition. They sit in a column at the right as wide as every
/// edition's region, from its left, so they line up from row to row.
fn regions(ui: &mut Ui, tree: &Tree, editions: u32) {
    if editions == 0 || editions == tree.every_edition() {
        return;
    }
    let theme = ui.theme();
    let s = theme.size;
    // Monospaced, so each letter fills the same cell whatever its shape.
    let style = TextStyle {
        font: theme.font.buffer,
        size: s.text,
        color: theme.color.dim,
    };
    // Clear of the scroll thumb, which sits half its width in from the edge.
    // TODO: drop once the thumb hides when it isn't in use.
    let padding = [s.gap, 0., 2.0 * s.scroll_bar, 0.];
    let letters = tree.editions.iter().filter_map(|id| region(id));
    let width = letters.fold(padding[0] + padding[2] - s.icon_gap, |width, letter| {
        width + ui.measure(style, letter)[0] + s.icon_gap
    });
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Length::Fixed(width), Length::Grow],
        padding,
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        let had = (0..).zip(&tree.editions);
        let had = had.filter(|&(bit, _)| editions >> bit & 1 != 0);
        for letter in had.filter_map(|(_, id)| region(id)) {
            ui.text(style, letter);
        }
    });
}

/// A game ID's region, its fourth letter.
fn region(id: &str) -> Option<&str> {
    id.get(3..4)
}
