//! `.rarc.toml`, inside the directory an archive opened into: what packs it
//! back besides its members' bytes.
//!
//! Each compression is recorded once: an archive's own at the top of its
//! sidecar, a plain member's on its line in the sidecar holding it.
//! A member that's an archive leaves it to its own sidecar.
//!
//! Written by its `Display`, one line per member.

use std::fmt::{self, Display};

use formats::rarc::{self, Rarc};
use serde::Deserialize;
use toml_writer::TomlWrite;

use super::{Compression, assign};

pub const NAME: &str = ".rarc.toml";

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct Sidecar {
    /// See `formats::rarc::Rarc::root`.
    pub root: String,
    pub compression: Option<Compression>,
    /// In entry order, which is also data order, so a rebuild keeps it.
    pub members: Vec<Member>,
}

#[derive(Deserialize, Debug, PartialEq, Eq)]
pub struct Member {
    /// Under the archive's directory.
    pub path: String,
    /// What other files reference it by. `None` for one a modder added.
    pub id: Option<u16>,
    pub compression: Option<Compression>,
    #[serde(default)]
    pub preload: Preload,
}

/// Mirrors `formats::rarc::Preload`.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Preload {
    #[default]
    Mram,
    Aram,
    Disc,
}

impl Preload {
    const fn name(self) -> &'static str {
        match self {
            Self::Mram => "mram",
            Self::Aram => "aram",
            Self::Disc => "disc",
        }
    }
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

impl Display for Sidecar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        assign(f, "root")?;
        f.value(self.root.as_str())?;
        f.newline()?;
        if let Some(compression) = self.compression {
            assign(f, "compression")?;
            f.value(compression.name())?;
            f.newline()?;
        }
        assign(f, "members")?;
        f.open_array()?;
        for member in &self.members {
            f.newline()?;
            f.write_str("    ")?;
            f.open_inline_table()?;
            f.space()?;
            assign(f, "path")?;
            f.value(member.path.as_str())?;
            if let Some(id) = member.id {
                f.val_sep()?;
                f.space()?;
                assign(f, "id")?;
                f.value(id)?;
            }
            if let Some(compression) = member.compression {
                f.val_sep()?;
                f.space()?;
                assign(f, "compression")?;
                f.value(compression.name())?;
            }
            if member.preload != Preload::Mram {
                f.val_sep()?;
                f.space()?;
                assign(f, "preload")?;
                f.value(member.preload.name())?;
            }
            f.space()?;
            f.close_inline_table()?;
            f.val_sep()?;
        }
        if !self.members.is_empty() {
            f.newline()?;
        }
        f.close_array()?;
        f.newline()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_line_per_member_and_reads_back() {
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
        let text = sidecar.to_string();
        assert_eq!(
            text,
            "root = \"archive\"\n\
             compression = \"yaz0\"\n\
             members = [\n    \
                 { path = \"zel_00.bmg\", id = 0, compression = \"yaz0\" },\n    \
                 { path = \"sub/a.bmd\", preload = \"aram\" },\n\
             ]\n"
        );
        assert_eq!(toml::from_str::<Sidecar>(&text).unwrap(), sidecar);
    }

    #[test]
    fn an_empty_archive_reads_back() {
        let sidecar = Sidecar {
            root: "empty".to_owned(),
            compression: None,
            members: Vec::new(),
        };
        let text = sidecar.to_string();
        assert_eq!(text, "root = \"empty\"\nmembers = []\n");
        assert_eq!(toml::from_str::<Sidecar>(&text).unwrap(), sidecar);
    }
}
