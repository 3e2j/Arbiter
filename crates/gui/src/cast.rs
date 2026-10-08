//! Float narrowing shared by [`platform`](crate::platform) and
//! [`render`](crate::render).

// winit gives f64, while wgpu and the glyphs take f32 positions and u32
// pixels, and Rust has no conversion into either from f64 that isn't `as`.
// Both saturate, which is what's wanted here.
#[allow(clippy::cast_possible_truncation)]
pub(crate) fn narrow(v: f64) -> f32 {
    v as f32
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) fn pixel(v: f64) -> u32 {
    v.round() as u32
}
