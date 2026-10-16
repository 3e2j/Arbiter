//! What stands in for the system's frame, for a window drawn without one, as
//! [`App::DECORATED`](crate::host::App::DECORATED) says. Only what it does to
//! the window: what it looks like is the app's.

use crate::input::{CursorIcon, PointerButton, WindowAction, WindowEdge};
use crate::ui::{Align, Anchor, Element, Length, Ui};

/// Makes the open box move the window as a title bar would, anywhere the
/// boxes declared in it before don't take the press: once dragged, it moves
/// the window, and double clicked, maximizes it or puts it back. Returns
/// whether it's held, for it to shade.
pub fn window_drag(ui: &mut Ui) -> bool {
    // Held from the pass after the press, once the boxes inside have had the
    // chance to take it.
    let held = ui.held(PointerButton::Left);
    // Waits for the pointer to move, since the system keeps the release of a
    // move, and with it any double click.
    let moved = ui
        .dragged(PointerButton::Left)
        .is_some_and(|[x, y]| x.abs() + y.abs() > 0.);
    if held && moved {
        ui.window(WindowAction::Move);
    }
    if ui.clicked(PointerButton::Left) && ui.input().pressed_again(PointerButton::Left) {
        ui.window(WindowAction::ToggleMaximize);
    }
    held
}

/// Each edge, where it is across then down, and its cursor.
const EDGES: [(WindowEdge, [Align; 2], CursorIcon); 8] = [
    (
        WindowEdge::North,
        [Align::Center, Align::Start],
        CursorIcon::ResizeV,
    ),
    (
        WindowEdge::NorthEast,
        [Align::End, Align::Start],
        CursorIcon::ResizeNeSw,
    ),
    (
        WindowEdge::East,
        [Align::End, Align::Center],
        CursorIcon::ResizeH,
    ),
    (
        WindowEdge::SouthEast,
        [Align::End, Align::End],
        CursorIcon::ResizeNwSe,
    ),
    (
        WindowEdge::South,
        [Align::Center, Align::End],
        CursorIcon::ResizeV,
    ),
    (
        WindowEdge::SouthWest,
        [Align::Start, Align::End],
        CursorIcon::ResizeNeSw,
    ),
    (
        WindowEdge::West,
        [Align::Start, Align::Center],
        CursorIcon::ResizeH,
    ),
    (
        WindowEdge::NorthWest,
        [Align::Start, Align::Start],
        CursorIcon::ResizeNwSe,
    ),
];

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
    for (edge, sides, cursor) in EDGES {
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
            cursor: Some(cursor),
            ..Element::DEFAULT
        };
        ui.element(element, |ui| {
            if ui.pressed(PointerButton::Left) {
                ui.window(WindowAction::Resize(edge));
            }
        });
    }
}
