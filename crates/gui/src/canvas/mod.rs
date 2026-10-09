//! What gets drawn. A pass writes quads and triangles into a [`Canvas`], and
//! [`render`](crate::render) draws it.
//!
//! Most shapes are one [`Quad`], 48 bytes. The shader rounds its corners and
//! draws its border per pixel, and text and icons are quads that sample an atlas. Anything
//! else, such as a curve or an arrow, is indexed triangles of [`Vertex`]es. A
//! new batch starts only where the clip or the kind of shape changes.

mod glyphs;

use std::ops::Range;

pub use glyphs::{
    AtlasUpdate, Error, FontFile, FontId, Format, Glyphs, IconId, Line, LineMetrics, PageWrite,
};

use crate::cast::sixteenths;

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

    /// Whether `[x, y]` is inside, counting the top and left edges but not
    /// the bottom and right, so boxes side by side never both contain it.
    #[must_use]
    pub fn contains(self, [x, y]: [f32; 2]) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    /// Whether both share any area, so touching edges don't count.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
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

/// sRGB, with straight alpha, a byte each. The shaders make it linear
/// before blending.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const TRANSPARENT: Self = Self([0; 4]);

    /// From `0xRRGGBB`, opaque.
    #[must_use]
    pub const fn hex(rgb: u32) -> Self {
        let [_, r, g, b] = rgb.to_be_bytes();
        Self([r, g, b, u8::MAX])
    }

    #[must_use]
    pub const fn alpha(self, a: u8) -> Self {
        let [r, g, b, _] = self.0;
        Self([r, g, b, a])
    }

    /// Linear RGBA, straight alpha, as the shaders blend in.
    #[must_use]
    pub fn linear(self) -> [f32; 4] {
        let [r, g, b, a] = self.0;
        [linear(r), linear(g), linear(b), f32::from(a) / 255.]
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
    /// `x, y, width, height` in texels on `page` of `atlas`. Zero-sized for
    /// a quad that samples nothing.
    texels: [u16; 4],
    /// Top left, top right, bottom right, bottom left, in sixteenths of a
    /// logical pixel.
    radii: [u16; 4],
    fill: Color,
    border: Color,
    border_width: f32,
    page: u16,
    /// A [`Format`]. Coverage scales the fill, and colour takes only its
    /// alpha.
    atlas: u16,
}

// The render pipeline's attribute offsets follow from this layout.
const _: () = assert!(size_of::<Quad>() == 48);

impl Quad {
    #[must_use]
    pub const fn new(rect: Rect, fill: Color) -> Self {
        Self {
            rect: [rect.x, rect.y, rect.w, rect.h],
            texels: [0; 4],
            radii: [0; 4],
            fill,
            border: Color::TRANSPARENT,
            border_width: 0.,
            page: 0,
            atlas: 0,
        }
    }

    /// `rect` must be the texel area's size in physical pixels, so each pixel
    /// reads the one texel under it.
    pub(crate) const fn sampled(
        rect: Rect,
        atlas: Format,
        page: u16,
        texels: [u16; 4],
        fill: Color,
    ) -> Self {
        let mut quad = Self::new(rect, fill);
        quad.texels = texels;
        quad.page = page;
        quad.atlas = atlas as u16;
        quad
    }

    #[must_use]
    pub const fn rounded(self, radius: f32) -> Self {
        self.corners([radius; 4])
    }

    /// Top left, top right, bottom right, bottom left. Kept to a sixteenth of
    /// a pixel.
    #[must_use]
    pub const fn corners(mut self, [a, b, c, d]: [f32; 4]) -> Self {
        self.radii = [sixteenths(a), sixteenths(b), sixteenths(c), sixteenths(d)];
        self
    }

    /// Shifted `[x, y]` right and down.
    #[must_use]
    pub(crate) const fn moved(mut self, [x, y]: [f32; 2]) -> Self {
        self.rect[0] += x;
        self.rect[1] += y;
        self
    }

