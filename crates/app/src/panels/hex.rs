//! A file's bytes, sixteen to a row: the offset, each byte in hex, then the
//! bytes as ASCII. What any file opens in until its format has an editor.

use std::{
    fmt::Write,
    io,
    path::{Path, PathBuf},
};

use gui::{
    components::{ListScroll, indexed_list, notice, rule},
    ui::{Element, TextStyle, Ui},
};

use crate::workspace::Places;

/// Bytes to a row.
const ROW: usize = 16;
/// Where a row's bytes get an extra space, to count them by eye.
const HALF: usize = ROW / 2;

enum Bytes {
    /// Asked of the [`Loader`](crate::loader::Loader), not back yet.
    Loading,
    Read(Vec<u8>),
    /// Why it couldn't be read.
    Failed(String),
}

pub struct Hex {
    path: PathBuf,
    /// The file's name.
    title: String,
    bytes: Bytes,
    scroll: ListScroll,
}

/// A row's columns, kept between rows so a pass formats into the same three.
#[derive(Default)]
struct Columns {
    offset: String,
    hex: String,
    ascii: String,
}

impl Hex {
    pub const KIND: &str = "hex";
    pub const PLACES: Places = Places::MAIN;

    /// Shows `path` once it's [`Self::load`]ed.
    pub fn new(path: PathBuf) -> Self {
        let title = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Self {
            path,
            title,
            bytes: Bytes::Loading,
            scroll: ListScroll::TOP,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    /// Shows what reading the file gave, from the top.
    pub fn load(&mut self, read: io::Result<Vec<u8>>) {
        self.bytes = match read {
            Ok(bytes) => Bytes::Read(bytes),
            Err(err) => Bytes::Failed(format!("Can't read {}: {err}", self.path.display())),
        };
        self.scroll = ListScroll::TOP;
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let bytes = match &self.bytes {
            Bytes::Loading => return notice(ui, "Loading…"),
            Bytes::Failed(err) => return notice(ui, err),
            Bytes::Read(bytes) if bytes.is_empty() => return notice(ui, "Empty file"),
            Bytes::Read(bytes) => bytes,
        };
        let theme = ui.theme();
        let (c, s) = (theme.color, theme.size);
        let style = |color| TextStyle {
            font: theme.font.buffer,
            ..theme.ui_text(color)
        };
        let mut columns = Columns::default();
        header(&mut columns);
        line(ui, style(c.dim), style(c.dim), &columns, false);
        rule(ui);
        let rows = bytes.len().div_ceil(ROW);
        indexed_list(ui, &mut self.scroll, rows, s.row, |ui, index| {
            let start = index * ROW;
            let row = bytes.get(start..).unwrap_or_default();
            columns.fill(start, row.get(..ROW).unwrap_or(row));
            line(ui, style(c.dim), style(c.text), &columns, true);
        });
    }
}

impl Columns {
    /// Up to [`ROW`] bytes from `offset`, the hex padded out to a whole row's
    /// width so the ASCII lines up under a short last row.
    fn fill(&mut self, offset: usize, row: &[u8]) {
        self.offset.clear();
        let _ = write!(self.offset, "{offset:08X}");
        self.hex.clear();
        self.ascii.clear();
        for at in 0..ROW {
            if at > 0 {
                self.hex.push(' ');
            }
            if at == HALF {
                self.hex.push(' ');
            }
            match row.get(at) {
                Some(byte) => {
                    let _ = write!(self.hex, "{byte:02X}");
                }
                None => self.hex.push_str("  "),
            }
        }
        let shown = |&byte: &u8| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '.'
            }
        };
        self.ascii.extend(row.iter().map(shown));
    }
}

/// Names the columns: each byte's place in its row, over its hex.
fn header(columns: &mut Columns) {
    columns.offset.clear();
    columns.offset.push_str("Offset  ");
    columns.hex.clear();
    for at in 0..ROW {
        if at > 0 {
            columns.hex.push(' ');
        }
        if at == HALF {
            columns.hex.push(' ');
        }
        let _ = write!(columns.hex, "{at:02X}");
    }
    columns.ascii.clear();
}

/// One row of `columns`, the offset in `dim` and the rest in `text`, lit
/// under the pointer while `hover`.
fn line(ui: &mut Ui, dim: TextStyle, text: TextStyle, columns: &Columns, hover: bool) {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        padding: [s.gap / 2., 0., s.gap / 2., 0.],
        gap: 2. * s.gap,
        clip: true,
        ..Element::line(s.row)
    };
    ui.element(element, |ui| {
        if hover {
            ui.style().background = ui.hovered().then_some(c.hover);
        }
        ui.text(dim, &columns.offset);
        ui.text(text, &columns.hex);
        if !columns.ascii.is_empty() {
            ui.text(dim, &columns.ascii);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_shows_its_offset_hex_and_ascii() {
        let mut columns = Columns::default();
        columns.fill(0x20, b"RARC\x00\x01 ~\xff0123456");
        assert_eq!(columns.offset, "00000020");
        assert_eq!(
            columns.hex,
            "52 41 52 43 00 01 20 7E  FF 30 31 32 33 34 35 36"
        );
        assert_eq!(columns.ascii, "RARC.. ~.0123456");
    }

    #[test]
    fn a_short_row_pads_its_hex_to_a_whole_row() {
        let mut columns = Columns::default();
        columns.fill(0x10, b"ab");
        assert_eq!(columns.hex.len(), ROW * 3);
        assert!(columns.hex.starts_with("61 62 "));
        assert_eq!(columns.ascii, "ab");
    }

    #[test]
    fn the_header_lines_up_with_a_row() {
        let (mut head, mut row) = (Columns::default(), Columns::default());
        header(&mut head);
        row.fill(0, &[0; ROW]);
        assert_eq!(head.offset.len(), row.offset.len());
        assert_eq!(head.hex.len(), row.hex.len());
    }
}
