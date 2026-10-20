//! Components cast by concept rather than by format: each a model and what
//! draws it. A panel's adapter fills the model from its format.

mod lines;

#[cfg(test)]
pub use lines::RunKind;
pub use lines::{Lines, line};
