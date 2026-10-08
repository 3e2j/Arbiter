//! What each glyph and icon looks like at a size, packed into the atlas the
//! first time it's drawn.

use std::collections::HashMap;

use resvg::{tiny_skia, usvg};
use skrifa::GlyphId;

use super::atlas::{Atlas, AtlasUpdate, Format, Spot};
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
    Packed(Packed),
    /// Drawn as an empty box in the text's colour.
    Missing,
}

/// A glyph or icon in an atlas.
#[derive(Clone, Copy)]
pub(super) struct Packed {
    /// Which atlas.
    pub format: Format,
    pub page: u16,
    /// `x, y, width, height` in texels on `page`.
    pub texels: [u16; 4],
    /// From the pen position on the baseline to the image's top left corner,
    /// `y` growing up.
    pub left: i16,
    pub top: i16,
}

struct Waiting {
    bitmap: Bitmap,
    /// Whether it was asked for this frame. One that wasn't is dropped.
    asked: bool,
}

/// A glyph or icon filled in, before it's packed.
pub(super) struct Bitmap {
    pub format: Format,
    pub width: u16,
    pub height: u16,
    /// As [`Packed::left`] and [`Packed::top`].
    pub left: i16,
    pub top: i16,
    /// `height` rows of `width` texels in `format`.
    pub texels: Vec<u8>,
}

pub(super) struct Inks {
    cache: HashMap<Key, Ink>,
    /// Images rasterized while their atlas had no room and nothing it could
    /// evict, so a later frame packs them without rasterizing again.
    waiting: HashMap<Key, Waiting>,
    pub coverage: Atlas,
    pub color: Atlas,
}

impl Default for Inks {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
            waiting: HashMap::new(),
            coverage: Atlas::new(Format::Coverage),
            color: Atlas::new(Format::Color),
        }
    }
}

impl Inks {
    /// Marks the page under a cached image as drawn this frame, so it isn't
    /// evicted while its quads are on screen.
    pub fn get(&mut self, fonts: &mut Fonts, icons: &[usvg::Tree], key: Key) -> Ink {
        if let Some(&ink) = self.cache.get(&key) {
            if let Ink::Packed(packed) = ink {
                self.atlas(packed.format).touch(packed.page);
            }
            return ink;
        }
        let bitmap = match self.waiting.remove(&key) {
            Some(waiting) => Some(waiting.bitmap),
            None => match key {
                Key::Glyph(font, id, pixels) => fonts.rasterize(font, id, pixels),
                Key::Icon(icon, pixels) => icons
                    .get(usize::from(icon.0))
                    .and_then(|tree| rasterize_icon(tree, pixels)),
            },
        };
        let Some(bitmap) = bitmap else {
            self.cache.insert(key, Ink::Missing);
            return Ink::Missing;
        };
        // Every page was drawn this frame. A box until one wasn't.
        let Some(ink) = self.pack(&bitmap) else {
            let asked = true;
            self.waiting.insert(key, Waiting { bitmap, asked });
            return Ink::Missing;
        };
        self.cache.insert(key, ink);
        ink
    }

    /// Drops the waiting images last frame didn't ask for.
    pub fn next_frame(&mut self) {
        self.waiting
            .retain(|_, waiting| std::mem::take(&mut waiting.asked));
        self.coverage.next_frame();
        self.color.next_frame();
    }

    /// Forgets every image, for a new scale.
    pub fn clear(&mut self) {
        self.cache.clear();
        self.waiting.clear();
        self.coverage.clear();
        self.color.clear();
    }

    /// Makes the next updates hold every page, for a GPU whose copy is gone.
    pub fn reupload(&mut self) {
        self.coverage.reupload();
        self.color.reupload();
    }

