//! Reads a project's trees on a thread of its own, so a big project doesn't
//! hold up a frame. Asks that come in during a read are one read after it.

use std::{
    io,
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

use gui::host::Wake;
use project::{
    Project,
    tree::{self, Trees},
};

pub struct TreeReader {
    // Dropping it ends the thread once a read in flight is done.
    asks: Sender<()>,
    reads: Receiver<Result<Trees, tree::TreeError>>,
}

impl TreeReader {
    /// Reads `project`'s trees when asked, and wakes the window with each.
    ///
    /// # Errors
    ///
    /// When the thread can't start.
    pub fn new(project: Arc<Project>, wake: Wake) -> io::Result<Self> {
        let (asks, asked) = mpsc::channel();
        let (done, reads) = mpsc::channel();
        thread::Builder::new()
            .name("trees".to_owned())
            .spawn(move || read(&project, &asked, &done, &wake))?;
        Ok(Self { asks, reads })
    }

    /// Reads again, after any read in flight.
    pub fn ask(&self) {
        // Only fails once the thread is gone, which already said why.
        let _ = self.asks.send(());
    }

    /// The latest read since the last call.
    pub fn take(&self) -> Option<Result<Trees, tree::TreeError>> {
        self.reads.try_iter().last()
    }
}

/// Until the reader is dropped.
fn read(
    project: &Project,
    asked: &Receiver<()>,
    done: &Sender<Result<Trees, tree::TreeError>>,
    wake: &Wake,
) {
    while asked.recv().is_ok() {
        for () in asked.try_iter() {}
        if done.send(project.trees()).is_err() {
            return;
        }
        wake.wake();
    }
}
