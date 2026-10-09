use std::panic::Location;

use super::*;
use crate::canvas::{Canvas, FontFile, Kind};
use crate::input::Cursor;

const WINDOW: Rect = Rect::new(0., 0., 300., 100.);

fn fixed(w: f32, h: f32) -> Element {
    Element {
        size: [Size::Fixed(w), Size::Fixed(h)],
        ..Element::DEFAULT
    }
}

fn grow() -> Element {
    Element {
        size: [Size::Grow; 2],
        ..Element::DEFAULT
    }
}

/// The rect each node in `layout` was placed at, the window first.
fn rects(layout: &Layout) -> Vec<Rect> {
    layout.nodes.iter().map(|node| node.rect).collect()
}

/// The `n`th box from one line, so tests tell boxes apart by number.
fn slot(n: u32) -> Slot {
    Slot {
        site: Location::caller(),
        n,
        dup: 0,
    }
}

fn leaf(layout: &mut Layout, n: u32, element: Element) {
    layout.open(slot(n), element);
    layout.close();
}

#[test]
fn grow_shares_what_is_left() {
    let mut layout = Layout::default();
    layout.clear();
    layout.open(
        slot(0),
        Element {
            gap: 10.,
            ..Element::row()
        },
    );
    leaf(&mut layout, 1, fixed(80., 20.));
    leaf(&mut layout, 2, grow());
    leaf(&mut layout, 3, grow());
    layout.close();
    layout.solve(WINDOW, 1.);
    let [_, row, a, b, c] = rects(&layout)[..] else {
        panic!("five nodes")
    };
    assert_eq!(row, WINDOW);
    assert_eq!(a, Rect::new(0., 0., 80., 20.));
    assert_eq!(b, Rect::new(90., 0., 100., 100.));
    assert_eq!(c, Rect::new(200., 0., 100., 100.));
}

#[test]
fn fit_wraps_children_and_padding() {
    let mut layout = Layout::default();
    layout.clear();
    let column = Element {
        gap: 4.,
        ..Element::DEFAULT.padded(3.)
    };
    layout.open(slot(0), column);
    leaf(&mut layout, 1, fixed(10., 5.));
    leaf(&mut layout, 2, fixed(20., 5.));
    layout.close();
    layout.solve(WINDOW, 1.);
    let [_, column, a, b] = rects(&layout)[..] else {
        panic!("four nodes")
    };
    assert_eq!(column, Rect::new(0., 0., 26., 20.));
    assert_eq!([a.x, a.y, b.y], [3., 3., 12.]);
}

#[test]
fn children_align_in_the_space_left() {
    let mut layout = Layout::default();
    layout.clear();
    let row = Element {
        align: [Align::End, Align::Center],
        ..Element::row()
    };
    layout.open(slot(0), row);
    leaf(&mut layout, 1, fixed(50., 20.));
    layout.close();
    layout.solve(WINDOW, 1.);
    assert_eq!(rects(&layout)[2], Rect::new(250., 40., 50., 20.));
}

#[test]
fn edges_land_on_physical_pixels() {
    let mut layout = Layout::default();
    layout.clear();
    layout.open(slot(0), Element::row());
    for n in 0..3 {
        leaf(&mut layout, n, grow());
    }
    layout.close();
    layout.solve(Rect::new(0., 0., 100., 10.), 1.5);
    let rects = rects(&layout);
    for pair in rects[2..].windows(2) {
        assert_eq!(pair[0].right(), pair[1].x);
    }
    for rect in &rects {
        let physical = rect.x * 1.5;
        assert!((physical - physical.round()).abs() < 1e-4, "{rect:?}");
    }
}

#[test]
fn an_unchanged_pass_hasnt_moved() {
    let mut layout = Layout::default();
    let pass = |layout: &mut Layout| {
        layout.clear();
        leaf(layout, 0, fixed(10., 10.));
        layout.solve(WINDOW, 1.)
    };
    assert!(pass(&mut layout));
    assert!(!pass(&mut layout));
}

#[test]
fn a_box_finds_its_rect_after_one_is_inserted_before_it() {
    let mut layout = Layout::default();
    layout.clear();
    leaf(&mut layout, 0, fixed(10., 10.));
    layout.solve(WINDOW, 1.);
    layout.clear();
    leaf(&mut layout, 1, fixed(10., 30.));
    let (_, last) = layout.open(slot(0), fixed(10., 10.));
    assert_eq!(layout.last_rect(last), Some(Rect::new(0., 0., 10., 10.)));
}

