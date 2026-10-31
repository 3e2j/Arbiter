//! Each message's text as one line, its tags as chips and its line breaks
//! as an icon. Built once, end to end, so a row slices rather than rebuilds
//! each pass.

use gui::canvas::Color;
use gui::ui::Ui;

use crate::assets::Icon;
use crate::components::{self, Atom, Chip, line_box, wrapped};
use crate::theme;

#[derive(Default)]
pub struct Lines {
    /// Every run's text, chip labels included.
    text: String,
    runs: Vec<Run>,
    /// Where each line's runs end in `runs`.
    ends: Vec<usize>,
    /// Where the line being built starts in `text`.
    start: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
    /// Where its text ends in `Lines::text`.
    end: usize,
    kind: RunKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunKind {
    Text,
    Chip,
    /// A color tag's chip, and the color text after it draws in, plain when
    /// `None`.
    Color {
        swatch: Color,
        tint: Option<Color>,
    },
    /// A line break, without text.
    Break,
}

impl Lines {
    /// `text` onto the line being built, each run of whitespace one space,
    /// none leading, and each line break a [`RunKind::Break`] between spaces.
    pub fn text(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                self.space();
                self.push(RunKind::Text);
                let end = self.text.len();
                self.runs.push(Run {
                    end,
                    kind: RunKind::Break,
                });
                self.text.push(' ');
            } else if ch.is_whitespace() {
                self.space();
            } else {
                self.text.push(ch);
            }
        }
        self.push(RunKind::Text);
    }

    /// A chip labelled whatever `label` writes.
    pub fn chip(&mut self, label: impl FnOnce(&mut String)) {
        label(&mut self.text);
        self.push(RunKind::Chip);
    }

    /// A color tag's chip, as [`RunKind::Color`].
    pub fn color(&mut self, label: impl FnOnce(&mut String), swatch: Color, tint: Option<Color>) {
        label(&mut self.text);
        self.push(RunKind::Color { swatch, tint });
    }

    /// How many lines there are.
    pub const fn len(&self) -> usize {
        self.ends.len()
    }

    /// Ends the line being built.
    pub fn end(&mut self) {
        self.trim();
        self.ends.push(self.runs.len());
        self.start = self.text.len();
    }

    /// A space onto the line being built, unless it's empty or ends on one.
    fn space(&mut self) {
        if self.text.len() > self.start && !self.text.ends_with(' ') {
            self.text.push(' ');
        }
    }

    /// Drops the space the line being built ends on, and its run if that
    /// leaves it empty.
    fn trim(&mut self) {
        if self.text.len() > self.start
            && self.text.ends_with(' ')
            && let Some(run) = self.runs.pop()
        {
            self.text.pop();
            self.push(run.kind);
        }
    }

    /// Ends a run of `kind` where `text` now ends, unless it'd be empty.
    fn push(&mut self, kind: RunKind) {
        let end = self.text.len();
        if end > self.runs.last().map_or(0, |run| run.end) {
            self.runs.push(Run { end, kind });
        }
    }

    /// The runs of the line at `index`, and their text, in order. None past
    /// the last.
    pub fn get(&self, index: usize) -> impl Iterator<Item = (RunKind, &str)> {
        let first = index
            .checked_sub(1)
            .and_then(|before| self.ends.get(before))
            .copied()
            .unwrap_or(0);
        let last = self.ends.get(index).copied().unwrap_or(first);
        let runs = self.runs.get(first..last).unwrap_or_default();
        let mut start = first
            .checked_sub(1)
            .and_then(|before| self.runs.get(before))
            .map_or(0, |run| run.end);
        runs.iter().map(move |run| {
            let text = self.text.get(start..run.end).unwrap_or_default();
            start = run.end;
            (run.kind, text)
        })
    }
}

/// What to look for in [`Lines`]' text.
pub struct Find {
    /// Lowercase unless `case`.
    needle: String,
    case: bool,
    word: bool,
    /// A run lowercased, kept to reuse.
    folded: String,
}

