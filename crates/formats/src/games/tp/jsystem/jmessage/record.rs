//! INF1 records, `JMSMesgEntry_c` in `d_msg_class.h`, except in `zel_unit.bmg`.

use crate::games::Versions;
use crate::games::entry::{Entry, entry};
use crate::games::tp::{ALL, JPN, Version};

/// The layout `version` reads `record_len` wide INF1 records with, or `None`
/// when it reads none that wide. The game reads each file with a fixed
/// struct, so the width alone picks one.
#[must_use]
pub fn layout(version: Version, record_len: u16) -> Option<&'static Layout> {
    LAYOUTS
        .iter()
        .find(|layout| layout.len == record_len && layout.versions.intersects(version.versions()))
}

/// A record struct the game reads INF1 with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// Record width, text offset included.
    pub len: u16,
    /// Every field after the 4-byte text offset, each in one section.
    pub sections: &'static [Section],
    pub versions: Versions,
}

/// Fields that show together in the inspector, under `title`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    pub fields: &'static [Field],
}

/// One field of an INF1 record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// Byte offset into the whole record, where 0x00 is the text offset.
    pub offset: usize,
    /// 1 or 2 bytes, big-endian.
    pub len: usize,
    pub name: &'static str,
    pub notes: Option<&'static str>,
    /// The table naming the field's values, read through [`super::values`].
    pub values: Option<&'static [Entry]>,
}

const fn field(offset: usize, len: usize, name: &'static str) -> Field {
    Field {
        offset,
        len,
        name,
        notes: None,
        values: None,
    }
}

impl Field {
    /// Its value in a message's attributes, or `None` when they end before
    /// it.
    #[must_use]
    pub fn read(self, attributes: &[u8]) -> Option<u16> {
        let start = self.offset.checked_sub(4)?;
        let bytes = attributes.get(start..start + self.len)?;
        Some(
            bytes
                .iter()
                .fold(0, |value, &byte| value << 8 | u16::from(byte)),
        )
    }

    const fn notes(self, notes: &'static str) -> Self {
        Self {
            notes: Some(notes),
            ..self
        }
    }

    const fn values(self, values: &'static [Entry]) -> Self {
        Self {
            values: Some(values),
            ..self
        }
    }
}

static LAYOUTS: &[Layout] = &[LAYOUT];

#[rustfmt::skip]
const LAYOUT: Layout = Layout { len: 20, versions: Versions::ANY, sections: &[
    Section { title: "Message", fields: &[
        field(0x04, 2, "Message id")    .notes("Id the game looks the message up by. Same as the MID1 id"),
        field(0x06, 2, "Event label")   .notes("`saveBitLabels` index set when the message displays"),
    ]},
    Section { title: "Box", fields: &[
        field(0x09, 1, "Box kind")      .values(BOX_KINDS),
        field(0x0A, 1, "Draw type")     .values(DRAW_TYPES),
        field(0x0B, 1, "Box position")  .values(BOX_POSITIONS),
        field(0x0D, 1, "Line alignment").values(LINE_ALIGNMENTS),
    ]},
    Section { title: "Speaker", fields: &[
        field(0x08, 1, "Speaker")       .notes("`Z2SpeechMgr2` voice bank id"),
        field(0x0E, 1, "Speaker mood")  .notes("Grunt emotion index for the voice bank"),
    ]},
    Section { title: "Camera and animation", fields: &[
        field(0x0F, 1, "Camera")        .notes("1 to 10 talk-actor slot, 11 and up talk-camera style"),
        field(0x10, 1, "Base animation").notes("How a NPC moves"),
        field(0x11, 1, "Face animation").notes("Facial animation choice, separate to body"),
    ]},
    Section { title: "Unused", fields: &[
        field(0x0C, 1, "Item")          .notes("Legacy `dItemNo`, 0xFF for none"),
        field(0x12, 1, "Lines per page").notes("The runtime uses `getLineMax()`"),
        field(0x13, 1, "Padding"),
    ]},
]};

