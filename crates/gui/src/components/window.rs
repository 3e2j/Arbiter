//! What stands in for the system's frame, for a window drawn without one, as
//! [`App::DECORATED`](crate::host::App::DECORATED) says.

use crate::input::{CursorIcon, PointerButton, WindowAction, WindowEdge};
use crate::ui::{Align, Anchor, Element, Length, Ui};

use super::toolbar;

/// A [`toolbar`] that, anywhere the boxes in `body` don't take the press,
/// shades while pressed, moves the window once dragged, and maximizes it or
/// puts it back when double clicked.
#[track_caller]
pub fn title_bar<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let (c, s) = (ui.theme().color, ui.theme().size);
    toolbar(ui, |ui| {
        let out = body(ui);
        // Held from the pass after the press, once the boxes in `body` have
        // had the chance to take it.
        let held = ui.held(PointerButton::Left);
        // Keeps inside the window's corners, which are only round while
        // it isn't maximized.
        let radius = if ui.input().maximized() {
            0.
        } else {
            s.window_radius
        };
        let style = ui.style();
        style.background = held.then_some(c.hover);
        style.radius = [radius, radius, 0., 0.];
        // Waits for the pointer to move, since the system keeps the release
        // of a move, and with it any double click.
        let moved = ui
            .dragged(PointerButton::Left)
            .is_some_and(|[x, y]| x.abs() + y.abs() > 0.);
        if held && moved {
            ui.window(WindowAction::Move);
        }
        if ui.clicked(PointerButton::Left) && ui.input().pressed_again(PointerButton::Left) {
            ui.window(WindowAction::ToggleMaximize);
        }
        out
    })
}

/// Strips `width` wide inside the open box's edges, and corners twice that,
/// that resize the window when dragged. Nothing while it's maximized, or
/// before the open box has a size.
///
/// Declare it last in the window's box, so it sits over everything else.
#[track_caller]
pub fn window_edges(ui: &mut Ui, width: f32) {
    let Some(rect) = ui.rect() else { return };
    if ui.input().maximized() {
        return;
    }
    for edge in WindowEdge::ALL {
        let sides = edge.sides();
        let [(x, w), (y, h)] = [0, 1].map(|axis| {
            let extent = [rect.w, rect.h][axis];
            // Thin along an edge, square at a corner.
            let depth = if sides[1 - axis] == Align::Center {
                width
            } else {
                2. * width
            };
            match sides[axis] {
                Align::Start => (0., depth),
                Align::Center => (2. * width, extent - 4. * width),
                Align::End => (extent - depth, depth),
            }
        });
        let element = Element {
            size: [Length::Fixed(w), Length::Fixed(h)],
            float: Some(Anchor::Parent {
                parent: [Align::Start; 2],
                own: [Align::Start; 2],
                offset: [x, y],
                clipped: false,
            }),
            cursor: Some(edge.cursor()),
            ..Element::DEFAULT
        };
        ui.element(element, |ui| {
            if ui.pressed(PointerButton::Left) {
                ui.window(WindowAction::Resize(edge));
            }
        });
    }
}

impl WindowEdge {
    const ALL: [Self; 8] = [
        Self::North,
        Self::NorthEast,
        Self::East,
        Self::SouthEast,
        Self::South,
        Self::SouthWest,
        Self::West,
        Self::NorthWest,
    ];

    /// Where it is across, then down.
    const fn sides(self) -> [Align; 2] {
        match self {
            Self::North => [Align::Center, Align::Start],
            Self::NorthEast => [Align::End, Align::Start],
            Self::East => [Align::End, Align::Center],
            Self::SouthEast => [Align::End, Align::End],
            Self::South => [Align::Center, Align::End],
            Self::SouthWest => [Align::Start, Align::End],
            Self::West => [Align::Start, Align::Center],
            Self::NorthWest => [Align::Start, Align::Start],
        }
    }

    const fn cursor(self) -> CursorIcon {
        match self {
            Self::North | Self::South => CursorIcon::ResizeV,
            Self::East | Self::West => CursorIcon::ResizeH,
            Self::NorthWest | Self::SouthEast => CursorIcon::ResizeNwSe,
            Self::NorthEast | Self::SouthWest => CursorIcon::ResizeNeSw,
        }
    }
}
