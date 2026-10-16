//! Reading both trees from disk: the target's manifests, `changes/`, and git.

use std::{fmt, fs, io, path::Path};

use serde::{
    Deserialize, Deserializer,
    de::{IgnoredAny, MapAccess, Visitor},
};

use super::{
    Entry, NodeId, NodeKind, NodeStatus, Tree, TreeError,
    git::{self, GitStatus},
    io_err, tree_order,
};
use crate::{
    Project, base, changes,
    config::{ProjectConfig, ProjectKind},
};

/// Both of a project's trees, read together since both need the target's paths.
#[derive(Clone)]
pub struct Trees {
    pub game: Tree,
    pub project: Tree,
}

impl Project {
    /// Reads both trees from disk.
    ///
    /// # Errors
    ///
    /// If a directory or a target edition's manifest can't be read, or the
    /// target has more than 32 editions.
    pub fn trees(&self) -> Result<Trees, TreeError> {
        Trees::read(&self.root, &self.config)
    }

    /// Whether a change at `path`, relative to the root with `/` between
    /// components, can change what [`Self::trees`] reads: a file either tree
    /// shows, a target's manifest, or what git compares against.
    // TODO: a repo above the root keeps its `.git` out of the watch, so its
    // commits wait for a manual refresh.
    #[must_use]
    pub fn trees_read(&self, path: &str) -> bool {
        let git = [
            ".git/HEAD",
            ".git/index",
            ".git/packed-refs",
            ".git/info/exclude",
        ];
        if git.contains(&path)
            || path.starts_with(".git/refs/")
            || path.rsplit('/').next() == Some(".gitignore")
        {
            return true;
        }
        if path.split('/').any(is_hidden) {
            return false;
        }
        // A mod's Game tree reads `base/` through its manifests alone.
        let below = path
            .strip_prefix(base::DIR)
            .and_then(|rest| rest.strip_prefix('/'));
        match below {
            Some(rest) if self.config.kind == ProjectKind::Mod => !rest.contains('/'),
            _ => true,
        }
    }
}

impl Trees {
    /// Writes where the Game tree's file `id` is read from into `out`,
    /// relative to the project root: its copy in `base/` of the first
    /// edition that has it. `false` for a folder, or a file only the mod
    /// adds, whose change isn't the file's bytes.
    pub fn game_file(&self, id: NodeId, out: &mut String) -> bool {
        let tree = &self.game;
        let node = tree[id];
        if node.kind != NodeKind::File || node.status == NodeStatus::Added {
            return false;
        }
        let mut editions = (0..).zip(&tree.editions);
        let Some((_, edition)) = editions.find(|&(bit, _)| node.editions >> bit & 1 != 0) else {
            return false;
        };
        tree.path(id, out);
        out.insert_str(0, &format!("{}/{edition}/", base::DIR));
        true
    }