#[test]
fn a_box_under_a_new_parent_is_new() {
    let mut layout = Layout::default();
    layout.clear();
    layout.open(slot(0), Element::DEFAULT);
    leaf(&mut layout, 0, fixed(10., 10.));
    layout.close();
    layout.solve(WINDOW, 1.);
    layout.clear();
    // Takes the old parent's index, so its child is looked up by slot.
    layout.open(slot(1), Element::DEFAULT);
    let (_, last) = layout.open(slot(0), fixed(10., 10.));
    assert_eq!(last, None);
}

#[test]
fn a_clip_cuts_its_children_but_not_itself() {
    let mut layout = Layout::default();
    layout.clear();
    let clipped = Element {
        clip: true,
        background: Some(Color::hex(0x10_20_30)),
        ..fixed(50., 50.)
    };
    layout.open(slot(0), clipped);
    leaf(
        &mut layout,
        99,
        Element {
            background: Some(Color::hex(0xff_ff_ff)),
            ..fixed(80., 80.)
        },
    );
    layout.close();
    leaf(
        &mut layout,
        99,
        Element {
            background: Some(Color::hex(0xff_ff_ff)),
            ..fixed(10., 10.)
        },
    );
    layout.solve(WINDOW, 1.);
    let mut canvas = Canvas::default();
    canvas.clear(WINDOW);
    layout.emit(&mut canvas, &mut Glyphs::default(), None);
    let batches: Vec<_> = canvas
        .batches()
        .map(|(clip, _, range)| (clip, range))
        .collect();
    assert_eq!(
        batches,
        [
            (WINDOW, 0..1),
            (Rect::new(0., 0., 50., 50.), 1..2),
            (WINDOW, 2..3),
        ]
    );
}

#[test]
fn a_float_draws_last_outside_its_parents_clip() {
    let mut layout = Layout::default();
    layout.clear();
    let menu = Element {
        clip: true,
        ..fixed(40., 20.)
    };
    layout.open(slot(0), menu);
    let popup = Element {
        float: Some(Anchor::BELOW),
        background: Some(Color::hex(0xff_ff_ff)),
        ..fixed(60., 60.)
    };
    leaf(&mut layout, 1, popup);
    layout.close();
    leaf(
        &mut layout,
        99,
        Element {
            background: Some(Color::hex(0x10_20_30)),
            ..fixed(10., 10.)
        },
    );
    layout.solve(WINDOW, 1.);
    let rects = rects(&layout);
    assert_eq!(rects[2], Rect::new(0., 20., 60., 60.));
    // Floating, so it takes no space in the column.
    assert_eq!(rects[3].y, 20.);
    let mut canvas = Canvas::default();
    canvas.clear(WINDOW);
    layout.emit(&mut canvas, &mut Glyphs::default(), None);
    let after = Quad::new(Rect::new(0., 20., 10., 10.), Color::hex(0x10_20_30));
    let popup = Quad::new(rects[2], Color::hex(0xff_ff_ff));
    assert_eq!(canvas.quads(), [after, popup]);
    let (clip, ..) = canvas.batches().last().unwrap();
    assert_eq!(clip, WINDOW);
}

#[test]
fn a_float_meets_its_parent_at_the_points_it_picks() {
    let mut layout = Layout::default();
    layout.clear();
    let button = Element {
        padding: [5.; 4],
        offset: [0., -30.],
        ..fixed(40., 20.)
    };
    leaf(&mut layout, 0, fixed(10., 50.));
    layout.open(slot(1), button);
    let tooltip = Element {
        float: Some(Anchor::Parent {
            parent: [Align::Center, Align::Start],
            own: [Align::Center, Align::End],
            offset: [0., -4.],
        }),
        ..fixed(60., 10.)
    };
    leaf(&mut layout, 2, tooltip);
    layout.close();
    layout.solve(WINDOW, 1.);
    // Centred over the button, ignoring its padding and scroll offset.
    assert_eq!(rects(&layout)[3], Rect::new(-10., 36., 60., 10.));
}

#[test]
fn a_growing_clip_takes_only_what_its_given() {
    let mut layout = Layout::default();
    layout.clear();
    let list = Element {
        clip: true,
        ..Element::column()
    };
    layout.open(slot(0), list);
    for n in 0..3 {
        leaf(&mut layout, n, fixed(10., 60.));
    }
    layout.close();
    layout.solve(WINDOW, 1.);
    assert_eq!(rects(&layout)[1], WINDOW);
}

