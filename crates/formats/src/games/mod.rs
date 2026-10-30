//! Known disc editions, and each game's tables over the formats' raw values.

pub mod edition;
pub mod tp;

mod entry;
mod versions;

pub use entry::Entry;
pub use versions::Versions;
