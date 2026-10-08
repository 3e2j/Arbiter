//! Text and icons, rasterized into the atlas the first time each is drawn at
//! a size.
//!
//! The app hands in its fonts and icons at startup and keeps the ids it gets
//! back, so nothing here knows which fonts or icons exist.
//!
//! A character the font can't draw goes to that font's own fallbacks in
//! order, as in Godot, then to a font installed on the system unless the font
//! turns that off, and whatever still can't be drawn is an empty box, as most
//! text renderers draw it.

use std::collections::HashMap;
use std::sync::Arc;

use fontique::Blob;

use resvg::{tiny_skia, usvg};
use skrifa::instance::{Location, Size};
use skrifa::outline::{
    DrawSettings, Engine, GlyphStyles, HintingInstance, HintingOptions, OutlinePen, SmoothMode,
    Target,
};
use skrifa::{FontRef, GlyphId, MetadataProvider};
use zeno::{Command, Format, Mask as Coverage, Origin, Vector};

use super::atlas::{Atlas, AtlasUpdate};
use super::system::SystemFonts;
use super::{Canvas, Color, Quad, Rect};

/// Glyphs go through the autohinter in light mode, like fontconfig's
/// `hintslight`, whether or not a font carries its own instructions. It snaps
/// heights to whole pixels, so baselines, the x-height and horizontal stems
/// land on pixel rows, and leaves widths and advances as designed.
const HINTING: Target = Target::Smooth {
    mode: SmoothMode::Light,
    symmetric_rendering: true,
    preserve_linear_metrics: true,
};

/// How tall the box for a character that couldn't be drawn is, as a share of
/// the text size, so it sits on the baseline about as tall as a capital.
const BOX_HEIGHT: f32 = 0.7;
/// The gap around that box, as a share of its character's advance or its
/// icon's square.
const BOX_INSET: f32 = 0.12;

/// The variation axis that [`Glyphs::add_font`]'s weight sets.
const WEIGHT_AXIS: &str = "wght";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the font {0} doesn't parse")]
    Font(&'static str),

    #[error("the icon {0} doesn't parse: {1}")]
    Icon(&'static str, usvg::Error),

    #[error("too many fonts or icons")]
    TooMany,
}

/// A font from [`Glyphs::add_font`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct FontId(u16);

/// An icon from [`Glyphs::add_icon`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct IconId(u16);

/// A font's file, and the name an [`Error::Font`] reports it by.
#[derive(Clone, Copy, Debug)]
pub struct FontFile {
    pub name: &'static str,
    pub data: &'static [u8],
}

/// How a line of text sits on its baseline, in logical pixels.
#[derive(Clone, Copy, Debug)]
pub struct LineMetrics {
    /// From the baseline up to the top of the tallest glyphs.
    pub ascent: f32,
    /// From the baseline down to the bottom of the lowest glyphs.
    pub descent: f32,
}

/// Draws text and icons as quads sampling the atlas. Masks are placed on
/// whole physical pixels, so text stays sharp when the position it's drawn at
/// is on one too.
pub struct Glyphs {
    atlas: Atlas,
    /// Physical pixels per logical pixel.
    scale: f32,
    /// Indexed by [`FontId`].
    fonts: Vec<Font>,
    /// For the characters a font and its fallbacks all lack.
    system: SystemFonts,
    /// The system fonts loaded into `fonts` so far, by their data's id and
    /// index in it.
    system_fonts: HashMap<(u64, u32), FontId>,
    /// Indexed by [`IconId`].
    icons: Vec<usvg::Tree>,
    cache: HashMap<Key, Glyph>,
}

struct Font {
    data: Blob<u8>,
    /// Which font in `data`, when it's a collection.
    index: u32,
    /// Also set on any system font loaded for the characters this one lacks.
    weight: f32,
    /// Tried in order for the characters this font lacks.
    fallbacks: Vec<FontId>,
    /// Whether the system's fonts are tried after `fallbacks`.
    system_fallback: bool,
    location: Location,
    /// What the autohinter works out about each glyph before hinting any, so
    /// it's done once per font rather than once per size.
    styles: GlyphStyles,
    /// One per physical pixel size drawn so far, keyed by the size's bits.
    instances: Vec<(u32, HintingInstance)>,
}

/// Sizes are stored as bits, since `f32` isn't `Hash`. They're whole physical
/// pixels, so equal sizes have equal bits.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key {
    Char(char, u32, FontId),
    /// A glyph of a fallback font. Every font that lacks a character shares
    /// it, so its mask is packed once.
    Fallback(FontId, GlyphId, u32),
    Icon(IconId, u32),
}

