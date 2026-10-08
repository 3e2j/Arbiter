//! How a disc file goes back onto the disc, apart from its bytes. An
//! archive's is in its sidecar instead, see `sidecar`.

use diag::{Address, Diagnostics, Key};
use serde::{Deserialize, Serialize};

use super::Compression;

/// Starts as the base's `Entry::compression`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Packing {
    pub compression: Option<Compression>,
}

/// Also what a mod stores. An omitted compression is none.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum Edit {
    Compression {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        compression: Option<Compression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Compression,
}

impl Address for Item {
    fn key(&self) -> Vec<Key> {
        match self {
            Self::Compression => vec![("field", "compression".to_owned())],
        }
    }
}

impl formats::Edit for Packing {
    type Edit = Edit;
    type Item = Item;

    fn apply(&mut self, edit: Edit) -> (Edit, Vec<Item>) {
        match edit {
            Edit::Compression { compression } => {
                let old = std::mem::replace(&mut self.compression, compression);
                (
                    Edit::Compression { compression: old },
                    vec![Item::Compression],
                )
            }
        }
    }
}

impl formats::Patch for Packing {
    type Change = Edit;

    fn diff(base: &Self, edited: &Self) -> Vec<Edit> {
        if base.compression == edited.compression {
            return Vec::new();
        }
        vec![Edit::Compression {
            compression: edited.compression,
        }]
    }

    /// Every change fits any base, so nothing is reported.
    fn patch(&mut self, changes: &[Edit], _: &mut Diagnostics) {
        for &change in changes {
            formats::Edit::apply(self, change);
        }
    }
}

#[cfg(test)]
mod tests {
    use formats::{Edit as _, Patch as _};
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
    struct Stored {
        change: Vec<Edit>,
    }

    #[test]
    fn applying_the_inverse_undoes() {
        let base = Packing {
            compression: Some(Compression::Yaz0),
        };
        let mut packing = base;
        let (inverse, touched) = packing.apply(Edit::Compression { compression: None });
        assert_eq!(packing.compression, None);
        assert_eq!(touched, [Item::Compression]);
        packing.apply(inverse);
        assert_eq!(packing, base);
    }

    #[test]
    fn a_diff_patches_the_base_into_the_edit() {
        let base = Packing {
            compression: Some(Compression::Yaz0),
        };
        let edited = Packing { compression: None };
        assert_eq!(Packing::diff(&base, &base), []);

        let changes = Packing::diff(&base, &edited);
        let mut patched = base;
        let mut diag = Diagnostics::default();
        patched.patch(&changes, &mut diag);
        assert_eq!(patched, edited);
        assert_eq!(diag.items, []);
    }

    #[test]
    fn changes_read_back_with_none_omitted() {
        let stored = Stored {
            change: vec![
                Edit::Compression { compression: None },
                Edit::Compression {
                    compression: Some(Compression::Yay0),
                },
            ],
        };
        let text = toml::to_string(&stored).unwrap();
        assert_eq!(
            text,
            "[[change]]\n\
             edit = \"compression\"\n\
             \n\
             [[change]]\n\
             edit = \"compression\"\n\
             compression = \"yay0\"\n"
        );
        assert_eq!(toml::from_str::<Stored>(&text).unwrap(), stored);
    }
}