    /// What changed in either atlas since the last call.
    pub fn take_updates(&mut self) -> impl Iterator<Item = AtlasUpdate<'_>> {
        [self.coverage.take_update(), self.color.take_update()]
            .into_iter()
            .flatten()
    }

    #[cfg(test)]
    pub fn keys(&self) -> impl Iterator<Item = Key> {
        self.cache.keys().copied()
    }

    fn atlas(&mut self, format: Format) -> &mut Atlas {
        match format {
            Format::Coverage => &mut self.coverage,
            Format::Color => &mut self.color,
        }
    }

    /// [`Ink::Missing`] for an image bigger than a page, and `None` when no
    /// page of its atlas has room and every one was drawn this frame.
    fn pack(&mut self, bitmap: &Bitmap) -> Option<Ink> {
        let Bitmap {
            format,
            width,
            height,
            left,
            top,
            ref texels,
        } = *bitmap;
        if width == 0 || height == 0 {
            return Some(Ink::Blank);
        }
        if !Atlas::fits(width, height) {
            return Some(Ink::Missing);
        }
        loop {
            if let Some(Spot { page, x, y }) = self.atlas(format).insert(width, height, texels) {
                return Some(Ink::Packed(Packed {
                    format,
                    page,
                    texels: [x, y, width, height],
                    left,
                    top,
                }));
            }
            let evicted = self.atlas(format).evict()?;
            self.cache.retain(|_, ink| {
                !matches!(ink, Ink::Packed(packed) if packed.format == format && packed.page == evicted)
            });
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
        format: Format::Coverage,
        width: pixels,
        height: pixels,
        left: 0,
        top: 0,
        // Premultiplied, so alpha alone is the coverage.
        texels: pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::atlas::{MAX_PAGES, PAGE_SIZE};
    use super::*;

    fn square() -> [usvg::Tree; 1] {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"><rect width="1" height="1"/></svg>"#;
        [usvg::Tree::from_data(svg, &usvg::Options::default()).unwrap()]
    }

    #[test]
    fn an_evicted_page_forgets_its_inks() {
        let icons = square();
        let mut fonts = Fonts::default();
        let mut inks = Inks::default();
        let old = Key::Icon(IconId(0), 16);
        inks.get(&mut fonts, &icons, old);
        let full = PAGE_SIZE - 1;
        // Fills the rest of the first page, under the old icon's shelf.
        inks.coverage.insert(full, PAGE_SIZE - 18, &[]).unwrap();
        while inks.coverage.insert(full, full, &[]).is_some() {}
        inks.next_frame();
        for page in 1..MAX_PAGES {
            inks.coverage.touch(page);
        }
        inks.next_frame();
        let new = Key::Icon(IconId(0), 32);
        assert!(
            matches!(inks.get(&mut fonts, &icons, new), Ink::Packed(packed) if packed.page == 0)
        );
        assert_eq!(inks.keys().collect::<Vec<_>>(), [new]);
    }

    /// Every page full and drawn this frame.
    fn full_atlas() -> Inks {
        let mut inks = Inks::default();
        let full = PAGE_SIZE - 1;
        while inks.coverage.insert(full, full, &[]).is_some() {}
        inks
    }

    #[test]
    fn a_mask_waits_while_every_page_is_on_screen() {
        let mut fonts = Fonts::default();
        let mut inks = full_atlas();
        let key = Key::Icon(IconId(0), 16);
        assert!(matches!(inks.get(&mut fonts, &square(), key), Ink::Missing));
        assert_eq!(inks.keys().count(), 0);
        inks.next_frame();
        // No icons to rasterize from, so only the waiting mask can be packed.
        assert!(matches!(inks.get(&mut fonts, &[], key), Ink::Packed(_)));
    }

    #[test]
    fn a_waiting_mask_not_asked_for_is_dropped() {
        let mut fonts = Fonts::default();
        let mut inks = full_atlas();
        let key = Key::Icon(IconId(0), 16);
        inks.get(&mut fonts, &square(), key);
        inks.next_frame();
        inks.next_frame();
        assert!(inks.waiting.is_empty());
    }
}
