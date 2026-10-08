//! The loaded fonts, which of them draws each character, and their glyphs
//! filled in: outlines into coverage, or a colour image when the font has one.

use std::collections::HashMap;
use std::sync::Arc;

use fontique::Blob;
use harfrust::{ShaperData, ShaperInstance, Variation};
use skrifa::instance::{Location, Size};
use skrifa::outline::{
    DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlinePen, SmoothMode,
    Target,
};
use skrifa::{FontRef, GlyphId, MetadataProvider, Tag};
use zeno::{Command, Mask as Coverage, Origin, Vector};

use super::atlas::Format;
use super::color;
use super::ink::Bitmap;
use super::system::SystemFonts;
use super::{Error, FontFile, FontId};

/// Glyphs go through the autohinter in light mode, like fontconfig's
/// `hintslight`, whether or not a font carries its own instructions. It snaps
/// heights to whole pixels, so baselines, the x-height and horizontal stems
/// land on pixel rows, and leaves widths and advances as designed, which is
/// what the shaper places them by.
const HINTING: Target = Target::Smooth {
    mode: SmoothMode::Light,
    symmetric_rendering: true,
    preserve_linear_metrics: true,
};

/// The variation axis that [`Fonts::add`]'s weight sets.
const WEIGHT_AXIS: Tag = Tag::new(b"wght");

#[derive(Default)]
pub(super) struct Fonts {
    /// Indexed by [`FontId`].
    list: Vec<Font>,
    /// For the characters a font and its fallbacks all lack.
    system: SystemFonts,
    /// The system fonts loaded into `list` so far, by their data's id and
    /// index in it.
    loaded: HashMap<(u64, u32), FontId>,
}

pub(super) struct Font {
    data: Blob<u8>,
    /// Which font in `data`, when it's a collection.
    index: u32,
    /// Also set on any system font loaded for the characters this one lacks.
    weight: f32,
    /// Tried in order for the characters this font lacks.
    pub fallbacks: Vec<FontId>,
    /// Whether the system's fonts are tried after `fallbacks`.
    pub system_fallback: bool,
    pub location: Location,
    pub units_per_em: u16,
    /// What the autohinter works out about each glyph before hinting any, so
    /// it's done once per font rather than once per size.
    styles: GlyphStyles,
    /// One per pixel size drawn so far.
    instances: Vec<(u16, HintingInstance)>,
    /// The tables harfrust reads, worked out once.
    pub shaper: ShaperData,
    /// `location` for harfrust.
    pub variations: ShaperInstance,
}

impl Fonts {
    /// Loads `file` with `weight` set on its weight axis, if it has one.
    pub fn add(&mut self, file: FontFile, weight: f32) -> Result<FontId, Error> {
        let font =
            Font::new(Blob::new(Arc::new(file.data)), 0, weight).ok_or(Error::Font(file.name))?;
        self.push(font)
    }

    pub fn get_mut(&mut self, id: FontId) -> Option<&mut Font> {
        self.list.get_mut(usize::from(id.0))
    }

    /// The font and its parsed tables. Parsing again each time only reads
    /// the table directory.
    pub fn face(&self, id: FontId) -> Option<(&Font, FontRef<'_>)> {
        let font = self.list.get(usize::from(id.0))?;
        let face = FontRef::from_index(font.data.as_ref(), font.index).ok()?;
        Some((font, face))
    }

    pub fn has(&self, id: FontId, c: char) -> bool {
        self.face(id)
            .and_then(|(_, face)| face.charmap().map(c))
            .is_some()
    }

    /// `font` if it has `c`, else the first of its fallbacks that does, else
    /// the system's font for `c` when `font` allows, loaded at `font`'s
    /// weight. `font` again when none does, so `c` is shaped as its missing
    /// glyph.
    pub fn for_char(&mut self, c: char, font: FontId) -> FontId {
        if self.has(font, c) {
            return font;
        }
        self.fallback(c, font).unwrap_or(font)
    }

    /// Glyph `id` in `font` at `pixels` per em. `None` for
    /// the font's own missing glyph too, so every font's missing characters
    /// look the same.
    pub fn rasterize(&mut self, font: FontId, id: GlyphId, pixels: u16) -> Option<Bitmap> {
        if id == GlyphId::NOTDEF {
            return None;
        }
        self.get_mut(font)?.rasterize(id, pixels)
    }

    fn push(&mut self, font: Font) -> Result<FontId, Error> {
        let id = FontId(u16::try_from(self.list.len()).map_err(|_| Error::TooMany)?);
        self.list.push(font);
        Ok(id)
    }

    fn fallback(&mut self, c: char, font: FontId) -> Option<FontId> {
        let drawn = self.list.get(usize::from(font.0))?;
        let bundled = drawn
            .fallbacks
            .iter()
            .copied()
            .find(|&fallback| self.has(fallback, c));
        if bundled.is_some() || !drawn.system_fallback {
            return bundled;
        }
        let weight = drawn.weight;
        let found = self.system.find(c)?;
        let key = (found.data.id(), found.index);
        if let Some(&fallback) = self.loaded.get(&key) {
            return Some(fallback);
        }
        let fallback = self
            .push(Font::new(found.data, found.index, weight)?)
            .ok()?;
        self.loaded.insert(key, fallback);
        Some(fallback)
    }
}

