//! Pages of glyph and icon masks, one coverage byte per texel, which the GPU
//! keeps as one texture array.

use std::ops::Range;

/// Masks packed left to right into shelves, page by page. A mask goes on the
/// first shelf at most a quarter taller than it, so glyphs of one size mostly
/// share shelves and few texels go unused.
///
/// A page is added when no page has room, up to [`MAX_PAGES`]. After that the
/// page that has gone longest without being drawn is emptied for the new
/// mask, and whatever was on it is rasterized again when it's next drawn.
pub struct Atlas {
    pages: Vec<Page>,
    /// Counts up each frame, for which page has gone longest unused.
    frame: u64,
    /// Whether the next update holds every page: after a page is added, the
    /// atlas clears, or the GPU loses its copy.
    whole: bool,
}

struct Page {
    /// [`PAGE_SIZE`] rows of [`PAGE_SIZE`] bytes.
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    /// Rows written since the last [`Atlas::take_update`].
    dirty: Option<Range<u16>>,
    /// The last frame a mask on this page was drawn or packed.
    used: u64,
}

struct Shelf {
    y: u16,
    height: u16,
    /// Where the next mask on this shelf goes.
    x: u16,
}

/// Where [`Atlas::insert`] put a mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Spot {
    pub page: u16,
    pub x: u16,
    pub y: u16,
}

/// What changed in the atlas, for [`Gpu`](crate::render::Gpu) to upload.
pub struct AtlasUpdate<'a> {
    /// Each page's width and height in texels.
    pub size: u16,
    /// How many pages there are. When it changes, the GPU's copy is made
    /// again and `writes` holds every page.
    pub pages: u16,
    pub writes: Vec<PageWrite<'a>>,
}

/// Rows of one page to upload.
pub struct PageWrite<'a> {
    pub page: u16,
    pub rows: Range<u16>,
    /// `rows`, each [`AtlasUpdate::size`] bytes long.
    pub pixels: &'a [u8],
}

pub(super) const PAGE_SIZE: u16 = 1024;
/// Sixteen pages hold as much as one 4096 by 4096 texture.
pub(super) const MAX_PAGES: u16 = 16;
/// Empty texels between masks, so a sample that lands just outside one
/// doesn't pick up its neighbor.
const PADDING: u16 = 1;

impl Default for Atlas {
    fn default() -> Self {
        Self {
            pages: vec![Page::default()],
            frame: 0,
            whole: true,
        }
    }
}

impl Default for Page {
    fn default() -> Self {
        let side = usize::from(PAGE_SIZE);
        Self {
            pixels: vec![0; side * side],
            shelves: Vec::new(),
            dirty: None,
            used: 0,
        }
    }
}

impl Atlas {
    /// Whether a `width` by `height` mask fits on a page at all.
    pub(super) fn fits(width: u16, height: u16) -> bool {
        width < PAGE_SIZE && height < PAGE_SIZE
    }

    /// Copies a `width` by `height` mask in, `coverage` holding its rows.
    /// `None` when no page has room and none can be added, which
    /// [`Self::evict`] makes.
    pub(super) fn insert(&mut self, width: u16, height: u16, coverage: &[u8]) -> Option<Spot> {
        if !Self::fits(width, height) {
            return None;
        }
        let padded = [width + PADDING, height + PADDING];
        let frame = self.frame;
        for (page, at) in (0..).zip(&mut self.pages) {
            if let Some([x, y]) = at.place(padded) {
                at.write([x, y, width, height], coverage, frame);
                return Some(Spot { page, x, y });
            }
        }
        let page = u16::try_from(self.pages.len()).ok()?;
        if page >= MAX_PAGES {
            return None;
        }
        let mut at = Page::default();
        let [x, y] = at.place(padded)?;
        at.write([x, y, width, height], coverage, frame);
        self.pages.push(at);
        self.whole = true;
        Some(Spot { page, x, y })
    }

    /// Empties the page that has gone longest without being drawn and
    /// returns it, so the masks on it can be forgotten. `None` when every
    /// page was drawn this frame.
    pub(super) fn evict(&mut self) -> Option<u16> {
        let frame = self.frame;
        let (page, at) = (0..)
            .zip(&mut self.pages)
            .filter(|(_, at)| at.used != frame)
            .min_by_key(|(_, at)| at.used)?;
        at.clear();
        Some(page)
    }

    /// Marks `page` as drawn this frame.
    pub(super) fn touch(&mut self, page: u16) {
        if let Some(at) = self.pages.get_mut(usize::from(page)) {
            at.used = self.frame;
        }
    }

    pub(super) fn next_frame(&mut self) {
        self.frame += 1;
    }

    /// Forgets every mask and every page but the first. Places handed out
    /// before are no longer valid.
    pub(super) fn clear(&mut self) {
        self.pages.truncate(1);
        for page in &mut self.pages {
            page.clear();
        }
        self.whole = true;
    }

    /// Makes the next update hold every page, for a GPU whose copy is gone.
    pub(super) fn reupload(&mut self) {
        self.whole = true;
    }

    /// What changed since the last call, if anything.
    pub fn take_update(&mut self) -> Option<AtlasUpdate<'_>> {
        let whole = std::mem::take(&mut self.whole);
        let pages = u16::try_from(self.pages.len()).ok()?;
        let side = usize::from(PAGE_SIZE);
        let mut writes = Vec::new();
        for (page, at) in (0..).zip(&mut self.pages) {
            let dirty = at.dirty.take();
            let Some(rows) = (if whole { Some(0..PAGE_SIZE) } else { dirty }) else {
                continue;
            };
            let at: &Page = at;
            let bytes = usize::from(rows.start) * side..usize::from(rows.end) * side;
            writes.push(PageWrite {
                page,
                pixels: at.pixels.get(bytes)?,
                rows,
            });
        }
        (!writes.is_empty()).then_some(AtlasUpdate {
            size: PAGE_SIZE,
            pages,
            writes,
        })
    }
}

