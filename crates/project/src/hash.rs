//! XXH3-128, written as `xxh3:<32 hex digits>` in project files.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

const PREFIX: &str = "xxh3:";

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Hash(pub u128);

impl Hash {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(xxhash_rust::xxh3::xxh3_128(bytes))
    }

    /// The bare digits, as an object's file name.
    #[must_use]
    pub fn hex(self) -> String {
        format!("{:032x}", self.0)
    }

    fn parse(s: &str) -> Option<Self> {
        let digits = s.strip_prefix(PREFIX)?;
        if digits.len() != 32 {
            return None;
        }
        u128::from_str_radix(digits, 16).ok().map(Self)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{:032x}", self.0)
    }
}

impl Serialize for Hash {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <&str>::deserialize(d)?;
        Self::parse(s).ok_or_else(|| de::Error::custom(format!("{s:?} isn't xxh3:<32 hex digits>")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_round_trips() {
        let hash = Hash::of(b"arbiter");
        assert_eq!(Hash::parse(&hash.to_string()), Some(hash));
        assert_eq!(Hash::parse(&format!("{PREFIX}{:x}", 0xabu8)), None);
        assert_eq!(Hash::parse(&hash.hex()), None);
    }
}
