//! Twilight Princess's message tables: what its attribute bytes, text tags,
//! colors, flow nodes and unit words mean.

mod color;
mod flow;
mod record;
mod tag;
mod unit;

use super::Rows;

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
