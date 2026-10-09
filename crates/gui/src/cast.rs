//! Numeric casts shared by [`platform`](crate::platform),
//! [`canvas`](crate::canvas), [`components`](crate::components) and [`render`](crate::render).

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

/// In sixteenths, as a [`Quad`](crate::canvas::Quad) stores its radii.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub(crate) const fn sixteenths(v: f32) -> u16 {
    (v * 16.).round() as u16
}

#[allow(clippy::cast_possible_truncation)]
pub(crate) fn pixel_offset(v: f64) -> i32 {
    v.round() as i32
}

/// A count as a length, such as rows times their height. Exact up to 2^24,
/// far past any list's length.
#[allow(clippy::cast_precision_loss)]
pub(crate) const fn count(v: usize) -> f32 {
    v as f32
}
