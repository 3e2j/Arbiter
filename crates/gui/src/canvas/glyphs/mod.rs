//! Text and icons, rasterized into the atlases the first time each is drawn
//! at a size. Outlines and icons are coverage, tinted by the colour they're
//! drawn in, and glyphs a font stores as colour images, such as emoji, keep
//! their own colours and take only the alpha.
//!
//! The app hands in its fonts and icons at startup and keeps the ids it gets
//! back, so nothing here knows which fonts or icons exist.
//!
//! A line is split into runs, each in the first font that has its characters:
//! the font asked for, then that font's own fallbacks in order, then a font
//! installed on the system unless the font turns that off. Each run is shaped,
//! and a shaped line is kept while it's drawn every frame. Whatever no font
//! has is an empty box.

mod atlas;
mod color;
mod fonts;
mod ink;
mod shape;
mod system;

use resvg::usvg;
use skrifa::MetadataProvider;
use skrifa::instance::Size;

pub use atlas::{AtlasUpdate, Format, PageWrite};
use fonts::Fonts;
use ink::{Ink, Inks, Key};
use shape::Lines;

use super::{Canvas, Color, Quad, Rect};
use crate::cast::pixel;

/// How tall the box for a character that couldn't be drawn is, as a share of
/// the text size, so it sits on the baseline about as tall as a capital.
const BOX_HEIGHT: f32 = 0.7;
/// The gap around that box, as a share of its character's advance or its
/// icon's square.
const BOX_INSET: f32 = 0.12;

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

/// Draws text and icons as quads sampling the atlases. Images are placed on
/// whole physical pixels, so text stays sharp when the position it's drawn at
/// is on one too.
pub struct Glyphs {
    /// Physical pixels per logical pixel.
    scale: f32,
    fonts: Fonts,
    /// Indexed by [`IconId`].
    icons: Vec<usvg::Tree>,
    lines: Lines,
    inks: Inks,
}

