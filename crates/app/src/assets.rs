//! The fonts and icons built into the editor, from the workspace's `assets/`.
//! Each folder there carries its license.

use std::sync::OnceLock;

use gui::canvas::{Error, FontFile, Glyphs, IconId};
use gui::ui::Fonts;

/// Every [`Icon`]'s id, set by [`load_icons`].
static ICONS: OnceLock<[IconId; Icon::ALL.len()]> = OnceLock::new();

/// A file under `assets/`.
macro_rules! asset {
    ($path:expr) => {
        include_bytes!(concat!("../../../assets/", $path))
    };
}

// TODO: from the user's Settings, once the editor reads them. Noto Sans JP
// defaults to its thinnest weight, so the weight is always set.
const WEIGHT: f32 = 400.;

/// Adds the fonts to `glyphs`. UI text falls back to Noto Sans JP, such as
/// for Japanese file names, and code to Noto Sans then Noto Sans JP, so a
/// missing character keeps the closest style there is. The system's fonts
/// take what all of them lack.
pub fn fonts(glyphs: &mut Glyphs) -> Result<Fonts, Error> {
    let ui = glyphs.add_font(
        FontFile {
            name: "Noto Sans",
            data: asset!("fonts/noto-sans/NotoSans[wdth,wght].ttf"),
        },
        WEIGHT,
    )?;
    let buffer = glyphs.add_font(
        FontFile {
            name: "JetBrains Mono",
            data: asset!("fonts/jetbrains-mono/JetBrainsMono[wght].ttf"),
        },
        WEIGHT,
    )?;
    let jp = glyphs.add_font(
        FontFile {
            name: "Noto Sans JP",
            data: asset!("fonts/noto-sans-jp/NotoSansJP[wght].ttf"),
        },
        WEIGHT,
    )?;
    glyphs.set_fallbacks(ui, &[jp]);
    glyphs.set_fallbacks(buffer, &[ui, jp]);
    Ok(Fonts { ui, buffer })
}

/// The icons in `assets/icons`, all drawn on a 24 unit grid.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Alert,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    CircleX,
    Close,
    Container,
    ContainerOpen,
    Disc,
    File,
    Filter,
    Folder,
    FolderOpen,
    Hammer,
    Menu,
    Message,
    More,
    Plus,
    Search,
    Settings,
    View,
}

impl Icon {
    pub const ALL: [Self; 22] = [
        Self::Alert,
        Self::Check,
        Self::ChevronDown,
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::CircleX,
        Self::Close,
        Self::Container,
        Self::ContainerOpen,
        Self::Disc,
        Self::File,
        Self::Filter,
        Self::Folder,
        Self::FolderOpen,
        Self::Hammer,
        Self::Menu,
        Self::Message,
        Self::More,
        Self::Plus,
        Self::Search,
        Self::Settings,
        Self::View,
    ];

    /// Its name, for errors, and its SVG.
    const fn file(self) -> (&'static str, &'static [u8]) {
        macro_rules! tabler {
            ($name:literal) => {
                ($name, asset!(concat!("icons/tabler/", $name, ".svg")))
            };
        }
        macro_rules! tabler_edited {
            ($name:literal) => {
                (
                    $name,
                    asset!(concat!("icons/tabler-edited/", $name, ".svg")),
                )
            };
        }
        match self {
            Self::Alert => tabler!("alert-triangle"),
            Self::Check => tabler!("check"),
            Self::ChevronDown => tabler!("chevron-down"),
            Self::ChevronLeft => tabler!("chevron-left"),
            Self::ChevronRight => tabler!("chevron-right"),
            Self::CircleX => tabler!("circle-x"),
            Self::Close => tabler!("x"),
            Self::Container => tabler!("package"),
            Self::ContainerOpen => tabler_edited!("package-open"),
            Self::Disc => tabler!("disc"),
            Self::File => tabler!("file-text"),
            Self::Filter => tabler!("filter"),
            Self::Folder => tabler!("folder"),
            Self::FolderOpen => tabler!("folder-open"),
            Self::Hammer => tabler!("hammer"),
            Self::Menu => tabler!("menu-2"),
            Self::Message => tabler!("message"),
            Self::More => tabler!("dots"),
            Self::Plus => tabler!("plus"),
            Self::Search => tabler!("search"),
            Self::Settings => tabler!("settings"),
            Self::View => tabler!("adjustments-horizontal"),
        }
    }
}

/// Adds every [`Icon`] to `glyphs`, so each converts to its [`IconId`].
pub fn load_icons(glyphs: &mut Glyphs) -> Result<(), Error> {
    let mut ids = [IconId::default(); Icon::ALL.len()];
    for (id, icon) in ids.iter_mut().zip(Icon::ALL) {
        let (name, svg) = icon.file();
        *id = glyphs.add_icon(name, svg)?;
    }
    // Already set by an earlier window, which loaded the same icons in the
    // same order.
    let _ = ICONS.set(ids);
    Ok(())
}

impl From<Icon> for IconId {
    /// The default id until [`load_icons`] runs, which the editor does
    /// before drawing anything.
    fn from(icon: Icon) -> Self {
        ICONS
            .get()
            .map_or_else(Self::default, |ids| ids[icon as usize])
    }
}
