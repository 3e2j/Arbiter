//! What the raw values in a [`Bmg`](super::bmg::Bmg) mean to each game.
//!
//! [`tables`] is the way in. It finds the game a disc holds and returns its
//! [`Tables`], which every lookup goes through. Each game fills in one
//! `Rows` in its own folder (`tp/`), and only the code here searches them.
//!
//! ```text
//! record layout   Tables::layout, by INF1 record width
//!   field values  Tables::values(field.values)
//! text tags       Tables::tag, by group and code
//!   colors        Tables::colors, drawn through Tables::rgb
//! flow nodes      Tables::queries (branch), Tables::events (event)
//! unit file       Tables::units, by record position
//! ```

mod layout;
mod tag;
mod tp;

use crate::games::{Entry, Versions};

pub use layout::{Field, Layout};
pub use tag::{Args, Tag};

pub(crate) use layout::field;
pub(crate) use tag::tag;

/// The tables for a disc's id and revision, or `None` when no game's tables
/// read it.
#[must_use]
pub fn tables(id: &str, revision: u8) -> Option<Tables> {
    crate::games::tp::Version::from_disc(id, revision).map(Tables::tp)
}

/// Everything one game names, as plain rows. A game without a table leaves
/// it empty.
#[derive(Debug)]
pub(crate) struct Rows {
    /// INF1 record structs. No two a version holds share a width.
    pub(crate) layouts: &'static [Layout],
    pub(crate) tags: &'static [Tag],
    pub(crate) queries: &'static [Entry],
    pub(crate) events: &'static [Entry],
    /// By record position in the unit file.
    pub(crate) units: &'static [Entry],
    /// What the color tag indexes.
    pub(crate) colors: &'static [Entry],
    /// Each color's RGB, indexed by its value.
    pub(crate) rgb: &'static [[u8; 3]],
    /// `(box kind, rgb)`: boxes that draw color 0 their own way.
    pub(crate) box_rgb: &'static [(u8, [u8; 3])],
    /// What a color past the end of `rgb` draws as.
    pub(crate) past_rgb: [u8; 3],
}

/// One game's message tables, as one of its versions reads them.
#[derive(Debug, Clone, Copy)]
pub struct Tables {
    rows: &'static Rows,
    version: Versions,
}

impl Tables {
    #[must_use]
    pub const fn tp(version: crate::games::tp::Version) -> Self {
        Self {
            rows: &tp::ROWS,
            version: version.versions(),
        }
    }

    /// The layout the game reads `record_len` wide INF1 records with, or
    /// `None` when it reads none that wide. The game reads each file with a
    /// fixed struct, so the width alone picks one.
    #[must_use]
    pub fn layout(self, record_len: u16) -> Option<&'static Layout> {
        self.rows
            .layouts
            .iter()
            .find(|layout| layout.len == record_len && layout.versions.intersects(self.version))
    }

    /// The tag for `group` and `code`, or `None` when nothing names it.
    #[must_use]
    pub fn tag(self, group: u8, code: u16) -> Option<&'static Tag> {
        self.tags()
            .find(|tag| tag.group == group && (tag.any_code || tag.code == code))
    }

    /// Every tag, for a picker to list.
    pub fn tags(self) -> impl Iterator<Item = &'static Tag> {
        let version = self.version;
        self.rows
            .tags
            .iter()
            .filter(move |tag| tag.versions.intersects(version))
    }

    /// Branch node queries, by the id a branch node stores.
    #[must_use]
    pub const fn queries(self) -> Values {
        self.values(self.rows.queries)
    }

    /// Event node events, by the id an event node stores.
    #[must_use]
    pub const fn events(self) -> Values {
        self.values(self.rows.events)
    }

    /// The words put after a number in text, by unit file record position.
    #[must_use]
    pub const fn units(self) -> Values {
        self.values(self.rows.units)
    }

    #[must_use]
    pub const fn colors(self) -> Values {
        self.values(self.rows.colors)
    }

    /// A `Field::values` or `Tag::values` table, as this version reads it.
    #[must_use]
    pub const fn values(self, rows: &'static [Entry]) -> Values {
        Values {
            rows,
            version: self.version,
        }
    }

    /// What `color` draws as in a box of `box_kind`.
    #[must_use]
    pub fn rgb(self, color: u8, box_kind: u8) -> [u8; 3] {
        let own = self
            .rows
            .box_rgb
            .iter()
            .find(|&&(kind, _)| kind == box_kind);
        if color == 0
            && let Some(&(_, rgb)) = own
        {
            return rgb;
        }
        let rgb = self.rows.rgb.get(usize::from(color));
        rgb.copied().unwrap_or(self.rows.past_rgb)
    }

    /// Whether `tag`'s argument is a color, which [`Self::rgb`] draws.
    #[must_use]
    pub fn is_color(self, tag: &Tag) -> bool {
        tag.values
            .is_some_and(|values| std::ptr::eq(values, self.rows.colors))
    }
}

/// One table, as one version reads it.
#[derive(Debug, Clone, Copy)]
pub struct Values {
    rows: &'static [Entry],
    version: Versions,
}

impl Values {
    /// The row naming `value`, or `None` when the table doesn't name it.
    #[must_use]
    pub fn get(self, value: u16) -> Option<&'static Entry> {
        self.iter().find(|entry| entry.value == value)
    }

    /// Every row, for a picker to list.
    pub fn iter(self) -> impl Iterator<Item = &'static Entry> {
        let version = self.version;
        self.rows
            .iter()
            .filter(move |entry| entry.versions.intersects(version))
    }
}

#[cfg(test)]
mod tests;
