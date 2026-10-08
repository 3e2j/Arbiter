//! Both project kinds and every operation the front ends call: open, save, undo, check, build.
//!
//! A mod project, as `Project::create` writes it. The root is also the git root.
//!
//! ```text
//! my-mod/
//! ├─ arbiter.toml   kind, and each edition's revision and files digest
//! ├─ .gitignore     keeps base/, .arbiter/ and working files out
//! ├─ base/          unpacked editions, read-only, never committed (see `base`)
//! ├─ changes/       .toml and .patch against base, beside their working files
//! ├─ sources/       the modder's own files, shipped as-is
//! ├─ textures/      image replacements at game paths
//! ├─ code/
//! └─ .arbiter/      cache, index, recovery, session
//! ```

pub mod base;
pub mod changes;
pub mod config;
pub mod hash;

use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs, io,
    path::{self, Path, PathBuf},
    process::Command,
};

pub use base::Staged;
pub use config::Recorded;
use config::{Config, Edition, Kind};
use diag::Diagnostics;

/// Every directory a mod project owns.
const MOD_DIRS: &[&str] = &[
    base::DIR,
    changes::DIR,
    "sources",
    "textures",
    "code",
    ".arbiter",
];

/// Working files in `changes/` stay local: only their `.toml` and `.patch` are committed.
const MOD_GITIGNORE: &str = "\
/base/
/.arbiter/
/changes/**
!/changes/**/
!/changes/**/*.toml
!/changes/**/*.patch
";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Base(#[from] base::Error),
    #[error("{0} isn't empty")]
    NotEmpty(PathBuf),
    #[error("{0} has no directory name to build a project beside")]
    NoName(PathBuf),
    #[error("{0} isn't a project, it has no arbiter.toml")]
    NotAProject(PathBuf),
    #[error("{path}: {source}")]
    Config {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("{0} is a game project, which has no base to unpack into")]
    NoBase(PathBuf),
    #[error("git init failed: {0}")]
    Git(String),
}

pub struct Project {
    pub root: PathBuf,
    pub config: Config,
}

/// A mod project built in `.<name>.partial/` beside its root, leaving the root
/// as it was until `finish` moves it in. Dropping it removes the partial tree.
pub struct Draft {
    pub project: Project,
    root: PathBuf,
    partial: Partial,
}

impl Draft {
    /// Replaces the root with the project. The old root waits at
    /// `.<name>.old/` until the new one is in, and goes back if it can't be.
    ///
    /// # Errors
    ///
    /// If a tree can't be moved, or the old root can't be removed.
    pub fn finish(self) -> Result<Project, Error> {
        let Self {
            mut project,
            root,
            partial,
        } = self;
        let old = beside(&root, "old");
        base::remove_tree(&old)?;
        if let Err(err) = fs::rename(&root, &old)
            && err.kind() != io::ErrorKind::NotFound
        {
            return Err(base::io_err(&root)(err).into());
        }
        if let Err(err) = fs::rename(&partial.0, &root) {
            // Fails harmlessly when there was no old root.
            let _ = fs::rename(&old, &root);
            return Err(base::io_err(&root)(err).into());
        }
        base::remove_tree(&old)?;
        project.root = root;
        Ok(project)
    }
}

struct Partial(PathBuf);

