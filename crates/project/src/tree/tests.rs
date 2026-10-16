use std::fs;

use super::*;
use crate::{
    Project, base,
    base::{Manifest, ManifestEntry},
    config::{Edition, Platform, ProjectConfig, ProjectKind},
    hash::Hash,
};

fn config(kind: ProjectKind, ids: &[&str], target: Option<&[&str]>) -> ProjectConfig {
    let edition = || Edition {
        platform: Platform::GameCube,
        region: None,
        country: None,
        revision: 0,
        disc_hash: Hash(0),
        files_digest: Hash(0),
    };
    ProjectConfig {
        kind,
        target: target.map(|t| t.iter().map(|&id| id.to_owned()).collect()),
        editions: ids.iter().map(|&id| (id.to_owned(), edition())).collect(),
    }
}

fn touch(root: &Path, paths: &[&str]) {
    for path in paths {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"").unwrap();
    }
}

fn manifest(root: &Path, id: &str, paths: &[&str]) {
    let manifest = Manifest {
        files: paths
            .iter()
            .map(|&p| {
                let entry = ManifestEntry {
                    hash: Hash(0),
                    compression: None,
                };
                (p.to_owned(), entry)
            })
            .collect(),
    };
    fs::create_dir_all(root.join(base::DIR)).unwrap();
    let path = root.join(base::DIR).join(format!("{id}.toml"));
    fs::write(path, toml::to_string(&manifest).unwrap()).unwrap();
}

/// One line per node shown when `open`, indented by depth: the name, `/`
/// for a folder, its status, its editions' region letters when it isn't
/// in all of them, and its git state.
fn show(tree: &Tree, open: impl Fn(NodeId) -> bool) -> Vec<String> {
    let all = tree.every_edition();
    tree.rows(open, |_| true, false)
        .map(|TreeRow { last: id, .. }| {
            let node = tree[id];
            let mut line = "  ".repeat(usize::from(node.depth - 1));
            line.push_str(tree.name(id));
            if node.kind == NodeKind::Folder {
                line.push('/');
            }
            line.push_str(match node.status {
                NodeStatus::Unchanged => "",
                NodeStatus::Changed => " M",
                NodeStatus::Added => " A",
                NodeStatus::Inside => " •",
            });
            if node.editions != all && node.editions != 0 {
                line.push(' ');
                for (id, i) in tree.editions.iter().zip(0..) {
                    if node.editions & 1 << i != 0 {
                        line.push_str(id.get(3..4).unwrap());
                    }
                }
            }
            line.push_str(match node.git {
                GitStatus::Clean => "",
                GitStatus::Added => " +",
                GitStatus::Modified => " ~",
                GitStatus::Deleted => " -",
            });
            line
        })
        .collect()
}

fn mod_project() -> (tempfile::TempDir, ProjectConfig) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let shared = [
        "sys/main.dol",
        "res/Layout/logo.bti",
        "res/Particle/common.jpc",
        "res/Object/al.arc/.rarc.toml",
        "res/Object/al.arc/al.bmd",
    ];
    manifest(
        root,
        "GZ2E01",
        &[&shared[..], &["res/Msgus/zel_00.bmg"]].concat(),
    );
    manifest(
        root,
        "GZ2P01",
        &[&shared[..], &["res/Msgus/zel_00.bmg"]].concat(),
    );
    manifest(
        root,
        "GZ2J01",
        &[&shared[..], &["res/Msgjp/zel_00.bmg"]].concat(),
    );
    // A reference, which the Game tree leaves out.
    manifest(root, "GZLE01", &["res/Object/Link.arc/link.bmd"]);
    touch(
        root,
        &[
            "arbiter.toml",
            ".gitignore",
            ".git/HEAD",
            ".arbiter/log",
            "code/mod.cpp",
            "changes/res/Layout/logo.bti",
            "changes/res/Layout/logo.bti.toml",
            "changes/res/Particle/common.GZ2P01.jpc.patch",
            "changes/res/Msgus/zel_50.bmg.toml",
        ],
    );
    fs::create_dir_all(root.join("assets")).unwrap();
    let ids = ["GZ2E01", "GZ2J01", "GZ2P01", "GZLE01"];
    let target = ["GZ2E01", "GZ2J01", "GZ2P01"];
    (dir, config(ProjectKind::Mod, &ids, Some(&target)))
}

