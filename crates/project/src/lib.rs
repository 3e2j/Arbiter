//! Arbiter projects, mod or game ([`config::ProjectKind`]), and how their
//! files are stored: creating and opening one, unpacking discs into its base,
//! and reading its files into a [`tree`] for the editor.
//!
//! A mod project as [`Project::create`] writes it, rooted at its git root:
//!
//! ```text
//! my-mod/
//! ├─ arbiter.toml   kind, and each edition's revision and files digest
//! ├─ .gitignore     keeps base/, .arbiter/ and working files out
//! ├─ base/          unpacked editions, read-only, never committed (see `base`)
//! ├─ changes/       .toml and .patch against base, beside their working files
//! ├─ assets/        the modder's own files, such as the icon and banner, shipped as-is
//! ├─ textures/      image replacements at game paths
//! ├─ code/
//! └─ .arbiter/      cache, index, recovery, session
//! ```

pub mod base;
pub mod changes;
pub mod config;
mod create;
mod disk;
pub mod hash;
mod local;
pub mod tree;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub use base::{PeekedDisc, StagedDisc};
pub use config::EditionChange;
use config::{Edition, ProjectConfig, ProjectKind};
pub use create::DraftProject;
use diag::Diagnostics;
pub use formats::games::edition::{DumpVerdict, Game};
use hash::Hash;
pub use local::{Entry, LOG, Level, Log};

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error(transparent)]
    Base(#[from] base::BaseError),
    #[error(transparent)]
    Tree(#[from] tree::TreeError),
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
    #[error(transparent)]
    Target(#[from] config::EditionConflict),
    #[error("git init failed: {0}")]
    Git(String),
}

pub struct Project {
    pub root: PathBuf,
    pub config: ProjectConfig,
}

impl Project {
    /// # Errors
    ///
    /// If `root` has no `arbiter.toml` or it can't be read.
    pub fn open(root: &Path) -> Result<Self, ProjectError> {
        let path = root.join(config::FILE);
        let text = match fs::read_to_string(&path) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Err(ProjectError::NotAProject(root.to_path_buf()));
            }
            text => text.map_err(disk::io_err(&path))?,
        };
        let config =
            toml::from_str(&text).map_err(|source| ProjectError::Config { path, source })?;
        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }

    /// A disc's id, revision and known game, without unpacking it, so the
    /// target's game and duplicate editions are settled before any disc is staged.
    ///
    /// # Errors
    ///
    /// If the disc can't be read.
    pub fn peek(disc: &Path) -> Result<PeekedDisc, ProjectError> {
        Ok(base::peek(disc)?)
    }

    /// Hashes whole discs in parallel and checks each against its edition's
    /// clean dump, before any is unpacked. Each hash goes to `stage`.
    ///
    /// # Errors
    ///
    /// If a disc can't be read.
    pub fn verify(
        discs: &[(&Path, &PeekedDisc)],
    ) -> Result<Vec<(Hash, DumpVerdict)>, ProjectError> {
        Ok(base::verify(discs)?)
    }

    /// Unpacks a disc beside `base/`, without touching the project. Then
    /// `switch_check` it if it switches revision, and `commit` or discard it.
    ///
    /// # Errors
    ///
    /// If this is a game project, or unpacking fails.
    pub fn stage(&self, disc: &Path, disc_hash: Hash) -> Result<StagedDisc, ProjectError> {
        if self.config.kind != ProjectKind::Mod {
            return Err(ProjectError::NoBase(self.root.clone()));
        }
        Ok(base::stage(
            &self.root.join(base::DIR),
            disc,
            disc_hash,
            &self.config,
        )?)
    }

    /// Every change in `changes/` that wouldn't apply cleanly to the staged base.
    #[must_use]
    pub fn switch_check(&self, staged: &StagedDisc) -> Diagnostics {
        let mut diag = Diagnostics::default();
        changes::check(&self.root.join(changes::DIR), staged.tree(), &mut diag);
        diag
    }

    /// Swaps the staged tree in as its edition's base, as a target or a
    /// reference. `arbiter.toml` changes only for a new edition, a revision
    /// switch, or a change of target. Returns the staged outcome.
    ///
    /// # Errors
    ///
    /// If `target` and it's a known edition of another game than the target's,
    /// the tree can't be swapped in, or the project can't be saved.
    pub fn commit(
        &mut self,
        staged: StagedDisc,
        target: bool,
    ) -> Result<EditionChange, ProjectError> {
        if target {
            self.config.check_target(&staged.id)?;
        }
        staged.commit(&self.root.join(base::DIR))?;
        let mut dirty = self.config.set_target(&staged.id, target);
        if let EditionChange::Added | EditionChange::Switched { .. } = staged.outcome {
            let edition = Edition {
                platform: staged.platform,
                region: staged.region,
                country: staged.country,
                revision: staged.revision,
                disc_hash: staged.disc_hash,
                files_digest: staged.manifest.files_digest(),
            };
            self.config.editions.insert(staged.id, edition);
            dirty = true;
        }
        if dirty {
            self.save()?;
        }
        Ok(staged.outcome)
    }

    /// The edition whose base `path` is in, relative to the root, by its id.
    #[must_use]
    pub fn edition_of(&self, path: &str) -> Option<(&str, &Edition)> {
        let rest = path.strip_prefix(base::DIR)?.strip_prefix('/')?;
        let (id, _) = rest.split_once('/')?;
        self.config
            .editions
            .get_key_value(id)
            .map(|(id, edition)| (id.as_str(), edition))
    }

    fn save(&self) -> Result<(), ProjectError> {
        let path = self.root.join(config::FILE);
        let text = toml::to_string(&self.config).map_err(|source| base::BaseError::Toml {
            path: path.clone(),
            source,
        })?;
        disk::write_atomic(&path, text.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::config::Platform;

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
        for d in create::MOD_DIRS.iter().chain(&[".git"]) {
            assert!(project.root.join(d).is_dir(), "{d}");
        }
        assert_eq!(
            Project::open(&project.root).unwrap().config.kind,
            ProjectKind::Mod
        );
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
            Err(ProjectError::NotEmpty(_))
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

    fn stage_fake(project: &Project, id: &str, revision: u8, file: &[u8]) -> StagedDisc {
        let tree = project.root.join(base::DIR).join(format!(".{id}.partial"));
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join("a.arc"), file).unwrap();
        let manifest = base::Manifest {
            files: [(
                "a.arc".to_owned(),
                base::ManifestEntry {
                    hash: Hash::of(file),
                    compression: None,
                },
            )]
            .into(),
        };
        let outcome = project.config.record(id, revision, manifest.files_digest());
        StagedDisc {
            id: id.to_owned(),
            revision,
            platform: Platform::Wii,
            region: None,
            country: None,
            manifest,
            disc_hash: Hash::of(file),
            edition: None,
            guess: None,
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
            project.commit(staged, true).unwrap()
        };

        assert_eq!(commit(0, b"rev 0"), EditionChange::Added);
        assert_eq!(commit(0, b"rev 0"), EditionChange::Same);
        let was = Hash::of(b"rev 0");
        let changed = EditionChange::Changed {
            was: base::Manifest {
                files: [(
                    "a.arc".to_owned(),
                    base::ManifestEntry {
                        hash: was,
                        compression: None,
                    },
                )]
                .into(),
            }
            .files_digest(),
        };
        assert_eq!(commit(0, b"scrubbed"), changed);
        assert_eq!(commit(2, b"rev 2"), EditionChange::Switched { from: 0 });

        let project = Project::open(&root).unwrap();
        assert_eq!(project.config.editions["RZDE01"].revision, 2);
        assert_eq!(project.config.editions["RZDE01"].platform, Platform::Wii);
        assert_eq!(fs::read(root.join("base/RZDE01/a.arc")).unwrap(), b"rev 2");
        assert!(!root.join("base/.RZDE01.partial").exists());
    }

    #[test]
    fn edition_of_reads_the_base_folder() {
        let (_dir, mut project) = fresh();
        let staged = stage_fake(&project, "GZ2E01", 0, b"file");
        project.commit(staged, true).unwrap();

        let id = |path| project.edition_of(path).map(|(id, _)| id);
        assert_eq!(id("base/GZ2E01/res/Msgus/zel_00.bmg"), Some("GZ2E01"));
        assert_eq!(id("base/GZ2P01/res/Msguk/zel_00.bmg"), None);
        assert_eq!(id("base/GZ2E01"), None);
        assert_eq!(id("changes/res/Msgus/zel_00.bmg"), None);
        assert_eq!(id("based/GZ2E01/a.bmg"), None);
    }

    #[test]
    fn commit_keeps_references_out_of_the_target() {
        let (_dir, mut project) = fresh();
        let root = project.root.clone();
        let staged = stage_fake(&project, "AAAA01", 0, b"unknown");
        project.commit(staged, false).unwrap();
        let staged = stage_fake(&project, "GZ2E01", 0, b"known");
        project.commit(staged, true).unwrap();

        let config = Project::open(&root).unwrap().config;
        assert!(config.editions.contains_key("AAAA01"));
        assert_eq!(config.target, Some(BTreeSet::from(["GZ2E01".to_owned()])));
    }
}
