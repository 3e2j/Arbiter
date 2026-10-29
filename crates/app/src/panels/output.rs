//! Everything in the [`LOG`] since startup, or since Clear, newest at the
//! bottom, filtered by level. The same entry again is one row.

use std::fmt::Write;

use crate::components::{self, ListScroll, button, list, spacer, toolbar};
use gui::{
    canvas::Color,
    ui::{Align, Element, Length, TextStyle, Ui},
};
use project::{Entry, LOG, Level};

use crate::assets::Icon;
use crate::theme::{self, ThemeColors};

pub struct Output {
    /// Where Clear left the log. Entries before it aren't shown or counted.
    from: usize,
    /// How far into the log `rows` and `counts` go.
    read: usize,
    /// How many came in at each level, repeats included, by [`Level`].
    counts: [u32; Level::ALL.len()],
    /// Which levels are shown, by [`Level`].
    shown: [bool; Level::ALL.len()],
    rows: Vec<Row>,
    scroll: ListScroll,
    /// Whether the newest row sat at the bottom last pass, so new ones stay
    /// in view.
    follow: bool,
}

/// A shown entry and how many times it came in a row.
struct Row {
    /// The last time it came in.
    index: usize,
    repeats: u32,
}

impl Output {
    pub const TITLE: &str = "Output";

    pub const fn new() -> Self {
        Self {
            from: 0,
            read: 0,
            counts: [0; Level::ALL.len()],
            shown: [true; Level::ALL.len()],
            rows: Vec::new(),
            scroll: ListScroll::TOP,
            follow: true,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        self.toolbar(ui);
        self.read_new();
        let height = theme::current().size.row;
        ui.element(Element::column(), |ui| {
            let view = ui.rect().map(|rect| rect.h);
            if let Some(view) = view
                && self.follow
            {
                self.scroll.jump_to_end(view, height, self.rows.len());
            }
            let (mut text, mut time) = (String::new(), String::new());
            list(ui, &mut self.scroll, &self.rows, height, |ui, row| {
                if let Some(entry) = LOG.entry(row.index, &mut text) {
                    entry_ui(ui, &entry, row.repeats, &text, &mut time);
                }
            });
            if let Some(view) = view {
                self.follow = self.scroll.at_end(view, height, self.rows.len());
            }
        });
    }

    /// Takes in what the log has had since last pass, and lets go of rows
    /// it dropped.
    // TODO: entries logged later in the pass, or off the main thread, wait
    // for the next input to show, until something can wake the window.
    fn read_new(&mut self) {
        let (counts, shown, rows) = (&mut self.counts, &self.shown, &mut self.rows);
        let kept = LOG.read(self.read, |index, entry| {
            counts[entry.level as usize] += 1;
            if !shown[entry.level as usize] {
                return;
            }
            match rows.last_mut() {
                Some(row) if entry.repeat && row.index + 1 == index => {
                    row.index = index;
                    row.repeats += 1;
                }
                _ => rows.push(Row { index, repeats: 1 }),
            }
        });
        self.read = kept.end;
        let gone = self.rows.partition_point(|row| row.index < kept.start);
        self.rows.drain(..gone);
    }

    /// Reads the log again from where Clear left it.
    fn reread(&mut self) {
        self.read = self.from;
        self.counts = [0; Level::ALL.len()];
        self.rows.clear();
    }

    /// Clear on the left, and a toggle with a count for each level on the
    /// right.
    fn toolbar(&mut self, ui: &mut Ui) {
        toolbar(ui, |ui| {
            if button(ui, "Clear") {
                self.from = self.read;
                self.reread();
                self.scroll.jump_to_top();
                self.follow = true;
            }
            spacer(ui);
            for level in Level::ALL {
                let shown = self.shown[level as usize];
                if toggle(ui, level, self.counts[level as usize], shown) {
                    self.shown[level as usize] = !shown;
                    self.reread();
                }
            }
        });
    }
}

const fn icon(level: Level) -> Icon {
    match level {
        Level::Error => Icon::CircleX,
        Level::Warning => Icon::Alert,
        Level::Info => Icon::Message,
        Level::Verbose => Icon::More,
    }
}

/// For its icon and message.
const fn color(level: Level, c: &ThemeColors) -> Color {
    match level {
        Level::Error => c.err,
        Level::Warning => c.warn,
        Level::Info => c.text,
        Level::Verbose => c.dim,
    }
}

/// A level's icon and how many came in, filled while `on`. Returns whether
/// it was clicked this pass.
fn toggle(ui: &mut Ui, level: Level, count: u32, on: bool) -> bool {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let mut element = Element {
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.icon_gap,
        radius: [s.radius; 4],
        ..Element::line(s.row)
    };
    element.size[0] = Length::Fit;
    let (clicked, ()) = components::row(ui, element, on, |ui| {
        let (icon_color, text_color) = if on {
            (color(level, &c), c.text)
        } else {
            (c.dim, c.dim)
        };
        ui.icon(icon(level), s.icon, icon_color);
        ui.text(theme.ui_text(text_color), &count.to_string());
    });
    clicked
}

/// Its level's icon, how many times it came in a row, when, where from, and
/// the first line of `text`. `time` is scratch space for formatting.
fn entry_ui(ui: &mut Ui, entry: &Entry, repeats: u32, text: &str, time: &mut String) {
    let theme = theme::current();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: s.gap,
        clip: true,
        ..Element::line(s.row)
    };
    let buffer = |color| TextStyle {
        font: theme.font.buffer,
        ..theme.ui_text(color)
    };
    ui.element(element, |ui| {
        ui.style().background = ui.hovered().then_some(c.hover);
        let color = color(entry.level, &c);
        ui.icon(icon(entry.level), s.icon, color);
        time.clear();
        let _ = write!(time, "{:>9.3}", entry.at.as_secs_f64());
        ui.text(buffer(c.dim), time);
        if repeats > 1 {
            let badge = Element {
                size: [Length::Fit, Length::Fixed(s.text + s.gap / 2.)],
                padding: [s.gap / 2., 0., s.gap / 2., 0.],
                align: [Align::Center; 2],
                background: Some(c.selected),
                radius: [s.radius; 4],
                ..Element::DEFAULT
            };
            ui.element(badge, |ui| {
                ui.text(theme.ui_text(c.text), &repeats.to_string());
            });
        }
        ui.text(theme.ui_text(c.dim), entry.target);
        let mut text = text.lines();
        ui.text(buffer(color), text.next().unwrap_or_default());
        if text.next().is_some() {
            ui.text(buffer(c.dim), "…");
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn push(level: Level, text: &str) {
        LOG.push(level, "test", Duration::ZERO, text);
    }

    fn repeats(output: &Output) -> Vec<u32> {
        output.rows.iter().map(|row| row.repeats).collect()
    }

    #[test]
    fn the_same_in_a_row_is_one_row_and_hidden_levels_are_still_counted() {
        let mut output = Output::new();
        for text in ["x", "x", "y"] {
            push(Level::Info, text);
        }
        push(Level::Warning, "y");
        output.read_new();
        assert_eq!(repeats(&output), [2, 1, 1]);
        output.shown[Level::Info as usize] = false;
        output.reread();
        output.read_new();
        assert_eq!(repeats(&output), [1]);
        assert_eq!(output.counts, [0, 1, 3, 0]);
    }
}
