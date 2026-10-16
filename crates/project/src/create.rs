//! Making a mod project: its layout, its `.gitignore`, and `git init`.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs, io,
    path::{self, Path, PathBuf},
    process::Command,
};

use crate::{
    Project, ProjectError, base, changes,
    config::{ProjectConfig, ProjectKind},
    disk,
};

/// Every directory a mod project owns.
pub(crate) const MOD_DIRS: &[&str] = &[
    base::DIR,
    changes::DIR,
    "assets",
    "textures",
    "code",
    ".arbiter",
];

/// Working files in `changes/` stay local: only their `.toml` and `.patch` are committed.
pub(crate) const MOD_GITIGNORE: &str = "\
/base/
/.arbiter/
/changes/**
!/changes/**/
!/changes/**/*.toml
!/changes/**/*.patch
";

/// A mod project built in `.<name>.partial/` beside its root, leaving the root
/// as it was until `finish` moves it in. Dropping it removes the partial tree.
pub struct DraftProject {
    pub project: Project,
    root: PathBuf,
    partial: Partial,
}

impl DraftProject {
    /// Replaces the root with the project. The old root waits at
    /// `.<name>.old/` until the new one is in, and goes back if it can't be.
    ///
    /// # Errors
    ///
    /// If a tree can't be moved, or the old root can't be removed.
    pub fn finish(self) -> Result<Project, ProjectError> {
        let Self {
            mut project,
            root,
            partial,
        } = self;
        let old = beside(&root, "old");
        disk::remove_tree(&old)?;
        if let Err(err) = fs::rename(&root, &old)
            && err.kind() != io::ErrorKind::NotFound
        {
            return Err(disk::io_err(&root)(err).into());
        }
        if let Err(err) = fs::rename(&partial.0, &root) {
            // Fails harmlessly when there was no old root.
            let _ = fs::rename(&old, &root);
            return Err(disk::io_err(&root)(err).into());
        }
        disk::remove_tree(&old)?;
        project.root = root;
        Ok(project)
    }
}

struct Partial(PathBuf);

impl Drop for Partial {
    fn drop(&mut self) {
        // Can't report from here. A leftover is cleared by the next `create`.
        let _ = disk::remove_tree(&self.0);
    }
}

/// `<parent>/.<name>.<suffix>`, on the same file system so a rename can swap them.
fn beside(root: &Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(root.file_name().unwrap_or_default());
    name.push(".");
    name.push(suffix);
    root.with_file_name(name)
}

impl Project {
    /// Builds a mod project for `root` and runs `git init` in it. Nothing
    /// in `root` changes until `DraftProject::finish`. A directory that isn't empty
    /// is replaced only if `force`.
    ///
    /// # Errors
    ///
    /// If `root` has no name, isn't empty and `force` isn't set, a file can't
    /// be written, or `git init` fails.
    pub fn create(root: &Path, force: bool) -> Result<DraftProject, ProjectError> {
        let root = path::absolute(root).map_err(disk::io_err(root))?;
        if root.file_name().is_none() {
            return Err(ProjectError::NoName(root));
        }
        let empty = match fs::read_dir(&root) {
            Ok(mut entries) => entries.next().is_none(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => true,
            Err(err) => return Err(disk::io_err(&root)(err).into()),
        };
        if !empty && !force {
            return Err(ProjectError::NotEmpty(root));
        }

        let partial = Partial(beside(&root, "partial"));
        disk::remove_tree(&partial.0)?;
        for dir in MOD_DIRS {
            let path = partial.0.join(dir);
            fs::create_dir_all(&path).map_err(disk::io_err(&path))?;
        }
        let gitignore = partial.0.join(".gitignore");
        fs::write(&gitignore, MOD_GITIGNORE).map_err(disk::io_err(&gitignore))?;
        let project = Self {
            root: partial.0.clone(),
            config: ProjectConfig {
                kind: ProjectKind::Mod,
                target: Some(BTreeSet::new()),
                editions: BTreeMap::new(),
            },
        };
        project.save()?;

        let out = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&partial.0)
            .output()
            .map_err(|err| ProjectError::Git(err.to_string()))?;
        if !out.status.success() {
            return Err(ProjectError::Git(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            ));
        }
        Ok(DraftProject {
            project,
            root,
            partial,
        })
    }
}
