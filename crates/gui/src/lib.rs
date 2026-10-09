//! Everything between an [`App`](host::App) and the screen.
//!
//! Everything outside [`platform`] and [`render`] is the pass, plain data in
//! and out, so a test can drive an app without a window. [`platform`] is the
//! only part that knows winit, and [`render`] the only part that knows wgpu.

pub mod canvas;
mod cast;
pub mod components;
pub mod host;
pub mod input;
mod layout;
pub mod platform;
pub mod render;
mod theme;
pub mod ui;
