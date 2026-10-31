//! Text broken onto lines as wide as its box, with atoms in it kept whole,
//! such as chips. Text breaks after spaces, and a word wider than a line
//! takes one of its own, cut off.
//!
//! It breaks to the width its box had last pass, so the layout runs again
//! when that changes.

use gui::ui::{Element, Length, TextStyle, Ui};

/// What [`wrapped`] breaks onto lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run<'a, A> {
    /// Breaks after spaces.
    Text(&'a str),
    /// Kept whole, and drawn by `A`.
    Atom(A, &'a str),
}

/// Something kept whole in a line of text, labelled.
pub trait Atom: Copy {
    fn width(self, ui: &mut Ui, label: &str) -> f32;
    fn draw(self, ui: &mut Ui, label: &str);
}

/// A box `height` tall and as wide as what it holds, centred down, to sit
/// beside the first line of [`wrapped`] text.
#[must_use]
pub const fn line_box(height: f32) -> Element {
    Element {
        size: [Length::Fit, Length::Fixed(height)],
        ..Element::line(height)
    }
}

/// `runs` wrapped onto lines `height` tall, text in `style`, filling the
/// open box across.
#[track_caller]
pub fn wrapped<'a, A: Atom>(
    ui: &mut Ui,
    style: TextStyle,
    height: f32,
    runs: impl IntoIterator<Item = Run<'a, A>>,
) {
    let column = Element {
        size: [Length::Grow, Length::Fit],
        clip: true,
        ..Element::DEFAULT
    };
    ui.element(column, |ui| {
        let width = ui.rect().map_or(f32::INFINITY, |rect| rect.w);
        let pieces = wrap(runs, width, |run| match run {
            Run::Text(text) => ui.measure(style, text)[0],
            Run::Atom(atom, label) => atom.width(ui, label),
        });
        for line in pieces.chunk_by(|a, b| a.0 == b.0) {
            ui.element(Element::line(height), |ui| {
                for &(_, run) in line {
                    match run {
                        Run::Text(text) => {
                            ui.text(style, text);
                        }
                        Run::Atom(atom, label) => atom.draw(ui, label),
                    }
                }
            });
        }
    });
}

/// `runs` broken onto lines `width` wide, as each piece's line and run, in
/// order and with nothing dropped. `measure` says how wide a run is.
pub fn wrap<'a, A: Copy>(
    runs: impl IntoIterator<Item = Run<'a, A>>,
    width: f32,
    mut measure: impl FnMut(Run<'a, A>) -> f32,
) -> Vec<(u32, Run<'a, A>)> {
    let mut out = Vec::new();
    let (mut line, mut x) = (0, 0.);
    for run in runs {
        let Run::Text(text) = run else {
            let w = measure(run);
            if x > 0. && x + w > width {
                (line, x) = (line + 1, 0.);
            }
            x += w;
            out.push((line, run));
            continue;
        };
        let mut push = |line, text: Option<&'a str>| {
            let text = text.filter(|text| !text.is_empty());
            out.extend(text.map(|text| (line, Run::Text(text))));
        };
        let mut start = 0;
        let mut at = 0;
        for word in text.split_inclusive(' ') {
            let w = measure(Run::Text(word));
            if x > 0. && x + w > width {
                push(line, text.get(start..at));
                (line, x, start) = (line + 1, 0., at);
            }
            x += w;
            at += word.len();
        }
        push(line, text.get(start..));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TAG: Run<'static, ()> = Run::Atom((), "a b");

    // Each character a pixel wide, an atom's label four more.
    fn broken(runs: &[Run<'static, ()>], width: f32) -> Vec<(u32, Run<'static, ()>)> {
        wrap(runs.iter().copied(), width, |run| match run {
            Run::Text(text) => text.chars().map(|_| 1.).sum(),
            Run::Atom((), label) => label.chars().map(|_| 1.).sum::<f32>() + 4.,
        })
    }

    #[test]
    fn text_breaks_at_spaces_and_atoms_stay_whole() {
        let runs = [Run::Text("one two"), TAG, Run::Text(" three four")];
        assert_eq!(
            broken(&runs, 10.),
            [
                (0, Run::Text("one two")),
                (1, TAG),
                (1, Run::Text(" ")),
                (2, Run::Text("three four")),
            ]
        );
        assert_eq!(
            broken(&runs, 100.),
            [
                (0, Run::Text("one two")),
                (0, TAG),
                (0, Run::Text(" three four"))
            ]
        );
    }

    #[test]
    fn an_atom_without_a_label_still_takes_its_place() {
        let mark = Run::Atom((), "");
        let runs = [Run::Text("one "), mark, Run::Text(" two")];
        assert_eq!(
            broken(&runs, 100.),
            [(0, Run::Text("one ")), (0, mark), (0, Run::Text(" two"))]
        );
    }
}
