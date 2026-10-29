//! Everything logged since startup, kept in one [`LOG`] for whatever shows
//! it, and written to the open project's `.arbiter/logs/arbiter.log`, with
//! the few before it kept beside.
//!
//! A thread writes new entries out together every [`INTERVAL`], or sooner for
//! a warning, an error, or a lot at once. What's logged before a project
//! opens waits until one does.

use std::{
    fmt::Write as _,
    fs::{self, File},
    io::{self, Write},
    ops::Range,
    path::Path,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    thread,
    time::Duration,
};

/// The one log, which anything can push to and read from.
pub static LOG: Log = Log::new();

/// Under the project root.
const DIR: &str = ".arbiter/logs";
/// The current log is `<NAME>.log`, and older ones `<NAME>.<age>.log`.
const NAME: &str = "arbiter";
/// How many old logs are kept beside the current one.
const KEEP: u32 = 4;
/// How many entries are kept before the oldest half are dropped.
const CAP: usize = 10_000;
/// The longest an entry waits to be written. A hard crash loses at most this
/// much, less any warnings and errors.
const INTERVAL: Duration = Duration::from_millis(100);
/// How many entries waiting wake the writer early.
const SOON: usize = 1_000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Error,
    Warning,
    Info,
    /// Debug and trace.
    Verbose,
}

/// An entry, less its text.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub level: Level,
    pub target: &'static str,
    /// Since startup.
    pub at: Duration,
    /// Whether the entry before has the same level, target and text.
    pub repeat: bool,
}

pub struct Log {
    entries: Mutex<Entries>,
    /// Held while writing, so entries land in the order they came in.
    file: Mutex<Option<File>>,
    wake: Condvar,
}

/// An entry's index counts from startup, dropped ones included.
struct Entries {
    list: Vec<(Entry, Range<usize>)>,
    /// Every entry's text, back to back.
    text: String,
    /// How many were dropped off the front.
    dropped: usize,
    /// How many the file has been given.
    written: usize,
    /// Whether the writer was woken and should write without waiting.
    due: bool,
    /// Whether a writer thread runs. Without one, each entry is written as it
    /// comes in.
    threaded: bool,
}

impl Level {
    pub const ALL: [Self; 4] = [Self::Error, Self::Warning, Self::Info, Self::Verbose];

    const fn name(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warning => "WARN",
            Self::Info => "INFO",
            Self::Verbose => "DEBUG",
        }
    }
}

impl Log {
    const fn new() -> Self {
        Self {
            entries: Mutex::new(Entries {
                list: Vec::new(),
                text: String::new(),
                dropped: 0,
                written: 0,
                due: false,
                threaded: false,
            }),
            file: Mutex::new(None),
            wake: Condvar::new(),
        }
    }

    /// Starts the writer thread.
    pub fn start(&'static self) {
        let spawned = thread::Builder::new()
            .name("log".into())
            .spawn(|| self.write_behind());
        self.entries().threaded = spawned.is_ok();
    }

    pub fn push(&self, level: Level, target: &'static str, at: Duration, text: &str) {
        let mut entries = self.entries();
        entries.push(level, target, at, text);
        if !entries.threaded {
            drop(entries);
            let _ = self.flush();
        } else if level <= Level::Warning || entries.end() - entries.written >= SOON {
            entries.due = true;
            self.wake.notify_one();
        }
    }

    /// Calls `each` with every entry kept from `from` on, by its index.
    /// Returns the indices kept. Logging from `each` deadlocks.
    pub fn read(&self, from: usize, mut each: impl FnMut(usize, &Entry)) -> Range<usize> {
        let entries = self.entries();
        let kept = entries.dropped..entries.end();
        for index in from.max(kept.start)..kept.end {
            if let Some((entry, _)) = entries.get(index) {
                each(index, entry);
            }
        }
        kept
    }

    /// Entry `index`, with its text in `text`, while it's kept.
    pub fn entry(&self, index: usize, text: &mut String) -> Option<Entry> {
        let entries = self.entries();
        let (entry, said) = entries.get(index)?;
        text.clear();
        text.push_str(said);
        Some(*entry)
    }

    /// Starts a new log file in the project at `root`, moving its old ones
    /// back by one and dropping the oldest. The file before keeps every entry
    /// up to now, and the first project also gets what waited for it.
    ///
    /// # Errors
    ///
    /// When a log can't be written, moved or created. Entries wait for the
    /// next project.
    pub fn attach(&self, root: &Path) -> io::Result<()> {
        let mut file = self.file();
        if let Some(old) = &mut *file {
            self.write_out(old, &mut String::new())?;
        }
        *file = None;
        let dir = root.join(DIR);
        fs::create_dir_all(&dir)?;
        let path = |age| dir.join(file_name(age));
        for age in (0..KEEP).rev() {
            match fs::rename(path(age), path(age + 1)) {
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                moved => moved?,
            }
        }
        let new = file.insert(File::create(path(0))?);
        self.write_out(new, &mut String::new())
    }

    /// Writes out every entry so far, on this thread.
    ///
    /// # Errors
    ///
    /// When the file can't be written to.
    pub fn flush(&self) -> io::Result<()> {
        self.flush_into(&mut String::new())
    }