#[test]
fn game_tree_merges_the_target_and_lays_changes_over_it() {
    let (dir, config) = mod_project();
    let trees = Trees::read(dir.path(), &config).unwrap();
    assert_eq!(trees.game.editions, ["GZ2E01", "GZ2J01", "GZ2P01"]);
    assert_eq!(
        show(&trees.game, |_| true),
        [
            "res/ •",
            "  Layout/ •",
            "    logo.bti M",
            "  Msgjp/ J",
            "    zel_00.bmg J",
            "  Msgus/ •",
            "    zel_00.bmg EP",
            "    zel_50.bmg A",
            "  Object/",
            "    al.arc/",
            "      al.bmd",
            "  Particle/ •",
            "    common.jpc M",
            "sys/",
            "  main.dol",
        ]
    );
}

#[test]
fn a_game_file_reads_from_the_first_edition_in_base_that_has_it() {
    let (dir, config) = mod_project();
    let trees = Trees::read(dir.path(), &config).unwrap();
    let game = &trees.game;
    let find = |path: &str| {
        let mut at = String::new();
        (0..)
            .map(NodeId)
            .take(game.nodes.len())
            .find(|&id| {
                game.path(id, &mut at);
                at == path
            })
            .unwrap()
    };
    let mut out = String::new();
    assert!(trees.game_file(find("res/Msgus/zel_00.bmg"), &mut out));
    assert_eq!(out, "base/GZ2E01/res/Msgus/zel_00.bmg");
    assert!(trees.game_file(find("res/Msgjp/zel_00.bmg"), &mut out));
    assert_eq!(out, "base/GZ2J01/res/Msgjp/zel_00.bmg");
    assert!(trees.game_file(find("res/Layout/logo.bti"), &mut out));
    assert_eq!(out, "base/GZ2E01/res/Layout/logo.bti");
    assert!(!trees.game_file(find("res/Msgus/zel_50.bmg"), &mut out));
    assert!(!trees.game_file(find("res/Msgus"), &mut out));
}

#[test]
fn an_added_file_scoped_by_name_has_only_its_edition() {
    let (dir, config) = mod_project();
    touch(dir.path(), &["changes/res/new.GZ2J01.bmg.toml"]);
    let trees = Trees::read(dir.path(), &config).unwrap();
    let lines = show(&trees.game, |_| true);
    assert!(lines.contains(&"  new.bmg A J".to_owned()), "{lines:#?}");
}

#[test]
fn project_tree_marks_change_files_as_the_game_tree_does() {
    let (dir, config) = mod_project();
    let trees = Trees::read(dir.path(), &config).unwrap();
    assert_eq!(
        show(&trees.project, |_| true),
        [
            "assets/",
            "changes/ •",
            "  res/ •",
            "    Layout/ •",
            "      logo.bti M",
            "      logo.bti.toml M",
            "    Msgus/ •",
            "      zel_50.bmg.toml A",
            "    Particle/ •",
            "      common.GZ2P01.jpc.patch M",
            "code/",
            "  mod.cpp",
            "arbiter.toml",
        ]
    );
}

/// Commits everything in `root` to a new repository.
fn commit(root: &Path) {
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(root)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    };
    git(&["init", "--quiet"]);
    git(&["add", "--all"]);
    git(&["commit", "--quiet", "--message", "base"]);
}

