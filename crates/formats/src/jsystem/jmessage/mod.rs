//! `JMessage`: the game's text, and the conversations it's strung into.
//!
//! - Messages: text with inline tags, and a fixed run of attribute bytes
//!   saying how each is shown.
//! - Flow: nodes that show a message, branch on a query, or fire an event.

pub mod bmg;