    /// Drawn inside the rect, so a border never changes a quad's size.
    #[must_use]
    pub const fn bordered(mut self, width: f32, color: Color) -> Self {
        self.border_width = width;
        self.border = color;
        self
    }
}

/// One corner of a triangle. The colour is interpolated across it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub at: [f32; 2],
    pub color: Color,
}

impl Vertex {
    #[must_use]
    pub const fn new(at: [f32; 2], color: Color) -> Self {
        Self { at, color }
    }
}

/// What a batch draws, so the renderer picks the pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Its range is into [`Canvas::quads`].
    Quads = 0,
    /// Its range is into [`Canvas::indices`].
    Triangles = 1,
}

/// A pass's shapes in draw order, split where the clip or the kind changes.
#[derive(Debug, Default)]
pub struct Canvas {
    /// What the surface is cleared to before any shape.
    pub background: Color,
    quads: Vec<Quad>,
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    batches: Vec<Batch>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Batch {
    clip: Rect,
    kind: Kind,
    /// Where each kind's shapes were when the batch started, indexed by
    /// [`Kind`], so the next batch ends this one whatever its kind.
    starts: [usize; 2],
}

impl Canvas {
    /// Empties the canvas for a new pass, clipped to `root`. Keeps its
    /// allocations, so a warm pass allocates nothing.
    pub fn clear(&mut self, root: Rect) {
        self.quads.clear();
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
        self.batches.push(Batch {
            clip: root,
            kind: Kind::Quads,
            starts: [0; 2],
        });
    }

    /// Cuts off every shape after this one at `clip`, until the next call.
    pub fn clip(&mut self, clip: Rect) {
        if let Some(last) = self.batches.last() {
            self.open(clip, last.kind);
        }
    }

    pub fn quad(&mut self, quad: Quad) {
        self.switch(Kind::Quads);
        self.quads.push(quad);
    }

    /// Every three indices are one triangle, indexing into `vertices`.
    /// Triangles drawn one after another under the same clip share a batch.
    pub fn triangles(&mut self, vertices: &[Vertex], indices: &[u32]) {
        self.triangles_at([0., 0.], vertices, indices);
    }

    /// As [`Self::triangles`], with each vertex shifted by `origin`.
    pub(crate) fn triangles_at(&mut self, origin: [f32; 2], vertices: &[Vertex], indices: &[u32]) {
        self.switch(Kind::Triangles);
        // A canvas past u32::MAX vertices is refused by the renderer, so a
        // saturated index is never drawn.
        let base = u32::try_from(self.vertices.len()).unwrap_or(u32::MAX);
        let [x, y] = origin;
        self.vertices.extend(vertices.iter().map(|vertex| Vertex {
            at: [vertex.at[0] + x, vertex.at[1] + y],
            ..*vertex
        }));
        self.indices
            .extend(indices.iter().map(|&i| base.saturating_add(i)));
    }

    #[must_use]
    pub fn quads(&self) -> &[Quad] {
        &self.quads
    }

    #[must_use]
    pub fn vertices(&self) -> &[Vertex] {
        &self.vertices
    }

    /// Already offset to index into all of [`Canvas::vertices`].
    #[must_use]
    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    /// Each clip with the kind and range of shapes it cuts, in draw order,
    /// skipping empty ones.
    pub fn batches(&self) -> impl Iterator<Item = (Rect, Kind, Range<usize>)> {
        let ends = self
            .batches
            .iter()
            .skip(1)
            .map(|next| next.starts)
            .chain([self.lens()]);
        self.batches
            .iter()
            .zip(ends)
            .map(|(batch, ends)| {
                let at = batch.kind as usize;
                (batch.clip, batch.kind, batch.starts[at]..ends[at])
            })
            .filter(|(_, _, range)| !range.is_empty())
    }

    /// Indexed by [`Kind`].
    fn lens(&self) -> [usize; 2] {
        [self.quads.len(), self.indices.len()]
    }

    /// Keeps the clip and starts a batch of `kind`, unless the last one is.
    fn switch(&mut self, kind: Kind) {
        if let Some(last) = self.batches.last() {
            self.open(last.clip, kind);
        }
    }

