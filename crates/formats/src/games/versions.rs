//! Sets of one game's versions, which every table row is keyed by.

/// A set of one game's versions. Each game numbers its own bits, see
/// [`super::tp::Version`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Versions(u16);

impl Versions {
    /// What a row holds for when it doesn't say.
    pub const ANY: Self = Self(u16::MAX);
    pub const NONE: Self = Self(0);

    pub(crate) const fn bit(at: u8) -> Self {
        Self(1 << at)
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    #[must_use]
    pub const fn is_disjoint(self, other: Self) -> bool {
        !self.intersects(other)
    }
}
