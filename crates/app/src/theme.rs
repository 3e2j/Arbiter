//! The editor's look, handed to gui at startup. Panels read it back from
//! [`Ui::theme`](gui::ui::Ui::theme).

use gui::canvas::Color;
use gui::ui::{Colors, Fonts, Sizes, Theme};

// TODO: from the user's Settings, once the editor reads them.
const COLORS: Colors = Colors {
    page: Color::hex(0x0e_0f_11),
    surface: Color::hex(0x17_18_1b),
    line: Color::hex(0x2a_2c_31),
    text: Color::hex(0xe6_e6_e4),
    dim: Color::hex(0x9a_9b_98),
    hover: Color::hex(0x24_26_2b),
    selected: Color::hex(0x33_36_3d),
    thumb: Color::hex(0xe6_e6_e4).alpha(0x30),
    thumb_hover: Color::hex(0xe6_e6_e4).alpha(0x60),
    accent: Color::hex(0x44_d4_d8),
};

/// Errors in the Output.
pub const ERROR: Color = Color::hex(0xf0_5a_5a);
/// Warnings in the Output.
pub const WARNING: Color = Color::hex(0xe8_b3_4b);

const SIZES: Sizes = Sizes {
    text: 13.,
    icon: 16.,
    icon_gap: 4.,
    row: 22.,
    indent: 16.,
    gap: 8.,
    radius: 4.,
    line: 1.,
    scroll_bar: 6.,
};

/// With the fonts from [`assets::fonts`](crate::assets::fonts).
pub const fn theme(font: Fonts) -> Theme {
    Theme {
        color: COLORS,
        size: SIZES,
        font,
    }
}
