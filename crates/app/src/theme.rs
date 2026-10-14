//! The editor's look, handed to gui at startup and again whenever the user
//! picks a [`Palette`]. Panels read it back from
//! [`Ui::theme`](gui::ui::Ui::theme).

use gui::{
    canvas::Color,
    ui::{Theme, ThemeColors, ThemeFonts, ThemeSizes},
};

/// A set of colours the user picks in Settings.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Palette {
    #[default]
    Default,
    Twilight,
}

impl Palette {
    // In order of appearance
    pub const ALL: [Self; 2] = [Self::Default, Self::Twilight];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Twilight => "Twilight",
        }
    }

    const fn swatch(self) -> Swatch {
        match self {
            Self::Default => Swatch {
                base: 0x0e_0f_11,
                panel: 0x17_18_1b,
                sunken: 0x0e_0f_11,
                raised: 0x24_26_2b,
                line: 0x2a_2c_31,
                text: 0xe6_e6_e4,
                dim: 0x9a_9b_98,
                sel: 0x33_36_3d,
                accent: 0x44_d4_d8,
                ok: 0x42_be_65,
                warn: 0xe8_b3_4b,
                err: 0xf0_5a_5a,
            },
            Self::Twilight => Swatch {
                base: 0x0f_0d_13,
                panel: 0x16_13_1c,
                sunken: 0x0f_0d_13,
                raised: 0x21_1c_29,
                line: 0x2d_27_35,
                text: 0xe4_df_e9,
                dim: 0x97_8e_a5,
                sel: 0x17_3a_3e,
                accent: 0x44_d4_d8,
                ok: 0x9c_bf_7a,
                warn: 0xe8_89_3c,
                err: 0xe5_48_4d,
            },
        }
    }
}

/// A palette in the roles it was designed with, as RGB.
struct Swatch {
    /// Window bars, gaps between panels.
    base: u32,
    panel: u32,
    sunken: u32,
    /// Hover and buttons.
    raised: u32,
    line: u32,
    text: u32,
    dim: u32,
    /// The selected row.
    sel: u32,
    accent: u32,
    ok: u32,
    warn: u32,
    err: u32,
}

impl Swatch {
    const fn colors(&self) -> ThemeColors {
        let text = Color::hex(self.text);
        let accent = Color::hex(self.accent);
        ThemeColors {
            page: Color::hex(self.base),
            surface: Color::hex(self.panel),
            sunken: Color::hex(self.sunken),
            line: Color::hex(self.line),
            text,
            dim: Color::hex(self.dim),
            hover: Color::hex(self.raised),
            selected: Color::hex(self.sel),
            selection: accent.alpha(0x3d),
            thumb: text.alpha(0x30),
            thumb_hover: text.alpha(0x60),
            accent,
            ok: Color::hex(self.ok),
            warn: Color::hex(self.warn),
            err: Color::hex(self.err),
        }
    }
}

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

/// In `palette`, with the fonts from [`assets::fonts`](crate::assets::fonts).
pub const fn theme(palette: Palette, font: ThemeFonts) -> Theme {
    Theme {
        color: palette.swatch().colors(),
        size: SIZES,
        font,
    }
}
