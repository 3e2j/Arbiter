//! Known editions by disc id.
//!
//! Which game each is, and the hash of a clean dump per revision, so an unpack
//! can tell one from a disc that was scrubbed, trimmed or modified.
//!
//! An unknown id has no edition and falls back to its header title. A hash
//! mismatch is for the caller to warn about, never a reason to refuse.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Game {
    TwilightPrincess,
}

#[derive(Debug)]
pub struct Edition {
    pub id: [u8; 6],
    pub game: Game,
    /// Revision and XXH3-128 of a clean dump's logical disc stream, see
    /// `pack::disc::hash`. Empty if none are catalogued.
    pub retail: &'static [(u8, u128)],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Retail,
    /// A catalogued revision with another hash.
    Mismatch {
        expected: u128,
    },
    /// No hash for this revision, or no edition at all.
    Uncatalogued,
}

/// Sorted by id, for `edition`'s binary search.
#[rustfmt::skip]
pub static EDITIONS: &[Edition] = &[
    // Twilight Princess.
    // Hashes from Dusklight's `src/dusk/iso_validate.cpp` so both tools agree.
    entry(*b"GZ2E01", Game::TwilightPrincess, &[(0, 0x14e8_86f0_8e54_8a00_0afd_e98a_3195_e788)]),
    entry(*b"GZ2J01", Game::TwilightPrincess, &[(0, 0x5967_dc7a_6a55_3652_f4d2_050a_eef6_f368)]),
    entry(*b"GZ2P01", Game::TwilightPrincess, &[(0, 0x9ef5_9758_8b00_35ca_9e91_b333_fa9a_8a7e)]),
    entry(*b"RZDE01", Game::TwilightPrincess, &[(0, 0xb3d9_1fbe_a59e_5c66_934d_04c0_1566_728e),
                                                (2, 0xc3ec_4209_21a1_b36d_6ae4_3f57_6491_d25c)]),
    entry(*b"RZDJ01", Game::TwilightPrincess, &[(0, 0xd386_6821_c7fc_6999_e6e8_bbef_8b68_75aa)]),
    entry(*b"RZDK01", Game::TwilightPrincess, &[]),
    entry(*b"RZDP01", Game::TwilightPrincess, &[(0, 0x6095_a924_a57e_5fb4_294a_c96f_b85a_09a1)]),
];

const fn entry(id: [u8; 6], game: Game, retail: &'static [(u8, u128)]) -> Edition {
    Edition { id, game, retail }
}

/// The edition with this disc id, if it's known.
#[must_use]
pub fn edition(id: &str) -> Option<&'static Edition> {
    let id = <[u8; 6]>::try_from(id.as_bytes()).ok()?;
    let i = EDITIONS.binary_search_by_key(&id, |e| e.id).ok()?;
    EDITIONS.get(i)
}

/// Game codes, the id's first three characters, sorted for `guess`.
/// GameCube and Wii releases of one game have different codes.
pub static GAME_CODES: &[([u8; 3], Game)] = &[
    (*b"GZ2", Game::TwilightPrincess),
    (*b"RZD", Game::TwilightPrincess),
];

/// The game an unknown id probably is, from its game code. Only a suggestion:
/// nothing stops a code from being reused.
#[must_use]
pub fn guess(id: &str) -> Option<Game> {
    let code = <[u8; 3]>::try_from(id.as_bytes().get(..3)?).ok()?;
    let i = GAME_CODES.binary_search_by_key(&code, |&(c, _)| c).ok()?;
    GAME_CODES.get(i).map(|&(_, game)| game)
}

impl Edition {
    /// How a disc's hash compares to the clean dump of its revision.
    #[must_use]
    pub fn check(&self, revision: u8, hash: u128) -> Verdict {
        match self.retail.iter().find(|&&(r, _)| r == revision) {
            Some(&(_, expected)) if expected == hash => Verdict::Retail,
            Some(&(_, expected)) => Verdict::Mismatch { expected },
            None => Verdict::Uncatalogued,
        }
    }
}

impl Game {
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::TwilightPrincess => "The Legend of Zelda: Twilight Princess",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_for_binary_search() {
        assert!(EDITIONS.windows(2).all(|w| w[0].id < w[1].id));
        assert!(GAME_CODES.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn finds_editions_by_id() {
        assert_eq!(edition("RZDE01").unwrap().game, Game::TwilightPrincess);
        assert_eq!(edition("RZDK01").unwrap().game, Game::TwilightPrincess);
        assert!(edition("GALE01").is_none());
        assert!(edition("GZ2E0").is_none());
    }

    #[test]
    fn every_edition_has_its_game_code() {
        for e in EDITIONS {
            let id = std::str::from_utf8(&e.id).unwrap();
            assert_eq!(guess(id), Some(e.game), "{id}");
        }
        assert_eq!(guess("GZ2X99"), Some(Game::TwilightPrincess));
        assert_eq!(guess("GZ"), None);
    }

    #[test]
    fn checks_revision_and_hash() {
        let na = edition("GZ2E01").unwrap();
        let hash = 0x14e8_86f0_8e54_8a00_0afd_e98a_3195_e788;
        assert_eq!(na.check(0, hash), Verdict::Retail);
        assert_eq!(na.check(0, 1), Verdict::Mismatch { expected: hash });
        assert_eq!(na.check(1, hash), Verdict::Uncatalogued);
        assert_eq!(
            edition("RZDK01").unwrap().check(0, hash),
            Verdict::Uncatalogued
        );
    }
}