#[test]
fn git_colours_names_and_folders_merge_what_is_inside() {
    let (dir, config) = mod_project();
    let root = dir.path();
    fs::remove_dir_all(root.join(".git")).unwrap();
    fs::write(root.join(".gitignore"), "base/\n").unwrap();
    touch(root, &["changes/res/Msgus/zel_00.bmg.toml"]);
    commit(root);
    fs::write(root.join("code/mod.cpp"), b"edit").unwrap();
    touch(root, &["code/new.cpp"]);
    fs::remove_file(root.join("changes/res/Layout/logo.bti")).unwrap();
    fs::remove_file(root.join("changes/res/Layout/logo.bti.toml")).unwrap();
    fs::remove_file(root.join("changes/res/Msgus/zel_50.bmg.toml")).unwrap();
    fs::remove_file(root.join(".gitignore")).unwrap();

    let trees = Trees::read(root, &config).unwrap();
    assert_eq!(
        show(&trees.project, |_| true),
        [
            "assets/",
            "changes/ • -",
            "  res/ • -",
            "    Layout/ -",
            "      logo.bti -",
            "      logo.bti.toml -",
            "    Msgus/ • -",
            "      zel_00.bmg.toml M",
            "      zel_50.bmg.toml -",
            "    Particle/ •",
            "      common.GZ2P01.jpc.patch M",
            "code/ ~",
            "  mod.cpp ~",
            "  new.cpp +",
            "arbiter.toml",
        ]
    );
    let game = show(&trees.game, |_| true);
    assert!(game.contains(&"    logo.bti -".to_owned()), "{game:#?}");
    assert!(game.contains(&"    zel_50.bmg -".to_owned()), "{game:#?}");
    assert!(
        game.contains(&"    zel_00.bmg M EP".to_owned()),
        "{game:#?}"
    );
}

#[test]
fn closed_folders_hide_their_subtree() {
    let (dir, config) = mod_project();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    let res = game.rows(|_| false, |_| true, false).next().unwrap().last;
    assert_eq!(game.name(res), "res");
    assert_eq!(show(&game, |id| id == res).len(), 7);
    assert_eq!(show(&game, |_| false), ["res/ •", "sys/"]);
    assert_eq!(game.len(), 15);
}

#[test]
fn a_folder_holding_only_a_folder_shares_its_line() {
    let (dir, config) = mod_project();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    let project = Trees::read(dir.path(), &config).unwrap().project;
    let lines = |tree: &Tree| {
        let mut name = String::new();
        let rows = tree.rows(|_| true, |_| true, true);
        rows.map(|row| {
            tree.row_name(row, &mut name);
            format!("{}{name}", "  ".repeat(usize::from(row.depth)))
        })
        .collect::<Vec<_>>()
    };
    assert_eq!(
        lines(&game),
        [
            "res",
            "  Layout",
            "    logo.bti",
            "  Msgjp",
            "    zel_00.bmg",
            "  Msgus",
            "    zel_00.bmg",
            "    zel_50.bmg",
            "  Object",
            "    al.arc",
            "      al.bmd",
            "  Particle",
            "    common.jpc",
            "sys",
            "  main.dol",
        ]
    );
    assert_eq!(
        lines(&project),
        [
            "assets",
            "changes/res",
            "  Layout",
            "    logo.bti",
            "    logo.bti.toml",
            "  Msgus",
            "    zel_50.bmg.toml",
            "  Particle",
            "    common.GZ2P01.jpc.patch",
            "code",
            "  mod.cpp",
            "arbiter.toml",
        ]
    );
    let mut name = String::new();
    let ends: Vec<_> = (project.rows(|_| true, |_| true, true))
        .filter(|row| row.ends > 0)
        .map(|row| {
            project.row_name(row, &mut name);
            (name.clone(), row.ends)
        })
        .collect();
    assert_eq!(
        ends,
        [
            ("logo.bti.toml".into(), 1),
            ("zel_50.bmg.toml".into(), 1),
            ("common.GZ2P01.jpc.patch".into(), 2),
            ("mod.cpp".into(), 1),
        ]
    );
    let closed = game.rows(|_| false, |_| true, true).count();
    assert_eq!(closed, 2);
}

