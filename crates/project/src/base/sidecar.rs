//! `.rarc.toml`, inside the directory an archive opened into: what packs it
//! back besides its members' bytes.
//!
//! Each compression is recorded once: an archive's own at the top of its
//! sidecar, a plain member's on its line in the sidecar holding it.
//! A member that's an archive leaves it to its own sidecar.
//!
//! A mod changes it through [`Edit`], keyed by member path.

use std::{collections::HashMap, mem};

use diag::{Address, Code, Diagnostic, Diagnostics, Key, Location, Severity};
use formats::rarc::{self, Rarc};
use serde::{Deserialize, Serialize};

use super::Compression;

pub const NAME: &str = ".rarc.toml";

pub const MISSING_MEMBER: Code = Code {
    id: "packing/missing-member",
    severity: Severity::Warning,
    summary: "a change names an archive member the base doesn't hold",
};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    /// See `formats::rarc::Rarc::root`.
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<Compression>,
    /// In entry order, which is also data order, so a rebuild keeps it.
    pub members: Vec<Member>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// Under the archive's directory.
    pub path: String,
    /// What other files reference it by. `None` for one a modder added.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compression: Option<Compression>,
    #[serde(default)]
    pub preload: Preload,
}

/// Mirrors `formats::rarc::Preload`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Preload {
    #[default]
    Mram,
    Aram,
    Disc,
}

impl From<rarc::Preload> for Preload {
    fn from(preload: rarc::Preload) -> Self {
        match preload {
            rarc::Preload::Mram => Self::Mram,
            rarc::Preload::Aram => Self::Aram,
            rarc::Preload::Disc => Self::Disc,
        }
    }
}

impl Sidecar {
    #[must_use]
    /// `members` is what came off each of `archive.files`.
    pub fn new(
        archive: &Rarc,
        compression: Option<Compression>,
        members: impl IntoIterator<Item = Option<Compression>>,
    ) -> Self {
        Self {
            root: archive.root.clone(),
            compression,
            members: archive
                .files
                .iter()
                .zip(members)
                .map(|(file, compression)| Member {
                    path: file.path.clone(),
                    id: file.id,
                    compression,
                    preload: file.preload.into(),
                })
                .collect(),
        }
    }
}

/// Also what a mod stores. An omitted compression is none.
///
/// A member that's an archive takes its compression from its own sidecar,
/// so setting it here is left for a check to catch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum Edit {
    Root {
        root: String,
    },
    Compression {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compression: Option<Compression>,
    },
    MemberCompression {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compression: Option<Compression>,
    },
    Preload {
        path: String,
        preload: Preload,
    },
}

