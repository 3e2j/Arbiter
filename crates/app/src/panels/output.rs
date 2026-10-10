//! Everything logged through `tracing` since startup, newest at the bottom,
//! filtered by level.

use std::{
    fmt::{self, Write},
    ops::Range,
    sync::{Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use gui::{
    canvas::{Color, IconId},
    components::{ListScroll, button, list},
    input::{Button, Cursor},
    ui::{Align, Direction, Element, Size, TextStyle, Ui},
};
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::layer::{Context, Layer};

use crate::assets::{Icon, Icons};
use crate::theme;

/// What [`Capture`] took in since the [`Output`] last looked. A static, since
/// the subscriber is set up before the editor exists.
static FEED: Mutex<Lines> = Mutex::new(Lines::new());

/// How many entries are kept before the oldest half are dropped.
const CAP: usize = 10_000;
/// The field `tracing` puts an event's message in.
const MESSAGE: &str = "message";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Level {
    Error,
    Warning,
    Info,
    /// Debug and trace.
    Verbose,
}

#[derive(Clone, Debug)]
struct Entry {
    level: Level,
    target: &'static str,
    /// Since startup, the last time it came in.
    at: Duration,
    /// In [`Lines::text`].
    text: Range<usize>,
    /// How many times it came in a row.
    repeats: u32,
}

/// Entries with their text back to back in one string.
#[derive(Debug)]
struct Lines {
    entries: Vec<Entry>,
    text: String,
    /// How many came in at each level, repeats included, by [`Level`].
    counts: [u32; Level::ALL.len()],
}

/// Feeds the [`Output`] from `tracing`.
pub struct Capture {
    start: Instant,
}

/// Formats an event's message, then its other fields as `name=value`.
#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

pub struct Output {
    icons: Icons,
    lines: Lines,
    /// Which levels are shown, by [`Level`].
    shown: [bool; Level::ALL.len()],
    /// The shown entries, by index into `lines.entries`.
    rows: Vec<usize>,
    /// Whether `rows` needs rebuilding.
    stale: bool,
    scroll: ListScroll,
    /// Whether the newest row sat at the bottom last pass, so new ones stay
    /// in view.
    follow: bool,
}

impl Level {
    const ALL: [Self; 4] = [Self::Error, Self::Warning, Self::Info, Self::Verbose];

    const fn of(level: tracing::Level) -> Self {
        match level {
            tracing::Level::ERROR => Self::Error,
            tracing::Level::WARN => Self::Warning,
            tracing::Level::INFO => Self::Info,
            tracing::Level::DEBUG | tracing::Level::TRACE => Self::Verbose,
        }
    }

    const fn icon(self) -> Icon {
        match self {
            Self::Error => Icon::CircleX,
            Self::Warning => Icon::Alert,
            Self::Info => Icon::Message,
            Self::Verbose => Icon::More,
        }
    }

    /// For its icon and message.
    const fn color(self, dim: Color, text: Color) -> Color {
        match self {
            Self::Error => theme::ERROR,
            Self::Warning => theme::WARNING,
            Self::Info => text,
            Self::Verbose => dim,
        }
    }
}

impl Lines {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            text: String::new(),
            counts: [0; Level::ALL.len()],
        }
    }

    /// Merges it into the last entry when it's the same.
    // Only the entry right before is compared, so two messages that take
    // turns are never merged. Merging across the whole log needs a map from
    // text to entry.
    fn push(&mut self, level: Level, target: &'static str, at: Duration, text: &str) {
        self.counts[level as usize] += 1;
        if let Some(last) = self.entries.last_mut()
            && last.level == level
            && last.target == target
            && self.text.get(last.text.clone()) == Some(text)
        {
            last.repeats += 1;
            last.at = at;
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        self.entries.push(Entry {
            level,
            target,
            at,
            text: start..self.text.len(),
            repeats: 1,
        });
        if self.entries.len() > CAP {
            self.drop_oldest_half();
        }
    }

    fn drop_oldest_half(&mut self) {
        self.entries.drain(..self.entries.len() / 2);
        let cut = self
            .entries
            .first()
            .map_or(self.text.len(), |entry| entry.text.start);
        self.text.drain(..cut);
        self.counts = [0; Level::ALL.len()];
        for entry in &mut self.entries {
            entry.text = entry.text.start - cut..entry.text.end - cut;
            self.counts[entry.level as usize] += entry.repeats;
        }
    }

    /// Moves `from`'s entries onto the end, leaving it empty.
    fn take(&mut self, from: &mut Self) {
        for entry in from.entries.drain(..) {
            let text = from.text.get(entry.text).unwrap_or_default();
            self.push(entry.level, entry.target, entry.at, text);
            if let Some(last) = self.entries.last_mut() {
                last.repeats += entry.repeats - 1;
            }
            self.counts[entry.level as usize] += entry.repeats - 1;
        }
        from.clear();
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.text.clear();
        self.counts = [0; Level::ALL.len()];
    }

    fn text(&self, entry: &Entry) -> &str {
        self.text.get(entry.text.clone()).unwrap_or_default()
    }
}

/// Whoever panicked holding it left at most one entry half pushed.
fn feed() -> MutexGuard<'static, Lines> {
    FEED.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Capture {
    /// Times entries from `start`.
    #[must_use]
    pub const fn new(start: Instant) -> Self {
        Self { start }
    }
}

impl<S: Subscriber> Layer<S> for Capture {
    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        if fields.message.is_empty() {
            fields.rest = fields.rest.trim_start().to_owned();
        }
        fields.message.push_str(&fields.rest);
        let meta = event.metadata();
        let at = self.start.elapsed();
        feed().push(Level::of(*meta.level()), meta.target(), at, &fields.message);
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

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let _ = if field.name() == MESSAGE {
            write!(self.message, "{value:?}")
        } else {
            write!(self.rest, " {}={value:?}", field.name())
        };
    }
}

