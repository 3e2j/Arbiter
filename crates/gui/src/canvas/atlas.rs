//! One image holding every glyph and icon mask, one coverage byte per texel.

use std::ops::Range;

/// Masks packed left to right into shelves. A mask goes on the first shelf at
/// most a quarter taller than it, so glyphs of one size mostly share shelves
/// and few texels go unused.
///
/// It grows by doubling, keeping every mask where it was, so the texel areas
/// already handed out stay valid.
pub struct Atlas {
    /// Width and height in texels.
    size: u16,
    /// `size` rows of `size` bytes.
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    /// Rows written since the last [`Atlas::take_update`].
    dirty: Option<Range<u16>>,
    /// Whether the next update holds every row: after the atlas grows or
    /// clears, or the GPU loses its copy.
    whole: bool,
}

struct Shelf {
    y: u16,
    height: u16,
    /// Where the next mask on this shelf goes.
    x: u16,
}

/// What changed in the atlas, for [`Gpu`](crate::render::Gpu) to upload.
pub struct AtlasUpdate<'a> {
    /// Width and height in texels.
    pub size: u16,
    /// The rows that changed, or every row after the atlas grows, clears or
    /// is reuploaded.
    pub rows: Range<u16>,
    /// `rows`, each `size` bytes long.
    pub pixels: &'a [u8],
}

const INITIAL_SIZE: u16 = 512;
const MAX_SIZE: u16 = 4096;
/// Empty texels between masks, so a sample that lands just outside one
/// doesn't pick up its neighbor.
const PADDING: u16 = 1;

impl Default for Atlas {
    fn default() -> Self {
        let side = usize::from(INITIAL_SIZE);
        Self {
            size: INITIAL_SIZE,
            pixels: vec![0; side * side],
            shelves: Vec::new(),
            dirty: None,
            whole: true,
        }
    }
}

impl Atlas {
    /// Copies a `width` by `height` mask in, `coverage` holding its rows, and
    /// returns its top left corner. `None` once the atlas can't grow to fit it.
    pub(super) fn insert(&mut self, width: u16, height: u16, coverage: &[u8]) -> Option<[u16; 2]> {
        let [x, y] = self.allocate(width, height)?;
        let side = usize::from(self.size);
        let columns = usize::from(x)..usize::from(x) + usize::from(width);
        let rows = self.pixels.chunks_exact_mut(side).skip(usize::from(y));
        for (row, mask) in rows.zip(coverage.chunks_exact(usize::from(width))) {
            if let Some(texels) = row.get_mut(columns.clone()) {
                texels.copy_from_slice(mask);
            }
        }
        let written = y..y + height;
        self.dirty = Some(match self.dirty.take() {
            Some(dirty) => dirty.start.min(written.start)..dirty.end.max(written.end),
            None => written,
        });
        Some([x, y])
    }

    /// Forgets every mask. Texel areas handed out before are no longer valid.
    ///
    /// Zeroes the texels too, since `insert` leaves the padding around a mask
    /// as it finds it.
    pub(super) fn clear(&mut self) {
        self.shelves.clear();
        self.pixels.fill(0);
        self.dirty = None;
        self.whole = true;
    }

    /// Makes the next update hold every row, for a GPU whose copy is gone.
    pub(super) fn reupload(&mut self) {
        self.whole = true;
    }

