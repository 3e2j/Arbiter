//! Components cast by concept rather than by format: each a model and what
//! draws it. A panel's adapter fills the model from its format.

mod inspection;
mod lines;

pub use inspection::{FieldKind, Inspection, Row};
#[cfg(test)]
pub use lines::RunKind;
pub use lines::{Find, Lines, line};
