//! The games Arbiter knows: their editions, versions, and the [`Entry`] rows
//! every library's `games` tables are made of. Never refuses a game it has no
//! tables for.
//!
//! Lookups return `Option`, since mods can change the tables, and no game
//! falls back to another's. Names come from the decomp, untested in game.

pub mod edition;
pub mod tp;

pub(crate) mod entry;

pub use entry::{Entry, Versions};
