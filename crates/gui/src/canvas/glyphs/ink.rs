//! What each glyph and icon looks like at a size, packed into the atlas the
//! first time it's drawn.

use std::collections::HashMap;

use resvg::{tiny_skia, usvg};
use skrifa::GlyphId;

use super::atlas::Atlas;
use super::fonts::Fonts;
use super::{FontId, IconId};

/// Sizes are in physical pixels per em, or per side for an icon.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) enum Key {
    /// Every font that falls back to the same font shares its glyphs, so
    /// each mask is packed once.
    Glyph(FontId, GlyphId, u16),
    Icon(IconId, u16),
}

#[derive(Clone, Copy)]
pub(super) enum Ink {
    /// Such as a space.
    Blank,
    Mask(Mask),
    /// Drawn as an empty box in the text's colour.
    Missing,
}

#[derive(Clone, Copy)]
pub(super) struct Mask {
    /// `x, y, width, height` in atlas texels.
    pub texels: [u16; 4],
    /// From the pen position on the baseline to the mask's top left corner,
    /// `y` growing up.
    pub left: i16,
    pub top: i16,
}

/// Coverage filled for a glyph or icon, before it's packed.
pub(super) struct Bitmap {
    pub width: u16,
    pub height: u16,
    /// As [`Mask::left`] and [`Mask::top`].
    pub left: i16,
    pub top: i16,
    /// `height` rows of `width` bytes.
    pub coverage: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Inks {
    cache: HashMap<Key, Ink>,
    pub atlas: Atlas,
}

impl Inks {
    pub fn get(&mut self, fonts: &mut Fonts, icons: &[usvg::Tree], key: Key) -> Ink {
        if let Some(&ink) = self.cache.get(&key) {
            return ink;
        }
        let bitmap = match key {
            Key::Glyph(font, id, pixels) => fonts.rasterize(font, id, pixels),
            Key::Icon(icon, pixels) => icons
                .get(usize::from(icon.0))
                .and_then(|tree| rasterize_icon(tree, pixels)),
        };
        let ink = bitmap.map_or(Ink::Missing, |bitmap| self.pack(&bitmap));
        self.cache.insert(key, ink);
        ink
    }

    /// Forgets every mask, for a new scale.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.atlas.clear();
    }

    #[cfg(test)]
    pub fn keys(&self) -> impl Iterator<Item = Key> {
        self.cache.keys().copied()
    }

    /// [`Ink::Missing`] once the atlas is full.
    fn pack(&mut self, bitmap: &Bitmap) -> Ink {
        let Bitmap {
            width,
            height,
            left,
            top,
            ref coverage,
        } = *bitmap;
        if width == 0 || height == 0 {
            return Ink::Blank;
        }
        match self.atlas.insert(width, height, coverage) {
            Some([x, y]) => Ink::Mask(Mask {
                texels: [x, y, width, height],
                left,
                top,
            }),
            None => Ink::Missing,
        }
    }
}

/// `tree` scaled to a square `pixels` wide.
fn rasterize_icon(tree: &usvg::Tree, pixels: u16) -> Option<Bitmap> {
    let mut pixmap = tiny_skia::Pixmap::new(u32::from(pixels), u32::from(pixels))?;
    let side = f32::from(pixels);
    let units = tree.size();
    let transform = tiny_skia::Transform::from_scale(side / units.width(), side / units.height());
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Some(Bitmap {
        width: pixels,
        height: pixels,
        left: 0,
        top: 0,
        // Premultiplied, so alpha alone is the coverage.
        coverage: pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect(),
    })
}