/// What a cached glyph or icon needs to be drawn, in physical pixels.
#[derive(Clone, Copy)]
struct Glyph {
    advance: f32,
    ink: Ink,
}

#[derive(Clone, Copy)]
enum Ink {
    /// Such as a space.
    Blank,
    Mask(Mask),
    /// Drawn as an empty box in the text's colour: a character `advance`
    /// wide standing on the baseline, or inset in an icon's square.
    Missing,
}

#[derive(Clone, Copy)]
struct Mask {
    /// `x, y, width, height` in atlas texels.
    texels: [u16; 4],
    /// From the pen position on the baseline to the mask's top left corner,
    /// `y` growing up.
    left: f32,
    top: f32,
}

impl Default for Glyphs {
    fn default() -> Self {
        Self {
            atlas: Atlas::default(),
            scale: 1.,
            fonts: Vec::new(),
            system: SystemFonts::default(),
            system_fonts: HashMap::new(),
            icons: Vec::new(),
            cache: HashMap::new(),
        }
    }
}

impl Glyphs {
    /// Loads `file` with `weight` set on its weight axis, if it has one.
    ///
    /// # Errors
    ///
    /// If the font doesn't parse.
    pub fn add_font(&mut self, file: FontFile, weight: f32) -> Result<FontId, Error> {
        let font =
            Font::new(Blob::new(Arc::new(file.data)), 0, weight).ok_or(Error::Font(file.name))?;
        self.push(font)
    }

    /// Draws the characters `font` lacks in the first of `fallbacks` that has
    /// them. Only `font`'s own list is tried, not its fallbacks' lists.
    pub fn set_fallbacks(&mut self, font: FontId, fallbacks: &[FontId]) {
        if let Some(font) = self.fonts.get_mut(usize::from(font.0)) {
            font.fallbacks = fallbacks.to_vec();
            self.cache.clear();
        }
    }

    /// Whether the characters `font` and its fallbacks all lack are drawn in
    /// a font installed on the system, the one the system picks for the
    /// character's script. On by default. What that font looks like, or
    /// whether there is one, differs by machine.
    ///
    /// The system's list is read the first time a character needs it, which
    /// can hold up that frame.
    pub fn set_system_fallback(&mut self, font: FontId, on: bool) {
        if let Some(font) = self.fonts.get_mut(usize::from(font.0)) {
            font.system_fallback = on;
            self.cache.clear();
        }
    }

    /// Loads an SVG icon. `name` is what an [`Error::Icon`] reports it by.
    ///
    /// # Errors
    ///
    /// If the SVG doesn't parse.
    pub fn add_icon(&mut self, name: &'static str, svg: &[u8]) -> Result<IconId, Error> {
        let id = IconId(u16::try_from(self.icons.len()).map_err(|_| Error::TooMany)?);
        let tree = usvg::Tree::from_data(svg, &usvg::Options::default())
            .map_err(|error| Error::Icon(name, error))?;
        self.icons.push(tree);
        Ok(id)
    }