impl Edit {
    /// The member it names, if it names one.
    fn member(&self) -> Option<&str> {
        match self {
            Self::Root { .. } | Self::Compression { .. } => None,
            Self::MemberCompression { path, .. } | Self::Preload { path, .. } => Some(path),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Root,
    Compression,
    /// By path.
    Member(String),
}

impl Address for Item {
    fn key(&self) -> Vec<Key> {
        match self {
            Self::Root => vec![("field", "root".to_owned())],
            Self::Compression => vec![("field", "compression".to_owned())],
            Self::Member(path) => vec![("member", path.clone())],
        }
    }
}

impl Sidecar {
    fn member_mut(&mut self, path: &str) -> Option<&mut Member> {
        self.members.iter_mut().find(|member| member.path == path)
    }
}

impl formats::Edit for Sidecar {
    type Edit = Edit;
    type Item = Item;

    fn apply(&mut self, edit: Edit) -> (Edit, Vec<Item>) {
        match edit {
            Edit::Root { root } => {
                let old = mem::replace(&mut self.root, root);
                (Edit::Root { root: old }, vec![Item::Root])
            }
            Edit::Compression { compression } => {
                let old = mem::replace(&mut self.compression, compression);
                (
                    Edit::Compression { compression: old },
                    vec![Item::Compression],
                )
            }
            Edit::MemberCompression { path, compression } => match self.member_mut(&path) {
                Some(member) => {
                    let old = mem::replace(&mut member.compression, compression);
                    let inverse = Edit::MemberCompression {
                        path: path.clone(),
                        compression: old,
                    };
                    (inverse, vec![Item::Member(path)])
                }
                None => (Edit::MemberCompression { path, compression }, Vec::new()),
            },
            Edit::Preload { path, preload } => match self.member_mut(&path) {
                Some(member) => {
                    let old = mem::replace(&mut member.preload, preload);
                    let inverse = Edit::Preload {
                        path: path.clone(),
                        preload: old,
                    };
                    (inverse, vec![Item::Member(path)])
                }
                None => (Edit::Preload { path, preload }, Vec::new()),
            },
        }
    }
}

impl formats::Patch for Sidecar {
    type Change = Edit;

    /// Members matched by path. One added or removed is a change to the
    /// archive's files, not to how it packs, so it makes no change here.
    fn diff(base: &Self, edited: &Self) -> Vec<Edit> {
        let mut changes = Vec::new();
        if base.root != edited.root {
            changes.push(Edit::Root {
                root: edited.root.clone(),
            });
        }
        if base.compression != edited.compression {
            changes.push(Edit::Compression {
                compression: edited.compression,
            });
        }

        let by_path: HashMap<&str, &Member> = base
            .members
            .iter()
            .map(|member| (member.path.as_str(), member))
            .collect();
        for member in &edited.members {
            let Some(old) = by_path.get(member.path.as_str()) else {
                continue;
            };
            if old.compression != member.compression {
                changes.push(Edit::MemberCompression {
                    path: member.path.clone(),
                    compression: member.compression,
                });
            }
            if old.preload != member.preload {
                changes.push(Edit::Preload {
                    path: member.path.clone(),
                    preload: member.preload,
                });
            }
        }
        changes
    }

    fn patch(&mut self, changes: &[Edit], diag: &mut Diagnostics) {
        for change in changes {
            let (_, touched) = formats::Edit::apply(self, change.clone());
            if let (Some(path), true) = (change.member(), touched.is_empty()) {
                diag.push(
                    Diagnostic::new(
                        &MISSING_MEMBER,
                        format!("`{path}` isn't in the archive, so its change is skipped"),
                    )
                    .at(Location::key("member", path)),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use formats::{Edit as _, Patch as _};

    use super::*;

    fn archive() -> Sidecar {
        Sidecar {
            root: "archive".to_owned(),
            compression: Some(Compression::Yaz0),
            members: vec![
                Member {
                    path: "zel_00.bmg".to_owned(),
                    id: Some(0),
                    compression: Some(Compression::Yaz0),
                    preload: Preload::Mram,
                },
                Member {
                    path: "sub/a.bmd".to_owned(),
                    id: Some(1),
                    compression: None,
                    preload: Preload::Mram,
                },
            ],
        }
    }

    fn edits() -> Vec<Edit> {
        vec![
            Edit::Root {
                root: "renamed".to_owned(),
            },
            Edit::Compression { compression: None },
            Edit::MemberCompression {
                path: "zel_00.bmg".to_owned(),
                compression: Some(Compression::Yay0),
            },
            Edit::Preload {
                path: "sub/a.bmd".to_owned(),
                preload: Preload::Aram,
            },
        ]
    }

    #[test]
    fn inverses_undo_in_reverse() {
        let base = archive();
        let mut sidecar = base.clone();
        let mut inverses = Vec::new();
        let mut touched = Vec::new();
        for edit in edits() {
            let (inverse, items) = sidecar.apply(edit);
            inverses.push(inverse);
            touched.extend(items);
        }
        assert_eq!(
            touched,
            [
                Item::Root,
                Item::Compression,
                Item::Member("zel_00.bmg".to_owned()),
                Item::Member("sub/a.bmd".to_owned()),
            ]
        );
        assert_ne!(sidecar, base);
        for inverse in inverses.into_iter().rev() {
            sidecar.apply(inverse);
        }
        assert_eq!(sidecar, base);
    }

    #[test]
    fn a_diff_patches_the_base_into_the_edit() {
        let base = archive();
        let mut edited = base.clone();
        for edit in edits() {
            edited.apply(edit);
        }
        assert_eq!(Sidecar::diff(&base, &base), []);

        let changes = Sidecar::diff(&base, &edited);
        assert_eq!(changes, edits());
        let mut patched = base;
        let mut diag = Diagnostics::default();
        patched.patch(&changes, &mut diag);
        assert_eq!(patched, edited);
        assert_eq!(diag.items, []);
    }

    #[test]
    fn a_member_the_base_lacks_is_skipped_and_reported() {
        let edit = Edit::Preload {
            path: "gone.bmg".to_owned(),
            preload: Preload::Disc,
        };
        let mut sidecar = archive();
        let (inverse, touched) = sidecar.apply(edit.clone());
        assert_eq!((inverse, touched), (edit.clone(), Vec::new()));
        assert_eq!(sidecar, archive());

        let mut diag = Diagnostics::default();
        sidecar.patch(&[edit], &mut diag);
        assert_eq!(sidecar, archive());
        let [found] = diag.items.as_slice() else {
            panic!("expected one diagnostic, got {:?}", diag.items);
        };
        assert_eq!(found.code, &MISSING_MEMBER);
        assert_eq!(found.at, Some(Location::key("member", "gone.bmg")));
    }

    #[test]
    fn changes_read_back_with_none_omitted() {
        #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
        struct Stored {
            change: Vec<Edit>,
        }

        let stored = Stored { change: edits() };
        let text = toml::to_string(&stored).unwrap();
        assert_eq!(
            text,
            "[[change]]\n\
             edit = \"root\"\n\
             root = \"renamed\"\n\
             \n\
             [[change]]\n\
             edit = \"compression\"\n\
             \n\
             [[change]]\n\
             edit = \"member_compression\"\n\
             path = \"zel_00.bmg\"\n\
             compression = \"yay0\"\n\
             \n\
             [[change]]\n\
             edit = \"preload\"\n\
             path = \"sub/a.bmd\"\n\
             preload = \"aram\"\n"
        );
        assert_eq!(toml::from_str::<Stored>(&text).unwrap(), stored);
    }

    #[test]
    fn reads_back() {
        let sidecar = Sidecar {
            root: "archive".to_owned(),
            compression: Some(Compression::Yaz0),
            members: vec![
                Member {
                    path: "zel_00.bmg".to_owned(),
                    id: Some(0),
                    compression: Some(Compression::Yaz0),
                    preload: Preload::Mram,
                },
                Member {
                    path: "sub/a.bmd".to_owned(),
                    id: None,
                    compression: None,
                    preload: Preload::Aram,
                },
            ],
        };
        let text = toml::to_string(&sidecar).unwrap();
        assert_eq!(toml::from_str::<Sidecar>(&text).unwrap(), sidecar);
    }

    #[test]
    fn an_empty_archive_reads_back() {
        let sidecar = Sidecar {
            root: "empty".to_owned(),
            compression: None,
            members: Vec::new(),
        };
        let text = toml::to_string(&sidecar).unwrap();
        assert_eq!(toml::from_str::<Sidecar>(&text).unwrap(), sidecar);
    }
}