    fn open(&mut self, clip: Rect, kind: Kind) {
        let batch = Batch {
            clip,
            kind,
            starts: self.lens(),
        };
        match self.batches.last() {
            Some(last) if last.clip == clip && last.kind == kind => {}
            // Nothing was drawn in the last batch, so it's replaced, not split.
            Some(last) if last.starts == batch.starts => {
                self.batches.pop();
                self.batches.push(batch);
            }
            _ => self.batches.push(batch),
        }
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

    fn triangle() -> ([Vertex; 3], [u32; 3]) {
        let vertex = Vertex::new([0.; 2], Color::TRANSPARENT);
        ([vertex; 3], [0, 1, 2])
    }

    fn batches(canvas: &Canvas) -> Vec<(Rect, Kind, Range<usize>)> {
        canvas.batches().collect()
    }

    #[test]
    fn one_clip_is_one_batch() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.quad(quad());
        canvas.clip(ROOT);
        canvas.quad(quad());
        assert_eq!(batches(&canvas), [(ROOT, Kind::Quads, 0..2)]);
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
        assert_eq!(
            batches(&canvas),
            [
                (ROOT, Kind::Quads, 0..1),
                (SIDE, Kind::Quads, 1..2),
                (ROOT, Kind::Quads, 2..3)
            ]
        );
    }

    #[test]
    fn an_unused_clip_is_replaced() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.clip(SIDE);
        canvas.quad(quad());
        canvas.clip(ROOT);
        assert_eq!(batches(&canvas), [(SIDE, Kind::Quads, 0..1)]);
    }

    #[test]
    fn clear_forgets_the_last_pass() {
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.clip(SIDE);
        canvas.quad(quad());
        canvas.clear(ROOT);
        canvas.quad(quad());
        assert_eq!(batches(&canvas), [(ROOT, Kind::Quads, 0..1)]);
    }

    #[test]
    fn triangles_split_from_quads() {
        let (vertices, indices) = triangle();
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.quad(quad());
        canvas.triangles(&vertices, &indices);
        canvas.quad(quad());
        assert_eq!(
            batches(&canvas),
            [
                (ROOT, Kind::Quads, 0..1),
                (ROOT, Kind::Triangles, 0..3),
                (ROOT, Kind::Quads, 1..2)
            ]
        );
    }

    #[test]
    fn triangles_merge_and_offset() {
        let (vertices, indices) = triangle();
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.triangles(&vertices, &indices);
        canvas.triangles(&vertices, &indices);
        assert_eq!(batches(&canvas), [(ROOT, Kind::Triangles, 0..6)]);
        assert_eq!(canvas.indices(), [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn a_clip_keeps_the_kind() {
        let (vertices, indices) = triangle();
        let mut canvas = Canvas::default();
        canvas.clear(ROOT);
        canvas.triangles(&vertices, &indices);
        canvas.clip(SIDE);
        canvas.triangles(&vertices, &indices);
        assert_eq!(
            batches(&canvas),
            [(ROOT, Kind::Triangles, 0..3), (SIDE, Kind::Triangles, 3..6)]
        );
    }

    #[test]
    fn intersect_without_overlap_is_empty() {
        let far = Rect::new(200., 200., 10., 10.);
        let overlap = ROOT.intersect(far);
        assert_eq!((overlap.w, overlap.h), (0., 0.));
    }

    #[test]
    fn hex_decodes_to_linear() {
        assert_eq!(Color::hex(0xff_ff_ff).linear(), [1., 1., 1., 1.]);
        assert_eq!(Color::hex(0).linear(), [0., 0., 0., 1.]);
        let [grey, ..] = Color::hex(0x80_80_80).linear();
        assert!((grey - 0.216).abs() < 1e-3, "{grey}");
    }

    #[test]
    fn radii_keep_sixteenths() {
        let quad = quad().corners([0.5, 4., 1. / 3., 100_000.]);
        assert_eq!(quad.radii, [8, 64, 5, u16::MAX]);
    }
}
