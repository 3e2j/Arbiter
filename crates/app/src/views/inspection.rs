//! Everything the Inspector shows for one selection. A document builds it
//! row by row: breadcrumb steps first, then each card, with its sections and
//! their fields. The Inspector reads the rows back in the same order to draw
//! them.

use std::fmt::{Display, Write};

#[derive(Default, Debug)]
pub struct Inspection {
    /// Every piece's text, end to end.
    text: String,
    /// Where each piece ends in `text`.
    ends: Vec<usize>,
    /// In order, each taking as many pieces as [`Kind::pieces`] says.
    kinds: Vec<Kind>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Crumb,
    Card,
    Section,
    Field(FieldKind),
}

/// How a field's value is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// In the code font, so digits line up.
    Number,
    /// Hex pairs in the code font, wrapped under the label, as they're too
    /// long to sit beside it.
    Bytes,
}

impl Kind {
    const fn pieces(self) -> usize {
        match self {
            Self::Crumb | Self::Section => 1,
            Self::Card | Self::Field(_) => 2,
        }
    }
}

/// A piece of an [`Inspection`], with its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row<'a> {
    /// A step of the breadcrumb, from the document down to what's selected.
    Crumb(&'a str),
    /// Starts a card. `id` may be empty.
    Card { title: &'a str, id: &'a str },
    /// Starts a section of the card, which folds.
    Section(&'a str),
    Field {
        label: &'a str,
        value: &'a str,
        kind: FieldKind,
    },
}

impl Inspection {
    pub fn crumb(&mut self, label: impl Display) {
        self.push(Kind::Crumb, [&label as &dyn Display]);
    }

    pub fn card(&mut self, title: impl Display, id: impl Display) {
        self.push(Kind::Card, [&title as &dyn Display, &id]);
    }

    pub fn section(&mut self, title: impl Display) {
        self.push(Kind::Section, [&title as &dyn Display]);
    }

    pub fn number(&mut self, label: impl Display, value: impl Display) {
        self.push(
            Kind::Field(FieldKind::Number),
            [&label as &dyn Display, &value],
        );
    }

    pub fn bytes(&mut self, label: impl Display, bytes: &[u8]) {
        let hex = Hex(bytes);
        self.push(
            Kind::Field(FieldKind::Bytes),
            [&label as &dyn Display, &hex],
        );
    }

    fn push<const N: usize>(&mut self, kind: Kind, pieces: [&dyn Display; N]) {
        debug_assert_eq!(kind.pieces(), N);
        for piece in pieces {
            let _ = write!(self.text, "{piece}");
            self.ends.push(self.text.len());
        }
        self.kinds.push(kind);
    }

    /// Every row, in the order it was built.
    pub fn rows(&self) -> impl Iterator<Item = Row<'_>> {
        let (mut start, mut piece) = (0, 0);
        let mut next = move || {
            let end = self.ends.get(piece).copied().unwrap_or(start);
            let text = self.text.get(start..end).unwrap_or_default();
            (start, piece) = (end, piece + 1);
            text
        };
        self.kinds.iter().map(move |kind| match kind {
            Kind::Crumb => Row::Crumb(next()),
            Kind::Card => Row::Card {
                title: next(),
                id: next(),
            },
            Kind::Section => Row::Section(next()),
            &Kind::Field(kind) => Row::Field {
                label: next(),
                value: next(),
                kind,
            },
        })
    }
}

/// Space-separated hex pairs.
struct Hex<'a>(&'a [u8]);

impl Display for Hex<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, byte) in (0..).zip(self.0) {
            if i > 0 {
                f.write_char(' ')?;
            }
            write!(f, "{byte:02X}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_come_back_in_order_with_their_text() {
        let mut inspection = Inspection::default();
        inspection.crumb("zel_03.bmg");
        inspection.crumb(format_args!("Message {}", 12));
        inspection.card("Message", 12);
        inspection.section("Box");
        inspection.number("Kind", 3);
        inspection.bytes("Attributes", &[0, 0x1F]);
        assert_eq!(
            inspection.rows().collect::<Vec<_>>(),
            [
                Row::Crumb("zel_03.bmg"),
                Row::Crumb("Message 12"),
                Row::Card {
                    title: "Message",
                    id: "12"
                },
                Row::Section("Box"),
                Row::Field {
                    label: "Kind",
                    value: "3",
                    kind: FieldKind::Number
                },
                Row::Field {
                    label: "Attributes",
                    value: "00 1F",
                    kind: FieldKind::Bytes
                },
            ]
        );
    }
}
