//! What changes on disk under the open project, a batch at a time.
//!
//! notify hands each event to a thread of ours, which holds them until the
//! disk is quiet for [`QUIET`], so a save written to a temp file and renamed
//! over the real one, or a tool writing many files, is one batch. A stream
//! that never goes quiet, such as a big copy, still sends one every
//! [`LONGEST`]. Paths the owner doesn't care about are dropped there, so a
//! batch of only those never wakes the window.
//!
//! The thread sets up the watch too, since that walks every folder. Its first
//! batch is a rescan, once watching or failing to, so a read started for it
//! misses nothing.

use std::{
    io,
    path::{Component, Path, PathBuf},
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
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Watch {
    /// The thread's own channel, to stop it with.
    stop: Sender<Got>,
    batches: Receiver<Batch>,
}

/// What the thread waits on.
enum Got {
    Event(notify::Result<Event>),
    Stop,
}

#[derive(Default)]
pub struct Batch {
    /// Relative to the root, with `/` between components.
    pub paths: Vec<String>,
    /// Events were lost, so anything may have changed.
    pub rescan: bool,
}

impl Watch {
    /// Watches everything under `root`, and wakes the window with each batch
    /// of the paths that `matters`, relative to `root` with `/` between
    /// components.
    ///
    /// # Errors
    ///
    /// When the thread can't start.
    pub fn new(
        root: PathBuf,
        matters: impl Fn(&str) -> bool + Send + 'static,
        wake: Wake,
    ) -> io::Result<Self> {
        let (stop, got) = mpsc::channel();
        let (out, batches) = mpsc::channel();
        let events = stop.clone();
        thread::Builder::new()
            .name("watch".to_owned())
            .spawn(move || run(&root, &matters, events, &got, &out, &wake))?;
        Ok(Self { stop, batches })
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

impl Drop for Watch {
    fn drop(&mut self) {
        // Fails only when the thread already ended.
        let _ = self.stop.send(Got::Stop);
    }
}

/// Until the watch is dropped.
fn run(
    root: &Path,
    matters: &dyn Fn(&str) -> bool,
    events: Sender<Got>,
    got: &Receiver<Got>,
    out: &Sender<Batch>,
    wake: &Wake,
) {
    let watching = start(root, events);
    let first = Batch {
        paths: Vec::new(),
        rescan: true,
    };
    if out.send(first).is_err() {
        return;
    }
    wake.wake();
    match watching {
        // Held until the thread ends.
        Ok((root, _watcher)) => batch(&root, matters, got, out, wake),
        Err(err) => tracing::warn!("can't watch the project, so its files won't refresh: {err}"),
    }
}

/// Watches `root`, giving back its resolved path with the watcher.
fn start(root: &Path, events: Sender<Got>) -> Result<(PathBuf, RecommendedWatcher), Error> {
    // notify reports resolved paths on some systems.
    let root = root.canonicalize()?;
    let mut watcher = notify::recommended_watcher(move |event| {
        // Fails only once the thread has ended.
        let _ = events.send(Got::Event(event));
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    Ok((root, watcher))
}

fn batch(
    root: &Path,
    matters: &dyn Fn(&str) -> bool,
    got: &Receiver<Got>,
    out: &Sender<Batch>,
    wake: &Wake,
) {
    while let Ok(Got::Event(event)) = got.recv() {
        let due = Instant::now() + LONGEST;
        let mut batch = Batch::default();
        batch.add(root, matters, event);
        loop {
            let wait = QUIET.min(due.saturating_duration_since(Instant::now()));
            match got.recv_timeout(wait) {
                Ok(Got::Event(event)) => batch.add(root, matters, event),
                Ok(Got::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => break,
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
    fn add(&mut self, root: &Path, matters: &dyn Fn(&str) -> bool, event: notify::Result<Event>) {
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
            if !path.is_empty() && matters(&path) {
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
    fn a_save_is_one_batch_without_what_doesnt_matter() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join(".arbiter/logs")).unwrap();
        fs::create_dir_all(root.join(".git/objects/ab")).unwrap();
        fs::create_dir_all(root.join("code")).unwrap();
        let (send, woken) = mpsc::channel();
        let wake = Wake::new(move || send.send(()).unwrap());
        let matters = |path: &str| !path.starts_with('.');
        let watch = Watch::new(root.to_owned(), matters, wake).unwrap();
        woken.recv_timeout(Duration::from_secs(5)).unwrap();
        let first = watch.take().unwrap();
        assert!(first.rescan && first.paths.is_empty());

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
