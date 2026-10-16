//! Reads whole files on a thread of its own, so a big one doesn't hold up a
//! frame. Each ask is read once, in the order asked.

use std::{
    fs, io,
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use gui::host::Wake;

/// A file's bytes, or why they couldn't be read.
pub type Loaded = (PathBuf, io::Result<Vec<u8>>);

pub struct Loader {
    // Dropping it ends the thread once a read in flight is done.
    asks: Sender<PathBuf>,
    loads: Receiver<Loaded>,
}

impl Loader {
    /// Reads each file asked for, and wakes the window with it.
    ///
    /// # Errors
    ///
    /// When the thread can't start.
    pub fn new(wake: Wake) -> io::Result<Self> {
        let (asks, asked) = mpsc::channel();
        let (done, loads) = mpsc::channel();
        thread::Builder::new()
            .name("load".to_owned())
            .spawn(move || load(&asked, &done, &wake))?;
        Ok(Self { asks, loads })
    }

    /// Reads `path`, after any read in flight.
    pub fn ask(&self, path: PathBuf) {
        // Only fails once the thread is gone, which already said why.
        let _ = self.asks.send(path);
    }

    /// Every read since the last call.
    pub fn take(&self) -> impl Iterator<Item = Loaded> + '_ {
        self.loads.try_iter()
    }
}

/// Until the loader is dropped.
fn load(asked: &Receiver<PathBuf>, done: &Sender<Loaded>, wake: &Wake) {
    for path in asked {
        let bytes = fs::read(&path);
        if done.send((path, bytes)).is_err() {
            return;
        }
        wake.wake();
    }
}
