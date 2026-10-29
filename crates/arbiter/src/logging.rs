//! Where `tracing` goes: stderr from info up, and the [`LOG`] from debug up,
//! both timed from startup. The log writes the open project's file and feeds
//! the editor's Output.

use std::{
    backtrace::Backtrace,
    fmt::{self as format, Write},
    io::{self, IsTerminal},
    panic,
    path::Path,
    time::Instant,
};

use project::{LOG, Level};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    Layer,
    filter::LevelFilter,
    fmt::{self, time::Uptime},
    layer::{Context, SubscriberExt},
    util::SubscriberInitExt,
};

/// The field `tracing` puts an event's message in.
const MESSAGE: &str = "message";

/// Pushes each event to the [`LOG`].
struct ToLog {
    start: Instant,
}

/// Formats an event's message, then its other fields as `name=value`.
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

/// Sets up the subscriber and starts the log's writer, and logs panics with
/// their backtrace, written out at once so a crash lands in the project's
/// log.
pub fn init() {
    let start = Instant::now();
    LOG.start();
    let stderr = fmt::layer()
        .with_writer(io::stderr)
        .with_ansi(io::stderr().is_terminal())
        .with_timer(Uptime::from(start))
        .with_filter(LevelFilter::INFO);
    let log = ToLog { start }.with_filter(LevelFilter::DEBUG);
    // Only fails when a subscriber is already set, and nothing else sets one.
    let _ = tracing_subscriber::registry()
        .with(stderr)
        .with(log)
        .try_init();
    panic::set_hook(Box::new(|info| {
        tracing::error!("{info}\n{}", Backtrace::force_capture());
        let _ = LOG.flush();
    }));
    tracing::debug!("arbiter {}", env!("CARGO_PKG_VERSION"));
}

/// Logs into the project at `root` from here on. Without it, entries only
/// reach stderr and the Output.
pub fn attach(root: &Path) {
    if let Err(err) = LOG.attach(root) {
        tracing::warn!("can't write a log in {}: {err}", root.display());
    }
}

impl<S: Subscriber> Layer<S> for ToLog {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.message.is_empty() {
            fields.rest = fields.rest.trim_start().to_owned();
        }
        fields.message.push_str(&fields.rest);
        let meta = event.metadata();
        let level = match *meta.level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warning,
            tracing::Level::INFO => Level::Info,
            tracing::Level::DEBUG | tracing::Level::TRACE => Level::Verbose,
        };
        LOG.push(level, meta.target(), self.start.elapsed(), &fields.message);
    }
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == MESSAGE {
            self.message.push_str(value);
        } else {
            let _ = write!(self.rest, " {}={value}", field.name());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn format::Debug) {
        let _ = if field.name() == MESSAGE {
            write!(self.message, "{value:?}")
        } else {
            write!(self.rest, " {}={value:?}", field.name())
        };
    }
}