/// Screen class, see `dMsgObject_c::talkStartInit`.
#[rustfmt::skip]
static BOX_KINDS: &[Entry] = &[
    entry(0,  "Talk"),
    entry(1,  "Demo caption").notes("Boxless cutscene caption"),
    entry(2,  "Sign"),
    entry(5,  "Plain")       .notes("Boxless system text"),
    entry(6,  "Kanban")      .notes("Signboard screen class, unused"),
    entry(7,  "Staff roll"),
    entry(8,  "Light spirit").notes("Glow"),
    entry(9,  "Item get"),
    entry(11, "Item name")   .notes("UI string fetch, no box"),
    entry(12, "Place name")  .notes("Shown on entering an area"),
    entry(13, "Midna")       .notes("Blue colors and glow"),
    entry(14, "Animal")      .notes("Green colors and glow (wolf-form)"),
    entry(15, "Notice")      .notes("Floating gameplay notice, can't be used during dialogue"),
    entry(16, "Save"),
    entry(17, "Howl"),
    entry(19, "Boss name"),
];

/// Text pacing, see `jmessage_tSequenceProcessor::do_begin`.
#[rustfmt::skip]
static DRAW_TYPES: &[Entry] = &[
    entry(0, "Typed")         .notes("A skips typing"),
    entry(1, "Instant")       .notes("Menus and prompts"),
    entry(2, "Typed, no skip"),
    entry(3, "Fade"),
    entry(4, "UI name")       .notes("UI string fetch (item names), no box pacing"),
    entry(5, "Typed slow")    .notes("Light spirit speech"),
    entry(7, "UI action")     .notes("UI string fetch (action button labels)"),
    entry(9, "Fade slow")     .notes("Staff credits"),
];

/// See `dMsgObject_c::fukiPosCalc`.
#[rustfmt::skip]
static BOX_POSITIONS: &[Entry] = &[
    entry(0, "Bottom"),
    entry(1, "Top"),
    entry(2, "Middle"),
    entry(3, "Auto")  .notes("Top or bottom, whichever avoids the speaker on screen"),
];

/// See `jmessage_tMeasureProcessor::do_begin`.
#[rustfmt::skip]
static LINE_ALIGNMENTS: &[Entry] = &[
    entry(0, "Centered")             .only(JPN),
    entry(0, "Left")                 .only(ALL.without(JPN)),
    entry(1, "Left (mute UI sounds)"),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::entry::assert_one_meaning;

    fn fields(layout: &Layout) -> impl Iterator<Item = &'static Field> {
        layout.sections.iter().flat_map(|section| section.fields)
    }

    /// Each layout's fields, whatever section they're in, tile its record
    /// after the text offset with no gap or overlap.
    #[test]
    fn the_fields_cover_the_record() {
        for layout in LAYOUTS {
            let mut spans: Vec<_> = fields(layout).map(|f| (f.offset, f.len)).collect();
            spans.sort_unstable();
            let end = spans
                .iter()
                .try_fold(4, |at, &(offset, len)| (offset == at).then_some(at + len));
            assert_eq!(end, Some(usize::from(layout.len)), "{layout:?}");
        }
    }

    /// [`layout`] picks by width alone.
    #[test]
    fn no_two_layouts_of_a_version_share_a_width() {
        for version in Version::ALL {
            let held = LAYOUTS
                .iter()
                .filter(|l| l.versions.intersects(version.versions()));
            for layout in held {
                assert_eq!(super::layout(version, layout.len), Some(layout));
            }
        }
    }

    #[test]
    fn every_value_has_one_meaning_per_version() {
        let tables = LAYOUTS
            .iter()
            .flat_map(fields)
            .filter_map(|field| field.values);
        for table in tables {
            assert_one_meaning(table, |entry| (entry.value, entry.versions));
        }
    }

    #[test]
    fn a_field_reads_big_endian_after_the_text_offset() {
        let id = field(0x04, 2, "Id");
        let kind = field(0x06, 1, "Kind");
        assert_eq!(id.read(&[0x12, 0x34, 9]), Some(0x1234));
        assert_eq!(kind.read(&[0x12, 0x34, 9]), Some(9));
        assert_eq!(kind.read(&[0x12, 0x34]), None);
    }
}
