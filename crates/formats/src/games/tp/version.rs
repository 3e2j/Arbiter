//! Which build of Twilight Princess a disc holds.
//!
//! The decomp branches on `REGION_*`, `PLATFORM_*` and `VERSION`. Region and
//! platform are both sets of versions, so every split here is a [`Versions`].

use crate::games::Versions;

/// One build of the game, the decomp's `VERSION_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Version {
    // Discriminants are the decomp's, and each is the version's bit in
    // `Versions`. No disc is known for `VERSION_WII_PAL_KIOSK`, and the
    // Shield builds aren't on disc at all.
    GcnUsa = 0,
    GcnPal = 1,
    GcnJpn = 2,
    WiiUsaR0 = 3,
    WiiUsaR2 = 4,
    WiiPal = 5,
    WiiJpn = 6,
    WiiKor = 7,
    /// The kiosk demo, `DZDE01`.
    WiiUsaKiosk = 8,
}

pub const ALL: Versions = of(&Version::ALL);

pub const GCN: Versions = of(&[Version::GcnUsa, Version::GcnPal, Version::GcnJpn]);
pub const WII: Versions = ALL.without(GCN);

pub const USA: Versions = of(&[
    Version::GcnUsa,
    Version::WiiUsaR0,
    Version::WiiUsaR2,
    Version::WiiUsaKiosk,
]);
pub const PAL: Versions = of(&[Version::GcnPal, Version::WiiPal]);
pub const JPN: Versions = of(&[Version::GcnJpn, Version::WiiJpn]);
pub const KOR: Versions = of(&[Version::WiiKor]);

/// `VERSION == VERSION_GCN_PAL`, which a few message tags test alone.
pub const GCN_PAL: Versions = of(&[Version::GcnPal]);

impl Version {
    pub const ALL: [Self; 9] = [
        Self::GcnUsa,
        Self::GcnPal,
        Self::GcnJpn,
        Self::WiiUsaR0,
        Self::WiiUsaR2,
        Self::WiiPal,
        Self::WiiJpn,
        Self::WiiKor,
        Self::WiiUsaKiosk,
    ];

    /// The version a disc's id and revision name, or `None` when they name
    /// none, like `RZDE01` revision 1. Only `RZDE01` splits by revision.
    #[must_use]
    pub fn from_disc(id: &str, revision: u8) -> Option<Self> {
        match (id, revision) {
            ("GZ2E01", _) => Some(Self::GcnUsa),
            ("GZ2P01", _) => Some(Self::GcnPal),
            ("GZ2J01", _) => Some(Self::GcnJpn),
            ("RZDE01", 0) => Some(Self::WiiUsaR0),
            ("RZDE01", 2) => Some(Self::WiiUsaR2),
            ("RZDP01", _) => Some(Self::WiiPal),
            ("RZDJ01", _) => Some(Self::WiiJpn),
            ("RZDK01", _) => Some(Self::WiiKor),
            ("DZDE01", _) => Some(Self::WiiUsaKiosk),
            _ => None,
        }
    }

    /// This version alone, as a set.
    #[must_use]
    pub const fn versions(self) -> Versions {
        Versions::bit(self as u8)
    }
}

const fn of(versions: &[Version]) -> Versions {
    let mut set = Versions::NONE;
    let mut at = 0;
    while at < versions.len() {
        set = set.union(versions[at].versions());
        at += 1;
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::edition::{EDITIONS, Game};

    #[test]
    fn only_rzde_reads_the_revision() {
        assert_eq!(Version::from_disc("GZ2E01", 1), Some(Version::GcnUsa));
        assert_eq!(Version::from_disc("RZDE01", 2), Some(Version::WiiUsaR2));
        assert_eq!(Version::from_disc("RZDE01", 1), None);
    }

    /// Every known edition of the game has tables to read it with, at its
    /// first revision and every catalogued one.
    #[test]
    fn every_known_edition_has_a_version() {
        for edition in EDITIONS.iter().filter(|e| e.game == Game::TwilightPrincess) {
            let id = std::str::from_utf8(&edition.id).unwrap();
            let revisions = edition.retail.iter().map(|&(revision, _)| revision);
            for revision in revisions.chain([0]) {
                assert!(
                    Version::from_disc(id, revision).is_some(),
                    "{id} rev {revision}"
                );
            }
        }
    }

    #[test]
    fn the_regions_split_every_version_once() {
        for version in Version::ALL {
            let regions = [USA, PAL, JPN, KOR];
            let holding = regions
                .iter()
                .filter(|region| region.intersects(version.versions()));
            assert_eq!(holding.count(), 1, "{version:?}");
        }
    }

    #[test]
    fn a_platform_is_the_other_ones_complement() {
        assert!(GCN.is_disjoint(WII));
        assert_eq!(GCN.union(WII), ALL);
        assert!(WII.intersects(Version::WiiUsaKiosk.versions()));
    }
}