impl Drop for Partial {
    fn drop(&mut self) {
        // Can't report from here. A leftover is cleared by the next `create`.
        let _ = base::remove_tree(&self.0);
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
    /// in `root` changes until `Draft::finish`. A directory that isn't empty
    /// is replaced only if `force`.
    ///
    /// # Errors
    ///
    /// If `root` has no name, isn't empty and `force` isn't set, a file can't
    /// be written, or `git init` fails.
    pub fn create(root: &Path, force: bool) -> Result<Draft, Error> {
        let root = path::absolute(root).map_err(base::io_err(root))?;
        if root.file_name().is_none() {
            return Err(Error::NoName(root));
        }
        let empty = match fs::read_dir(&root) {
            Ok(mut entries) => entries.next().is_none(),
            Err(err) if err.kind() == io::ErrorKind::NotFound => true,
            Err(err) => return Err(base::io_err(&root)(err).into()),
        };
        if !empty && !force {
            return Err(Error::NotEmpty(root));
        }

        let partial = Partial(beside(&root, "partial"));
        base::remove_tree(&partial.0)?;
        for dir in MOD_DIRS {
            let path = partial.0.join(dir);
            fs::create_dir_all(&path).map_err(base::io_err(&path))?;
        }
        let gitignore = partial.0.join(".gitignore");
        fs::write(&gitignore, MOD_GITIGNORE).map_err(base::io_err(&gitignore))?;
        let project = Self {
            root: partial.0.clone(),
            config: Config {
                kind: Kind::Mod,
                editions: BTreeMap::new(),
            },
        };
        project.save()?;

        let out = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(&partial.0)
            .output()
            .map_err(|err| Error::Git(err.to_string()))?;
        if !out.status.success() {
            return Err(Error::Git(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            ));
        }
        Ok(Draft {
            project,
            root,
            partial,
        })
    }

    /// # Errors
    ///
    /// If `root` has no `arbiter.toml` or it can't be read.
    pub fn open(root: &Path) -> Result<Self, Error> {
        let path = root.join(config::FILE);
        let text = match fs::read_to_string(&path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(Error::NotAProject(root.to_path_buf()));
            }
            text => text.map_err(base::io_err(&path))?,
        };
        let config = toml::from_str(&text).map_err(|source| Error::Config { path, source })?;
        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }

    /// Unpacks a disc beside `base/`, without touching the project. Then
    /// `switch_check` it if it switches revision, and `commit` or discard it.
    ///
    /// # Errors
    ///
    /// If this is a game project, or unpacking fails.
    pub fn stage(&self, disc: &Path) -> Result<Staged, Error> {
        if self.config.kind != Kind::Mod {
            return Err(Error::NoBase(self.root.clone()));
        }
        Ok(base::stage(&self.root.join(base::DIR), disc, &self.config)?)
    }

    /// Every change in `changes/` that wouldn't apply cleanly to the staged base.
    #[must_use]
    pub fn switch_check(&self, staged: &Staged) -> Diagnostics {
        let mut diag = Diagnostics::default();
        changes::check(&self.root.join(changes::DIR), staged.tree(), &mut diag);
        diag
    }

    /// Swaps the staged tree in as its edition's base. `arbiter.toml` changes
    /// only for a new edition or a revision switch. Returns the staged outcome.
    ///
    /// # Errors
    ///
    /// If the tree can't be swapped in or the project can't be saved.
    pub fn commit(&mut self, staged: Staged) -> Result<Recorded, Error> {
        staged.commit(&self.root.join(base::DIR))?;
        if let Recorded::Added | Recorded::Switched { .. } = staged.outcome {
            let edition = Edition {
                platform: staged.platform,
                revision: staged.revision,
                files_digest: staged.manifest.files_digest(),
            };
            self.config.editions.insert(staged.id, edition);
            self.save()?;
        }
        Ok(staged.outcome)
    }