    /// A panic while holding either lock leaves nothing half done, so a
    /// poisoned one is still fine to use.
    fn entries(&self) -> MutexGuard<'_, Entries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn file(&self) -> MutexGuard<'_, Option<File>> {
        self.file.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The writer thread: writes what's new every [`INTERVAL`], or when
    /// woken. It reports no errors, since logging one would only feed it
    /// another entry.
    fn write_behind(&self) {
        let mut spare = String::new();
        loop {
            {
                let mut entries = self.entries();
                if !entries.due {
                    entries = self
                        .wake
                        .wait_timeout(entries, INTERVAL)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
                entries.due = false;
            }
            let _ = self.flush_into(&mut spare);
        }
    }

    /// Writes what's new, if there's a file. `spare` is formatted into, so
    /// it doesn't allocate again.
    fn flush_into(&self, spare: &mut String) -> io::Result<()> {
        match &mut *self.file() {
            Some(file) => self.write_out(file, spare),
            None => Ok(()),
        }
    }

    /// Lets entries keep coming in while it writes.
    fn write_out(&self, file: &mut File, spare: &mut String) -> io::Result<()> {
        {
            let mut entries = self.entries();
            for index in entries.written.max(entries.dropped)..entries.end() {
                if let Some((entry, text)) = entries.get(index) {
                    let (at, level) = (entry.at.as_secs_f64(), entry.level.name());
                    let _ = writeln!(spare, "{at:>12.6}s {level:>5} {}: {text}", entry.target);
                }
            }
            entries.written = entries.end();
        }
        let written = file.write_all(spare.as_bytes());
        spare.clear();
        written
    }
}

impl Entries {
    const fn end(&self) -> usize {
        self.dropped + self.list.len()
    }

    fn get(&self, index: usize) -> Option<(&Entry, &str)> {
        let (entry, text) = self.list.get(index.checked_sub(self.dropped)?)?;
        Some((entry, self.text.get(text.clone()).unwrap_or_default()))
    }

    fn push(&mut self, level: Level, target: &'static str, at: Duration, text: &str) {
        let last = self.end().checked_sub(1).and_then(|last| self.get(last));
        let repeat = last.is_some_and(|(last, said)| {
            last.level == level && last.target == target && said == text
        });
        let start = self.text.len();
        self.text.push_str(text);
        let entry = Entry {
            level,
            target,
            at,
            repeat,
        };
        self.list.push((entry, start..self.text.len()));
        if self.list.len() > CAP {
            self.drop_oldest_half();
        }
    }

    fn drop_oldest_half(&mut self) {
        let half = self.list.len() / 2;
        self.list.drain(..half);
        self.dropped += half;
        let cut = self
            .list
            .first()
            .map_or(self.text.len(), |(_, text)| text.start);
        self.text.drain(..cut);
        for (_, text) in &mut self.list {
            *text = text.start - cut..text.end - cut;
        }
    }
}

/// The log `age` runs back, the current one at 0.
fn file_name(age: u32) -> String {
    match age {
        0 => format!("{NAME}.log"),
        age => format!("{NAME}.{age}.log"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One with no writer thread, so each entry is written as it comes in.
    fn log() -> &'static Log {
        Box::leak(Box::new(Log::new()))
    }

    fn info(log: &Log, text: &str) {
        log.push(Level::Info, "test", Duration::ZERO, text);
    }

    /// The text of each line in the log `age` runs back.
    fn read(root: &Path, age: u32) -> Vec<String> {
        let path = root.join(DIR).join(file_name(age));
        let file = fs::read_to_string(path).unwrap_or_default();
        let text = |line: &str| line.split_once(": ").map(|(_, text)| text.to_owned());
        file.lines().filter_map(text).collect()
    }

    #[test]
    fn what_waited_is_written_on_attach() {
        let dir = tempfile::tempdir().unwrap();
        let log = log();
        info(log, "before");
        log.attach(dir.path()).unwrap();
        info(log, "after");
        assert_eq!(read(dir.path(), 0), ["before", "after"]);
    }

    #[test]
    fn keeps_the_current_and_four_old() {
        let dir = tempfile::tempdir().unwrap();
        for run in 0..7 {
            let log = log();
            log.attach(dir.path()).unwrap();
            info(log, &run.to_string());
        }
        assert_eq!(read(dir.path(), 0), ["6"]);
        assert_eq!(read(dir.path(), KEEP), ["2"]);
        assert_eq!(fs::read_dir(dir.path().join(DIR)).unwrap().count(), 5);
    }

    #[test]
    fn another_project_keeps_the_tail_of_the_one_before() {
        let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let log = log();
        info(log, "start");
        log.attach(first.path()).unwrap();
        info(log, "first");
        log.attach(second.path()).unwrap();
        info(log, "second");
        assert_eq!(read(first.path(), 0), ["start", "first"]);
        assert_eq!(read(second.path(), 0), ["second"]);
    }

    #[test]
    fn the_same_again_is_a_repeat() {
        let log = log();
        for text in ["x", "x", "y", "x"] {
            info(log, text);
        }
        log.push(Level::Warning, "test", Duration::ZERO, "x");
        let mut repeats = Vec::new();
        log.read(0, |_, entry| repeats.push(entry.repeat));
        assert_eq!(repeats, [false, true, false, false, false]);
    }

    #[test]
    fn past_the_cap_drops_the_oldest_half() {
        let log = log();
        for n in 0..=CAP {
            info(log, &n.to_string());
        }
        let kept = log.read(0, |_, _| {});
        assert_eq!(kept, CAP / 2..CAP + 1);
        let mut text = String::new();
        log.entry(kept.start, &mut text).unwrap();
        assert_eq!(text, (CAP / 2).to_string());
        assert!(log.entry(kept.start - 1, &mut text).is_none());
    }
}