/// The rows `query` and `filter` leave of `tree`, each revealed folder
/// open and the rest closed, indented by depth.
fn narrow(tree: &Tree, query: &str, filter: TreeFilter) -> Vec<String> {
    let mut narrowed = Narrowed::default();
    tree.narrow(query, filter, &mut narrowed);
    let rows = tree.rows(|id| narrowed.revealed(id), |id| narrowed.kept(id), false);
    rows.map(|row| {
        let indent = "  ".repeat(usize::from(row.depth));
        format!("{indent}{}", tree.name(row.last))
    })
    .collect()
}

#[test]
fn a_search_keeps_matches_and_opens_the_folders_above_them() {
    let (dir, config) = mod_project();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    assert_eq!(
        narrow(&game, "ZEL", TreeFilter::default()),
        [
            "res",
            "  Msgjp",
            "    zel_00.bmg",
            "  Msgus",
            "    zel_00.bmg",
            "    zel_50.bmg",
        ]
    );
    // A folder that matches keeps what's inside, but stays closed.
    assert_eq!(
        narrow(&game, "msgus", TreeFilter::default()),
        ["res", "  Msgus"]
    );
    assert_eq!(narrow(&game, "nothing", TreeFilter::default()), [""; 0]);
}

#[test]
fn changed_keeps_the_mods_files_and_opens_every_folder_above_them() {
    let (dir, config) = mod_project();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    let modded = TreeFilter {
        modded: true,
        ..TreeFilter::default()
    };
    assert_eq!(
        narrow(&game, "", modded),
        [
            "res",
            "  Layout",
            "    logo.bti",
            "  Msgus",
            "    zel_50.bmg",
            "  Particle",
            "    common.jpc",
        ]
    );
}

#[test]
fn editions_keep_only_files_not_in_every_edition_and_open_nothing() {
    let (dir, config) = mod_project();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    // GZ2E01 is the first edition, GZ2J01 the second.
    let american = TreeFilter {
        editions: 0b001,
        ..TreeFilter::default()
    };
    let japanese = TreeFilter {
        editions: 0b010,
        ..TreeFilter::default()
    };
    assert_eq!(narrow(&game, "", japanese), ["res"]);
    let mut narrowed = Narrowed::default();
    game.narrow("", american, &mut narrowed);
    let rows = game.rows(|_| true, |id| narrowed.kept(id), false);
    let names: Vec<_> = rows.map(|row| game.name(row.last)).collect();
    // zel_50.bmg, added by the mod, is in every edition.
    assert_eq!(names, ["res", "Msgus", "zel_00.bmg"]);
    // A search still opens the folders above what it finds.
    assert_eq!(
        narrow(&game, "zel", japanese),
        ["res", "  Msgjp", "    zel_00.bmg"]
    );
}

#[test]
fn a_row_left_out_moves_where_its_siblings_lead_and_end() {
    let (dir, config) = mod_project();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    let mut narrowed = Narrowed::default();
    game.narrow("zel_50", TreeFilter::default(), &mut narrowed);
    let rows = game.rows(|id| narrowed.revealed(id), |id| narrowed.kept(id), false);
    let rows: Vec<_> = rows
        .map(|row| (game.name(row.last), row.leads, row.ends))
        .collect();
    assert_eq!(
        rows,
        [
            ("res", true, 0),
            ("Msgus", true, 0),
            ("zel_50.bmg", true, 2)
        ]
    );
}

#[test]
fn an_edition_not_unpacked_is_left_out() {
    let (dir, config) = mod_project();
    fs::remove_file(dir.path().join("base/GZ2J01.toml")).unwrap();
    let game = Trees::read(dir.path(), &config).unwrap().game;
    assert_eq!(game.editions, ["GZ2E01", "GZ2P01"]);
    assert!(!show(&game, |_| true).iter().any(|l| l.contains("Msgjp")));
}

