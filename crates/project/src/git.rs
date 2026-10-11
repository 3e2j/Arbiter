//! What git sees changed in the project since the last commit, read from the
//! `git` command so a project without git, or a machine without it, just
//! shows nothing.

use std::{path::Path, process::Command};

/// A file's state against the last commit. A folder merges everything below
/// it: one state when they all agree, `Modified` when they don't.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Status {
    #[default]
    Clean,
    /// Untracked, or staged as new.
    Added,
    /// Changed, or in a merge conflict.
    Modified,
    /// Gone from disk but still committed.
    Deleted,
}

impl Status {
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Clean, other) => other,
            (this, Self::Clean) => this,
            (this, other) if this == other => this,
            _ => Self::Modified,
        }
    }
}

/// Every path under `root` git sees changed, relative to `root` and sorted,
/// each once. `None` when `root` isn't in a work tree or git can't run.
pub(crate) fn status(root: &Path) -> Option<Vec<(String, Status)>> {
    let prefix = git(root, &["rev-parse", "--show-prefix"])?;
    let prefix = prefix.trim_end_matches('\n');
    // Without renames, a rename is its old path deleted and its new one added.
    let out = git(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )?;
    let mut paths: Vec<(String, Status)> = out
        .split_terminator('\0')
        .filter_map(|record| {
            let (xy, path) = (record.get(..2)?, record.get(3..)?);
            let path = path.strip_prefix(prefix)?;
            Some((path.to_owned(), parse(xy)))
        })
        .collect();
    paths.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    // A path removed from the index but still on disk is listed twice.
    paths.dedup_by(|next, first| {
        let same = next.0 == first.0;
        if same {
            first.1 = first.1.merge(next.1);
        }
        same
    });
    Some(paths)
}

/// A path's state in `paths` from [`status`].
pub(crate) fn find(paths: &[(String, Status)], path: &str) -> Status {
    paths
        .binary_search_by(|(p, _)| p.as_str().cmp(path))
        .map_or(Status::Clean, |i| paths[i].1)
}

/// A porcelain record's two letters: the index's (staged), then the work tree's.
fn parse(xy: &str) -> Status {
    match xy {
        "??" | "A " | "AM" => Status::Added,
        // Unmerged, both sides touched it.
        "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU" => Status::Modified,
        _ if xy.contains('D') => Status::Deleted,
        _ => Status::Modified,
    }
}

/// `git args` in `dir`, its output when it succeeds.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}
