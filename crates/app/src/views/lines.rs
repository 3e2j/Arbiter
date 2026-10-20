//! Many rows of text with chips in it, such as tags, wrapped, and a badge at
//! the row's end. Built once, end to end, so a row slices rather than
//! rebuilds each pass.

use gui::canvas::Color;
use gui::ui::Ui;

use crate::components::{self, Chip, chip, line_box, wrapped};
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
    /// With the color it sets, if any.
    Chip(Option<Color>),
    /// Drawn after the line, at the row's end.
    Badge,
}

impl Lines {
    /// `text` onto the line being built, each run of whitespace one space,
    /// none leading.
    pub fn text(&mut self, text: &str) {
        for ch in text.chars() {
            if !ch.is_whitespace() {
                self.text.push(ch);
            } else if self.text.len() > self.start && !self.text.ends_with(' ') {
                self.text.push(' ');
            }
        }
        self.push(RunKind::Text);
    }

    /// A chip labelled whatever `label` writes.
    pub fn chip(&mut self, swatch: Option<Color>, label: impl FnOnce(&mut String)) {
        label(&mut self.text);
        self.push(RunKind::Chip(swatch));
    }

    /// The badge labelled whatever `label` writes. Last of a line's runs.
    pub fn badge(&mut self, label: impl FnOnce(&mut String)) {
        self.trim();
        label(&mut self.text);
        self.push(RunKind::Badge);
    }

    /// Ends the line being built.
    pub fn end(&mut self) {
        self.trim();
        self.ends.push(self.runs.len());
        self.start = self.text.len();
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

/// The line at `index` into the open row, wrapped onto lines `height` tall,
/// then its badge beside the first. A dim "empty" for a line without runs.
#[track_caller]
pub fn line(ui: &mut Ui, lines: &Lines, index: usize, height: f32) {
    let theme = theme::current();
    let c = theme.color;
    let beside = line_box(height);
    let mut runs = lines.get(index).peekable();
    if runs.peek().is_none() {
        ui.element(beside, |ui| ui.text(theme.ui_text(c.dim), "empty"));
        return;
    }
    let mut badge = None;
    let text = runs.filter_map(|(kind, run)| match kind {
        RunKind::Text => Some(components::Run::Text(run)),
        RunKind::Chip(swatch) => Some(components::Run::Atom(Chip(swatch), run)),
        RunKind::Badge => {
            badge = Some(run);
            None
        }
    });
    wrapped(ui, theme.ui_text(c.text), height, text);
    if let Some(label) = badge {
        ui.element(beside, |ui| chip(ui, label, None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_line_flattens_its_text_around_chips() {
        let mut lines = Lines::default();
        lines.text("  Hey,\nyou!  ");
        lines.end();
        lines.end();
        lines.text("a ");
        lines.chip(None, |out| out.push_str("tag"));
        lines.text("\n caf\u{e9} ");
        lines.badge(|out| out.push_str("box"));
        lines.end();
        let line = |index| lines.get(index).collect::<Vec<_>>();
        assert_eq!(line(0), [(RunKind::Text, "Hey, you!")]);
        assert_eq!(line(1), []);
        assert_eq!(
            line(2),
            [
                (RunKind::Text, "a "),
                (RunKind::Chip(None), "tag"),
                (RunKind::Text, " café"),
                (RunKind::Badge, "box"),
            ]
        );
        assert_eq!(line(3), []);
    }

    #[test]
    fn find_matches_text_only_by_case_and_word() {
        let mut lines = Lines::default();
        lines.text("Cat cat concat ca");
        lines.chip(None, |out| out.push_str("t cat"));
        lines.end();
        let count = |query, case, word| Find::new(query, case, word).count(&lines, 0);
        assert_eq!(count("cat", false, false), 3, "never across the chip");
        assert_eq!(count("cat", true, false), 2);
        assert_eq!(count("cat", false, true), 2);
        assert_eq!(count("Cat", true, true), 1);
        assert_eq!(count("cat cat", false, false), 1);
    }
}