impl Font {
    /// `None` if `data` doesn't parse.
    fn new(data: Blob<u8>, index: u32, weight: f32) -> Option<Self> {
        let face = FontRef::from_index(data.as_ref(), index).ok()?;
        let location = face.axes().location([(WEIGHT_AXIS, weight)]);
        let units_per_em = face.metrics(Size::unscaled(), &location).units_per_em;
        let styles = GlyphStyles::new(&face.outline_glyphs());
        let shaper = ShaperData::new(&face);
        let variations = ShaperInstance::from_variations(
            &face,
            [Variation {
                tag: WEIGHT_AXIS,
                value: weight,
            }],
        );
        Some(Self {
            data,
            index,
            weight,
            fallbacks: Vec::new(),
            system_fallback: true,
            location,
            units_per_em,
            styles,
            instances: Vec::new(),
            shaper,
            variations,
        })
    }

    /// The glyph's colour image if the font has one, else its outline. Empty
    /// for a blank glyph, `None` for one with neither.
    fn rasterize(&mut self, id: GlyphId, pixels: u16) -> Option<Bitmap> {
        let Self {
            data,
            index,
            location,
            styles,
            instances,
            ..
        } = self;
        let face = FontRef::from_index(data.as_ref(), *index).ok()?;
        if let Some(image) = color::rasterize(&face, id, pixels) {
            return Some(image);
        }
        let outlines = face.outline_glyphs();
        let glyph = outlines.get(id)?;
        let at = if let Some(at) = instances.iter().position(|&(size, _)| size == pixels) {
            at
        } else {
            let options = HintingOptions {
                engine: Engine::Auto(Some(styles.clone())),
                target: HINTING,
            };
            let size = Size::new(f32::from(pixels));
            let instance = HintingInstance::new(&outlines, size, &*location, options).ok()?;
            instances.push((pixels, instance));
            instances.len() - 1
        };
        let (_, instance) = instances.get(at)?;
        let mut path = Path::default();
        glyph
            .draw(DrawSettings::hinted(instance, false), &mut path)
            .ok()?;
        let (coverage, placement) = Coverage::new(&path.0[..])
            .format(zeno::Format::Alpha)
            .origin(Origin::BottomLeft)
            .render();
        let height = u16::try_from(placement.height).ok()?;
        Some(Bitmap {
            format: Format::Coverage,
            width: u16::try_from(placement.width).ok()?,
            height,
            left: i16::try_from(placement.left).ok()?,
            // With a bottom-left origin and no size set beforehand, zeno's
            // `top` is the mask's bottom edge, so the height makes it the top.
            top: i16::try_from(placement.top.checked_add(i32::from(height))?).ok()?,
            texels: coverage,
        })
    }
}

/// Collects a glyph's outline for zeno to fill.
#[derive(Default)]
struct Path(Vec<Command>);

impl OutlinePen for Path {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::MoveTo(Vector::new(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push(Command::LineTo(Vector::new(x, y)));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0
            .push(Command::QuadTo(Vector::new(cx0, cy0), Vector::new(x, y)));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.push(Command::CurveTo(
            Vector::new(cx0, cy0),
            Vector::new(cx1, cy1),
            Vector::new(x, y),
        ));
    }

    fn close(&mut self) {
        self.0.push(Command::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{JP, MONO, SANS};
    use super::*;

    #[test]
    fn glyphs_sit_on_the_baseline() {
        let mut fonts = Fonts::default();
        let sans = fonts.add(SANS, 400.).unwrap();
        let mut place = |c| {
            let (_, face) = fonts.face(sans).unwrap();
            let id = face.charmap().map(c).unwrap();
            fonts.rasterize(sans, id, 21).unwrap()
        };
        let x = place('x');
        assert_eq!(i32::from(x.top), i32::from(x.height));
        assert_eq!(place('l').top, place('d').top);
        let g = place('g');
        assert!(i32::from(g.top) < i32::from(g.height));
        assert!(place('.').top < x.top / 2);
    }

    #[test]
    fn fallbacks_are_tried_in_order() {
        let mut fonts = Fonts::default();
        let sans = fonts.add(SANS, 400.).unwrap();
        let mono = fonts.add(MONO, 400.).unwrap();
        let jp = fonts.add(JP, 400.).unwrap();
        let font = fonts.get_mut(mono).unwrap();
        font.fallbacks = vec![sans, jp];
        font.system_fallback = false;
        assert!(!fonts.has(mono, 'ǐ'));
        assert!(fonts.has(jp, 'ǐ'));
        assert_eq!(fonts.for_char('ǐ', mono), sans);
        assert_eq!(fonts.for_char('日', mono), jp);
        assert_eq!(fonts.for_char('🦀', mono), mono);
    }
}