    /// Switches to drawing at `scale` physical pixels per logical pixel,
    /// dropping every mask when it changes.
    pub(crate) fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() < f32::EPSILON {
            return;
        }
        self.scale = scale;
        self.cache.clear();
        self.atlas.clear();
    }

    /// Makes the next update hold the whole atlas, for a GPU that starts empty.
    pub(crate) fn reupload(&mut self) {
        self.atlas.reupload();
    }

    /// The atlas changes since the last call, for the GPU to upload.
    pub(crate) fn take_update(&mut self) -> Option<AtlasUpdate<'_>> {
        self.atlas.take_update()
    }

    /// How `font` sits on its baseline at `size`. An em above the baseline
    /// for a font that isn't loaded.
    #[must_use]
    pub fn line_metrics(&self, font: FontId, size: f32) -> LineMetrics {
        let Some((font, face)) = self.font(font).and_then(|font| Some((font, font.face()?))) else {
            return LineMetrics {
                ascent: size,
                descent: 0.,
            };
        };
        let metrics = face.metrics(Size::new(size), &font.location);
        LineMetrics {
            ascent: metrics.ascent,
            // The font stores it as a distance below the baseline.
            descent: -metrics.descent,
        }
    }

    /// Draws `text` in `font` at `size` logical pixels in one line, starting
    /// at `x` on the baseline `y`. Returns where the line ends.
    pub fn text(
        &mut self,
        canvas: &mut Canvas,
        font: FontId,
        [x, y]: [f32; 2],
        size: f32,
        text: &str,
        color: Color,
    ) -> f32 {
        let pixels = (size * self.scale).round();
        let mut pen = self.to_physical(x);
        let baseline = self.to_physical(y);
        for c in text.chars() {
            let glyph = self.glyph(Key::Char(c, pixels.to_bits(), font));
            match glyph.ink {
                Ink::Blank => {}
                // The pen keeps its fraction so advances don't drift, but each
                // mask lands on a whole pixel, where its texels map one to one.
                Ink::Mask(mask) => {
                    let left = (pen + mask.left).round();
                    canvas.quad(self.masked(mask, [left, baseline - mask.top], color));
                }
                Ink::Missing => {
                    let inset = (glyph.advance * BOX_INSET).round();
                    let height = (pixels * BOX_HEIGHT).round();
                    let rect = [
                        pen.round() + inset,
                        baseline - height,
                        glyph.advance.round() - inset * 2.,
                        height,
                    ];
                    canvas.quad(self.missing_box(rect, color));
                }
            }
            pen += glyph.advance;
        }
        pen / self.scale
    }

    /// How far [`Self::text`] moves the pen for `text` in `font` at `size`,
    /// in logical pixels.
    pub fn text_width(&mut self, font: FontId, size: f32, text: &str) -> f32 {
        let pixels = (size * self.scale).round();
        let advance: f32 = text
            .chars()
            .map(|c| self.glyph(Key::Char(c, pixels.to_bits(), font)).advance)
            .sum();
        advance / self.scale
    }

    /// Draws `icon` in a square `size` logical pixels wide, top left at
    /// `corner`.
    pub fn icon(
        &mut self,
        canvas: &mut Canvas,
        icon: IconId,
        corner: [f32; 2],
        size: f32,
        color: Color,
    ) {
        let pixels = (size * self.scale).round();
        let glyph = self.glyph(Key::Icon(icon, pixels.to_bits()));
        let [x, y] = corner.map(|value| self.to_physical(value));
        match glyph.ink {
            Ink::Blank => {}
            Ink::Mask(mask) => canvas.quad(self.masked(mask, [x, y], color)),
            Ink::Missing => {
                let inset = (pixels * BOX_INSET).round();
                let inner = pixels - inset * 2.;
                canvas.quad(self.missing_box([x + inset, y + inset, inner, inner], color));
            }
        }
    }

    fn push(&mut self, font: Font) -> Result<FontId, Error> {
        let id = FontId(u16::try_from(self.fonts.len()).map_err(|_| Error::TooMany)?);
        self.fonts.push(font);
        Ok(id)
    }

    fn font(&self, id: FontId) -> Option<&Font> {
        self.fonts.get(usize::from(id.0))
    }

    /// Rounds a logical position to the nearest physical pixel, in physical
    /// pixels.
    fn to_physical(&self, value: f32) -> f32 {
        (value * self.scale).round()
    }

    /// A quad for `mask` with its top left at `corner` in physical pixels.
    fn masked(&self, mask: Mask, [x, y]: [f32; 2], color: Color) -> Quad {
        let [u, v, width, height] = mask.texels.map(f32::from);
        let scale = self.scale;
        let rect = Rect::new(x / scale, y / scale, width / scale, height / scale);
        Quad::sampled(rect, [u, v, width, height], color)
    }

    /// An outline one physical pixel wide around `[x, y, width, height]` in
    /// physical pixels.
    fn missing_box(&self, [x, y, width, height]: [f32; 4], color: Color) -> Quad {
        let scale = self.scale;
        let rect = Rect::new(x / scale, y / scale, width / scale, height / scale);
        Quad::new(rect, Color::TRANSPARENT).bordered(1. / scale, color)
    }

    fn glyph(&mut self, key: Key) -> Glyph {
        if let Some(&glyph) = self.cache.get(&key) {
            return glyph;
        }
        let glyph = match key {
            Key::Char(c, pixels, font) => self.rasterize_char(c, f32::from_bits(pixels), font),
            Key::Fallback(font, id, pixels) => {
                self.rasterize_fallback(font, id, f32::from_bits(pixels))
            }
            Key::Icon(icon, pixels) => self.rasterize_icon(icon, f32::from_bits(pixels)),
        };
        self.cache.insert(key, glyph);
        glyph
    }

    /// Draws `c` from `font`, else one of its fallbacks, else as [`Ink::Missing`] as
    /// wide as `font`'s missing glyph.
    fn rasterize_char(&mut self, c: char, pixels: f32, font: FontId) -> Glyph {
        let drawn = self.fonts.get_mut(usize::from(font.0)).and_then(|drawn| {
            let id = drawn.glyph_id(c)?;
            drawn.draw(id, pixels)
        });
        if let Some((advance, raster)) = drawn {
            return self.glyph_from(advance, raster);
        }
        if let Some((fallback, id)) = self.fallback_for(c, font) {
            return self.glyph(Key::Fallback(fallback, id, pixels.to_bits()));
        }
        let advance = self
            .font(font)
            .map_or(pixels / 2., |font| font.advance(GlyphId::NOTDEF, pixels));
        Glyph {
            advance,
            ink: Ink::Missing,
        }
    }

    /// The first of `font`'s fallbacks that has `c`, else the system's font
    /// for it when `font` allows, loaded at the weight of the font that first
    /// needed it.
    fn fallback_for(&mut self, c: char, font: FontId) -> Option<(FontId, GlyphId)> {
        let drawn = self.font(font)?;
        let bundled = drawn
            .fallbacks
            .iter()
            .find_map(|&fallback| Some((fallback, self.font(fallback)?.glyph_id(c)?)));
        if bundled.is_some() || !drawn.system_fallback {
            return bundled;
        }
        let weight = drawn.weight;
        let found = self.system.find(c)?;
        let key = (found.data.id(), found.index);
        let fallback = if let Some(&fallback) = self.system_fonts.get(&key) {
            fallback
        } else {
            let fallback = self
                .push(Font::new(found.data, found.index, weight)?)
                .ok()?;
            self.system_fonts.insert(key, fallback);
            fallback
        };
        Some((fallback, self.font(fallback)?.glyph_id(c)?))
    }

    fn rasterize_fallback(&mut self, font: FontId, id: GlyphId, pixels: f32) -> Glyph {
        let drawn = self
            .fonts
            .get_mut(usize::from(font.0))
            .and_then(|font| font.draw(id, pixels));
        match drawn {
            Some((advance, raster)) => self.glyph_from(advance, raster),
            None => Glyph {
                advance: pixels / 2.,
                ink: Ink::Missing,
            },
        }
    }

    /// Packs `raster` into the atlas.
    fn glyph_from(&mut self, advance: f32, raster: Raster) -> Glyph {
        let (coverage, placement) = raster;
        let ink = match (
            u16::try_from(placement.width),
            u16::try_from(placement.height),
            i16::try_from(placement.left),
            i16::try_from(placement.top),
        ) {
            (Ok(width), Ok(height), Ok(left), Ok(top)) => {
                self.pack(width, height, &coverage, left.into(), top.into())
            }
            _ => Ink::Missing,
        };
        Glyph { advance, ink }
    }

    fn rasterize_icon(&mut self, icon: IconId, pixels: f32) -> Glyph {
        let missing = Glyph {
            advance: pixels,
            ink: Ink::Missing,
        };
        let Some(tree) = self.icons.get(usize::from(icon.0)) else {
            return missing;
        };
        let Some(size) = usvg::Size::from_wh(pixels, pixels) else {
            return missing;
        };
        let size = size.to_int_size();
        let Some(mut pixmap) = tiny_skia::Pixmap::new(size.width(), size.height()) else {
            return missing;
        };
        let units = tree.size();
        let transform =
            tiny_skia::Transform::from_scale(pixels / units.width(), pixels / units.height());
        resvg::render(tree, transform, &mut pixmap.as_mut());
        // Premultiplied, so alpha alone is the coverage.
        let coverage: Vec<u8> = pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect();
        let (Ok(width), Ok(height)) = (u16::try_from(size.width()), u16::try_from(size.height()))
        else {
            return missing;
        };
        Glyph {
            ink: self.pack(width, height, &coverage, 0., 0.),
            ..missing
        }
    }

    /// [`Ink::Missing`] once the atlas is full.
    fn pack(&mut self, width: u16, height: u16, coverage: &[u8], left: f32, top: f32) -> Ink {
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

impl Font {
    /// `None` if `data` doesn't parse.
    fn new(data: Blob<u8>, index: u32, weight: f32) -> Option<Self> {
        let face = FontRef::from_index(data.as_ref(), index).ok()?;
        let location = face.axes().location([(WEIGHT_AXIS, weight)]);
        let styles = GlyphStyles::new(&face.outline_glyphs());
        Some(Self {
            data,
            index,
            weight,
            fallbacks: Vec::new(),
            system_fallback: true,
            location,
            styles,
            instances: Vec::new(),
        })
    }

    /// Parsed again each time, which only reads its table directory. `None`
    /// never happens, since [`Self::new`] parsed it once.
    fn face(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.data.as_ref(), self.index).ok()
    }

    fn glyph_id(&self, c: char) -> Option<GlyphId> {
        self.face()?.charmap().map(c)
    }

    /// How far glyph `id` moves the pen at `pixels` physical pixels per em,
    /// and its [`Self::rasterize`]d coverage. `None` when it can't be drawn.
    fn draw(&mut self, id: GlyphId, pixels: f32) -> Option<(f32, Raster)> {
        Some((self.advance(id, pixels), self.rasterize(id, pixels)?))
    }

    fn advance(&self, id: GlyphId, pixels: f32) -> f32 {
        self.face()
            .and_then(|face| {
                face.glyph_metrics(Size::new(pixels), &self.location)
                    .advance_width(id)
            })
            .unwrap_or_default()
    }

    /// The coverage of glyph `id` at `pixels` physical pixels per em, and
    /// where it sits from the pen position on the baseline, `y` growing up.
    /// Empty for a blank glyph.
    fn rasterize(&mut self, id: GlyphId, pixels: f32) -> Option<Raster> {
        let Self {
            data,
            index,
            location,
            styles,
            instances,
            ..
        } = self;
        let outlines = FontRef::from_index(data.as_ref(), *index)
            .ok()?
            .outline_glyphs();
        let glyph = outlines.get(id)?;
        let key = pixels.to_bits();
        let at = if let Some(at) = instances.iter().position(|&(size, _)| size == key) {
            at
        } else {
            let options = HintingOptions {
                engine: Engine::Auto(Some(styles.clone())),
                target: HINTING,
            };
            let instance =
                HintingInstance::new(&outlines, Size::new(pixels), &*location, options).ok()?;
            instances.push((key, instance));
            instances.len() - 1
        };
        let (_, instance) = instances.get(at)?;
        let mut path = Path::default();
        glyph
            .draw(DrawSettings::hinted(instance, false), &mut path)
            .ok()?;
        let (coverage, mut placement) = Coverage::new(&path.0[..])
            .format(Format::Alpha)
            .origin(Origin::BottomLeft)
            .render();
        // With a bottom-left origin and no size set beforehand, zeno's `top`
        // is the mask's bottom edge, so the height is added to make it the top.
        placement.top += i32::try_from(placement.height).ok()?;
        Some((coverage, placement))
    }
}