#[test]
fn boxes_outside_the_clip_draw_nothing() {
    let mut layout = Layout::default();
    layout.clear();
    let list = Element {
        clip: true,
        ..fixed(50., 50.)
    };
    layout.open(slot(0), list);
    let row = Element {
        background: Some(Color::hex(0xff_ff_ff)),
        ..fixed(10., 30.)
    };
    for n in 0..3 {
        leaf(&mut layout, n, row);
    }
    layout.close();
    layout.solve(WINDOW, 1.);
    let mut canvas = Canvas::default();
    canvas.clear(WINDOW);
    layout.emit(&mut canvas, &mut Glyphs::default(), None);
    // The third row starts at the clip's bottom edge.
    assert_eq!(canvas.quads().len(), 2);
}

#[test]
fn a_float_under_a_hidden_clip_still_draws() {
    let mut layout = Layout::default();
    layout.clear();
    leaf(&mut layout, 0, fixed(10., 100.));
    let below = Element {
        clip: true,
        ..fixed(40., 20.)
    };
    layout.open(slot(1), below);
    let popup = Element {
        float: Some(Anchor::At([0., 0.])),
        background: Some(Color::hex(0xff_ff_ff)),
        ..fixed(60., 60.)
    };
    leaf(&mut layout, 2, popup);
    layout.close();
    layout.solve(WINDOW, 1.);
    let mut canvas = Canvas::default();
    canvas.clear(WINDOW);
    layout.emit(&mut canvas, &mut Glyphs::default(), None);
    let popup = Quad::new(Rect::new(0., 0., 60., 60.), Color::hex(0xff_ff_ff));
    assert_eq!(canvas.quads(), [popup]);
}

#[test]
fn the_cursor_comes_from_the_last_box_drawn_under_the_pointer() {
    let mut layout = Layout::default();
    layout.clear();
    let clipped = Element {
        clip: true,
        ..fixed(40., 20.)
    };
    layout.open(slot(0), clipped);
    let link = Element {
        cursor: Some(Cursor::Pointer),
        ..fixed(80., 20.)
    };
    leaf(&mut layout, 1, link);
    let field = Element {
        float: Some(Anchor::At([0., 0.])),
        cursor: Some(Cursor::Text),
        ..fixed(10., 10.)
    };
    leaf(&mut layout, 2, field);
    layout.close();
    layout.solve(WINDOW, 1.);
    let mut cursor_at = |at| layout.emit(&mut Canvas::default(), &mut Glyphs::default(), Some(at));
    assert_eq!(cursor_at([20., 15.]), Some(Cursor::Pointer));
    // Past the clip, the link isn't drawn there.
    assert_eq!(cursor_at([60., 15.]), None);
    // The floating box draws over the link.
    assert_eq!(cursor_at([5., 5.]), Some(Cursor::Text));
}

#[test]
fn a_custom_box_draws_in_its_place() {
    let mut layout = Layout::default();
    layout.clear();
    let (node, _) = layout.custom(slot(0), fixed(10., 10.));
    layout.paint_triangles(
        node,
        &[Vertex::new([0., 0.], Color::TRANSPARENT); 3],
        &[0, 1, 2],
    );
    layout.solve(WINDOW, 1.);
    let mut canvas = Canvas::default();
    canvas.clear(WINDOW);
    layout.emit(&mut canvas, &mut Glyphs::default(), None);
    let kinds: Vec<_> = canvas.batches().map(|(_, kind, _)| kind).collect();
    assert_eq!(kinds, [Kind::Triangles]);
}

const SANS: FontFile = FontFile {
    name: "Noto Sans",
    data: include_bytes!("../../../../assets/fonts/noto-sans/NotoSans[wdth,wght].ttf"),
};

#[test]
fn text_keeps_its_line_and_inks_between_passes() {
    let mut glyphs = Glyphs::default();
    let font = glyphs.add_font(SANS, 400.).unwrap();
    let style = TextStyle {
        font,
        size: 14.,
        color: Color::hex(0xff_ff_ff),
    };
    let mut layout = Layout::default();
    let mut canvas = Canvas::default();
    let mut pass = |text: &str| {
        layout.clear();
        layout.text(&mut glyphs, slot(0), style, text);
        layout.solve(WINDOW, 1.);
        canvas.clear(WINDOW);
        layout.emit(&mut canvas, &mut glyphs, None);
        let line = &layout.texts[0].line;
        (line.inked(), glyphs.line_width(line))
    };
    let (_, width) = pass("label");
    assert!(width > 0.);
    // Drawn last pass, so its inks came with it.
    assert!(pass("label").0);
    assert!(pass("labels").1 > width);
}
