//! A line of the file tree: a node's icon, names and guides, with its gutter
//! and edition letters when asked.

use gui::ui::{Align, Border, Direction, Element, Length, TextStyle, Ui};
use project::tree::{GitStatus, NodeKind, NodeStatus, Tree, TreeRow};

use crate::assets::Icon;
use crate::components::{self, rule};
use crate::theme;

/// Width of the gutter's bar.
const BAR: f32 = 3.;
/// How strong a folder's gutter bar is beside a file's.
// Low alpha so the color is noticeably different
const INSIDE: u8 = 0x39;

/// Which of a row's optional parts it shows.
#[derive(Clone, Copy)]
pub struct Columns {
    /// A [`gutter`], which in the Project tree would mark every file.
    pub gutter: bool,
    /// Its [`regions`].
    pub letters: bool,
}

/// With [`Columns::gutter`] a [`gutter`],
/// then an icon for its kind, open or not,
/// then its nodes' names split by `/`, each coloured by its git state, behind
/// a guide for each row it's inside,
/// then with [`Columns::letters`] its [`regions`].
/// Returns whether it was clicked this pass.
pub fn row_ui(
    ui: &mut Ui,
    tree: &Tree,
    row: TreeRow,
    open: bool,
    selected: bool,
    columns: Columns,
) -> bool {
    let theme = theme::current();
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
                    GitStatus::Added => c.ok,
                    GitStatus::Modified => c.warn,
                    GitStatus::Deleted => c.err,
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

/// A bar for what the mod does to a node: solid where it changes a file,
/// outlined where it adds one, faint on a folder with either inside.
fn gutter(ui: &mut Ui, status: NodeStatus) {
    let theme = theme::current();
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
    let theme = theme::current();
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
