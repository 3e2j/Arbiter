//! The open project's files, as the Game or the Project tree, one at a time.
//! See [`project::tree`].

use gui::{
    components::{ListScroll, list},
    input::{Button, Cursor},
    ui::{Align, Direction, Element, Size, Ui},
};
use project::tree::{Kind, NodeId, Row, Tree, Trees};

use crate::assets::{Icon, Icons};
use crate::workspace::Places;

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

/// The open folders, a bit per node by [`NodeId::index`].
#[derive(Default)]
struct Open(Vec<u64>);

/// What a tab remembers of its tree, so switching back finds it as it was.
#[derive(Default)]
struct View {
    open: Open,
    selected: Option<NodeId>,
    scroll: ListScroll,
    /// The lines shown, rebuilt when `stale`.
    rows: Vec<Row>,
    stale: bool,
}

pub struct Files {
    icons: Icons,
    /// `None` until a project is open.
    trees: Option<Box<Trees>>,
    tab: Tab,
    /// By [`Tab`].
    views: [View; Tab::ALL.len()],
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
}

impl View {
    fn new() -> Self {
        Self {
            stale: true,
            ..Self::default()
        }
    }
}

impl Files {
    pub const TITLE: &str = "Files";
    pub const PLACES: Places = Places::BANDS;

    pub fn new(icons: Icons) -> Self {
        Self {
            icons,
            trees: None,
            tab: Tab::Project,
            views: [View::new(), View::new()],
        }
    }

    /// Shows `trees` in place of what was shown, every folder closed.
    pub fn set(&mut self, trees: Trees) {
        self.trees = Some(Box::new(trees));
        self.views = [View::new(), View::new()];
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.tabs(ui);
        let Some(trees) = &self.trees else {
            return empty(ui, "No project open");
        };
        let tree = match self.tab {
            Tab::Game => &trees.game,
            Tab::Project => &trees.project,
        };
        let view = &mut self.views[self.tab as usize];
        if tree.is_empty() {
            return empty(ui, "Nothing here yet");
        }
        if view.stale {
            view.rows.clear();
            view.rows.extend(tree.rows(|id| view.open.has(id), true));
            view.stale = false;
        }
        let height = ui.theme().size.row;
        let icons = &self.icons;
        let mut clicked = None;
        list(ui, &mut view.scroll, &view.rows, height, |ui, &row| {
            let open = view.open.has(row.last);
            let selected = view.selected == Some(row.last);
            if row_ui(ui, tree, row, open, selected, icons) {
                clicked = Some(row.last);
            }
        });
        if let Some(id) = clicked {
            view.selected = Some(id);
            if tree[id].kind == Kind::Folder {
                view.open.toggle(id);
                view.stale = true;
            }
        }
    }

    /// A tab for each tree along the top, the shown one underlined, over a
    /// line across the panel.
    fn tabs(&mut self, ui: &mut Ui) {
        let theme = ui.theme();
        let (c, s) = (theme.color, theme.size);
        let strip = Element {
            direction: Direction::LeftToRight,
            size: [Size::Grow, Size::Fit],
            padding: [s.gap, 0., s.gap, 0.],
            ..Element::DEFAULT
        };
        let tab = Element {
            size: [Size::Fit; 2],
            cursor: Some(Cursor::Pointer),
            ..Element::column()
        };
        let label = Element {
            size: [Size::Fit, Size::Fixed(s.row + s.gap)],
            padding: [s.gap, 0., s.gap, 0.],
            align: [Align::Center; 2],
            ..Element::DEFAULT
        };
        let underline = Element {
            size: [Size::Grow, Size::Fixed(2. * s.line)],
            ..Element::DEFAULT
        };
        let rule = Element {
            size: [Size::Grow, Size::Fixed(s.line)],
            background: Some(c.line),
            ..Element::DEFAULT
        };
        let top = Element {
            size: [Size::Grow, Size::Fit],
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
                        ui.clicked(Button::Left)
                    });
                    if clicked {
                        self.tab = each;
                    }
                }
            });
            ui.element(rule, |_| ());
        });
    }
}

/// An icon for its kind, open or not, then its nodes' names split by `/`,
/// behind a guide for each row it's inside. Returns whether it was clicked
/// this pass.
fn row_ui(ui: &mut Ui, tree: &Tree, row: Row, open: bool, selected: bool, icons: &Icons) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Size::Grow, Size::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        clip: true,
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    // Under the middle of the parent's icon, kept on a whole pixel.
    let guide = Element {
        size: [Size::Fixed(s.indent), Size::Grow],
        padding: [((s.icon - s.line) / 2.).floor(), 0., 0., 0.],
        ..Element::DEFAULT
    };
    let line = Element {
        size: [Size::Fixed(s.line), Size::Grow],
        background: Some(c.line),
        ..Element::DEFAULT
    };
    let content = Element {
        direction: Direction::LeftToRight,
        size: [Size::Grow; 2],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.style().background = if selected {
            Some(c.selected)
        } else {
            ui.hovered().then_some(c.hover)
        };
        // A guide starts a little below its parent's row and ends a little
        // above the bottom of its last, as far as the icon sits from an edge.
        let inset = (s.row - s.icon) / 2.;
        let first = tree[row.first].parent.index() + 1 == row.first.index();
        for level in 1..=row.depth {
            let mut guide = guide;
            if first && level == row.depth {
                guide.padding[1] = inset;
            }
            if level > row.depth - row.ends {
                guide.padding[3] = inset;
            }
            ui.element(guide, |ui| ui.element(line, |_| ()));
        }
        ui.element(content, |ui| {
            let arc = tree.name(row.last).rsplit_once('.');
            let arc = arc.is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("arc"));
            let icon = match (tree[row.last].kind, arc, open) {
                (Kind::Folder, true, true) => Icon::ContainerOpen,
                (Kind::Folder, true, false) => Icon::Container,
                (Kind::Folder, false, true) => Icon::FolderOpen,
                (Kind::Folder, false, false) => Icon::Folder,
                (Kind::File, ..) => Icon::File,
            };
            ui.icon(icons.get(icon), s.icon, c.dim);
            // Each folder in a run is its own text, to be picked on its own.
            for id in row.nodes() {
                if id != row.first {
                    ui.text(theme.ui_text(c.dim), "/");
                }
                ui.text(theme.ui_text(c.text), tree.name(id));
            }
        });
        ui.clicked(Button::Left)
    })
}

/// `text`, dim and centred, in place of a tree.
fn empty(ui: &mut Ui, text: &str) {
    let theme = ui.theme();
    let middle = Element {
        align: [Align::Center; 2],
        ..Element::column()
    };
    ui.element(middle, |ui| {
        ui.text(theme.ui_text(theme.color.dim), text);
    });
}
