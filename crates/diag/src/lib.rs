//! Diagnostics every crate reports into. Knows nothing else.
//!
//! A diagnostic is state: what's wrong now, tied to a location.
//! Logs are events and go through `tracing` instead.

mod diagnostic;
mod sink;

pub use diagnostic::{Code, Diagnostic, Key, Location, Severity, Snippet, Span};
pub use sink::Diagnostics;
