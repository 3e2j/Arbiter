//! Everything between an [`App`](host::App) and the screen.
//!
//! Everything outside [`platform`] and [`render`] is the pass, plain data in
//! and out, so a test can drive an app without a window. [`platform`] is the
//! only part that knows winit, [`render`] the only part that knows wgpu,
//! and [`clipboard`] the only part that knows the system's clipboard.

pub mod anim;
pub mod canvas;
pub mod clipboard;
pub mod edit;
pub mod host;
pub mod input;
pub mod keymap;
pub mod platform;
pub mod render;
pub mod ui;