#[test]
fn a_game_project_shows_only_its_files() {
    let dir = tempfile::tempdir().unwrap();
    touch(dir.path(), &["arbiter.toml", "res/a.bmg"]);
    let config = config(ProjectKind::Game, &[], None);
    let trees = Trees::read(dir.path(), &config).unwrap();
    assert!(trees.game.is_empty());
    assert_eq!(
        show(&trees.project, |_| true),
        ["res/", "  a.bmg", "arbiter.toml"]
    );
}

#[test]
fn an_archive_never_shares_a_line() {
    let entries = vec![Entry::file("a/b.arc/c/d/e.bmd".to_owned())];
    let tree = Tree::build(entries, Vec::new()).unwrap();
    let mut name = String::new();
    let lines: Vec<_> = (tree.rows(|_| true, |_| true, true))
        .map(|row| {
            tree.row_name(row, &mut name);
            name.clone()
        })
        .collect();
    assert_eq!(lines, ["a", "b.arc", "c/d", "e.bmd"]);
}

#[test]
fn sorting_by_name_keeps_the_order_a_tree_is_read_in() {
    let (dir, config) = mod_project();
    let mut game = Trees::read(dir.path(), &config).unwrap().game;
    let before = show(&game, |_| true);
    let map = game.sort(SortOrder::default());
    assert!((0..).map(NodeId).zip(map).all(|(old, new)| old == new));
    assert_eq!(show(&game, |_| true), before);
}

#[test]
fn sorting_moves_whole_subtrees_and_keeps_folders_first() {
    let (dir, config) = mod_project();
    let mut game = Trees::read(dir.path(), &config).unwrap().game;
    let names = |tree: &Tree| -> Vec<String> {
        let rows = tree.rows(|_| true, |_| true, false);
        rows.map(|row| {
            let indent = "  ".repeat(usize::from(row.depth));
            format!("{indent}{}", tree.name(row.last))
        })
        .collect()
    };
    let msgjp = names(&game).iter().position(|n| n == "  Msgjp");
    game.sort(SortOrder {
        key: SortKey::Name,
        descending: true,
    });
    let sorted = names(&game);
    assert_eq!(&sorted[..2], ["sys", "  main.dol"]);
    assert_eq!(&sorted[2..4], ["res", "  Particle"]);
    assert_ne!(sorted.iter().position(|n| n == "  Msgjp"), msgjp);
    // Each folder still holds what it did, right after it.
    let at = sorted.iter().position(|n| n == "  Msgjp").unwrap();
    assert_eq!(sorted[at + 1], "    zel_00.bmg");
}

#[test]
fn sorting_by_type_groups_extensions_and_maps_old_ids_to_new() {
    let entry = |path: &str, kind| Entry {
        kind,
        ..Entry::file(path.to_owned())
    };
    let entries = vec![
        entry("b.txt", NodeKind::File),
        entry("c.bmg", NodeKind::File),
        entry("a.bmg", NodeKind::File),
        entry("y.arc/al.bmd", NodeKind::File),
        entry("z", NodeKind::Folder),
    ];
    let mut tree = Tree::build(entries, Vec::new()).unwrap();
    let names = |tree: &Tree| -> Vec<String> {
        let rows = tree.rows(|_| true, |_| true, false);
        rows.map(|row| tree.name(row.last).to_owned()).collect()
    };
    assert_eq!(
        names(&tree),
        ["y.arc", "al.bmd", "z", "a.bmg", "b.txt", "c.bmg"]
    );
    let bmd = NodeId(2);
    assert_eq!(tree.name(bmd), "al.bmd");
    let order = SortOrder {
        key: SortKey::Type,
        ..SortOrder::default()
    };
    let map = tree.sort(order);
    assert_eq!(
        names(&tree),
        ["z", "y.arc", "al.bmd", "a.bmg", "c.bmg", "b.txt"]
    );
    assert_eq!(tree.name(map[bmd.index()]), "al.bmd");
}

