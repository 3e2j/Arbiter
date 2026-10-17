//! Twilight Princess's message tables, as rows.

mod color;
mod flow;
mod record;
mod tag;
mod unit;

use crate::jsystem::jmessage::Rows;

pub(crate) static ROWS: Rows = Rows {
    layouts: &[record::LAYOUT, unit::LAYOUT, unit::JPN_LAYOUT],
    tags: tag::TAGS,
    queries: flow::QUERIES,
    events: flow::EVENTS,
    units: unit::UNITS,
    colors: color::COLORS,
    rgb: color::RGB,
    box_rgb: color::BOX_RGB,
    past_rgb: color::WHITE,
};
