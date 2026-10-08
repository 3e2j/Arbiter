//! Lines split into runs, each shaped by harfrust, which applies kerning,
//! ligatures and the rules of scripts like Arabic and Devanagari.
//!
//! Runs go left to right in the order they're written. A right-to-left run
//! is shaped right to left, but runs aren't reordered around each other.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use harfrust::{ShapeOptions, UnicodeBuffer};
use skrifa::GlyphId;
use unicode_script::{Script, UnicodeScript};

use super::FontId;
use super::fonts::Fonts;
use super::ink::Ink;
use crate::cast::narrow;

/// Shaped lines, kept while they're drawn every frame.
#[derive(Default)]
pub(super) struct Lines {
    /// The lines shaped or drawn this frame, by [`line_hash`].
    this_frame: HashMap<u64, Line>,
    /// Last frame's lines, which move to `this_frame` when they're drawn
    /// again. Whatever isn't is dropped when the next frame starts.
    last_frame: HashMap<u64, Line>,
    /// Kept between runs so shaping one allocates nothing new.
    buffer: Option<UnicodeBuffer>,
}

/// A line of text in one font and size, shaped. In physical pixels from a pen
/// starting at zero on the baseline.
pub(super) struct Line {
    /// To tell lines apart whose hashes collide.
    text: Box<str>,
    pub glyphs: Vec<Placed>,
    pub width: f32,
}

#[derive(Clone, Copy)]
pub(super) struct Placed {
    pub font: FontId,
    pub id: GlyphId,
    /// The pen position, plus whatever the shaper moved this glyph by, such
    /// as a mark over its letter. `y` grows up.
    pub at: [f32; 2],
    pub advance: f32,
    /// What it was last drawn with, so drawing the line again skips looking
    /// each glyph up while its atlas page still holds it.
    pub ink: Option<Ink>,
}

/// A stretch of a line drawn in one font.
struct Run {
    font: FontId,
    /// The first that isn't common to every script, such as a space or a
    /// digit.
    script: Script,
    bytes: Range<usize>,
}

impl Lines {
    /// `text` shaped in `font` at `pixels` per em, from this frame, else last
    /// frame, else shaped now.
    pub fn get(&mut self, fonts: &mut Fonts, font: FontId, pixels: u16, text: &str) -> &mut Line {
        let hash = line_hash(font, pixels, text);
        let line = match self.this_frame.remove(&hash) {
            Some(line) if *line.text == *text => line,
            _ => match self.last_frame.remove(&hash) {
                Some(line) if *line.text == *text => line,
                _ => self.shape(fonts, font, pixels, text),
            },
        };
        self.this_frame.entry(hash).insert_entry(line).into_mut()
    }

    /// Starts a frame, dropping the lines last frame didn't draw.
    pub fn next_frame(&mut self) {
        std::mem::swap(&mut self.this_frame, &mut self.last_frame);
        self.this_frame.clear();
    }

    /// For when which font draws a character changes.
    pub fn clear(&mut self) {
        self.this_frame.clear();
        self.last_frame.clear();
    }

    fn shape(&mut self, fonts: &mut Fonts, font: FontId, pixels: u16, text: &str) -> Line {
        let mut glyphs = Vec::new();
        let mut pen = 0.;
        for run in runs(fonts, font, text) {
            let Some(text) = text.get(run.bytes) else {
                continue;
            };
            pen = self.shape_run(fonts, run.font, pixels, text, pen, &mut glyphs);
        }
        Line {
            text: text.into(),
            glyphs,
            width: pen,
        }
    }

    /// Shapes `text` in `font`, adding its glyphs to `glyphs` from `pen`, and
    /// returns where the pen ends.
    fn shape_run(
        &mut self,
        fonts: &Fonts,
        font: FontId,
        pixels: u16,
        text: &str,
        mut pen: f32,
        glyphs: &mut Vec<Placed>,
    ) -> f32 {
        let Some((drawn, face)) = fonts.face(font) else {
            return pen;
        };
        let shaper = drawn
            .shaper
            .shaper(&face)
            .instance(Some(&drawn.variations))
            .build();
        let mut buffer = self.buffer.take().unwrap_or_default();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let output = shaper.shape(buffer, ShapeOptions::new());
        let per_unit = f64::from(pixels) / f64::from(drawn.units_per_em);
        let to_pixels = |units: i32| narrow(f64::from(units) * per_unit);
        for (info, position) in output.glyph_infos().iter().zip(output.glyph_positions()) {
            let advance = to_pixels(position.x_advance);
            glyphs.push(Placed {
                font,
                id: GlyphId::new(info.glyph_id),
                at: [
                    pen + to_pixels(position.x_offset),
                    to_pixels(position.y_offset),
                ],
                advance,
                ink: None,
            });
            pen += advance;
        }
        self.buffer = Some(output.clear());
        pen
    }
}

/// Splits `text` where the font that draws it changes, or where it changes
/// script, so each run shapes by one script's rules.
///
/// A mark, joiner or variation selector stays with the character before it,
/// and so does a space or punctuation the run's font has, so a sequence like
/// an accented letter or a joined emoji isn't split.
fn runs(fonts: &mut Fonts, font: FontId, text: &str) -> Vec<Run> {
    let mut runs: Vec<Run> = Vec::new();
    for (at, c) in text.char_indices() {
        let bytes = at..at + c.len_utf8();
        let script = c.script();
        if let Some(run) = runs.last_mut() {
            let joins =
                script == Script::Inherited || (script == Script::Common && fonts.has(run.font, c));
            if joins {
                run.bytes.end = bytes.end;
                continue;
            }
        }
        let drawn = fonts.for_char(c, font);
        if let Some(run) = runs.last_mut()
            && run.font == drawn
            && (run.script == script || run.script == Script::Common)
        {
            run.script = script;
            run.bytes.end = bytes.end;
            continue;
        }
        runs.push(Run {
            font: drawn,
            script,
            bytes,
        });
    }
    runs
}

/// Which line `text` in `font` at `pixels` is. Equal hashes can still be
/// different text, which [`Line::text`] tells apart.
fn line_hash(font: FontId, pixels: u16, text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    (font, pixels, text).hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::super::tests::SANS;
    use super::*;

    fn sans() -> (Fonts, FontId) {
        let mut fonts = Fonts::default();
        let sans = fonts.add(SANS, 400.).unwrap();
        (fonts, sans)
    }

    #[test]
    fn kerning_pulls_pairs_together() {
        let (mut fonts, sans) = sans();
        let mut lines = Lines::default();
        let mut width = |text| lines.get(&mut fonts, sans, 40, text).width;
        let apart = width("A") + width("V");
        assert!(width("AV") < apart - 1.);
    }

    #[test]
    fn a_line_is_kept_while_its_drawn() {
        let (mut fonts, sans) = sans();
        let mut lines = Lines::default();
        lines.get(&mut fonts, sans, 14, "kept");
        lines.get(&mut fonts, sans, 14, "dropped");
        lines.next_frame();
        lines.get(&mut fonts, sans, 14, "kept");
        assert_eq!(lines.this_frame.len() + lines.last_frame.len(), 2);
        lines.next_frame();
        assert_eq!(lines.last_frame.len(), 1);
        assert!(lines.last_frame.values().all(|line| &*line.text == "kept"));
    }
}
