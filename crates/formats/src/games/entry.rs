//! One named value, the row every table is made of, and the versions it
//! holds for.

/// One named value in a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// Wide enough for every 1- and 2-byte value a table names.
    pub value: u16,
    pub name: &'static str,
    pub notes: Option<&'static str>,
    /// The versions this row holds for. A value whose meaning differs by
    /// version has one row per meaning, with disjoint sets.
    pub versions: Versions,
}

pub(crate) const fn entry(value: u16, name: &'static str) -> Entry {
    Entry {
        value,
        name,
        notes: None,
        versions: Versions::ANY,
    }
}

impl Entry {
    pub(crate) const fn notes(self, notes: &'static str) -> Self {
        Self {
            notes: Some(notes),
            ..self
        }
    }

    pub(crate) const fn only(self, versions: Versions) -> Self {
        Self { versions, ..self }
    }
}

/// A set of one game's versions. Each game numbers its own bits, see
/// [`crate::games::tp::Version`].
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

/// Fails when two rows share a key on one version. `key` gives a row's key
/// and the versions it holds for.
#[cfg(test)]
pub(crate) fn assert_one_meaning<R, K: PartialEq + std::fmt::Debug>(
    rows: &[R],
    key: impl Fn(&R) -> (K, Versions),
) {
    let mut rest = rows;
    while let [row, tail @ ..] = rest {
        let (value, versions) = key(row);
        for other in tail {
            let (other_value, other_versions) = key(other);
            assert!(
                value != other_value || versions.is_disjoint(other_versions),
                "{value:?} has two meanings on one version",
            );
        }
        rest = tail;
    }
}
