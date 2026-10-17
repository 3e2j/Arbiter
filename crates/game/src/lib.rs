//! What the game's raw values mean. Never refuses a game it has no tables for.
//!
//! [`libs`] holds the shared engine code, each game ([`tp`]) its own rows.
//! Lookups return `Option`, since mods can change the tables, and no game
//! falls back to another's. Names come from the decomp, untested in game.

pub mod edition;
pub mod libs;
pub mod tp;

mod entry;

pub use entry::{Entry, Versions};
