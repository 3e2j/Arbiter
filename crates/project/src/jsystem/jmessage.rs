//! `JMessage`'s BMG: the format's structs and the game's tables, re-exported
//! so front ends need only `project`.

pub use formats::jsystem::jmessage::*;
pub use game::Entry;
pub use game::jsystem::jmessage::*;

use crate::Project;

/// The tables for the base edition `path` is in, relative to the root.
/// `None` outside `base/`, or for an edition no game's tables read.
#[must_use]
pub fn tables(project: &Project, path: &str) -> Option<Tables> {
    let (id, edition) = project.edition_of(path)?;
    game::jsystem::jmessage::tables(id, edition.revision)
}
