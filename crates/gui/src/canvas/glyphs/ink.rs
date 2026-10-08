//! What each glyph and icon looks like at a size, packed into the atlas the
//! first time it's drawn.

use std::collections::HashMap;

use resvg::{tiny_skia, usvg};
use skrifa::GlyphId;

use super::atlas::{Atlas, Spot};
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
    pub page: u16,
    /// `x, y, width, height` in texels on `page`.
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
    /// Marks the page under a cached mask as drawn this frame, so it isn't
    /// evicted while its quads are on screen.
    pub fn get(&mut self, fonts: &mut Fonts, icons: &[usvg::Tree], key: Key) -> Ink {
        if let Some(&ink) = self.cache.get(&key) {
            if let Ink::Mask(mask) = ink {
                self.atlas.touch(mask.page);
            }
            return ink;
        }
        let bitmap = match key {
            Key::Glyph(font, id, pixels) => fonts.rasterize(font, id, pixels),
            Key::Icon(icon, pixels) => icons
                .get(usize::from(icon.0))
                .and_then(|tree| rasterize_icon(tree, pixels)),
        };
        let packed = bitmap.map_or(Some(Ink::Missing), |bitmap| self.pack(&bitmap));
        // Not cached when every page was drawn this frame, so it's packed
        // again on a later one.
        let Some(ink) = packed else {
            return Ink::Missing;
        };
        self.cache.insert(key, ink);
        ink
    }

    pub fn next_frame(&mut self) {
        self.atlas.next_frame();
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

    /// [`Ink::Missing`] for a mask bigger than a page, and `None` when no
    /// page has room and every page was drawn this frame.
    fn pack(&mut self, bitmap: &Bitmap) -> Option<Ink> {
        let Bitmap {
            width,
            height,
            left,
            top,
            ref coverage,
        } = *bitmap;
        if width == 0 || height == 0 {
            return Some(Ink::Blank);
        }
        if !Atlas::fits(width, height) {
            return Some(Ink::Missing);
        }
        loop {
            if let Some(Spot { page, x, y }) = self.atlas.insert(width, height, coverage) {
                return Some(Ink::Mask(Mask {
                    page,
                    texels: [x, y, width, height],
                    left,
                    top,
                }));
            }
            let evicted = self.atlas.evict()?;
            self.cache
                .retain(|_, ink| !matches!(ink, Ink::Mask(mask) if mask.page == evicted));
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

#[cfg(test)]
mod tests {
    use super::super::atlas::{MAX_PAGES, PAGE_SIZE};
    use super::*;

    #[test]
    fn an_evicted_page_forgets_its_inks() {
        let square = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1"/></svg>"#;
        let icons = [usvg::Tree::from_data(square, &usvg::Options::default()).unwrap()];
        let mut fonts = Fonts::default();
        let mut inks = Inks::default();
        let old = Key::Icon(IconId(0), 16);
        inks.get(&mut fonts, &icons, old);
        let full = PAGE_SIZE - 1;
        // Fills the rest of the first page, under the old icon's shelf.
        inks.atlas.insert(full, PAGE_SIZE - 18, &[]).unwrap();
        while inks.atlas.insert(full, full, &[]).is_some() {}
        inks.next_frame();
        for page in 1..MAX_PAGES {
            inks.atlas.touch(page);
        }
        inks.next_frame();
        let new = Key::Icon(IconId(0), 32);
        assert!(matches!(inks.get(&mut fonts, &icons, new), Ink::Mask(mask) if mask.page == 0));
        assert_eq!(inks.keys().collect::<Vec<_>>(), [new]);
    }

    #[test]
    fn a_mask_waits_while_every_page_is_on_screen() {
        let square = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1"/></svg>"#;
        let icons = [usvg::Tree::from_data(square, &usvg::Options::default()).unwrap()];
        let mut fonts = Fonts::default();
        let mut inks = Inks::default();
        let full = PAGE_SIZE - 1;
        while inks.atlas.insert(full, full, &[]).is_some() {}
        let key = Key::Icon(IconId(0), 16);
        assert!(matches!(inks.get(&mut fonts, &icons, key), Ink::Missing));
        assert_eq!(inks.keys().count(), 0);
        inks.next_frame();
        assert!(matches!(inks.get(&mut fonts, &icons, key), Ink::Mask(_)));
    }
}