impl Output {
    pub const TITLE: &str = "Output";

    pub const fn new(icons: Icons) -> Self {
        Self {
            icons,
            lines: Lines::new(),
            shown: [true; Level::ALL.len()],
            rows: Vec::new(),
            stale: true,
            scroll: ListScroll::TOP,
            follow: true,
        }
    }

    // TODO: entries logged later in the pass, or off the main thread, wait
    // for the next input to show, until something can wake the window.
    pub fn ui(&mut self, ui: &mut Ui) {
        {
            let mut feed = feed();
            if !feed.entries.is_empty() {
                self.lines.take(&mut feed);
                self.stale = true;
            }
        }
        self.toolbar(ui);
        if self.stale {
            self.rows.clear();
            let shown = (0..)
                .zip(&self.lines.entries)
                .filter(|(_, entry)| self.shown[entry.level as usize])
                .map(|(index, _)| index);
            self.rows.extend(shown);
            self.stale = false;
        }
        let height = ui.theme().size.row;
        ui.element(Element::column(), |ui| {
            let view = ui.rect().map(|rect| rect.h);
            if let Some(view) = view
                && self.follow
            {
                self.scroll = ListScroll::end(view, height, self.rows.len());
            }
            let mut time = String::new();
            list(ui, &mut self.scroll, &self.rows, height, |ui, &index| {
                if let Some(entry) = self.lines.entries.get(index) {
                    entry_ui(ui, &self.lines, entry, &self.icons, &mut time);
                }
            });
            if let Some(view) = view {
                self.follow = self.scroll.at_end(view, height, self.rows.len());
            }
        });
    }

    /// Clear on the left, and a toggle with a count for each level on the
    /// right.
    fn toolbar(&mut self, ui: &mut Ui) {
        let size = ui.theme().size;
        let strip = Element {
            direction: Direction::LeftToRight,
            size: [Size::Grow, Size::Fit],
            padding: [size.gap / 2.; 4],
            gap: size.gap / 2.,
            align: [Align::Start, Align::Center],
            ..Element::DEFAULT
        };
        ui.element(strip, |ui| {
            if button(ui, "Clear") {
                self.lines.clear();
                self.stale = true;
                self.scroll = ListScroll::TOP;
                self.follow = true;
            }
            ui.element(
                Element {
                    size: [Size::Grow, Size::Fixed(0.)],
                    ..Element::DEFAULT
                },
                |_| {},
            );
            for (level, shown) in Level::ALL.into_iter().zip(&mut self.shown) {
                let count = self.lines.counts[level as usize];
                if toggle(ui, self.icons.get(level.icon()), level, count, *shown) {
                    *shown = !*shown;
                    self.stale = true;
                }
            }
        });
    }
}

