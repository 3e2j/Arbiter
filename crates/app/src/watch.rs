//! What changes on disk under the open project, a batch at a time.
//!
//! notify hands each event to a thread of ours, which holds them until the
//! disk is quiet for [`QUIET`], so a save written to a temp file and renamed
//! over the real one, or a tool writing many files, is one batch. A stream
//! that never goes quiet, such as a big copy, still sends one every
//! [`LONGEST`]. Noise is dropped there, see [`project::is_noise`], and what's
//! left is for each reader to judge.

use std::{
    io,
    path::{Component, Path},
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use gui::host::Wake;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

const QUIET: Duration = Duration::from_millis(150);
const LONGEST: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Notify(#[from] notify::Error),
    #[error("couldn't start the watch thread: {0}")]
    Thread(#[from] io::Error),
}

pub struct Watch {
    // Dropping it ends the batching thread too.
    _watcher: RecommendedWatcher,
    batches: Receiver<Batch>,
}

#[derive(Default)]
pub struct Batch {
    /// Relative to the root, with `/` between components.
    pub paths: Vec<String>,
    /// Events were lost, so anything may have changed.
    pub rescan: bool,
}

impl Watch {
    /// Watches everything under `root`, and wakes the window with each batch.
    ///
    /// # Errors
    ///
    /// When `root` can't be watched, or the thread can't start.
    pub fn new(root: &Path, wake: Wake) -> Result<Self, Error> {
        // notify reports resolved paths on some systems.
        let root = root.canonicalize()?;
        let (events, raw) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(events)?;
        watcher.watch(&root, RecursiveMode::Recursive)?;
        let (out, batches) = mpsc::channel();
        thread::Builder::new()
            .name("watch".to_owned())
            .spawn(move || batch(&root, &raw, &out, &wake))?;
        Ok(Self {
            _watcher: watcher,
            batches,
        })
    }

    /// Every batch since the last call, as one.
    pub fn take(&self) -> Option<Batch> {
        let mut batches = self.batches.try_iter();
        let mut all = batches.next()?;
        for batch in batches {
            all.paths.extend(batch.paths);
            all.rescan |= batch.rescan;
        }
        Some(all)
    }
}

/// Until the watcher is dropped.
fn batch(root: &Path, raw: &Receiver<notify::Result<Event>>, out: &Sender<Batch>, wake: &Wake) {
    while let Ok(event) = raw.recv() {
        let due = Instant::now() + LONGEST;
        let mut batch = Batch::default();
        batch.add(root, event);
        loop {
            let wait = QUIET.min(due.saturating_duration_since(Instant::now()));
            match raw.recv_timeout(wait) {
                Ok(event) => batch.add(root, event),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if batch.paths.is_empty() && !batch.rescan {
            continue;
        }
        batch.paths.sort_unstable();
        batch.paths.dedup();
        if out.send(batch).is_err() {
            return;
        }
        wake.wake();
    }
}

impl Batch {
    fn add(&mut self, root: &Path, event: notify::Result<Event>) {
        let event = match event {
            Ok(event) => event,
            Err(err) => {
                tracing::warn!("watch: {err}");
                self.rescan = true;
                return;
            }
        };
        self.rescan |= event.need_rescan();
        // Opening, reading and closing change nothing.
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in &event.paths {
            let Some(path) = relative(root, path) else {
                continue;
            };
            if !path.is_empty() && !project::is_noise(&path) {
                self.paths.push(path);
            }
        }
    }
}

/// `path` under `root`, with `/` between components whatever the system uses.
fn relative(root: &Path, path: &Path) -> Option<String> {
    let mut rel = String::new();
    for part in path.strip_prefix(root).ok()?.components() {
        let Component::Normal(part) = part else {
            return None;
        };
        if !rel.is_empty() {
            rel.push('/');
        }
        rel.push_str(&part.to_string_lossy());
    }
    Some(rel)
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::mpsc};

    use super::*;

    #[test]
    fn a_save_is_one_batch_without_the_noise_beside_it() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".arbiter/logs")).unwrap();
        fs::create_dir_all(root.join(".git/objects/ab")).unwrap();
        fs::create_dir_all(root.join("code")).unwrap();
        let (send, woken) = mpsc::channel();
        let wake = Wake::new(move || send.send(()).unwrap());
        let watch = Watch::new(root, wake).unwrap();

        fs::write(root.join(".arbiter/logs/arbiter.log"), "line").unwrap();
        fs::write(root.join(".git/objects/ab/cdef"), "blob").unwrap();
        // As an editor saves: to a temp file, renamed over the real one.
        fs::write(root.join("code/mod.cpp.tmp"), "int x;").unwrap();
        fs::rename(root.join("code/mod.cpp.tmp"), root.join("code/mod.cpp")).unwrap();

        woken.recv_timeout(Duration::from_secs(5)).unwrap();
        let batch = watch.take().unwrap();
        assert!(!batch.rescan);
        assert_eq!(batch.paths, ["code/mod.cpp", "code/mod.cpp.tmp"]);
        assert!(watch.take().is_none());
    }
}
