//! The open project's log, `.arbiter/logs/arbiter.log`, and the few before it.
//!
//! Lines collect in memory and a thread writes them out together every [`INTERVAL`],
//! or sooner for a warning, an error, or a lot at once. What's logged before
//! a project opens waits until one does.

use std::{
    fs::{self, File},
    io::{self, Write},
    mem,
    path::Path,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

/// Under the project root.
const DIR: &str = ".arbiter/logs";
/// The current log is `<NAME>.log`, and older ones `<NAME>.<age>.log`.
const NAME: &str = "arbiter";
/// How many old logs are kept beside the current one.
const KEEP: u32 = 4;
/// How many lines wait for a project before the oldest half are dropped.
const WAITING: usize = 10_000;
/// The longest a line waits to be written. A hard crash loses at most this
/// much, less any warnings and errors.
const INTERVAL: Duration = Duration::from_millis(100);
/// How many bytes waiting wake the writer early.
const SOON: usize = 64 * 1024;

/// Written to through a [`LogWriter`], one line per write. Dropping it writes
/// out what's left.
pub struct Log(Arc<Shared>);

/// One line on its way to a [`Log`].
pub struct LogWriter<'a> {
    log: &'a Log,
    /// Wakes the writer instead of waiting for the interval.
    urgent: bool,
}

struct Shared {
    lines: Mutex<Lines>,
    /// Held while writing, so lines land in the order they came in.
    file: Mutex<Option<File>>,
    wake: Condvar,
}

#[derive(Default)]
struct Lines {
    /// Not written yet, back to back.
    pending: Vec<u8>,
    /// Where each line in `pending` ends, until a project opens.
    ends: Option<Vec<usize>>,
    /// Whether the writer was woken and should write without waiting.
    due: bool,
    /// Whether a writer thread runs. Without one, each line is written as it
    /// comes in.
    threaded: bool,
    closed: bool,
}

impl Log {
    /// Starts the writer thread.
    #[must_use]
    pub fn start() -> Self {
        let shared = Arc::new(Shared {
            lines: Mutex::new(Lines {
                ends: Some(Vec::new()),
                ..Lines::default()
            }),
            file: Mutex::default(),
            wake: Condvar::new(),
        });
        let writer = Arc::clone(&shared);
        let spawned = thread::Builder::new()
            .name("log".into())
            .spawn(move || writer.write_behind());
        shared.lines().threaded = spawned.is_ok();
        Self(shared)
    }

    /// For one line. `urgent` writes it out without waiting for the interval.
    #[must_use]
    pub const fn writer(&self, urgent: bool) -> LogWriter<'_> {
        LogWriter { log: self, urgent }
    }

    /// Starts a new log in the project at `root`, moving its old ones back by
    /// one and dropping the oldest. The log before keeps every line up to
    /// now, and the first project also gets what waited for it.
    ///
    /// # Errors
    ///
    /// When a log can't be written, moved or created. Lines wait for the next
    /// project.
    pub fn attach(&self, root: &Path) -> io::Result<()> {
        let mut file = self.0.file();
        if let Some(old) = &mut *file {
            self.0.write_out(old, &mut Vec::new())?;
        }
        *file = None;
        self.0.lines().ends = Some(Vec::new());
        let dir = root.join(DIR);
        fs::create_dir_all(&dir)?;
        let path = |age| dir.join(file_name(age));
        for age in (0..KEEP).rev() {
            match fs::rename(path(age), path(age + 1)) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                moved => moved?,
            }
        }
        let mut new = File::create(path(0))?;
        let mut lines = self.0.lines();
        new.write_all(&lines.pending)?;
        lines.pending.clear();
        lines.ends = None;
        drop(lines);
        *file = Some(new);
        Ok(())
    }

    /// Writes out every line so far, on this thread.
    ///
    /// # Errors
    ///
    /// When the file can't be written to.
    pub fn flush(&self) -> io::Result<()> {
        self.0.flush(&mut Vec::new())
    }
}

/// The log `age` runs back, the current one at 0.
fn file_name(age: u32) -> String {
    match age {
        0 => format!("{NAME}.log"),
        age => format!("{NAME}.{age}.log"),
    }
}

impl Drop for Log {
    fn drop(&mut self) {
        self.0.lines().closed = true;
        self.0.wake.notify_one();
        let _ = self.flush();
    }
}

