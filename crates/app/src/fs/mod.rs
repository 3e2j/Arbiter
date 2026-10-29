//! Everything the app reads from or writes to disk. [`FileLoader`] reads
//! whole files, [`TreeReader`] a project's trees, and [`Watch`] what changes
//! under it, each on a thread of its own that wakes the window once it has
//! something to take. [`settings`] reads and writes the settings file.

mod file_loader;
pub mod settings;
mod tree_reader;
mod watch;

pub use file_loader::{FileLoader, Loaded};
pub use tree_reader::TreeReader;
pub use watch::Watch;
