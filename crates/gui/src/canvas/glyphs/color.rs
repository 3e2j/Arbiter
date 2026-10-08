//! Colour glyphs a font stores as images, such as Noto Color Emoji's PNGs,
//! scaled from the nearest size the font has to the size drawn.

use resvg::tiny_skia::{Pixmap, PremultipliedColorU8};
use skrifa::bitmap::{BitmapData, BitmapGlyph, BitmapStrikes, Origin};
use skrifa::instance::Size;
use skrifa::{FontRef, GlyphId};

use super::atlas::Format;
use super::ink::Bitmap;
use crate::cast::{pixel, pixel_offset};

/// Glyph `id` from `face`'s images at `pixels` per em, as straight sRGB
/// colour. `None` when the font has no image for it.
pub(super) fn rasterize(face: &FontRef, id: GlyphId, pixels: u16) -> Option<Bitmap> {
    let size = Size::new(f32::from(pixels));
    let glyph = BitmapStrikes::new(face).glyph_for_size(size, id)?;
    // Bottom left is sbix, whose placement isn't handled yet.
    if glyph.placement_origin != Origin::TopLeft {
        return None;
    }
    let image = decode(&glyph)?;
    let scale = f64::from(pixels) / f64::from(glyph.ppem_y);
    let scaled = |n: u32| u16::try_from(pixel(f64::from(n) * scale)).ok();
    let offset = |n: f32| i16::try_from(pixel_offset(f64::from(n) * scale)).ok();
    let width = scaled(image.width())?;
    let height = scaled(image.height())?;
    Some(Bitmap {
        format: Format::Color,
        width,
        height,
        left: offset(glyph.inner_bearing_x)?,
        top: offset(glyph.inner_bearing_y)?,
        texels: resize(&image, width, height),
    })
}

fn decode(glyph: &BitmapGlyph) -> Option<Pixmap> {
    match glyph.data {
        BitmapData::Png(png) => Pixmap::decode_png(png).ok(),
        BitmapData::Bgra(bgra) => {
            let mut image = Pixmap::new(glyph.width, glyph.height)?;
            for (texel, &[b, g, r, a]) in image.pixels_mut().iter_mut().zip(bgra.as_chunks().0) {
                *texel = PremultipliedColorU8::from_rgba(r, g, b, a)?;
            }
            Some(image)
        }
        BitmapData::Mask(_) => None,
    }
}

/// `image` resized to `width` by `height`, each texel the average of the
/// area of `image` under it, as straight RGBA.
///
/// Averaging over the whole area keeps detail from aliasing when an emoji
/// drawn at 16 pixels comes from a 109 pixel image.
fn resize(image: &Pixmap, width: u16, height: u16) -> Vec<u8> {
    let columns: Vec<_> = (0..width).map(|x| span(x, image.width(), width)).collect();
    let mut texels = Vec::with_capacity(usize::from(width) * usize::from(height) * 4);
    for y in 0..height {
        let rows = span(y, image.height(), height);
        for columns in &columns {
            let mut sum = [0.; 4];
            for &(row, row_weight) in &rows {
                for &(column, column_weight) in columns {
                    let Some(texel) = image.pixel(column, row) else {
                        continue;
                    };
                    let weight = row_weight * column_weight;
                    let channels = [texel.red(), texel.green(), texel.blue(), texel.alpha()];
                    for (sum, channel) in sum.iter_mut().zip(channels) {
                        *sum += f64::from(channel) * weight;
                    }
                }
            }
            texels.extend(straight(sum));
        }
    }
    texels
}

/// The source texels under texel `at` of `to`, when `from` texels are
/// resized to `to`, each with the share of `at` it covers.
fn span(at: u16, from: u32, to: u16) -> Vec<(u32, f64)> {
    let step = f64::from(from) / f64::from(to);
    let start = f64::from(at) * step;
    let end = start + step;
    (pixel(start.floor())..pixel(end.ceil()))
        .map(|texel| {
            let left = f64::from(texel);
            let covered = end.min(left + 1.) - start.max(left);
            (texel, covered / step)
        })
        .collect()
}

/// Premultiplied channels back to straight bytes.
fn straight([r, g, b, a]: [f64; 4]) -> [u8; 4] {
    let byte = |value: f64| u8::try_from(pixel(value)).unwrap_or(u8::MAX);
    if a <= 0. {
        return [0; 4];
    }
    let [r, g, b] = [r, g, b].map(|channel| byte(channel / a * 255.));
    [r, g, b, byte(a)]
}

#[cfg(test)]
mod tests {
    use resvg::tiny_skia::ColorU8;

    use super::*;

    fn image(width: u32, texels: &[[u8; 4]]) -> Pixmap {
        let height = u32::try_from(texels.len()).unwrap() / width;
        let mut image = Pixmap::new(width, height).unwrap();
        for (texel, &[r, g, b, a]) in image.pixels_mut().iter_mut().zip(texels) {
            *texel = ColorU8::from_rgba(r, g, b, a).premultiply();
        }
        image
    }

    #[test]
    fn shrinking_averages_the_area_under_each_texel() {
        let red = [255, 0, 0, 255];
        let clear = [0; 4];
        let shrunk = resize(&image(2, &[red, clear, clear, red]), 1, 1);
        // Half covered, but still fully red where it is.
        assert_eq!(shrunk, [255, 0, 0, 128]);
    }

    #[test]
    fn an_uneven_shrink_weighs_texels_by_how_much_they_cover() {
        let weights = span(1, 3, 2);
        assert_eq!(weights, [(1, 0.5 / 1.5), (2, 1. / 1.5)]);
        let grey = [90, 90, 90, 255];
        let shrunk = resize(&image(3, &[[0, 0, 0, 255], grey, [255; 4]]), 2, 1);
        assert_eq!(shrunk[4..], [200, 200, 200, 255]);
    }
}
