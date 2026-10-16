//! Threads that work the disk so a frame never waits on it: [`Loader`] reads
//! whole files, [`TreeReader`] a project's trees, and [`Watch`] what changes
//! under it. Each wakes the window once it has something to take.

mod loader;
mod reader;
mod watch;

pub use loader::Loader;
pub use reader::TreeReader;
pub use watch::Watch;
