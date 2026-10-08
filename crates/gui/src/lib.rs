//! Everything between an [`App`](host::App) and the screen.
//!
//! Everything outside [`platform`] is the pass, plain data in and out, so a
//! test can drive an app without a window. [`platform`] is the only part that
//! knows winit or wgpu.

pub mod host;
pub mod platform;
