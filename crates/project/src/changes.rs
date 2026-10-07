//! `changes/`: what the mod changes, stored against `base/`.

use std::path::Path;

use diag::Diagnostics;

pub const DIR: &str = "changes";

/// Applies every change in `changes` to the base tree at `base`, and reports
/// each one that doesn't apply cleanly.
pub fn check(_changes: &Path, _base: &Path, _diag: &mut Diagnostics) {
    // TODO: No change formats exist yet, so this reports nothing.
}
