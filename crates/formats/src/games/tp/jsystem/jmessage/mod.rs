//! Twilight Princess's BMG tables.

use crate::games::Entry;
use crate::games::tp::Version;

mod flow;
mod record;
mod tag;

pub use flow::{
    Control, Event, Kind, Param, Query, Results, Select, events, find_event, find_query, queries,
};
pub use record::{Field, Layout, Section, layout};
pub use tag::{Args, Tag, find_tag, tags, text_color};

/// The rows of a [`Field`]'s or [`Param`]'s values table that `version`
/// reads.
pub fn values(version: Version, table: &'static [Entry]) -> impl Iterator<Item = &'static Entry> {
    table
        .iter()
        .filter(move |entry| entry.versions.intersects(version.versions()))
}
