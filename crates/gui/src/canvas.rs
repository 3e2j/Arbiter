//! What gets drawn. A pass writes quads into a [`Canvas`], and
//! [`platform::gpu`](crate::platform::gpu) draws it.
//!
//! Every shape is one [`Quad`]. The shader rounds its corners and draws its
//! border per pixel, so a whole screen batches into a draw per clip.

use std::ops::Range;

/// In logical pixels, from the window's top left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    #[must_use]
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    #[must_use]
    pub const fn right(self) -> f32 {
        self.x + self.w
    }

    #[must_use]
    pub const fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// The overlap of both, zero-sized when they don't touch.
    #[must_use]
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let w = (self.right().min(other.right()) - x).max(0.);
        let h = (self.bottom().min(other.bottom()) - y).max(0.);
        Self { x, y, w, h }
    }

    /// Shrunk by `by` on every side.
    #[must_use]
    pub fn inset(self, by: f32) -> Self {
        Self {
            x: self.x + by,
            y: self.y + by,
            w: (self.w - by * 2.).max(0.),
            h: (self.h - by * 2.).max(0.),
        }
    }
}

/// Linear RGBA, straight alpha.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color(pub [f32; 4]);

impl Color {
    pub const TRANSPARENT: Self = Self([0.; 4]);

    /// From an sRGB `0xRRGGBB`, as colours are usually written.
    #[must_use]
    pub fn hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self([linear(r), linear(g), linear(b), 1.])
    }

    #[must_use]
    pub const fn alpha(self, a: f32) -> Self {
        let [r, g, b, _] = self.0;
        Self([r, g, b, a])
    }
}

fn linear(channel: u8) -> f32 {
    let c = f32::from(channel) / 255.;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// One shape, laid out as the shader reads it.
///
/// Built as `Quad::new(rect, fill).rounded(4.).bordered(1., line)`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Quad {
    rect: [f32; 4],
    fill: [f32; 4],
    border: [f32; 4],
    /// Top left, top right, bottom right, bottom left.
    radii: [f32; 4],
    border_width: f32,
}

impl Quad {
    #[must_use]
    pub const fn new(rect: Rect, fill: Color) -> Self {
        Self {
            rect: [rect.x, rect.y, rect.w, rect.h],
            fill: fill.0,
            border: [0.; 4],
            radii: [0.; 4],
            border_width: 0.,
        }
    }

    #[must_use]
    pub const fn rounded(self, radius: f32) -> Self {
        self.corners([radius; 4])
    }

    /// Top left, top right, bottom right, bottom left.
    #[must_use]
    pub const fn corners(mut self, radii: [f32; 4]) -> Self {
        self.radii = radii;
        self
    }

    /// Drawn inside the rect, so a border never changes a quad's size.
    #[must_use]
    pub const fn bordered(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border = color.0;
        self
    }
}

/// A pass's quads in draw order, split where the clip changes.
#[derive(Debug, Default)]
pub struct Canvas {
    /// What the surface is cleared to before any quad.
    pub background: Color,
    quads: Vec<Quad>,
    batches: Vec<Batch>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Batch {
    clip: Rect,
    start: usize,
}

impl Canvas {
    /// Empties the canvas for a new pass, clipped to `root`. Keeps its
    /// allocations, so a warm pass allocates nothing.
    pub fn clear(&mut self, root: Rect) {
        self.quads.clear();
        self.batches.clear();
        self.batches.push(Batch {
            clip: root,
            start: 0,
        });
    }

    /// Cuts off every quad after this one at `clip`, until the next call.
    pub fn clip(&mut self, clip: Rect) {
        let start = self.quads.len();
        match self.batches.last_mut() {
            Some(last) if last.clip == clip => {}
            // Nothing was drawn under the last clip, so it's replaced, not split.
            Some(last) if last.start == start => last.clip = clip,
            _ => self.batches.push(Batch { clip, start }),
        }
    }

    pub fn quad(&mut self, quad: Quad) {
        self.quads.push(quad);
    }

    #[must_use]
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    /// Each clip with the quads it cuts, in draw order, skipping empty ones.
    pub fn batches(&self) -> impl Iterator<Item = (Rect, Range<usize>)> {
        let ends = self
            .batches
            .iter()
            .skip(1)
            .map(|next| next.start)
            .chain([self.quads.len()]);
        self.batches
            .iter()
            .zip(ends)
            .map(|(batch, end)| (batch.clip, batch.start..end))
            .filter(|(_, range)| !range.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT: Rect = Rect::new(0., 0., 100., 100.);
    const SIDE: Rect = Rect::new(0., 0., 20., 100.);

    fn quad() -> Quad {
        Quad::new(ROOT, Color::TRANSPARENT)
    }

    fn batches(canvas: &Canvas) -> Vec<(Rect, Range<usize>)> {
        canvas.batches().collect()
    }

    #[test]
    fn one_clip_is_one_batch() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.quad(quad());
        canvas.clip(ROOT);
        canvas.quad(quad());
        assert_eq!(batches(&canvas), [(ROOT, 0..2)]);
    }

    #[test]
    fn a_new_clip_splits() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.quad(quad());
        canvas.clip(SIDE);
        canvas.quad(quad());
        canvas.clip(ROOT);
        canvas.quad(quad());
        assert_eq!(batches(&canvas), [(ROOT, 0..1), (SIDE, 1..2), (ROOT, 2..3)]);
    }

    #[test]
    fn an_unused_clip_is_replaced() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.clip(SIDE);
        canvas.quad(quad());
        canvas.clip(ROOT);
        assert_eq!(batches(&canvas), [(SIDE, 0..1)]);
    }

    #[test]
    fn clear_forgets_the_last_pass() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.clip(SIDE);
        canvas.quad(quad());
        canvas.clear(ROOT);
        canvas.quad(quad());
        assert_eq!(batches(&canvas), [(ROOT, 0..1)]);
    }

    #[test]
    fn intersect_without_overlap_is_empty() {
        let far = Rect::new(200., 200., 10., 10.);
        let overlap = ROOT.intersect(far);
        assert_eq!((overlap.w, overlap.h), (0., 0.));
    }

    #[test]
    fn hex_is_linear() {
        assert_eq!(Color::hex(0xff_ff_ff), Color([1., 1., 1., 1.]));
        assert_eq!(Color::hex(0), Color([0., 0., 0., 1.]));
    }
}
