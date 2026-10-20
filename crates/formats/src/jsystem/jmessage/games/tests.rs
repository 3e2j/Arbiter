//! Every game's rows are checked here, through the one surface that reads
//! them.

use super::*;
use crate::games::entry::assert_one_meaning;
use crate::games::tp::Version;

const USA: Tables = Tables::tp(Version::GcnUsa);

fn every_version() -> impl Iterator<Item = Tables> {
    Version::ALL.into_iter().map(Tables::tp)
}

fn layouts(tables: Tables) -> impl Iterator<Item = &'static Layout> {
    let version = tables.version;
    tables
        .rows
        .layouts
        .iter()
        .filter(move |layout| layout.versions.intersects(version))
}

/// Each layout's fields tile its record after the text offset, with no gap
/// or overlap.
#[test]
fn the_fields_cover_the_record() {
    for layout in every_version().flat_map(layouts) {
        let end = layout.fields.iter().try_fold(4, |at, field| {
            (field.offset == at).then_some(at + field.len)
        });
        assert_eq!(end, Some(usize::from(layout.len)), "{layout:?}");
    }
}

/// [`Tables::layout`] picks by width alone.
#[test]
fn no_two_layouts_of_a_version_share_a_width() {
    for tables in every_version() {
        for layout in layouts(tables) {
            assert_eq!(tables.layout(layout.len), Some(layout));
        }
    }
}

#[test]
fn the_id_is_a_field() {
    for layout in every_version().flat_map(layouts) {
        assert!(layout.id.is_none_or(|id| layout.fields.contains(&id)));
    }
}

#[test]
fn jpn_reads_its_own_unit_records() {
    assert!(USA.layout(8).is_some());
    assert_eq!(USA.layout(28), None);
    let jpn = Tables::tp(Version::WiiJpn);
    assert!(jpn.layout(28).is_some());
    assert_eq!(jpn.layout(8), None);
}

#[test]
fn the_tags_are_sorted() {
    assert!(USA.rows.tags.is_sorted_by_key(|tag| (tag.group, tag.code)));
}

#[test]
fn sound_and_camera_match_any_code() {
    assert_eq!(USA.tag(1, 20).map(|tag| tag.name), Some("Sound"));
    assert_eq!(USA.tag(2, 7).map(|tag| tag.name), Some("Camera"));
}

#[test]
fn a_named_code_is_found() {
    assert_eq!(USA.tag(0, 7).map(|tag| tag.name), Some("Pause"));
}

#[test]
fn an_unnamed_code_is_none() {
    assert_eq!(USA.tag(0, 64), None);
    assert_eq!(USA.tag(7, 0), None);
}

#[test]
fn a_code_is_read_as_its_version_means_it() {
    let jpn = Tables::tp(Version::WiiJpn);
    assert_eq!(jpn.tag(0, 33).map(|tag| tag.name), Some("Kana initial"));
    assert_eq!(USA.tag(0, 33).map(|tag| tag.name), Some("Unknown name"));
}

#[test]
fn a_field_value_is_read_as_its_version_means_it() {
    let alignment = |tables: Tables| {
        let field = layouts(tables)
            .flat_map(|layout| layout.fields)
            .find(|field| field.name == "Line alignment")?;
        tables.values(field.values?).get(0).map(|entry| entry.name)
    };
    assert_eq!(alignment(Tables::tp(Version::GcnJpn)), Some("Centered"));
    assert_eq!(alignment(USA), Some("Left"));
}

/// On every version these are dense from 0, so a value's position among
/// that version's rows is its id. A gap or a duplicate is a transcription
/// slip.
#[test]
fn the_id_tables_are_dense() {
    for tables in every_version() {
        for values in [
            tables.queries(),
            tables.events(),
            tables.units(),
            tables.colors(),
        ] {
            assert!((0..).zip(values.iter()).all(|(id, row)| row.value == id));
        }
    }
}

#[test]
fn each_value_has_one_meaning_per_version() {
    let rows = USA.rows;
    let fields = rows.layouts.iter().flat_map(|layout| layout.fields);
    for table in fields.filter_map(|field| field.values) {
        assert_one_meaning(table, |entry| (entry.value, entry.versions));
    }
    assert_one_meaning(rows.tags, |tag| ((tag.group, tag.code), tag.versions));
}

#[test]
fn every_color_has_an_rgb() {
    assert_eq!(USA.rows.colors.len(), USA.rows.rgb.len());
}

#[test]
fn the_default_color_follows_the_box() {
    let white = [0xFF, 0xFF, 0xFF];
    assert_eq!(USA.rgb(0, 0), white);
    assert_eq!(USA.rgb(0, 13), [0x82, 0xE6, 0xE6]);
    assert_eq!(USA.rgb(1, 13), [0xF0, 0x78, 0x78]);
    assert_eq!(USA.rgb(9, 0), white);
}

#[test]
fn a_disc_finds_its_tables() {
    assert!(tables("GZ2E01", 0).is_some());
    assert!(tables("RZDE01", 1).is_none());
    assert!(tables("GALE01", 0).is_none());
}
