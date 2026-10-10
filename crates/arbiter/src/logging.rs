//! Where `tracing` goes: stderr from info up, and the open project's log and
//! the editor's Output from debug up, all timed from startup.

use std::{
    backtrace::Backtrace,
    io::{self, IsTerminal},
    panic,
    path::Path,
    sync::Arc,
    time::Instant,
};

use project::{Log, LogWriter};
use tracing::{Level, Metadata};
use tracing_subscriber::{
    Layer,
    filter::LevelFilter,
    fmt::{self, MakeWriter, time::Uptime},
    layer::SubscriberExt,
    util::SubscriberInitExt,
};

/// Hands the project's log each line, waking its writer for warnings and
/// errors.
struct ToLog(Arc<Log>);

/// Sets up the subscriber, feeding the Output too when `editor`, and logs
/// panics with their backtrace, written out at once so a crash lands in the
/// project's log.
pub fn init(editor: bool) -> Arc<Log> {
    let start = Instant::now();
    let log = Arc::new(Log::start());
    let stderr = fmt::layer()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .with_timer(Uptime::from(start))
        .with_filter(LevelFilter::INFO);
    let file = fmt::layer()
        .with_writer(ToLog(Arc::clone(&log)))
        .with_ansi(false)
        .with_timer(Uptime::from(start))
        .with_filter(LevelFilter::DEBUG);
    let output = editor.then(|| app::Capture::new(start).with_filter(LevelFilter::DEBUG));
    // Only fails when a subscriber is already set, and nothing else sets one.
    let _ = tracing_subscriber::registry()
        .with(stderr)
        .with(file)
        .with(output)
        .try_init();
    let flushed = Arc::clone(&log);
    panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}\n{}", Backtrace::force_capture());
        let _ = flushed.flush();
    }));
    tracing::debug!("arbiter {}", env!("CARGO_PKG_VERSION"));
    log
}

/// Logs into the project at `root` from here on. Without it, lines keep
/// going to stderr only.
pub fn attach(log: &Log, root: &Path) {
    if let Err(err) = log.attach(root) {
        tracing::warn!("can't write a log in {}: {err}", root.display());
    }
}

impl<'a> MakeWriter<'a> for ToLog {
    type Writer = LogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        self.0.writer(false)
    }

    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        self.0.writer(*meta.level() <= Level::WARN)
    }
}
