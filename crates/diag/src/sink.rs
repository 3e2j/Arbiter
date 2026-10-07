use crate::{Diagnostic, Severity};

/// Passed down as `&mut Diagnostics` to everything that can report. Nothing
/// converts on the way up: producers push, the caller publishes the lot.
#[derive(Debug, Default)]
pub struct Diagnostics {
    pub items: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.items.push(diagnostic);
    }

    /// A later stage doesn't run on input that already failed.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity() == Severity::Error)
    }
}
