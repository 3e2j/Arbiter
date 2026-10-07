use std::fmt::Display;

/// How wrong, never "nothing is wrong": that's status or a log. Ordered so
/// the worst compares greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    Warning,
    Error,
}

/// Defined once as a const in the crate that emits it, and listed in that
/// crate's `CODES`. The id's prefix is its category (`bmg/faulty-tag`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Code {
    pub id: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
}

/// One step of a path into a document, such as `("message", "0x1234")`. An
/// edit's touched items use the same keys, so the store can match the two.
pub type Key = (&'static str, String);

/// Relative to the file the store holds it under. A producer knows keys and
/// offsets inside its own document, never the path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Location {
    /// Outermost first.
    pub key: Vec<Key>,
    /// Bytes into the file, for the hex view.
    pub offset: Option<u64>,
}

/// Bytes into a snippet's UTF-8 text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// The text a problem is in, with `label` under `span`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Snippet {
    pub text: String,
    pub span: Span,
    pub label: String,
}

/// `Diagnostic::new(&CODE, message)` is the minimum, and each optional part is
/// one builder method.
#[must_use = "a diagnostic does nothing until it's pushed into a `Diagnostics`"]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Diagnostic {
    pub code: &'static Code,
    pub message: String,
    pub at: Option<Location>,
    pub snippet: Option<Snippet>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn new(code: &'static Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            at: None,
            snippet: None,
            notes: Vec::new(),
        }
    }

    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.code.severity
    }

    pub fn at(mut self, at: Location) -> Self {
        self.at = Some(at);
        self
    }

    pub fn snippet(
        mut self,
        text: impl Into<String>,
        span: Span,
        label: impl Into<String>,
    ) -> Self {
        self.snippet = Some(Snippet {
            text: text.into(),
            span,
            label: label.into(),
        });
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}

impl Location {
    pub fn key(key: &'static str, value: impl Display) -> Self {
        Self {
            key: vec![(key, value.to_string())],
            offset: None,
        }
    }

    #[must_use]
    pub const fn offset(offset: u64) -> Self {
        Self {
            key: Vec::new(),
            offset: Some(offset),
        }
    }

    /// A key nested inside the ones before it.
    #[must_use]
    pub fn and(mut self, key: &'static str, value: impl Display) -> Self {
        self.key.push((key, value.to_string()));
        self
    }

    #[must_use]
    pub const fn at_offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    /// Whether this is at `item` or inside it. A location with no keys is the
    /// whole file and inside nothing.
    #[must_use]
    pub fn starts_with(&self, item: &[Key]) -> bool {
        !item.is_empty() && self.key.starts_with(item)
    }
}

impl Span {
    #[must_use]
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_with_matches_the_item_and_inside_it() {
        let message = [("message", "0x12".to_owned())];
        assert!(Location::key("message", "0x12").starts_with(&message));
        assert!(
            Location::key("message", "0x12")
                .and("tag", 2)
                .starts_with(&message)
        );
        assert!(!Location::key("message", "0x13").starts_with(&message));
        assert!(!Location::offset(0x40).starts_with(&message));
        assert!(!Location::key("message", "0x12").starts_with(&[]));
    }
}
