//! The fonts and icons built into the editor, from the workspace's `assets/`.
//! Each folder there carries its license.

use gui::canvas::{Error, FontFile, FontId, Glyphs, IconId};

/// A file under `assets/`.
macro_rules! asset {
    ($path:expr) => {
        include_bytes!(concat!("../../../assets/", $path))
    };
}

// TODO: from the user's Settings, once the editor reads them. Noto Sans JP
// defaults to its thinnest weight, so the weight is always set.
const WEIGHT: f32 = 400.;

/// The fonts text is drawn in.
#[derive(Clone, Copy)]
pub struct Fonts {
    /// Labels, tabs and the file tree.
    pub ui: FontId,
    /// Code and logs, where text lines up in columns.
    pub buffer: FontId,
}

impl Fonts {
    /// Adds the fonts to `glyphs`. UI text falls back to Noto Sans JP, such as
    /// for Japanese file names, and code to Noto Sans then Noto Sans JP, so a
    /// missing character keeps the closest style there is. The system's fonts
    /// take what all of them lack.
    pub fn load(glyphs: &mut Glyphs) -> Result<Self, Error> {
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
        Ok(Self { ui, buffer })
    }
}

/// The icons in `assets/icons`, all drawn on a 24 unit grid.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Icon {
    Alert,
    Check,
    ChevronDown,
    ChevronRight,
    CircleX,
    Close,
    Container,
    ContainerOpen,
    Disc,
    File,
    Folder,
    FolderOpen,
    Hammer,
    Message,
    More,
    Plus,
    Search,
    Settings,
}

impl Icon {
    pub const ALL: [Self; 18] = [
        Self::Alert,
        Self::Check,
        Self::ChevronDown,
        Self::ChevronRight,
        Self::CircleX,
        Self::Close,
        Self::Container,
        Self::ContainerOpen,
        Self::Disc,
        Self::File,
        Self::Folder,
        Self::FolderOpen,
        Self::Hammer,
        Self::Message,
        Self::More,
        Self::Plus,
        Self::Search,
        Self::Settings,
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
            Self::ChevronRight => tabler!("chevron-right"),
            Self::CircleX => tabler!("circle-x"),
            Self::Close => tabler!("x"),
            Self::Container => tabler!("package"),
            Self::ContainerOpen => tabler_edited!("package-open"),
            Self::Disc => tabler!("disc"),
            Self::File => tabler!("file-text"),
            Self::Folder => tabler!("folder"),
            Self::FolderOpen => tabler!("folder-open"),
            Self::Hammer => tabler!("hammer"),
            Self::Message => tabler!("message"),
            Self::More => tabler!("dots"),
            Self::Plus => tabler!("plus"),
            Self::Search => tabler!("search"),
            Self::Settings => tabler!("settings"),
        }
    }
}

/// Each [`Icon`]'s id in the glyphs, indexed by the icon.
#[derive(Clone, Copy)]
pub struct Icons([IconId; Icon::ALL.len()]);

impl Icons {
    pub fn load(glyphs: &mut Glyphs) -> Result<Self, Error> {
        let mut ids = [IconId::default(); Icon::ALL.len()];
        for (id, icon) in ids.iter_mut().zip(Icon::ALL) {
            let (name, svg) = icon.file();
            *id = glyphs.add_icon(name, svg)?;
        }
        Ok(Self(ids))
    }

    pub const fn get(&self, icon: Icon) -> IconId {
        self.0[icon as usize]
    }
}
