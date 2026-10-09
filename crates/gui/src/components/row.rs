//! A row of an icon and a line of text, the unit of every list, as tall as
//! the theme's rows so every list lines up.

use crate::canvas::IconId;
use crate::input::{Button, Cursor};
use crate::ui::{Align, Direction, Element, Size, Ui};

#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub icon: Option<IconId>,
    pub label: &'a str,
    /// How many levels it's nested, each one indent further right.
    pub depth: u16,
    pub selected: bool,
}

impl<'a> Row<'a> {
    /// Just `label`, at the top level, not selected.
    #[must_use]
    pub const fn new(label: &'a str) -> Self {
        Self {
            icon: None,
            label,
            depth: 0,
            selected: false,
        }
    }
}

/// Fills its parent across. Returns whether it was pressed this pass.
#[track_caller]
pub fn row(ui: &mut Ui, row: Row) -> bool {
    let theme = ui.theme();
    let (c, s) = (theme.color, theme.size);
    let element = Element {
        direction: Direction::LeftToRight,
        size: [Size::Grow, Size::Fixed(s.row)],
        padding: [
            s.gap / 2. + f32::from(row.depth) * s.indent,
            0.,
            s.gap / 2.,
            0.,
        ],
        gap: s.icon_gap,
        align: [Align::Start, Align::Center],
        radius: [s.radius; 4],
        cursor: Some(Cursor::Pointer),
        ..Element::DEFAULT
    };
    ui.element(element, |ui| {
        ui.style().background = if row.selected {
            Some(c.selected)
        } else {
            ui.hovered().then_some(c.hover)
        };
        if let Some(icon) = row.icon {
            ui.icon(icon, s.icon, c.dim);
        }
        ui.text(theme.ui_text(c.text), row.label);
        ui.pressed(Button::Left)
    })
}
