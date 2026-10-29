//! Reads whole files on a thread of its own, so a big one doesn't hold up a
//! frame. Each ask is read once, in the order asked, and its bytes go back
//! to whoever holds what [`FileLoader::ask`] gave.

use std::{
    io,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use gui::host::Wake;

/// A file's bytes, or why they couldn't be read.
pub type Loaded = io::Result<Vec<u8>>;

pub struct FileLoader {
    // Dropping it ends the thread once a read in flight is done.
    asks: Sender<(PathBuf, Sender<Loaded>)>,
}

impl FileLoader {
    /// Reads each file asked for, and wakes the window with it.
    ///
    /// # Errors
    ///
    /// When the thread can't start.
    pub fn new(wake: Wake) -> io::Result<Self> {
        let (asks, asked) = mpsc::channel();
        thread::Builder::new()
            .name("load".to_owned())
            .spawn(move || load(&asked, &wake))?;
        Ok(Self { asks })
    }

    /// Reads `path`, after any read in flight. Its bytes come out of what
    /// this gives, unless that's dropped first.
    pub fn ask(&self, path: PathBuf) -> Receiver<Loaded> {
        let (done, loaded) = mpsc::channel();
        // Only fails once the thread is gone, which the receiver then says.
        let _ = self.asks.send((path, done));
        loaded
    }
}

/// Until the loader is dropped.
fn load(asked: &Receiver<(PathBuf, Sender<Loaded>)>, wake: &Wake) {
    for (path, done) in asked {
        // Fails when whoever asked stopped waiting, such as a closed tab.
        if done.send(std::fs::read(&path)).is_ok() {
            wake.wake();
        }
    }
}
