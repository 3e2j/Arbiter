//! Discs and what comes off them.
//!
//! [`disc`] reads a disc's files from any container, and
//! [`unpack`] takes each apart into the files a base stores.
//!
//! Building, turning a project back into a target's files, belongs here too
//! but doesn't exist yet.

pub mod disc;
pub mod unpack;
