//! `.arbiter/`, what the editor keeps beside a project and never commits.

mod log;
mod session;

pub use log::{Entry, LOG, Level, Log};
