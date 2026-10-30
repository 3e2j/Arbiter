//! Twilight Princess's BMG tables.
//!
//! INF1 records are `JMSMesgEntry_c` in `d_msg_class.h`, except in `zel_unit.bmg`.
//! Text tags are handled in `jmessage_tSequenceProcessor` and
//! `jmessage_tRenderingProcessor`.

use crate::games::Versions;
use crate::games::entry::{Entry, entry};
use crate::games::tp::{ALL, GCN_PAL, JPN, Version};

/// The layout `version` reads `record_len` wide INF1 records with, or `None`
/// when it reads none that wide. The game reads each file with a fixed
/// struct, so the width alone picks one.
#[must_use]
pub fn layout(version: Version, record_len: u16) -> Option<&'static Layout> {
    LAYOUTS
        .iter()
        .find(|layout| layout.len == record_len && layout.versions.intersects(version.versions()))
}

/// The rows of a [`Field`]'s `values` table that `version` reads.
pub fn values(version: Version, table: &'static [Entry]) -> impl Iterator<Item = &'static Entry> {
    table
        .iter()
        .filter(move |entry| entry.versions.intersects(version.versions()))
}

/// The tag `version` reads for `group` and `code`, or `None` when nothing
/// names it. The group and code are the ones `bmg::TextSegment::Tag` holds.
#[must_use]
pub fn find_tag(version: Version, group: u8, code: u16) -> Option<&'static Tag> {
    tags(version).find(|tag| tag.group == group && (tag.any_code || tag.code == code))
}

/// Every tag `version` reads, for a picker to list.
pub fn tags(version: Version) -> impl Iterator<Item = &'static Tag> {
    TAGS.iter()
        .filter(move |tag| tag.versions.intersects(version.versions()))
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
    /// The table naming the field's values, read through [`values`].
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

/// One kind of text tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    pub group: u8,
    /// 0 and meaningless when `any_code` is set.
    pub code: u16,
    /// Every code in the group is this tag, and the code is its value, like
    /// a sound effect id.
    pub any_code: bool,
    pub name: &'static str,
    pub args: Args,
    pub notes: Option<&'static str>,
    /// As [`Entry::versions`].
    pub versions: Versions,
}

/// What a tag's argument bytes hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Args {
    None,
    /// One byte, which some tags carry but no handler reads.
    U8,
    U16,
    U32,
    /// A u8, then the reading as 2-byte characters. The renderer keeps the
    /// u8, likely the base character count. The reading can end on half a
    /// character.
    Ruby,
}

const fn tag(group: u8, code: u16, name: &'static str, args: Args) -> Tag {
    Tag {
        group,
        code,
        any_code: false,
        name,
        args,
        notes: None,
        versions: Versions::ANY,
    }
}

impl Tag {
    const fn any_code(self) -> Self {
        Self {
            any_code: true,
            ..self
        }
    }

    const fn notes(self, notes: &'static str) -> Self {
        Self {
            notes: Some(notes),
            ..self
        }
    }

    const fn only(self, versions: Versions) -> Self {
        Self { versions, ..self }
    }
}