impl Page {
    /// Puts a `[width, height]` area on the first shelf it fits, or on a new
    /// shelf under the rest.
    fn place(&mut self, [width, height]: [u16; 2]) -> Option<[u16; 2]> {
        let fits = |shelf: &&mut Shelf| {
            shelf.height >= height
                && shelf.height - height <= height / 4
                && PAGE_SIZE - shelf.x >= width
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
        if PAGE_SIZE - y < height || PAGE_SIZE < width {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height,
            x: width,
        });
        Some([0, y])
    }

    fn write(&mut self, [x, y, width, height]: [u16; 4], coverage: &[u8], frame: u64) {
        let columns = usize::from(x)..usize::from(x) + usize::from(width);
        let rows = self
            .pixels
            .chunks_exact_mut(usize::from(PAGE_SIZE))
            .skip(usize::from(y));
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
        self.used = frame;
    }

    /// Zeroes the texels too, since `write` leaves the padding around a mask
    /// as it finds it.
    fn clear(&mut self) {
        self.shelves.clear();
        self.pixels.fill(0);
        self.dirty = Some(0..PAGE_SIZE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: u16 = PAGE_SIZE - PADDING;

    fn fill_every_page(atlas: &mut Atlas) {
        while atlas.insert(FULL, FULL, &[]).is_some() {}
    }

    #[test]
    fn masks_of_one_height_share_a_shelf() {
        let mut atlas = Atlas::default();
        let a = atlas.insert(4, 10, &[1; 40]).unwrap();
        let b = atlas.insert(6, 10, &[2; 60]).unwrap();
        assert_eq!([a.x, a.y], [0, 0]);
        assert_eq!([b.x, b.y], [4 + PADDING, 0]);
    }

    #[test]
    fn a_full_page_starts_another() {
        let mut atlas = Atlas::default();
        let first = atlas.insert(3, 2, &[7; 6]).unwrap();
        atlas.take_update();
        let second = atlas.insert(FULL, FULL, &[]).unwrap();
        assert_eq!((first.page, second.page), (0, 1));
        let update = atlas.take_update().unwrap();
        assert_eq!(update.pages, 2);
        let pages: Vec<_> = update.writes.iter().map(|write| write.page).collect();
        assert_eq!(pages, [0, 1]);
        assert!(
            update
                .writes
                .iter()
                .all(|write| write.rows == (0..PAGE_SIZE))
        );
        let side = usize::from(PAGE_SIZE);
        assert_eq!(update.writes[0].pixels[side + 2], 7);
    }

    #[test]
    fn pages_stop_at_the_limit() {
        let mut atlas = Atlas::default();
        fill_every_page(&mut atlas);
        assert_eq!(atlas.pages.len(), usize::from(MAX_PAGES));
        assert!(atlas.insert(1, 1, &[1]).is_none());
    }

    #[test]
    fn the_page_unused_longest_is_evicted() {
        let mut atlas = Atlas::default();
        fill_every_page(&mut atlas);
        atlas.next_frame();
        for page in 0..MAX_PAGES {
            if page != 5 {
                atlas.touch(page);
            }
        }
        atlas.next_frame();
        atlas.take_update();
        assert_eq!(atlas.evict(), Some(5));
        assert_eq!(atlas.insert(2, 2, &[1; 4]).unwrap().page, 5);
        let update = atlas.take_update().unwrap();
        let [write] = &update.writes[..] else {
            panic!("one page written")
        };
        assert_eq!((write.page, write.rows.clone()), (5, 0..PAGE_SIZE));
        assert_eq!(write.pixels[2], 0);
    }

    #[test]
    fn a_page_drawn_this_frame_is_kept() {
        let mut atlas = Atlas::default();
        fill_every_page(&mut atlas);
        assert_eq!(atlas.evict(), None);
    }

    #[test]
    fn clearing_keeps_one_empty_page() {
        let mut atlas = Atlas::default();
        atlas.insert(8, 4, &[9; 32]).unwrap();
        atlas.insert(FULL, FULL, &[]).unwrap();
        atlas.clear();
        atlas.insert(2, 2, &[1; 4]).unwrap();
        let update = atlas.take_update().unwrap();
        assert_eq!(update.pages, 1);
        let [write] = &update.writes[..] else {
            panic!("one page written")
        };
        assert_eq!(write.rows, 0..PAGE_SIZE);
        assert_eq!(write.pixels[2], 0);
        assert_eq!(write.pixels[2 * usize::from(PAGE_SIZE)], 0);
    }

    #[test]
    fn updates_cover_only_written_rows() {
        let mut atlas = Atlas::default();
        atlas.take_update();
        assert!(atlas.take_update().is_none());
        atlas.insert(2, 3, &[1; 6]).unwrap();
        assert_eq!(atlas.take_update().unwrap().writes[0].rows, 0..3);
    }

    #[test]
    fn a_reupload_holds_every_row() {
        let mut atlas = Atlas::default();
        atlas.insert(2, 3, &[1; 6]).unwrap();
        atlas.take_update();
        atlas.reupload();
        let update = atlas.take_update().unwrap();
        assert_eq!(update.writes[0].rows, 0..PAGE_SIZE);
        assert_eq!(update.writes[0].pixels[0], 1);
    }
}
