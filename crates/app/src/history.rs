//! Undo and redo for an [`Editable`] document, kept as the edits that reverse
//! each step. Edits [`Editable::merges`] joins become one step while each
//! comes within [`PAUSE`] of the last, until the step is sealed, such as when
//! typing moves to another field. Each step keeps the cursor from before and
//! after it, which undo and redo give back.

use std::time::{Duration, Instant};

use formats::Editable;

/// How long typing stops for before what's typed after is a step of its own.
pub const PAUSE: Duration = Duration::from_millis(300);

struct Step<E, C> {
    edit: E,
    before: C,
    after: C,
}

pub struct History<E, C> {
    undo: Vec<Step<E, C>>,
    redo: Vec<Step<E, C>>,
    /// When the last step was last edited, while the next edit may join it.
    open: Option<Instant>,
}

impl<E, C> Default for History<E, C> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            open: None,
        }
    }
}

impl<E, C> History<E, C> {
    /// `edit` on `doc` at `now`, moving the cursor from `before` to `after`,
    /// as a step, or as part of the last one while it's open, merges, and was
    /// edited within [`PAUSE`]. A joined edit keeps the step's `before`. An
    /// edit that touches nothing isn't a step. Returns what it touched.
    pub fn apply<D: Editable<Edit = E>>(
        &mut self,
        doc: &mut D,
        edit: E,
        [before, after]: [C; 2],
        now: Instant,
    ) -> Vec<D::Item> {
        let recent = self
            .open
            .is_some_and(|last| now.saturating_duration_since(last) < PAUSE);
        let joins = recent
            .then(|| self.undo.last_mut())
            .flatten()
            .filter(|last| D::merges(&last.edit, &edit));
        let (undo, touched) = doc.apply(edit);
        if touched.is_empty() {
            return touched;
        }
        match joins {
            Some(last) => last.after = after,
            None => self.undo.push(Step {
                edit: undo,
                before,
                after,
            }),
        }
        self.redo.clear();
        self.open = Some(now);
        touched
    }

    /// Reverses the last step. Returns what it touched and the cursor from
    /// before it, or `None` when there's none.
    pub fn undo<D: Editable<Edit = E>>(&mut self, doc: &mut D) -> Option<(Vec<D::Item>, &C)> {
        let step = self.undo.pop()?;
        let (redo, touched) = doc.apply(step.edit);
        self.redo.push(Step { edit: redo, ..step });
        self.open = None;
        self.redo.last().map(|step| (touched, &step.before))
    }

    /// Does the last step undone again. Returns what it touched and the
    /// cursor from after it, or `None` when there's none.
    pub fn redo<D: Editable<Edit = E>>(&mut self, doc: &mut D) -> Option<(Vec<D::Item>, &C)> {
        let step = self.redo.pop()?;
        let (undo, touched) = doc.apply(step.edit);
        self.undo.push(Step { edit: undo, ..step });
        self.open = None;
        self.undo.last().map(|step| (touched, &step.after))
    }

    /// Ends the last step, so the next edit starts its own.
    pub const fn seal(&mut self) {
        self.open = None;
    }
}

#[cfg(test)]
mod tests {
    use diag::{Address, AddressStep};

    use super::*;

    /// A number set whole, where sets in a row merge.
    #[derive(Debug, PartialEq, Eq)]
    struct Number(u8);

    struct Whole;

    impl Address for Whole {
        fn key(&self) -> Vec<AddressStep> {
            Vec::new()
        }
    }

    impl Editable for Number {
        type Edit = u8;
        type Item = Whole;

        fn apply(&mut self, edit: u8) -> (u8, Vec<Whole>) {
            (std::mem::replace(&mut self.0, edit), vec![Whole])
        }

        fn merges(_: &u8, _: &u8) -> bool {
            true
        }
    }

    #[test]
    fn edits_in_an_open_step_undo_together() {
        let (mut doc, mut history) = (Number(0), History::default());
        let now = Instant::now();
        history.apply(&mut doc, 1, [0; 2], now);
        history.apply(&mut doc, 2, [0; 2], now);
        history.seal();
        history.apply(&mut doc, 3, [0; 2], now);
        assert!(history.undo(&mut doc).is_some());
        assert_eq!(doc, Number(2));
        assert!(history.undo(&mut doc).is_some());
        assert_eq!(doc, Number(0));
        assert!(history.undo(&mut doc).is_none());
    }

    #[test]
    fn redo_does_again_until_a_new_edit() {
        let (mut doc, mut history) = (Number(0), History::default());
        let now = Instant::now();
        history.apply(&mut doc, 1, [0; 2], now);
        history.undo(&mut doc);
        history.redo(&mut doc);
        assert_eq!(doc, Number(1));
        history.undo(&mut doc);
        history.apply(&mut doc, 5, [0; 2], now);
        assert!(history.redo(&mut doc).is_none());
        assert_eq!(doc, Number(5));
    }

    #[test]
    fn an_edit_after_undo_starts_its_own_step() {
        let (mut doc, mut history) = (Number(0), History::default());
        let now = Instant::now();
        history.apply(&mut doc, 1, [0; 2], now);
        history.seal();
        history.apply(&mut doc, 2, [0; 2], now);
        history.undo(&mut doc);
        history.apply(&mut doc, 3, [0; 2], now);
        history.undo(&mut doc);
        assert_eq!(doc, Number(1));
    }

    #[test]
    fn a_pause_starts_a_step_and_typing_keeps_one_open() {
        let (mut doc, mut history) = (Number(0), History::default());
        let start = Instant::now();
        for (value, ms) in [(1, 0), (2, 200), (3, 400), (4, 800)] {
            history.apply(&mut doc, value, [0; 2], start + Duration::from_millis(ms));
        }
        history.undo(&mut doc);
        assert_eq!(doc, Number(3));
        history.undo(&mut doc);
        assert_eq!(doc, Number(0));
    }

    #[test]
    fn undo_gives_the_cursor_before_a_step_and_redo_after() {
        let (mut doc, mut history) = (Number(0), History::default());
        let now = Instant::now();
        history.apply(&mut doc, 1, [3, 4], now);
        history.apply(&mut doc, 2, [4, 5], now);
        assert_eq!(history.undo(&mut doc).map(|(_, &cursor)| cursor), Some(3));
        assert_eq!(history.redo(&mut doc).map(|(_, &cursor)| cursor), Some(5));
    }
}