    /// What changed since the last call, if anything.
    pub fn take_update(&mut self) -> Option<AtlasUpdate<'_>> {
        let rows = if std::mem::take(&mut self.whole) {
            self.dirty = None;
            0..self.size
        } else {
            self.dirty.take()?
        };
        let side = usize::from(self.size);
        let bytes = usize::from(rows.start) * side..usize::from(rows.end) * side;
        Some(AtlasUpdate {
            size: self.size,
            pixels: self.pixels.get(bytes)?,
            rows,
        })
    }

    fn allocate(&mut self, width: u16, height: u16) -> Option<[u16; 2]> {
        let padded = [width.checked_add(PADDING)?, height.checked_add(PADDING)?];
        loop {
            if let Some(corner) = self.place(padded) {
                return Some(corner);
            }
            self.grow()?;
        }
    }

    /// Puts a `[width, height]` area on the first shelf it fits, or on a new
    /// shelf under the rest.
    fn place(&mut self, [width, height]: [u16; 2]) -> Option<[u16; 2]> {
        let size = self.size;
        let fits = |shelf: &&mut Shelf| {
            shelf.height >= height && shelf.height - height <= height / 4 && size - shelf.x >= width
        };
        if let Some(shelf) = self.shelves.iter_mut().find(fits) {
            let x = shelf.x;
            shelf.x += width;
            return Some([x, shelf.y]);
        }
        let y = self
            .shelves
            .last()
            .map_or(0, |shelf| shelf.y + shelf.height);
        if size - y < height || size < width {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            x: width,
        });
        Some([0, y])
    }

    /// Doubles the width and height, keeping every mask in place. `None` at
    /// [`MAX_SIZE`].
    fn grow(&mut self) -> Option<()> {
        if self.size >= MAX_SIZE {
            return None;
        }
        let (old, new) = (usize::from(self.size), usize::from(self.size) * 2);
        let mut pixels = vec![0; new * new];
        for (to, from) in pixels
            .chunks_exact_mut(new)
            .zip(self.pixels.chunks_exact(old))
        {
            if let Some(to) = to.get_mut(..old) {
                to.copy_from_slice(from);
            }
        }
        self.pixels = pixels;
        self.size *= 2;
        self.whole = true;
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_of_one_height_share_a_shelf() {
        let mut atlas = Atlas::default();
        let a = atlas.insert(4, 10, &[1; 40]).unwrap();
        let b = atlas.insert(6, 10, &[2; 60]).unwrap();
        assert_eq!(a, [0, 0]);
        assert_eq!(b, [4 + PADDING, 0]);
    }

    #[test]
    fn growing_keeps_masks_in_place() {
        let mut atlas = Atlas::default();
        let first = atlas.insert(3, 2, &[7; 6]).unwrap();
        atlas.take_update();
        atlas
            .insert(INITIAL_SIZE - 1, INITIAL_SIZE - 1, &[])
            .unwrap();
        let update = atlas.take_update().unwrap();
        assert_eq!(update.size, INITIAL_SIZE * 2);
        assert_eq!(update.rows, 0..INITIAL_SIZE * 2);
        let side = usize::from(update.size);
        let [x, y] = first.map(usize::from);
        assert_eq!(update.pixels[(y + 1) * side + x + 2], 7);
    }

    #[test]
    fn clearing_leaves_no_coverage_in_the_padding() {
        let mut atlas = Atlas::default();
        atlas.insert(8, 4, &[9; 32]).unwrap();
        atlas.clear();
        atlas.insert(2, 2, &[1; 4]).unwrap();
        let update = atlas.take_update().unwrap();
        assert_eq!(update.rows, 0..INITIAL_SIZE);
        assert_eq!(update.pixels[2], 0);
        assert_eq!(update.pixels[2 * usize::from(update.size)], 0);
    }

    #[test]
    fn updates_cover_only_written_rows() {
        let mut atlas = Atlas::default();
        atlas.take_update();
        assert!(atlas.take_update().is_none());
        atlas.insert(2, 3, &[1; 6]).unwrap();
        assert_eq!(atlas.take_update().unwrap().rows, 0..3);
    }

    #[test]
    fn a_reupload_holds_every_row() {
        let mut atlas = Atlas::default();
        atlas.insert(2, 3, &[1; 6]).unwrap();
        atlas.take_update();
        atlas.reupload();
        let update = atlas.take_update().unwrap();
        assert_eq!(update.rows, 0..INITIAL_SIZE);
        assert_eq!(update.pixels[0], 1);
    }
}