impl Shared {
    /// A panic while holding either lock leaves nothing half done, so a
    /// poisoned one is still fine to use.
    fn lines(&self) -> MutexGuard<'_, Lines> {
        self.lines.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn file(&self) -> MutexGuard<'_, Option<File>> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The writer thread: writes what's pending every [`INTERVAL`], or when
    /// woken, until the log is dropped. It reports no errors, since logging
    /// one would only feed it another line.
    fn write_behind(&self) {
        let mut spare = Vec::new();
        loop {
            {
                let mut lines = self.lines();
                if !lines.due && !lines.closed {
                    lines = self
                        .wake
                        .wait_timeout(lines, INTERVAL)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
                if lines.closed {
                    return;
                }
                lines.due = false;
            }
            let _ = self.flush(&mut spare);
        }
    }

    /// Writes what's pending, if there's a file and anything to write.
    /// `spare` is swapped in for it, so neither allocates again.
    fn flush(&self, spare: &mut Vec<u8>) -> io::Result<()> {
        match &mut *self.file() {
            Some(file) => self.write_out(file, spare),
            None => Ok(()),
        }
    }

    /// Lets lines keep coming in while it writes.
    fn write_out(&self, file: &mut File, spare: &mut Vec<u8>) -> io::Result<()> {
        {
            let mut lines = self.lines();
            if lines.pending.is_empty() {
                return Ok(());
            }
            mem::swap(&mut lines.pending, spare);
        }
        let written = file.write_all(spare);
        spare.clear();
        written
    }
}

impl Write for LogWriter<'_> {
    fn write(&mut self, line: &[u8]) -> io::Result<usize> {
        let shared = &self.log.0;
        let mut guard = shared.lines();
        let lines = &mut *guard;
        lines.pending.extend_from_slice(line);
        if let Some(ends) = &mut lines.ends {
            ends.push(lines.pending.len());
            if ends.len() > WAITING {
                let cut = ends.drain(..ends.len() / 2).next_back().unwrap_or(0);
                lines.pending.drain(..cut);
                for end in ends {
                    *end -= cut;
                }
            }
        } else if !lines.threaded {
            drop(guard);
            shared.flush(&mut Vec::new())?;
        } else if self.urgent || lines.pending.len() >= SOON {
            lines.due = true;
            shared.wake.notify_one();
        }
        Ok(line.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.log.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(root: &Path, age: u32) -> String {
        fs::read_to_string(root.join(DIR).join(file_name(age))).unwrap_or_default()
    }

    fn line(log: &Log, text: &str) {
        log.writer(false).write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn what_waited_is_written_on_attach() {
        let dir = tempfile::tempdir().unwrap();
        let log = Log::start();
        line(&log, "before\n");
        log.attach(dir.path()).unwrap();
        line(&log, "after\n");
        log.flush().unwrap();
        assert_eq!(read(dir.path(), 0), "before\nafter\n");
    }

    #[test]
    fn dropping_writes_out_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let log = Log::start();
        log.attach(dir.path()).unwrap();
        line(&log, "last\n");
        drop(log);
        assert_eq!(read(dir.path(), 0), "last\n");
    }

    #[test]
    fn keeps_the_current_and_four_old() {
        let dir = tempfile::tempdir().unwrap();
        for run in 0..7 {
            let log = Log::start();
            log.attach(dir.path()).unwrap();
            line(&log, &format!("{run}\n"));
        }
        assert_eq!(read(dir.path(), 0), "6\n");
        assert_eq!(read(dir.path(), KEEP), "2\n");
        assert_eq!(fs::read_dir(dir.path().join(DIR)).unwrap().count(), 5);
    }

    #[test]
    fn another_project_keeps_the_tail_of_the_one_before() {
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let log = Log::start();
        line(&log, "start\n");
        log.attach(first.path()).unwrap();
        line(&log, "first\n");
        log.attach(second.path()).unwrap();
        line(&log, "second\n");
        log.flush().unwrap();
        assert_eq!(read(first.path(), 0), "start\nfirst\n");
        assert_eq!(read(second.path(), 0), "second\n");
    }

    #[test]
    fn too_much_waiting_drops_the_oldest_half() {
        let log = Log::start();
        for n in 0..=WAITING {
            line(&log, &format!("{n}\n"));
        }
        let lines = log.0.lines();
        assert_eq!(lines.ends.as_ref().map(Vec::len), Some(WAITING / 2 + 1));
        assert!(
            lines
                .pending
                .starts_with(format!("{}\n", WAITING / 2).as_bytes())
        );
    }
}
