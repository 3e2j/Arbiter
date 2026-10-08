//! `arbiter.toml`: the project's kind and the editions its base is made of.

use std::collections::{BTreeMap, BTreeSet};

use game::edition::{self, Game};

use serde::{Deserialize, Serialize};

use crate::hash::Hash;

pub const FILE: &str = "arbiter.toml";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Config {
    pub kind: Kind,
    /// The editions changes apply to, and builds are made for.
    /// Every other edition is a reference: read-only and never built.
    ///
    /// Known editions in here are all one game, unknown ones are the user's call.
    ///
    /// Absent in a project from before targets, where every edition is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<BTreeSet<String>>,
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

/// A known edition of one game can't join a target of another.
#[derive(Debug, thiserror::Error)]
#[error("{id} is {}, but the target is {}", .game.title(), .target.title())]
pub struct Conflict {
    pub id: String,
    pub game: Game,
    pub target: Game,
}

impl Config {
    #[must_use]
    pub fn is_target(&self, id: &str) -> bool {
        self.target.as_ref().is_none_or(|t| t.contains(id))
    }

    /// The game of the target's known editions. `None` if it has none.
    #[must_use]
    pub fn target_game(&self) -> Option<Game> {
        self.editions
            .keys()
            .filter(|id| self.is_target(id))
            .find_map(|id| Some(edition::edition(id)?.game))
    }

    /// Whether `id` can join the target.
    ///
    /// # Errors
    ///
    /// If it's a known edition of another game than the target's.
    pub fn check_target(&self, id: &str) -> Result<(), Conflict> {
        let (Some(e), Some(target)) = (edition::edition(id), self.target_game()) else {
            return Ok(());
        };
        if e.game == target {
            return Ok(());
        }
        Err(Conflict {
            id: id.to_owned(),
            game: e.game,
            target,
        })
    }

    /// Adds or removes `id`. Returns whether that changed anything.
    pub(crate) fn set_target(&mut self, id: &str, target: bool) -> bool {
        if self.is_target(id) == target {
            return false;
        }
        let set = self
            .target
            .get_or_insert_with(|| self.editions.keys().cloned().collect());
        if target {
            set.insert(id.to_owned());
        } else {
            set.remove(id);
        }
        true
    }

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

/// Mirrors `pack::disc::Platform`, which stays out of the config format.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    GameCube,
    Wii,
}

impl From<pack::disc::Platform> for Platform {
    fn from(platform: pack::disc::Platform) -> Self {
        match platform {
            pack::disc::Platform::GameCube => Self::GameCube,
            pack::disc::Platform::Wii => Self::Wii,
        }
    }
}

/// Mirrors `pack::disc::Region`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Region {
    NtscJ,
    NtscU,
    Pal,
    NtscK,
}

impl From<pack::disc::Region> for Region {
    fn from(region: pack::disc::Region) -> Self {
        match region {
            pack::disc::Region::NtscJ => Self::NtscJ,
            pack::disc::Region::NtscU => Self::NtscU,
            pack::disc::Region::Pal => Self::Pal,
            pack::disc::Region::NtscK => Self::NtscK,
        }
    }
}

/// Mirrors `pack::disc::Country`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Country {
    World,
    Usa,
    Japan,
    Korea,
    Taiwan,
    Europe,
    Germany,
    France,
    Italy,
    Netherlands,
    Russia,
    Spain,
    Australia,
}

impl From<pack::disc::Country> for Country {
    fn from(country: pack::disc::Country) -> Self {
        use pack::disc::Country as C;
        match country {
            C::World => Self::World,
            C::Usa => Self::Usa,
            C::Japan => Self::Japan,
            C::Korea => Self::Korea,
            C::Taiwan => Self::Taiwan,
            C::Europe => Self::Europe,
            C::Germany => Self::Germany,
            C::France => Self::France,
            C::Italy => Self::Italy,
            C::Netherlands => Self::Netherlands,
            C::Russia => Self::Russia,
            C::Spain => Self::Spain,
            C::Australia => Self::Australia,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Edition {
    pub platform: Platform,
    /// From the disc, see `pack::disc::Disc::region`. Absent if it wasn't one of the four.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<Region>,
    /// From the disc, see `pack::disc::Disc::country`. Absent for an uncatalogued code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<Country>,
    /// The disc header's version. Stored here only: `base/` is local, and a
    /// clone needs to know which disc to ask for.
    pub revision: u8,
    /// XXH3-128 of the whole disc, see `pack::disc::hash`.
    ///
    /// Catalog hash for a retail disc.
    ///
    /// Used to verify that a disc is unmodified from whats expected in a catalog.
    pub disc_hash: Hash,
    /// XXH3-128 over all unpacked files' paths and hashes, sorted by path.
    /// Unpacked file-based check, not a disc check (see `disc_hash` for that).
    ///
    /// Used to verify all unpacked files have the same bytes (when unpacking again),
    /// even if it came from a modified disc.
    // A changed digest only means "something" has changed, not what. So this is
    // used as a fast-path check before checking against every file that differs.
    pub files_digest: Hash,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let config = Config {
            kind: Kind::Mod,
            target: Some(BTreeSet::from(["GZ2E01".to_owned()])),
            editions: BTreeMap::from([(
                "GZ2E01".to_owned(),
                Edition {
                    platform: Platform::GameCube,
                    region: Some(Region::NtscU),
                    country: Some(Country::Usa),
                    revision: 0,
                    disc_hash: Hash(0x5678),
                    files_digest: Hash(0x1234),
                },
            )]),
        };
        let text = toml::to_string(&config).unwrap();
        assert_eq!(toml::from_str::<Config>(&text).unwrap(), config);
    }

    fn with(ids: &[&str], target: Option<&[&str]>) -> Config {
        let edition = || Edition {
            platform: Platform::GameCube,
            region: None,
            country: None,
            revision: 0,
            disc_hash: Hash(0),
            files_digest: Hash(0),
        };
        Config {
            kind: Kind::Mod,
            target: target.map(|t| t.iter().map(|&id| id.to_owned()).collect()),
            editions: ids.iter().map(|&id| (id.to_owned(), edition())).collect(),
        }
    }

    #[test]
    fn no_target_means_every_edition() {
        let mut config = with(&["GZ2E01", "GZ2P01"], None);
        assert!(config.is_target("GZ2P01"));
        assert!(config.set_target("GZ2P01", false));
        assert_eq!(config.target, Some(BTreeSet::from(["GZ2E01".to_owned()])));
        assert!(!config.set_target("GZ2P01", false));
    }

    #[test]
    fn target_game_skips_unknown_and_reference_editions() {
        let config = with(&["AAAA01", "GZ2E01"], Some(&["AAAA01"]));
        assert_eq!(config.target_game(), None);
        let config = with(&["AAAA01", "GZ2E01"], Some(&["AAAA01", "GZ2E01"]));
        assert_eq!(config.target_game(), Some(Game::TwilightPrincess));
        assert!(config.check_target("RZDP01").is_ok());
        assert!(config.check_target("BBBB01").is_ok());
    }
}
