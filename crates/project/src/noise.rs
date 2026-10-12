//! Changes under a project that happen all the time and matter to no one, so
//! a watch drops them before anything reads them. What does matter is up to
//! each reader, such as [`Project::trees_read`](crate::Project::trees_read).

use crate::log;

/// Whether a change at `path`, relative to the project root with `/` between
/// components, is noise: Arbiter's own logs, or anything in `.git` that doesn't
/// change what git compares against.
#[must_use]
pub fn is_noise(path: &str) -> bool {
    under(path, log::DIR) || path.strip_prefix(".git/").is_some_and(is_git_noise)
}

/// `path` inside `.git`, but for what git compares against. Editors' swap
/// and lock files aren't here: they're hidden, which hides them from the
/// trees, and the rest only appear around a save, which is a change anyway.
fn is_git_noise(path: &str) -> bool {
    const DIRS: &[&str] = &[
        "objects",
        "logs",
        "hooks",
        "lfs",
        "fsmonitor--daemon",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ];
    const FILES: &[&str] = &[
        "COMMIT_EDITMSG",
        "FETCH_HEAD",
        "ORIG_HEAD",
        "BISECT_LOG",
        "gc.pid",
    ];
    let name = path.rsplit('/').next().unwrap_or(path);
    let top = !path.contains('/');
    has_extension(name, &["lock"])
        || top && has_extension(name, &["new", "tmp"])
        || FILES.contains(&path)
        || DIRS.iter().any(|dir| under(path, dir))
        || under(path, "info") && path != "info/exclude"
}

/// Whether `path` is `dir` or inside it.
fn under(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn has_extension(name: &str, any: &[&str]) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| any.iter().any(|want| ext.eq_ignore_ascii_case(want)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_and_git_internals_are_noise() {
        for path in [
            ".arbiter/logs/arbiter.log",
            ".git/objects/ab/cdef",
            ".git/index.lock",
            ".git/refs/heads/main.lock",
            ".git/logs/HEAD",
            ".git/FETCH_HEAD",
            ".git/COMMIT_EDITMSG",
            ".git/rebase-merge/done",
            ".git/info/attributes",
            ".git/packed-refs.new",
        ] {
            assert!(is_noise(path), "{path}");
        }
    }

    #[test]
    fn what_a_reader_might_want_is_not() {
        for path in [
            ".arbiter/cache/preview.png",
            ".arbiter/logsheet",
            ".git/index",
            ".git/HEAD",
            ".git/refs/heads/main",
            ".git/info/exclude",
            ".git/refs/heads/feature.tmp",
            ".gitignore",
            "changes/.gitignore",
            "base/GZ2E01.toml",
            "base/GZ2E01/res/Msgus/zel_00.bmg",
            "changes/res/Msgus/zel_00.bmg",
            "code/.mod.cpp.swp",
        ] {
            assert!(!is_noise(path), "{path}");
        }
    }
}