    pub(super) fn read(root: &Path, config: &ProjectConfig) -> Result<Self, TreeError> {
        let git = git::status(root).unwrap_or_default();
        // TODO: a game project's Game tree, once game projects have a layout.
        if config.kind == ProjectKind::Game {
            let mut project = Vec::new();
            walk(root, None, &mut String::new(), &mut project)?;
            mark(&mut project, &git);
            return Ok(Self {
                game: Tree::build(Vec::new(), Vec::new())?,
                project: Tree::build(project, Vec::new())?,
            });
        }
        let target = Target::read(root, config)?;

        // The Game tree shows what's in `base/`.
        let mut project = Vec::new();
        walk(root, Some(base::DIR), &mut String::new(), &mut project)?;
        let prefix = format!("{}/", changes::DIR);
        for entry in project.iter_mut().filter(|e| e.kind == NodeKind::File) {
            if let Some(rel) = entry.path.strip_prefix(&prefix) {
                let (path, _) = target.game_path(rel, config);
                entry.status = if target.has(&path) {
                    NodeStatus::Changed
                } else {
                    NodeStatus::Added
                };
            }
        }
        mark(&mut project, &git);

        let mut changed = Vec::new();
        walk(
            &root.join(changes::DIR),
            None,
            &mut String::new(),
            &mut changed,
        )?;
        let changed = changed.into_iter().filter(|e| e.kind == NodeKind::File);
        let changed: Vec<_> = changed
            .map(|entry| {
                let git = git::find(&git, &format!("{prefix}{}", entry.path));
                let (path, editions) = target.game_path(&entry.path, config);
                Entry {
                    status: NodeStatus::Added,
                    editions,
                    git,
                    ..Entry::file(path)
                }
            })
            .collect();
        // A deleted change file colours its game path, and stands in for it
        // when only the change had it.
        let deleted = git
            .iter()
            .filter(|(_, status)| *status == GitStatus::Deleted);
        let deleted = deleted.filter_map(|(path, _)| path.strip_prefix(&prefix));
        let deleted: Vec<_> = deleted
            .filter(|rel| !rel.split('/').any(is_hidden))
            .map(|rel| {
                let (path, editions) = target.game_path(rel, config);
                Entry {
                    editions,
                    git: GitStatus::Deleted,
                    ..Entry::file(path)
                }
            })
            .collect();
        let (all, editions) = (target.all, target.editions);
        let mut game = target.paths;
        game.extend(changed);
        game.sort_unstable_by(|a, b| tree_order(a, b).then(a.status.cmp(&b.status)));
        // A base path sorts before the changes at it, and several change files
        // can share one game path, so each run collapses into its first entry.
        game.dedup_by(|change, first| {
            if change.path != first.path || change.kind != first.kind {
                return false;
            }
            if first.status == NodeStatus::Unchanged {
                first.status = NodeStatus::Changed;
            }
            first.editions |= change.editions;
            first.git = first.git.merge(change.git);
            true
        });
        for ghost in deleted {
            match game.binary_search_by(|e| tree_order(e, &ghost)) {
                Ok(i) => game[i].git = game[i].git.merge(ghost.git),
                Err(i) => game.insert(i, ghost),
            }
        }
        // Only what isn't in the target has no editions.
        for entry in game.iter_mut().filter(|e| e.editions == 0) {
            entry.editions = all;
        }

        Ok(Self {
            game: Tree::build(game, editions)?,
            project: Tree::build(project, Vec::new())?,
        })
    }
}

/// The target editions' files that are unpacked, from their manifests.
struct Target {
    /// Sorted by path, each base file once with the editions that have it.
    paths: Vec<Entry>,
    editions: Vec<String>,
    /// A bit for each of `editions`.
    all: u32,
}

/// A manifest's paths alone, so the hashes aren't parsed.
#[derive(Deserialize)]
struct Paths {
    files: Keys,
}

/// A table's keys, in its order.
struct Keys(Vec<String>);

impl<'de> Deserialize<'de> for Keys {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> Visitor<'de> for Visit {
            type Value = Keys;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a table of paths")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Keys, A::Error> {
                let mut keys = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((key, IgnoredAny)) = map.next_entry()? {
                    keys.push(key);
                }
                Ok(Keys(keys))
            }
        }
        d.deserialize_map(Visit)
    }
}