/// Sorted by group, then code.
#[rustfmt::skip]
static TAGS: &[Tag] = &[
    tag(0,   0,  "Player name",            Args::None).notes("Inserts the player's name"),
    tag(0,   1,  "Instant",                Args::None).notes("Rest of the page appears at once"),
    tag(0,   2,  "Type",                   Args::None).notes("Back to per-character typing"),
    tag(0,   3,  "Auto box (unknown)",     Args::U16) .notes("Handled the same as auto box"),
    tag(0,   4,  "Auto box",               Args::U16) .notes("Page advances by itself after the frames. 0 advances at once"),
    tag(0,   5,  "Box at most",            Args::U16) .notes("Page advances after the frames, or earlier on A"),
    tag(0,   6,  "Unknown pause",          Args::U16) .notes("Pauses typing, and sets the per-character pause to the frames"),
    tag(0,   7,  "Pause",                  Args::U16) .notes("Pauses typing for the frames"),
    tag(0,   8,  "Select 2",               Args::U8)  .notes("Two-way choice option. 1 makes it the cursor's start"),
    tag(0,   9,  "Select 3",               Args::U8)  .notes("Three-way choice option. 1 makes it the cursor's start"),
    tag(0,   10, "A button",               Args::None),
    tag(0,   11, "B button",               Args::None),
    tag(0,   12, "C stick",                Args::None),
    tag(0,   13, "L button",               Args::None),
    tag(0,   14, "R button",               Args::None),
    tag(0,   15, "X button",               Args::None),
    tag(0,   16, "Y button",               Args::None),
    tag(0,   17, "Z button",               Args::None),
    tag(0,   18, "D-pad",                  Args::None),
    tag(0,   19, "Stick, all directions",  Args::None),
    tag(0,   20, "Left arrow",             Args::None),
    tag(0,   21, "Right arrow",            Args::None),
    tag(0,   22, "Up arrow",               Args::None),
    tag(0,   23, "Down arrow",             Args::None),
    tag(0,   24, "Stick up",               Args::None),
    tag(0,   25, "Stick down",             Args::None),
    tag(0,   26, "Stick left",             Args::None),
    tag(0,   27, "Stick right",            Args::None),
    tag(0,   28, "Stick vertical",         Args::None),
    tag(0,   29, "Stick horizontal",       Args::None),
    tag(0,   30, "Inline select 2, next",  Args::U8)  .notes("Second option of an inline two-way choice"),
    tag(0,   31, "Inline select 2, first", Args::U8)  .notes("First option of an inline two-way choice"),
    tag(0,   32, "Await choice",           Args::None).notes("Line break, then shows the choice options"),
    tag(0,   33, "Kana initial",           Args::None).only(JPN).notes("Inserts the player name's first character when it is kana"),
    tag(0,   33, "Unknown name",           Args::None).only(ALL.without(JPN)).notes("Calls `do_name1`, which does nothing outside JPN"),
    tag(0,   34, "Horse name",             Args::None).notes("Inserts Epona's name"),
    tag(0,   35, "Red target",             Args::None),
    tag(0,   36, "Yellow target",          Args::None),
    tag(0,   37, "Input value",            Args::U32) .notes("Number entry prompt. 1 also sets temp flag label 80"),
    tag(0,   38, "Acknowledge",            Args::None).notes("Waits for a button before continuing"),
    tag(0,   39, "A button, star",         Args::None),
    tag(0,   40, "Demo box",               Args::U32) .notes("Box closes after the frames (cutscene captions)"),
    tag(0,   41, "Scent name",             Args::None).notes("Inserts the current scent's name"),
    tag(0,   42, "White target",           Args::None),
    tag(0,   43, "Portal name",            Args::None).notes("Inserts a warp portal's name"),
    tag(0,   44, "Warp icon",              Args::None),
    tag(0,   45, "Bomb name",              Args::None).notes("Inserts the selected bomb type's name"),
    tag(0,   46, "X/Y buttons",            Args::None),
    tag(0,   47, "Y/X buttons",            Args::None),
    tag(0,   48, "Bomb bag icon",          Args::U8)  .notes("Adds bag `arg - 1` to the bomb selection"),
    tag(0,   49, "Bomb count",             Args::None).notes("Inserts the selected bomb count"),
    tag(0,   50, "Bomb price",             Args::None).notes("Inserts the selected bomb price"),
    tag(0,   51, "Inline select 3, next",  Args::U8)  .notes("Later option of an inline three-way choice"),
    tag(0,   52, "Inline select 3, first", Args::U8)  .notes("First option of an inline three-way choice"),
    tag(0,   53, "Unknown",                Args::None),
    tag(0,   54, "Box at least",           Args::U16) .notes("A can't advance the page until the frames pass"),
    tag(0,   55, "Bomb max",               Args::U8)  .notes("Bag capacity: 0 bombs, 1 water bombs, 2 bomblings"),
    tag(0,   56, "Arrow max",              Args::None).notes("Inserts the quiver capacity"),
    tag(0,   57, "Heart",                  Args::None),
    tag(0,   58, "Quaver",                 Args::None).notes("Music note"),
    tag(0,   59, "Insect name",            Args::None).notes("Inserts a golden bug's name"),
    tag(0,   60, "Letter name",            Args::None).notes("Inserts a letter's name (query 47 stores it)"),
    tag(0,   61, "Line down",              Args::None),
    tag(0,   62, "Letter page",            Args::None).notes("Inserts the current letter page"),
    tag(0,   63, "Letter page count",      Args::None).notes("Inserts the letter's page count"),
    tag(1,   0,  "Sound",                  Args::None).any_code().notes("Plays the sound effect the code's low byte names, at the speaker"),
    tag(2,   0,  "Camera",                 Args::None).any_code().notes("Passes the code's low byte to the talk camera"),
    tag(3,   0,  "Message id override",    Args::U32),
    tag(3,   1,  "Wii A",                  Args::None),
    tag(3,   2,  "Wii B",                  Args::None),
    tag(3,   3,  "Wii Home",               Args::None),
    tag(3,   4,  "Wii minus",              Args::None),
    tag(3,   5,  "Wii plus",               Args::None),
    tag(3,   6,  "Wii 1",                  Args::None),
    tag(3,   7,  "Wii 2",                  Args::None),
    tag(3,   8,  "Wii D-pad, item",        Args::None),
    tag(3,   9,  "Wii D-pad up",           Args::None),
    tag(3,   10, "Wii D-pad down",         Args::None),
    tag(3,   11, "Wii D-pad horizontal",   Args::None),
    tag(3,   12, "Wii D-pad right",        Args::None),
    tag(3,   13, "Wii D-pad left",         Args::None),
    tag(3,   14, "Wii Remote",             Args::None),
    tag(3,   15, "Wii reticule",           Args::None),
    tag(3,   16, "Nunchuk",                Args::None),
    tag(3,   17, "Wii Remote, second",     Args::None),
    tag(3,   18, "Wii fairy",              Args::None),
    tag(3,   19, "Wii C",                  Args::None),
    tag(3,   20, "Wii Z",                  Args::None),
    tag(4,   0,  "Dollar sign",            Args::None).notes("$"),
    tag(4,   1,  "Backslash",              Args::None).notes("\\"),
    tag(4,   2,  "At mark",                Args::None).notes("@"),
    tag(4,   3,  "Sharp",                  Args::None),
    tag(4,   4,  "Flat",                   Args::None),
    tag(4,   5,  "Square root",            Args::None),
    tag(4,   6,  "Percent",                Args::None).notes("%"),
    tag(4,   7,  "Hectare",                Args::None),
    tag(4,   8,  "Are",                    Args::None),
    tag(4,   9,  "Litre",                  Args::None),
    tag(4,   10, "Watt",                   Args::None),
    tag(4,   11, "Calorie",                Args::None),
    tag(4,   12, "Dollar",                 Args::None),
    tag(4,   13, "Cent",                   Args::None),
    tag(5,   0,  "Time",                   Args::U8)  .notes("0 message timer, 2 race time, else the meter timer"),
    tag(5,   1,  "Count",                  Args::None).notes("Message count number"),
    tag(5,   2,  "Count, second unit",     Args::None).notes("Message count number, unit 1"),
    tag(5,   3,  "Insect info",            Args::U8)  .notes("0 golden bugs found, else bugs left of 24"),
    tag(5,   4,  "Unknown value",          Args::None).notes("Unit 3 with value 0"),
    tag(5,   5,  "Tears needed",           Args::None).notes("Tears of light still needed"),
    tag(5,   6,  "Unknown register",       Args::None).notes("20 minus event register 0xFF1F"),
    tag(5,   7,  "River points",           Args::None).notes("Current count"),
    tag(5,   8,  "Fish length",            Args::None).notes("Message count number"),
    tag(5,   9,  "Donations remaining",    Args::U32) .notes("The argument minus donations so far"),
    tag(5,   10, "New letter count",       Args::None),
    tag(5,   11, "Poe souls",              Args::None),
    tag(5,   12, "Balloon score",          Args::U8)  .notes("0 message count number, else the balloon score"),
    tag(5,   13, "Fish count",             Args::None).notes("Message count number"),
    tag(5,   14, "Rollgoal level",         Args::None).notes("Message count number"),
    tag(6,   0,  "Player genitive",        Args::None).only(GCN_PAL).notes("Player's name in the possessive, German rules when German"),
    tag(6,   0,  "Player genitive",        Args::None).only(ALL.without(GCN_PAL)).notes("Only GCN PAL fills it in. Elsewhere the game prints an uninitialized buffer"),
    tag(6,   1,  "Horse genitive",         Args::None).only(GCN_PAL).notes("Epona's name in the possessive, German rules when German"),
    tag(6,   1,  "Horse genitive",         Args::None).only(ALL.without(GCN_PAL)).notes("Only GCN PAL fills it in. Elsewhere the game prints an uninitialized buffer"),
    tag(6,   2,  "Male icon",              Args::None),
    tag(6,   3,  "Female icon",            Args::None),
    tag(6,   4,  "Star icon",              Args::None),
    tag(6,   5,  "Reference mark",         Args::None),
    tag(6,   6,  "Thin left arrow",        Args::None),
    tag(6,   7,  "Thin right arrow",       Args::None),
    tag(6,   8,  "Thin up arrow",          Args::None),
    tag(6,   9,  "Thin down arrow",        Args::None),
    tag(6,   10, "Bullet",                 Args::None),
    tag(6,   11, "Bullet space",           Args::None),
    tag(255, 0,  "Color",                  Args::U8)  .notes("Text color from here on"),
    tag(255, 1,  "Scale",                  Args::U16) .notes("Text scale percent from here on. 100 is normal"),
    tag(255, 2,  "Ruby",                   Args::Ruby).notes("Furigana. The argument is the reading"),
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

    #[test]
    fn the_tags_are_sorted() {
        assert!(TAGS.is_sorted_by_key(|tag| (tag.group, tag.code)));
    }

    #[test]
    fn every_tag_has_one_meaning_per_version() {
        assert_one_meaning(TAGS, |tag| ((tag.group, tag.code), tag.versions));
    }

    #[test]
    fn a_tag_is_found_by_group_and_code() {
        let name = |group, code| find_tag(Version::GcnUsa, group, code).map(|tag| tag.name);
        assert_eq!(name(0, 7), Some("Pause"));
        assert_eq!(name(1, 20), Some("Sound"));
        assert_eq!(name(2, 7), Some("Camera"));
        assert_eq!(name(0, 64), None);
        assert_eq!(name(7, 0), None);
    }
}
