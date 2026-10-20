//! `zel_unit.bmg`: the words `dMsgUnit_c::setTag` puts after a number in
//! message text, like "3 arrows".
//!
//! The file has no MID1, so the game finds a record by its position. That
//! position is the unit [`UNITS`] names. Every field is an offset into STR1.

use crate::games::Entry;
use crate::games::entry::entry;
use crate::games::tp::{ALL, JPN};
use crate::jsystem::jmessage::games::{Layout, field};

/// Units, by the record position the game passes to `dMsgUnit_setTag`. The
/// game handles 0x10000 and 0x10001 in code without reading the file.
#[rustfmt::skip]
pub(super) static UNITS: &[Entry] = &[
    entry(0,  "Arrows")        .notes("Arrow capacity, and the Count tag"),
    entry(1,  "Rupees")        .notes("Bomb prices, and the second-unit Count tag"),
    entry(2,  "Bugs")          .notes("Golden bugs"),
    entry(3,  "Hours")         .notes("Clock time. Only read on JPN, which adds unit 4 after it"),
    entry(4,  "Minutes")       .notes("Timers and clock time. Only read on JPN, which adds unit 5 after a timer"),
    entry(5,  "Seconds")       .notes("Only read on JPN, after unit 4"),
    entry(6,  "Goats")         .notes("20 minus event register 0xFF1F"),
    entry(7,  "Bombs")         .notes("Bomb counts and capacity"),
    entry(8,  "River points"),
    entry(9,  "Fish length")   .notes("Inches in English, centimeters in every other language"),
    entry(10, "Donation")      .notes("Rupees still to donate"),
    entry(11, "Letters")       .notes("New letters"),
    entry(12, "Poe souls"),
    entry(13, "Balloon points"),
    entry(14, "Fish")          .notes("Fish caught"),
];

/// `dMsgUnit_inf1_entry`. The decomp names the fields `startFrame` and
/// `endFrame`. The DAT1 text is empty and never read.
#[rustfmt::skip]
pub(super) const LAYOUT: Layout = Layout { len: 8, id: None, versions: ALL.without(JPN), fields: &[
    field(0x04, 2, "Singular").string().notes("Shown for 1. On PAL, also for 0 when the language is French"),
    field(0x06, 2, "Plural")  .string().notes("Shown for every other count"),
]};

/// The DAT1 text is the counter word, and the game shows the reading over it
/// as ruby. An empty reading shows none. The decomp's retail `REGION_JPN`
/// struct stops at 0x1A, but the file's records are 28 bytes.
#[rustfmt::skip]
pub(super) const JPN_LAYOUT: Layout = Layout { len: 28, id: None, versions: JPN, fields: &[
    field(0x04, 2, "Reading for 0")     .string(),
    field(0x06, 2, "Reading, ends in 1").string(),
    field(0x08, 2, "Reading, ends in 2").string(),
    field(0x0A, 2, "Reading, ends in 3").string(),
    field(0x0C, 2, "Reading, ends in 4").string(),
    field(0x0E, 2, "Reading, ends in 5").string(),
    field(0x10, 2, "Reading, ends in 6").string(),
    field(0x12, 2, "Reading, ends in 7").string(),
    field(0x14, 2, "Reading, ends in 8").string(),
    field(0x16, 2, "Reading, ends in 9").string(),
    field(0x18, 2, "Reading, ends in 0").string().notes("For counts past 0"),
    field(0x1A, 2, "Padding")                    .notes("Always 0, and nothing reads it"),
]};