impl Target {
    fn read(root: &Path, config: &ProjectConfig) -> Result<Self, TreeError> {
        let dir = root.join(base::DIR);
        let mut paths = Vec::new();
        let mut editions = Vec::new();
        let ids: Vec<_> = config
            .editions
            .keys()
            .filter(|id| config.is_target(id))
            .collect();
        if ids.len() > 32 {
            return Err(TreeError::Editions(ids.len()));
        }
        for id in ids {
            let path = dir.join(format!("{id}.toml"));
            // Not unpacked yet, as after a clone. Opening asks for it.
            let text = match fs::read_to_string(&path) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                text => text.map_err(io_err(&path))?,
            };
            let manifest: Paths =
                toml::from_str(&text).map_err(|source| TreeError::Manifest { path, source })?;
            let bit = 1u32 << editions.len();
            editions.push(id.clone());
            let files = manifest.files.0.into_iter();
            paths.extend(
                files
                    .filter(|p| !p.split('/').any(is_hidden))
                    .map(|path| Entry {
                        editions: bit,
                        ..Entry::file(path)
                    }),
            );
        }
        paths.sort_unstable_by(|a, b| a.path.cmp(&b.path));
        paths.dedup_by(|next, first| {
            let same = next.path == first.path;
            if same {
                first.editions |= next.editions;
            }
            same
        });
        let all = editions.iter().fold(0, |all, _| all << 1 | 1);
        Ok(Self {
            paths,
            editions,
            all,
        })
    }

    fn has(&self, path: &str) -> bool {
        self.paths
            .binary_search_by(|e| e.path.as_str().cmp(path))
            .is_ok()
    }

    /// Where a file under `changes/` lands in the game, and the editions its
    /// name scopes it to, if any. A stored change sits beside its working
    /// file as `.toml` or `.patch`, and an edition-scoped one names the
    /// edition just before its extension, as in `common.GZ2P01.jpc`.
    fn game_path(&self, path: &str, config: &ProjectConfig) -> (String, u32) {
        let working = path
            .strip_suffix(".toml")
            .or_else(|| path.strip_suffix(".patch"))
            .unwrap_or(path);
        let (dir, name) = working
            .rsplit_once('/')
            .map_or(("", working), |(d, n)| (d, n));
        let mut parts = name.rsplitn(3, '.');
        let (Some(ext), Some(id), Some(stem)) = (parts.next(), parts.next(), parts.next()) else {
            return (working.to_owned(), 0);
        };
        if !config.editions.contains_key(id) {
            return (working.to_owned(), 0);
        }
        let bit = self
            .editions
            .iter()
            .zip(0u32..)
            .find_map(|(e, i)| (e == id).then(|| 1 << i))
            .unwrap_or(0);
        let path = if dir.is_empty() {
            format!("{stem}.{ext}")
        } else {
            format!("{dir}/{stem}.{ext}")
        };
        (path, bit)
    }
}

/// A dotfile or dot-folder, such as `.git` or an archive's `.rarc.toml`,
/// left out of both trees with everything inside it.
// TODO: a toggle to show them.
// Even then, keep `.git/` and `.arbiter/`'s logs and cache out of both `walk`
// and `Project::trees_read`, through one list, or every log line and git object
// written reads the trees again.
fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Gives each of `entries` its state in `git`, and adds an entry for each
/// file deleted from disk that git still has.
fn mark(entries: &mut Vec<Entry>, git: &[(String, GitStatus)]) {
    for entry in entries.iter_mut().filter(|e| e.kind == NodeKind::File) {
        entry.git = git::find(git, &entry.path);
    }
    let deleted = git
        .iter()
        .filter(|(_, status)| *status == GitStatus::Deleted);
    let deleted = deleted.filter(|(path, _)| !path.split('/').any(is_hidden));
    entries.extend(deleted.map(|(path, status)| Entry {
        git: *status,
        ..Entry::file(path.clone())
    }));
}

/// Appends every file and folder under `dir`, at paths after `rel`, but for
/// a top-level entry named `skip`. A missing `dir` adds nothing.
fn walk(
    dir: &Path,
    skip: Option<&str>,
    rel: &mut String,
    out: &mut Vec<Entry>,
) -> Result<(), TreeError> {
    let entries = match fs::read_dir(dir) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        entries => entries.map_err(io_err(dir))?,
    };
    for entry in entries {
        let entry = entry.map_err(io_err(dir))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if is_hidden(&name) || rel.is_empty() && skip == Some(&*name) {
            continue;
        }
        let len = rel.len();
        if !rel.is_empty() {
            rel.push('/');
        }
        rel.push_str(&name);
        let path = entry.path();
        let folder = entry.file_type().map_err(io_err(&path))?.is_dir();
        out.push(Entry {
            kind: if folder {
                NodeKind::Folder
            } else {
                NodeKind::File
            },
            ..Entry::file(rel.clone())
        });
        if folder {
            walk(&path, None, rel, out)?;
        }
        rel.truncate(len);
    }
    Ok(())
}