/// A level's icon and how many came in, filled while `on`. Returns whether
/// it was clicked this pass.
fn toggle(ui: &mut Ui, icon: IconId, level: Level, count: u32, on: bool) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Size::Fit, Size::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        radius: [s.radius; 4],
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.style().background = if on {
            Some(c.selected)
        } else {
            ui.hovered().then_some(c.hover)
        };
        let (icon_color, text_color) = if on {
            (level.color(c.dim, c.text), c.text)
        } else {
            (c.dim, c.dim)
        };
        ui.icon(icon, s.icon, icon_color);
        ui.text(theme.ui_text(text_color), &count.to_string());
        ui.clicked(Button::Left)
    })
}

/// Its level's icon, how many times it repeated, when, where from, and the
/// first line of its message. `time` is scratch space for formatting.
fn entry_ui(ui: &mut Ui, lines: &Lines, entry: &Entry, icons: &Icons, time: &mut String) {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Size::Grow, Size::Fixed(s.row)],
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.gap,
        align: [Align::Start, Align::Center],
        clip: true,
        ..Element::DEFAULT
    };
    let buffer = |color| TextStyle {
        font: theme.font.buffer,
        ..theme.ui_text(color)
    };
    ui.element(element, |ui| {
        ui.style().background = ui.hovered().then_some(c.hover);
        let color = entry.level.color(c.dim, c.text);
        ui.icon(icons.get(entry.level.icon()), s.icon, color);
        time.clear();
        let _ = write!(time, "{:>9.3}", entry.at.as_secs_f64());
        ui.text(buffer(c.dim), time);
        if entry.repeats > 1 {
            let badge = Element {
                size: [Size::Fit, Size::Fixed(s.text + s.gap / 2.)],
                padding: [s.gap / 2., 0., s.gap / 2., 0.],
                align: [Align::Center; 2],
                background: Some(c.selected),
                radius: [s.radius; 4],
                ..Element::DEFAULT
            };
            ui.element(badge, |ui| {
                ui.text(theme.ui_text(c.text), &entry.repeats.to_string());
            });
        }
        ui.text(theme.ui_text(c.dim), entry.target);
        let mut text = lines.text(entry).lines();
        ui.text(buffer(color), text.next().unwrap_or_default());
        if text.next().is_some() {
            ui.text(buffer(c.dim), "…");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Duration = Duration::ZERO;

    #[test]
    fn the_same_in_a_row_merges() {
        let mut lines = Lines::new();
        lines.push(Level::Info, "a", AT, "x");
        lines.push(Level::Info, "a", AT, "x");
        lines.push(Level::Info, "a", AT, "y");
        lines.push(Level::Info, "a", AT, "x");
        let repeats: Vec<_> = lines.entries.iter().map(|entry| entry.repeats).collect();
        assert_eq!(repeats, [2, 1, 1]);
        assert_eq!(lines.counts[Level::Info as usize], 4);
    }

    #[test]
    fn another_level_or_target_doesnt_merge() {
        let mut lines = Lines::new();
        lines.push(Level::Info, "a", AT, "x");
        lines.push(Level::Warning, "a", AT, "x");
        lines.push(Level::Warning, "b", AT, "x");
        assert_eq!(lines.entries.len(), 3);
    }

    #[test]
    fn taking_merges_across_and_keeps_repeats() {
        let (mut lines, mut feed) = (Lines::new(), Lines::new());
        lines.push(Level::Info, "a", AT, "x");
        feed.push(Level::Info, "a", AT, "x");
        feed.push(Level::Info, "a", AT, "x");
        feed.push(Level::Error, "a", AT, "y");
        lines.take(&mut feed);
        let repeats: Vec<_> = lines.entries.iter().map(|entry| entry.repeats).collect();
        assert_eq!(repeats, [3, 1]);
        assert_eq!(lines.counts, [1, 0, 3, 0]);
        assert!(feed.entries.is_empty() && feed.text.is_empty());
    }

    #[test]
    fn past_the_cap_drops_the_oldest_half() {
        let mut lines = Lines::new();
        for n in 0..=CAP {
            lines.push(Level::Info, "a", AT, &n.to_string());
        }
        assert_eq!(lines.entries.len(), CAP / 2 + 1);
        let first = lines.entries.first().map(|entry| lines.text(entry));
        assert_eq!(first, Some((CAP / 2).to_string().as_str()));
        assert_eq!(
            lines.counts[Level::Info as usize],
            u32::try_from(CAP / 2 + 1).unwrap()
        );
    }
}