#[test]
fn folders_sort_first_then_by_name_ignoring_case() {
    let entry = |path: &str, kind| Entry {
        kind,
        ..Entry::file(path.to_owned())
    };
    let mut entries = [
        entry("b", NodeKind::File),
        entry("A", NodeKind::File),
        entry("z/y", NodeKind::File),
        entry("c", NodeKind::Folder),
        entry("a", NodeKind::File),
    ];
    entries.sort_by(tree_order);
    let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["c", "z/y", "A", "a", "b"]);
}

#[test]
fn ids_follow_paths_into_a_tree_read_again() {
    let entry = |path: &str, kind| Entry {
        kind,
        ..Entry::file(path.to_owned())
    };
    let old = Tree::build(
        vec![
            entry("res/Msgus/zel_00.bmg", NodeKind::File),
            entry("res/Msgus/zel_01.bmg", NodeKind::File),
            entry("res/al.arc", NodeKind::File),
            entry("sys/main.dol", NodeKind::File),
        ],
        Vec::new(),
    )
    .unwrap();
    let mut new = Tree::build(
        vec![
            entry("res/Msgus/zel_00.bmg", NodeKind::File),
            entry("res/al.arc/al.bmd", NodeKind::File),
            entry("res/Layout/logo.bti", NodeKind::File),
            entry("sys/main.dol", NodeKind::File),
        ],
        Vec::new(),
    )
    .unwrap();
    new.sort(SortOrder {
        descending: true,
        ..SortOrder::default()
    });
    let map = old.ids_in(&new);
    let path = |tree: &Tree, mut id: NodeId| {
        let mut parts = Vec::new();
        while id != NodeId::ROOT {
            parts.push(tree.name(id));
            id = tree[id].parent;
        }
        parts.reverse();
        parts.join("/")
    };
    let followed: Vec<_> = (0..)
        .map(NodeId)
        .zip(&map)
        .skip(1)
        .map(|(id, new_id)| (path(&old, id), new_id.map(|new_id| path(&new, new_id))))
        .collect();
    let same = |p: &str| (p.to_owned(), Some(p.to_owned()));
    assert_eq!(
        followed,
        [
            same("res"),
            same("res/Msgus"),
            same("res/Msgus/zel_00.bmg"),
            ("res/Msgus/zel_01.bmg".to_owned(), None),
            // A file that became a folder isn't the same node.
            ("res/al.arc".to_owned(), None),
            same("sys"),
            same("sys/main.dol"),
        ]
    );
}

#[test]
fn trees_read_what_they_show_and_what_git_compares() {
    let project = |kind| Project {
        root: PathBuf::new(),
        config: config(kind, &["GZ2E01"], None),
    };
    let mod_project = project(ProjectKind::Mod);
    for path in [
        "changes/res/Msgus/zel_00.bmg",
        "code/mod.cpp",
        "base",
        "base/GZ2E01.toml",
        ".gitignore",
        "changes/.gitignore",
        ".git/index",
        ".git/HEAD",
        ".git/refs/heads/main",
        ".git/packed-refs",
        ".git/info/exclude",
    ] {
        assert!(mod_project.trees_read(path), "{path}");
    }
    for path in [
        "base/GZ2E01/res/Msgus/zel_00.bmg",
        "base/.GZ2E01.partial",
        ".arbiter/cache/preview.png",
        ".git/config",
        "res/Object/al.arc/.rarc.toml",
    ] {
        assert!(!mod_project.trees_read(path), "{path}");
    }
    assert!(project(ProjectKind::Game).trees_read("base/GZ2E01/res/Msgus/zel_00.bmg"));
}
