//! What an inspector shows for one selection, built by the editor that owns
//! it: breadcrumb steps first, then sections and their fields. Any editor
//! builds one the same way, and the inspector reads the rows back in order
//! to draw them, knowing no format.

use std::fmt::{Display, Write};

#[derive(Default, Debug)]
pub struct Page {
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
    Section(&'static str),
    Field(FieldKind, &'static str),
}

/// How a field's value is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// Beside its label.
    Value,
    /// Hex pairs wrapped under its label, as they're too long to sit beside
    /// it.
    Bytes,
}

impl Kind {
    const fn pieces(self) -> usize {
        match self {
            Self::Section(_) => 0,
            Self::Crumb => 1,
            Self::Field(..) => 2,
        }
    }
}

/// A piece of an [`Page`], with its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row<'a> {
    /// A step of the breadcrumb, from the file down to what's selected.
    Crumb(&'a str),
    Section(&'static str),
    Field {
        label: &'static str,
        value: &'a str,
        /// For its tooltip. Empty for none.
        help: &'a str,
        kind: FieldKind,
    },
}

impl Page {
    pub fn crumb(&mut self, label: impl Display) {
        self.push(Kind::Crumb, [&label as &dyn Display]);
    }

    pub fn section(&mut self, title: &'static str) {
        self.push(Kind::Section(title), []);
    }

    pub fn field(&mut self, label: &'static str, value: impl Display, help: &str) {
        self.push(
            Kind::Field(FieldKind::Value, label),
            [&value as &dyn Display, &help],
        );
    }

    pub fn bytes(&mut self, label: &'static str, bytes: &[u8], help: &str) {
        self.push(
            Kind::Field(FieldKind::Bytes, label),
            [&Hex(bytes) as &dyn Display, &help],
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
        self.kinds.iter().map(move |&kind| match kind {
            Kind::Crumb => Row::Crumb(next()),
            Kind::Section(title) => Row::Section(title),
            Kind::Field(kind, label) => Row::Field {
                label,
                value: next(),
                help: next(),
                kind,
            },
        })
    }

    /// The last step of the breadcrumb, if it has one.
    pub fn last_crumb(&self) -> Option<&str> {
        let crumbs = self.rows().map_while(|row| match row {
            Row::Crumb(label) => Some(label),
            _ => None,
        });
        crumbs.last()
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
        let mut page = Page::default();
        page.crumb("zel_03.bmg");
        page.crumb(format_args!("Message {}", 12));
        page.section("Box");
        page.field("Kind", 3, "Which box.");
        page.bytes("Attributes", &[0, 0x1F], "");
        assert_eq!(
            page.rows().collect::<Vec<_>>(),
            [
                Row::Crumb("zel_03.bmg"),
                Row::Crumb("Message 12"),
                Row::Section("Box"),
                Row::Field {
                    label: "Kind",
                    value: "3",
                    help: "Which box.",
                    kind: FieldKind::Value
                },
                Row::Field {
                    label: "Attributes",
                    value: "00 1F",
                    help: "",
                    kind: FieldKind::Bytes
                },
            ]
        );
        assert_eq!(page.last_crumb(), Some("Message 12"));
    }
}
