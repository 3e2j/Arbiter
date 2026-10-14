//! The editor's look, handed to gui at startup. Panels read it back from
//! [`Ui::theme`](gui::ui::Ui::theme).

use gui::{
    canvas::Color,
    ui::{Theme, ThemeColors, ThemeFonts, ThemeSizes},
};

// TODO: from the user's Settings, once the editor reads them.
const COLORS: ThemeColors = ThemeColors {
    page: Color::hex(0x0e_0f_11),
    surface: Color::hex(0x17_18_1b),
    sunken: Color::hex(0x0e_0f_11),
    line: Color::hex(0x2a_2c_31),
    text: Color::hex(0xe6_e6_e4),
    dim: Color::hex(0x9a_9b_98),
    hover: Color::hex(0x24_26_2b),
    selected: Color::hex(0x33_36_3d),
    selection: Color::hex(0x44_d4_d8).alpha(0x3d),
    thumb: Color::hex(0xe6_e6_e4).alpha(0x30),
    thumb_hover: Color::hex(0xe6_e6_e4).alpha(0x60),
    accent: Color::hex(0x44_d4_d8),
    ok: Color::hex(0x42_be_65),
    warn: Color::hex(0xe8_b3_4b),
    err: Color::hex(0xf0_5a_5a),
};

const SIZES: ThemeSizes = ThemeSizes {
    text: 13.,
    icon: 16.,
    icon_gap: 4.,
    row: 22.,
    indent: 16.,
    gap: 8.,
    radius: 4.,
    line: 1.,
    scroll_bar: 6.,
    window_radius: 10.,
};

/// With the fonts from [`assets::fonts`](crate::assets::fonts).
pub const fn theme(font: ThemeFonts) -> Theme {
    Theme {
        color: COLORS,
        size: SIZES,
        font,
    }
}