impl Default for Glyphs {
    fn default() -> Self {
        Self {
            scale: 1.,
            fonts: Fonts::default(),
            icons: Vec::new(),
            lines: Lines::default(),
            inks: Inks::default(),
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
        self.fonts.add(file, weight)
    }

    /// Draws the characters `font` lacks in the first of `fallbacks` that has
    /// them. Only `font`'s own list is tried, not its fallbacks' lists.
    pub fn set_fallbacks(&mut self, font: FontId, fallbacks: &[FontId]) {
        if let Some(font) = self.fonts.get_mut(font) {
            font.fallbacks = fallbacks.to_vec();
            self.lines.clear();
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
        if let Some(font) = self.fonts.get_mut(font) {
            font.system_fallback = on;
            self.lines.clear();
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
    /// dropping every mask and line when it changes.
    pub(crate) fn set_scale(&mut self, scale: f32) {
        if (self.scale - scale).abs() < f32::EPSILON {
            return;
        }
        self.scale = scale;
        self.inks.clear();
        self.lines.clear();
    }

    /// Starts a frame. The lines last frame drew but this one doesn't are
    /// dropped.
    pub(crate) fn next_frame(&mut self) {
        self.lines.next_frame();
        self.inks.next_frame();
    }

    /// Makes the next updates hold the whole atlases, for a GPU that starts
    /// empty.
    pub(crate) fn reupload(&mut self) {
        self.inks.reupload();
    }

    /// The atlas changes since the last call, for the GPU to upload.
    pub(crate) fn take_updates(&mut self) -> impl Iterator<Item = AtlasUpdate<'_>> {
        self.inks.take_updates()
    }

    /// How `font` sits on its baseline at `size`. An em above the baseline
    /// for a font that isn't loaded.
    #[must_use]
    pub fn line_metrics(&self, font: FontId, size: f32) -> LineMetrics {
        let Some((font, face)) = self.fonts.face(font) else {
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
        let pixels = self.pixels(size);
        let start = self.to_physical(x);
        let baseline = self.to_physical(y);
        let height = (f32::from(pixels) * BOX_HEIGHT).round();
        let line = self.lines.get(&mut self.fonts, font, pixels, text);
        for glyph in &line.glyphs {
            let key = Key::Glyph(glyph.font, glyph.id, pixels);
            let ink = self.inks.get(&mut self.fonts, &self.icons, key);
            let pen = [start + glyph.at[0], baseline - glyph.at[1]];
            let inset = (glyph.advance * BOX_INSET).round();
            let missing = [
                pen[0].round() + inset,
                baseline - height,
                glyph.advance.round() - inset * 2.,
                height,
            ];
            place(canvas, self.scale, ink, pen, missing, color);
        }
        (start + line.width) / self.scale
    }

    /// How far [`Self::text`] moves the pen for `text` in `font` at `size`,
    /// in logical pixels. Shapes the line, so drawing it after is free.
    pub fn text_width(&mut self, font: FontId, size: f32, text: &str) -> f32 {
        let pixels = self.pixels(size);
        self.lines.get(&mut self.fonts, font, pixels, text).width / self.scale
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
        let pixels = self.pixels(size);
        let ink = self
            .inks
            .get(&mut self.fonts, &self.icons, Key::Icon(icon, pixels));
        let [x, y] = corner.map(|value| self.to_physical(value));
        let square = f32::from(pixels);
        let inset = (square * BOX_INSET).round();
        let inner = square - inset * 2.;
        let missing = [x + inset, y + inset, inner, inner];
        place(canvas, self.scale, ink, [x, y], missing, color);
    }

    /// `size` logical pixels in whole physical pixels.
    fn pixels(&self, size: f32) -> u16 {
        u16::try_from(pixel(f64::from(size * self.scale))).unwrap_or(u16::MAX)
    }

    /// Rounds a logical position to the nearest physical pixel, in physical
    /// pixels.
    fn to_physical(&self, value: f32) -> f32 {
        (value * self.scale).round()
    }
}

/// Draws `ink` from the pen position `pen`, `y` growing down, or as an
/// outline one physical pixel wide around `missing`, `[x, y, width, height]`.
/// Both in physical pixels.
fn place(
    canvas: &mut Canvas,
    scale: f32,
    ink: Ink,
    pen: [f32; 2],
    missing: [f32; 4],
    color: Color,
) {
    let logical = |rect: [f32; 4]| {
        let [x, y, width, height] = rect.map(|value| value / scale);
        Rect::new(x, y, width, height)
    };
    match ink {
        Ink::Blank => {}
        // The pen keeps its fraction so advances don't drift, but each image
        // lands on a whole pixel, where its texels map one to one.
        Ink::Packed(packed) => {
            let [.., width, height] = packed.texels.map(f32::from);
            let x = (pen[0] + f32::from(packed.left)).round();
            let y = pen[1].round() - f32::from(packed.top);
            let rect = logical([x, y, width, height]);
            canvas.quad(Quad::sampled(
                rect,
                packed.format,
                packed.page,
                packed.texels,
                color,
            ));
        }
        Ink::Missing => {
            let rect = logical(missing);
            canvas.quad(Quad::new(rect, Color::TRANSPARENT).bordered(1. / scale, color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file under the workspace's `assets/`.
    macro_rules! asset {
        ($path:expr) => {
            include_bytes!(concat!("../../../../../assets/", $path))
        };
    }

    pub(super) const SANS: FontFile = FontFile {
        name: "Noto Sans",
        data: asset!("fonts/noto-sans/NotoSans[wdth,wght].ttf"),
    };
    pub(super) const MONO: FontFile = FontFile {
        name: "JetBrains Mono",
        data: asset!("fonts/jetbrains-mono/JetBrainsMono[wght].ttf"),
    };
    pub(super) const JP: FontFile = FontFile {
        name: "Noto Sans JP",
        data: asset!("fonts/noto-sans-jp/NotoSansJP[wght].ttf"),
    };
    const WHITE: Color = Color([u8::MAX; 4]);

    struct Loaded {
        glyphs: Glyphs,
        sans: FontId,
        mono: FontId,
        jp: FontId,
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
        Loaded {
            glyphs,
            sans,
            mono,
            jp,
        }
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
        assert!(!glyphs.fonts.has(sans, '日'));
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
            ..
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
            jp,
        } = loaded(1.);
        let mut canvas = canvas();
        glyphs.text(&mut canvas, sans, [0., 20.], 14., "日", WHITE);
        glyphs.text(&mut canvas, mono, [0., 20.], 14., "日", WHITE);
        let keys: Vec<_> = glyphs.inks.keys().collect();
        assert!(matches!(keys[..], [Key::Glyph(font, ..)] if font == jp));
    }

    #[test]
    fn a_mark_sits_over_its_letter() {
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(1.);
        let mut canvas = canvas();
        let end = glyphs.text(&mut canvas, sans, [0., 40.], 30., "q\u{301}", WHITE);
        assert!((end - glyphs.text_width(sans, 30., "q")).abs() < 1e-3);
        let [q, acute] = canvas.quads() else {
            panic!("two quads")
        };
        assert!(acute.rect[0] > q.rect[0] && acute.rect[0] < q.rect[0] + q.rect[2]);
        assert!(acute.rect[1] + acute.rect[3] <= q.rect[1] + 1.);
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
            assert_eq!(quad.texels, [0; 4]);
            assert_eq!(quad.fill, Color::TRANSPARENT);
            assert_eq!(quad.border, WHITE);
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
        assert!(canvas.quads().iter().all(|quad| quad.texels[2] > 0));
    }

    #[test]
    #[ignore = "needs a system font with colour emoji"]
    fn emoji_are_drawn_from_the_colour_atlas() {
        let Loaded {
            mut glyphs, sans, ..
        } = loaded(1.);
        glyphs.set_system_fallback(sans, true);
        let mut canvas = canvas();
        glyphs.text(&mut canvas, sans, [0., 20.], 16., "a🦀", WHITE);
        let [a, crab] = canvas.quads() else {
            panic!("two quads")
        };
        assert_eq!((a.atlas, crab.atlas), (0, 1));
        // Noto Color Emoji is drawn a little over an em wide.
        assert!((16. ..=22.).contains(&crab.rect[2]), "{:?}", crab.rect);
        assert!(crab.rect[1] < 20. - 10. && crab.rect[1] + crab.rect[3] > 20.);
        let updates: Vec<_> = glyphs.take_updates().map(|update| update.format).collect();
        assert_eq!(updates, [Format::Coverage, Format::Color]);
    }

    #[test]
    fn an_icon_that_isnt_loaded_is_a_box() {
        let mut glyphs = Glyphs::default();
        let mut canvas = canvas();
        glyphs.icon(&mut canvas, IconId(3), [0., 0.], 16., WHITE);
        let [quad] = canvas.quads() else {
            panic!("one quad")
        };
        assert_eq!(quad.border, WHITE);
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
        glyphs.take_updates().for_each(drop);
        assert_eq!(glyphs.take_updates().count(), 0);
        glyphs.set_scale(2.);
        assert_eq!(glyphs.take_updates().count(), 2);
    }
}
