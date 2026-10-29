//! Threads that work the disk so a frame never waits on it: [`FileLoader`] reads
//! whole files, [`TreeReader`] a project's trees, and [`Watch`] what changes
//! under it. Each wakes the window once it has something to take.

mod file_loader;
mod tree_reader;
mod watch;

pub use file_loader::FileLoader;
pub use tree_reader::TreeReader;
pub use watch::Watch;
