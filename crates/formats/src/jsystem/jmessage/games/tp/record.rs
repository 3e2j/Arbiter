//! `JMSMesgEntry_c`, the INF1 record for every message file but
//! `zel_unit.bmg`: how one message is displayed. `zel_unit.bmg` has its own,
//! see `unit.rs`.

use crate::games::entry::entry;
use crate::games::tp::{ALL, JPN};
use crate::games::{Entry, Versions};
use crate::jsystem::jmessage::games::{Field, Layout, field};

/// The message id, which a file with a MID1 repeats from its MID1 entry.
const ID: Field =
    field(0x04, 2, "Message id").notes("Id the game looks the message up by. Same as the MID1 id");

const BOX_KIND: Field = field(0x09, 1, "Box kind").values(BOX_KINDS);

#[rustfmt::skip]
pub(super) const LAYOUT: Layout = Layout { len: 20, id: Some(ID), box_kind: Some(BOX_KIND), versions: Versions::ANY, fields: &[
    ID,
    field(0x06, 2, "Event label")   .notes("`saveBitLabels` index set when the message displays"),
    field(0x08, 1, "Speaker")       .notes("`Z2SpeechMgr2` voice bank id"),
    BOX_KIND,
    field(0x0A, 1, "Draw type")     .values(DRAW_TYPES).notes("Text pacing"),
    field(0x0B, 1, "Box position")  .values(BOX_POSITIONS),
    field(0x0C, 1, "Item")          .notes("Unused. Legacy `dItemNo`, 0xFF for none"),
    field(0x0D, 1, "Line alignment").values(LINE_ALIGNMENTS).notes("Also copied to `jmessage_tReference::mForm`"),
    field(0x0E, 1, "Speaker mood")  .notes("Grunt emotion index for the voice bank"),
    field(0x0F, 1, "Camera")        .notes("1 to 10 talk-actor slot, 11 and up talk-camera style"),
    field(0x10, 1, "Talk animation").notes("NPC talk motion attribute"),
    field(0x11, 1, "Face animation").notes("NPC talk face attribute"),
    field(0x12, 1, "Lines per page").notes("Unused. The runtime uses `getLineMax()`"),
    field(0x13, 1, "Padding"),
]};

/// Screen class, see `dMsgObject_c::talkStartInit`.
#[rustfmt::skip]
pub(super) static BOX_KINDS: &[Entry] = &[
    entry(0,  "Talk")        .notes("Ordinary dialogue box"),
    entry(1,  "Demo caption").notes("Boxless cutscene caption"),
    entry(2,  "Sign")        .notes("Signs and posted notices"),
    entry(5,  "Plain")       .notes("Boxless system text"),
    entry(6,  "Kanban")      .notes("Signboard screen class, unused"),
    entry(7,  "Staff roll"),
    entry(8,  "Light spirit").notes("Spirit text window and glow"),
    entry(9,  "Item get")    .notes("Centered item-get box"),
    entry(11, "Item name")   .notes("UI string fetch, no box"),
    entry(12, "Place name")  .notes("Area intro banner"),
    entry(13, "Midna")       .notes("Midna dialogue colors and glow"),
    entry(14, "Animal")      .notes("Wolf-form animal speech glow"),
    entry(15, "Notice")      .notes("Floating gameplay notice, can't be used during dialogue"),
    entry(16, "Save")        .notes("Save and memory card prompts"),
    entry(17, "Howl")        .notes("Howling stone UI"),
    entry(19, "Boss name")   .notes("Boss intro banner"),
];

/// Text pacing, see `jmessage_tSequenceProcessor::do_begin`.
#[rustfmt::skip]
pub(super) static DRAW_TYPES: &[Entry] = &[
    entry(0, "Typed")         .notes("Types per character, A skips typing"),
    entry(1, "Instant")       .notes("Whole page at once (menus, prompts)"),
    entry(2, "Typed, no skip").notes("Types per character, A does not skip"),
    entry(3, "Fade")          .notes("Page fades in"),
    entry(4, "UI name")       .notes("UI string fetch (item names), no box pacing"),
    entry(5, "Typed slow")    .notes("Weighted slow typing (light spirit speech)"),
    entry(7, "UI action")     .notes("UI string fetch (action button labels)"),
    entry(9, "Fade slow")     .notes("Slow page fade (staff credits)"),
];

/// See `dMsgObject_c::fukiPosCalc`.
#[rustfmt::skip]
pub(super) static BOX_POSITIONS: &[Entry] = &[
    entry(0, "Bottom"),
    entry(1, "Top"),
    entry(2, "Middle"),
    entry(3, "Auto")  .notes("Top or bottom, whichever avoids the speaker on screen"),
];

#[rustfmt::skip]
pub(super) static LINE_ALIGNMENTS: &[Entry] = &[
    entry(0, "Centered").only(JPN),
    entry(0, "Left")    .only(ALL.without(JPN)).notes("Outside JPN the game draws 0 as 1"),
    entry(1, "Left"),
];