/// A glyph's coverage rows and where they sit.
type Raster = (Vec<u8>, zeno::Placement);

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
    use super::*;

    /// A file under the workspace's `assets/`.
    macro_rules! asset {
        ($path:expr) => {
            include_bytes!(concat!("../../../../assets/", $path))
        };
    }

    const SANS: FontFile = FontFile {
        name: "Noto Sans",
        data: asset!("fonts/noto-sans/NotoSans[wdth,wght].ttf"),
    };
    const MONO: FontFile = FontFile {
        name: "JetBrains Mono",
        data: asset!("fonts/jetbrains-mono/JetBrainsMono[wght].ttf"),
    };
    const JP: FontFile = FontFile {
        name: "Noto Sans JP",
        data: asset!("fonts/noto-sans-jp/NotoSansJP[wght].ttf"),
    };
    const WHITE: Color = Color([1.; 4]);

    struct Loaded {
        glyphs: Glyphs,
        sans: FontId,
        mono: FontId,
    }

    fn loaded(scale: f32) -> Loaded {
        let mut glyphs = Glyphs::default();
        glyphs.set_scale(scale);
        let sans = glyphs.add_font(SANS, 400.).unwrap();
        let mono = glyphs.add_font(MONO, 400.).unwrap();
        let jp = glyphs.add_font(JP, 400.).unwrap();
        glyphs.set_fallbacks(sans, &[jp]);
        glyphs.set_fallbacks(mono, &[sans, jp]);
        // What the system has differs by machine.
        for font in [sans, mono] {
            glyphs.set_system_fallback(font, false);
        }
        Loaded { glyphs, sans, mono }
    }

    fn canvas() -> Canvas {
        let mut canvas = Canvas::default();
        canvas.clear(Rect::new(0., 0., 100., 100.));
        canvas
    }

    #[test]
    fn japanese_falls_back() {
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(1.5);
        assert!(glyphs.font(sans).unwrap().glyph_id('日').is_none());
        let mut canvas = canvas();
        glyphs.text(&mut canvas, sans, [0., 20.], 14., "日本語のファイル", WHITE);
        assert_eq!(canvas.quads().len(), 8);
    }

    #[test]
    fn glyphs_land_on_whole_physical_pixels() {
        let scale = 1.5;
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(scale);
        let mut canvas = canvas();
        glyphs.text(&mut canvas, sans, [3.3, 20.], 14., "illuminated", WHITE);
        assert_eq!(canvas.quads().len(), 11);
        for quad in canvas.quads() {
            for value in &quad.rect[..2] {
                let physical = value * scale;
                assert!((physical - physical.round()).abs() < 1e-3, "{physical}");
            }
        }
    }

    #[test]
    fn a_mono_font_keeps_every_character_the_same_width() {
        let Loaded {
            mut glyphs,
            sans,
            mono,
        } = loaded(1.);
        let narrow = glyphs.text_width(mono, 14., "iiii");
        let wide = glyphs.text_width(mono, 14., "WWWW");
        assert!((narrow - wide).abs() < 1e-3, "{narrow} {wide}");
        assert!(glyphs.text_width(sans, 14., "iiii") < narrow);
    }

    #[test]
    fn fonts_share_the_fallback_glyphs() {
        let Loaded {
            mut glyphs,
            sans,
            mono,
        } = loaded(1.);
        glyphs.text_width(sans, 14., "日");
        glyphs.text_width(mono, 14., "日");
        let fallback = glyphs
            .cache
            .keys()
            .filter(|key| matches!(key, Key::Fallback(..)))
            .count();
        assert_eq!(fallback, 1);
    }

    #[test]
    fn fallbacks_are_tried_in_order() {
        let Loaded {
            mut glyphs,
            sans,
            mono,
        } = loaded(1.);
        assert!(glyphs.font(sans).unwrap().glyph_id('ǐ').is_some());
        glyphs.text_width(mono, 14., "ǐ");
        let fallbacks: Vec<_> = glyphs
            .cache
            .keys()
            .filter_map(|key| match key {
                Key::Fallback(font, ..) => Some(*font),
                _ => None,
            })
            .collect();
        assert_eq!(fallbacks, [sans]);
    }

    #[test]
    fn glyphs_sit_on_the_baseline() {
        let Loaded { mut glyphs, .. } = loaded(1.);
        let sans = glyphs.fonts.first_mut().unwrap();
        let mut place = |c| {
            let id = sans.glyph_id(c).unwrap();
            let (_, placement) = sans.rasterize(id, 21.).unwrap();
            placement
        };
        let x = place('x');
        assert_eq!(x.top, x.height.cast_signed());
        assert_eq!(place('l').top, place('d').top);
        let g = place('g');
        assert!(g.top < g.height.cast_signed());
        assert!(place('.').top < x.top / 2);
    }

    #[test]
    fn icons_are_square_masks() {
        let mut glyphs = Glyphs::default();
        let folder = glyphs
            .add_icon("folder", asset!("icons/tabler/folder.svg"))
            .unwrap();
        let mut canvas = canvas();
        glyphs.icon(&mut canvas, folder, [0., 0.], 16., WHITE);
        let [quad] = canvas.quads() else {
            panic!("one quad")
        };
        assert_eq!(quad.rect[2..], [16., 16.]);
    }

    #[test]
    fn a_character_no_font_has_is_a_box() {
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(1.);
        let mut canvas = canvas();
        let end = glyphs.text(&mut canvas, sans, [0., 20.], 14., "🦀한", WHITE);
        let [crab, hangul] = canvas.quads() else {
            panic!("two quads")
        };
        for quad in [crab, hangul] {
            assert_eq!(quad.texels, [0.; 4]);
            assert_eq!(quad.fill, Color::TRANSPARENT.0);
            assert_eq!(quad.border, WHITE.0);
            assert!((quad.rect[1] + quad.rect[3] - 20.).abs() < 1e-3);
        }
        assert!(hangul.rect[0] + hangul.rect[2] < end);
    }

    #[test]
    #[ignore = "needs a system font with Hangul"]
    fn system_fonts_draw_what_no_bundled_font_has() {
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(1.);
        glyphs.set_system_fallback(sans, true);
        let mut canvas = canvas();
        glyphs.text(&mut canvas, sans, [0., 20.], 14., "한국어", WHITE);
        assert_eq!(canvas.quads().len(), 3);
        assert!(canvas.quads().iter().all(|quad| quad.texels[2] > 0.));
    }

    #[test]
    fn an_icon_that_isnt_loaded_is_a_box() {
        let mut glyphs = Glyphs::default();
        let mut canvas = canvas();
        glyphs.icon(&mut canvas, IconId(3), [0., 0.], 16., WHITE);
        let [quad] = canvas.quads() else {
            panic!("one quad")
        };
        assert_eq!(quad.border, WHITE.0);
        let [x, y, w, h] = quad.rect;
        assert!(
            x > 0. && y > 0. && x + w < 16. && y + h < 16.,
            "{:?}",
            quad.rect
        );
    }

    #[test]
    fn a_new_scale_reuploads_the_atlas() {
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(1.);
        glyphs.text_width(sans, 14., "a");
        glyphs.take_update();
        assert!(glyphs.take_update().is_none());
        glyphs.set_scale(2.);
        assert!(glyphs.take_update().is_some());
    }
}
