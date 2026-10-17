//! The palette the color tag indexes, see `getFontCCColorTable`.

use crate::Entry;
use crate::entry::entry;

#[rustfmt::skip]
pub(super) static COLORS: &[Entry] = &[
    entry(0, "Default").notes("White, or the box kind's own: Midna, Animal"),
    entry(1, "Red"),
    entry(2, "Green"),
    entry(3, "Blue"),
    entry(4, "Yellow"),
    entry(5, "Sky"),
    entry(6, "Purple"),
    entry(7, "White"),
    entry(8, "Orange"),
];

pub(super) static RGB: &[[u8; 3]] = &[
    WHITE,
    [0xF0, 0x78, 0x78],
    [0xAA, 0xDC, 0x8C],
    [0xA0, 0xB4, 0xDC],
    [0xDC, 0xDC, 0x82],
    [0xB4, 0xC8, 0xE6],
    [0xC8, 0xA0, 0xDC],
    WHITE,
    [0xDC, 0xAA, 0x78],
];

/// Midna, then Animal, as `BOX_KINDS` numbers them.
pub(super) static BOX_RGB: &[(u8, [u8; 3])] = &[(13, [0x82, 0xE6, 0xE6]), (14, [0x96, 0xDC, 0x64])];

pub(super) const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
