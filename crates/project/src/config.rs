//! `arbiter.toml`: the project's kind and the editions its base is made of.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;

pub const FILE: &str = "arbiter.toml";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    /// Keyed by edition id, such as `GZ2E01`. Empty in a game project.
    ///
    /// One revision per edition, on purpose. IE: RZDE01 ships in rev 0 and 2,
    /// and holding both would mean linking them and building per revision
    /// for that one disc. Unpacking another revision switches the project to
    /// it, see `Project::commit`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub editions: BTreeMap<String, Edition>,
}

/// Picked at creation and never changed.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Game,
}

impl Config {
    /// What committing a disc with this edition would do to the project.
    #[must_use]
    pub fn record(&self, id: &str, revision: u8, files_digest: Hash) -> Recorded {
        match self.editions.get(id) {
            None => Recorded::Added,
            Some(old) if old.revision != revision => Recorded::Switched { from: old.revision },
            Some(old) if old.files_digest == files_digest => Recorded::Same,
            Some(old) => Recorded::Changed {
                was: old.files_digest,
            },
        }
    }
}

/// What committing a staged disc does to its edition.
#[derive(Debug, PartialEq, Eq)]
pub enum Recorded {
    Added,
    Same,
    /// Same revision, different files: likely a modified or bad dump.
    /// `arbiter.toml` keeps the files digest the project was made against.
    Changed {
        was: Hash,
    },
    /// Another revision replaced the old one, see `Config::editions`.
    Switched {
        from: u8,
    },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Edition {
    /// The disc header's version. Stored here only: `base/` is local, and a
    /// clone needs to know which disc to ask for.
    pub revision: u8,
    /// XXH3-128 over the unpacked files' paths and hashes, sorted by path.
    /// Equal when two bases hold the same files, whatever container or dump
    /// they came from.
    ///
    /// Used to checks a collaborator's base after a clone from a repo
    /// (where they need to unpack the game again as `base/` is gitignored).
    pub files_digest: Hash,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let config = Config {
            kind: Kind::Mod,
            editions: BTreeMap::from([(
                "GZ2E01".to_owned(),
                Edition {
                    revision: 0,
                    files_digest: Hash(0x1234),
                },
            )]),
        };
        let text = toml::to_string(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }
}