impl Find {
    /// `query`, matching case when `case`, and only as a whole word when
    /// `word`.
    pub fn new(query: &str, case: bool, word: bool) -> Self {
        let mut needle = String::new();
        fold(&mut needle, query, case);
        Self {
            needle,
            case,
            word,
            folded: String::new(),
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.needle.is_empty()
    }

    /// How many times it's in the text of the line at `index`, never across
    /// a chip.
    pub fn count(&mut self, lines: &Lines, index: usize) -> usize {
        let mut count = 0;
        for (kind, run) in lines.get(index) {
            if kind != RunKind::Text {
                continue;
            }
            fold(&mut self.folded, run, self.case);
            let text = self.folded.as_str();
            let word = |at: Option<char>| at.is_some_and(char::is_alphanumeric);
            count += text
                .match_indices(self.needle.as_str())
                .filter(|&(at, found)| {
                    let before = text.get(..at).and_then(|s| s.chars().next_back());
                    let after = text.get(at + found.len()..).and_then(|s| s.chars().next());
                    !self.word || !(word(before) || word(after))
                })
                .count();
        }
        count
    }
}

/// `text` into `out` in place of what it held, lowercased unless `case`.
fn fold(out: &mut String, text: &str, case: bool) {
    out.clear();
    if case {
        out.push_str(text);
    } else {
        out.extend(text.chars().flat_map(char::to_lowercase));
    }
}

/// The line at `index` into the open row, wrapped onto lines `height` tall.
/// A dim "empty" for a line without runs.
#[track_caller]
pub fn line(ui: &mut Ui, lines: &Lines, index: usize, height: f32) {
    let theme = theme::current();
    let c = theme.color;
    let mut runs = lines.get(index).peekable();
    if runs.peek().is_none() {
        ui.element(line_box(height), |ui| {
            ui.text(theme.ui_text(c.dim), "empty")
        });
        return;
    }
    let text = runs.map(|(kind, run)| match kind {
        RunKind::Text => components::Run::Text(run),
        RunKind::Chip => components::Run::Atom(Mark::Chip, run),
        RunKind::Color { swatch, tint } => components::Run::Atom(Mark::Color(swatch, tint), run),
        RunKind::Break => components::Run::Atom(Mark::Break, run),
    });
    wrapped(ui, theme.ui_text(c.text), height, text);
}

/// What a line keeps whole between its text.
#[derive(Clone, Copy)]
enum Mark {
    Chip,
    /// Its swatch, and the color text after it draws in, plain when `None`.
    Color(Color, Option<Color>),
    Break,
}

impl Atom for Mark {
    fn width(self, ui: &mut Ui, label: &str) -> f32 {
        match self {
            Self::Chip => Chip(None).width(ui, label),
            Self::Color(swatch, _) => Chip(Some(swatch)).width(ui, label),
            Self::Break => theme::current().size.icon,
        }
    }

    fn draw(self, ui: &mut Ui, label: &str) {
        match self {
            Self::Chip => Chip(None).draw(ui, label),
            Self::Color(swatch, _) => Chip(Some(swatch)).draw(ui, label),
            Self::Break => {
                let theme = theme::current();
                ui.icon(Icon::CornerDownLeft, theme.size.icon, theme.color.dim);
            }
        }
    }

    fn text_color(self, style: Color) -> Option<Color> {
        match self {
            Self::Color(_, tint) => Some(tint.unwrap_or(style)),
            Self::Chip | Self::Break => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_collapses_whitespace_and_marks_breaks() {
        let mut lines = Lines::default();
        lines.text("  Hey,  you!  ");
        lines.end();
        lines.end();
        lines.text("a ");
        lines.chip(|out| out.push_str("tag"));
        lines.text("\n caf\u{e9}\n");
        lines.end();
        let line = |index| lines.get(index).collect::<Vec<_>>();
        assert_eq!(line(0), [(RunKind::Text, "Hey, you!")]);
        assert_eq!(line(1), []);
        assert_eq!(
            line(2),
            [
                (RunKind::Text, "a "),
                (RunKind::Chip, "tag"),
                (RunKind::Text, " "),
                (RunKind::Break, ""),
                (RunKind::Text, " café "),
                (RunKind::Break, ""),
            ]
        );
        assert_eq!(line(3), []);
    }

    #[test]
    fn find_matches_text_only_by_case_and_word() {
        let mut lines = Lines::default();
        lines.text("Cat cat concat ca");
        lines.chip(|out| out.push_str("t cat"));
        lines.end();
        let count = |query, case, word| Find::new(query, case, word).count(&lines, 0);
        assert_eq!(count("cat", false, false), 3, "never across the chip");
        assert_eq!(count("cat", true, false), 2);
        assert_eq!(count("cat", false, true), 2);
        assert_eq!(count("Cat", true, true), 1);
        assert_eq!(count("cat cat", false, false), 1);
    }
}