    fn save(&self) -> Result<(), Error> {
        let path = self.root.join(config::FILE);
        let text = toml::to_string(&self.config).map_err(|source| base::Error::Toml {
            path: path.clone(),
            source,
        })?;
        base::write_atomic(&path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Platform, hash::Hash};

    /// A fresh project at `<tmp>/mod`, so its siblings stay inside `<tmp>`.
    fn fresh() -> (tempfile::TempDir, Project) {
        let dir = tempfile::tempdir().unwrap();
        let project = Project::create(&dir.path().join("mod"), false)
            .unwrap()
            .finish()
            .unwrap();
        (dir, project)
    }

    #[test]
    fn creates_a_mod_layout() {
        let (dir, project) = fresh();
        for d in MOD_DIRS.iter().chain(&[".git"]) {
            assert!(project.root.join(d).is_dir(), "{d}");
        }
        assert_eq!(Project::open(&project.root).unwrap().config.kind, Kind::Mod);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn force_replaces_everything() {
        let (dir, project) = fresh();
        let readme = project.root.join("README.md");
        let marker = project.root.join(".git/arbiter-test");
        fs::write(&readme, "").unwrap();
        fs::write(&marker, "").unwrap();

        assert!(matches!(
            Project::create(&project.root, false),
            Err(Error::NotEmpty(_))
        ));
        assert!(readme.exists());

        Project::create(&project.root, true)
            .unwrap()
            .finish()
            .unwrap();
        assert!(!readme.exists());
        assert!(!marker.exists());
        assert!(project.root.join(".git").is_dir());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_dropped_draft_leaves_the_old_root() {
        let (dir, project) = fresh();
        let readme = project.root.join("README.md");
        fs::write(&readme, "").unwrap();

        drop(Project::create(&project.root, true).unwrap());
        assert!(readme.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    fn stage_fake(project: &Project, id: &str, revision: u8, file: &[u8]) -> Staged {
        let tree = project.root.join(base::DIR).join(format!(".{id}.partial"));
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join("a.arc"), file).unwrap();
        let manifest = base::Manifest {
            files: [("a.arc".to_owned(), Hash::of(file))].into(),
        };
        let outcome = project.config.record(id, revision, manifest.files_digest());
        Staged {
            id: id.to_owned(),
            revision,
            platform: Platform::Wii,
            manifest,
            bytes: file.len() as u64,
            outcome,
            tree,
        }
    }

    #[test]
    fn commit_records_editions() {
        let (_dir, mut project) = fresh();
        let root = project.root.clone();
        let mut commit = |revision, file: &[u8]| {
            let staged = stage_fake(&project, "RZDE01", revision, file);
            project.commit(staged).unwrap()
        };

        assert_eq!(commit(0, b"rev 0"), Recorded::Added);
        assert_eq!(commit(0, b"rev 0"), Recorded::Same);
        let was = Hash::of(b"rev 0");
        let changed = Recorded::Changed {
            was: base::Manifest {
                files: [("a.arc".to_owned(), was)].into(),
            }
            .files_digest(),
        };
        assert_eq!(commit(0, b"scrubbed"), changed);
        assert_eq!(commit(2, b"rev 2"), Recorded::Switched { from: 0 });

        let project = Project::open(&root).unwrap();
        assert_eq!(project.config.editions["RZDE01"].revision, 2);
        assert_eq!(project.config.editions["RZDE01"].platform, Platform::Wii);
        assert_eq!(fs::read(root.join("base/RZDE01/a.arc")).unwrap(), b"rev 2");
        assert!(!root.join("base/.RZDE01.partial").exists());
    }

    #[test]
    #[ignore = "needs a retail disc at discs/NA.iso"]
    fn unpacks_a_retail_disc_into_a_project() {
        let disc = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../discs/NA.iso");
        let (_dir, mut project) = fresh();
        let root = project.root.clone();

        let staged = project.stage(&disc).unwrap();
        assert_eq!(staged.id, "GZ2E01");
        assert_eq!(staged.platform, Platform::GameCube);
        assert_eq!(project.commit(staged).unwrap(), Recorded::Added);
        for file in ["res/Msgus/bmgres.arc", "sys/main.dol"] {
            let file = root.join("base/GZ2E01").join(file);
            assert!(fs::metadata(&file).unwrap().permissions().readonly());
        }

        let staged = project.stage(&disc).unwrap();
        assert_eq!(project.commit(staged).unwrap(), Recorded::Same);
        assert!(!root.join("base/.GZ2E01.partial").exists());
        assert!(
            Project::open(&root)
                .unwrap()
                .config
                .editions
                .contains_key("GZ2E01")
        );
    }
}
