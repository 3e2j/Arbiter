use super::*;
use crate::input::{Button, Event};
use crate::ui::tests::Kept;
use crate::ui::{Element, Size};

fn fixed(w: f32, h: f32) -> Element {
    Element {
        size: [Size::Fixed(w), Size::Fixed(h)],
        ..Element::DEFAULT
    }
}

/// A hundred rows ten tall, in a window a hundred tall. Returns the rows
/// declared.
fn list_pass(kept: &mut Kept, state: &mut ListScroll) -> Vec<u32> {
    let items: Vec<u32> = (0..100).collect();
    let mut shown = Vec::new();
    kept.pass(|ui| list(ui, state, &items, 10., |_, &item| shown.push(item)));
    shown
}

#[test]
fn a_list_declares_only_the_rows_in_view() {
    let mut kept = Kept::default();
    let mut state = ListScroll::TOP;
    // No size yet, so no rows.
    assert_eq!(list_pass(&mut kept, &mut state), []);
    assert_eq!(
        list_pass(&mut kept, &mut state),
        (0..10).collect::<Vec<_>>()
    );
    // The wheel goes to the box the pointer was over last pass.
    kept.input.push(Event::Pointer(Some([5., 5.])));
    list_pass(&mut kept, &mut state);
    kept.input.push(Event::Scroll([0., -25.]));
    // The top row is half out of view, so one more shows at the bottom.
    assert_eq!(
        list_pass(&mut kept, &mut state),
        (2..13).collect::<Vec<_>>()
    );
    assert_eq!(state, ListScroll { top: 2, offset: 5. });
}

#[test]
fn a_list_stops_at_both_ends() {
    let mut kept = Kept::default();
    let mut state = ListScroll::TOP;
    list_pass(&mut kept, &mut state);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    list_pass(&mut kept, &mut state);
    kept.input.push(Event::Scroll([0., -10_000.]));
    list_pass(&mut kept, &mut state);
    assert!(state.at_end(100., 10., 100));
    assert_eq!(
        state,
        ListScroll {
            top: 90,
            offset: 0.
        }
    );
    kept.input.push(Event::Scroll([0., 10_000.]));
    list_pass(&mut kept, &mut state);
    assert_eq!(state, ListScroll::TOP);
}

#[test]
fn dragging_the_thumb_scrolls_as_far_as_it_moves() {
    let mut kept = Kept::default();
    kept.theme.size.scroll_bar = 6.;
    // The thumb is never shorter than a row.
    kept.theme.size.row = 10.;
    let mut state = ListScroll::TOP;
    for _ in 0..3 {
        list_pass(&mut kept, &mut state);
    }
    // The thumb is 10 tall in a track of 94, so it travels 84 for 900.
    kept.input.push(Event::Pointer(Some([294., 5.])));
    kept.input.push(Event::Pressed(Button::Left));
    list_pass(&mut kept, &mut state);
    assert_eq!(state, ListScroll::TOP);
    kept.input.push(Event::Pointer(Some([294., 47.])));
    list_pass(&mut kept, &mut state);
    assert_eq!(
        state,
        ListScroll {
            top: 45,
            offset: 0.
        }
    );
}

#[test]
fn a_scroll_box_stops_where_its_content_ends() {
    let mut kept = Kept::default();
    let mut state = Scroll::default();
    let pass = |kept: &mut Kept, state: &mut Scroll| {
        kept.pass(|ui| {
            scroll(ui, state, Element::column(), |ui| {
                for _ in 0..10 {
                    ui.element(fixed(10., 30.), |_| ());
                }
            });
        });
    };
    pass(&mut kept, &mut state);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    pass(&mut kept, &mut state);
    kept.input.push(Event::Scroll([0., -1000.]));
    pass(&mut kept, &mut state);
    assert_eq!(state.offset, 200.);
}

#[test]
fn only_the_innermost_scroll_box_takes_the_wheel() {
    let mut kept = Kept::default();
    let mut outer = Scroll::default();
    let mut inner = Scroll::default();
    let pass = |kept: &mut Kept, outer: &mut Scroll, inner: &mut Scroll| {
        kept.pass(|ui| {
            scroll(ui, outer, Element::column(), |ui| {
                scroll(ui, inner, fixed(100., 50.), |ui| {
                    for _ in 0..10 {
                        ui.element(fixed(10., 30.), |_| ());
                    }
                });
                ui.element(fixed(10., 200.), |_| ());
            });
        });
    };
    pass(&mut kept, &mut outer, &mut inner);
    pass(&mut kept, &mut outer, &mut inner);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    pass(&mut kept, &mut outer, &mut inner);
    kept.input.push(Event::Scroll([0., -20.]));
    pass(&mut kept, &mut outer, &mut inner);
    assert_eq!((outer.offset, inner.offset), (0., 20.));
    // Past the inner box, the outer one takes it.
    kept.input.push(Event::Pointer(Some([200., 5.])));
    pass(&mut kept, &mut outer, &mut inner);
    kept.input.push(Event::Scroll([0., -20.]));
    pass(&mut kept, &mut outer, &mut inner);
    assert_eq!((outer.offset, inner.offset), (20., 20.));
}

#[test]
fn a_button_reports_a_click_over_it() {
    let mut kept = Kept::default();
    kept.theme.size.row = 20.;
    kept.theme.size.gap = 8.;
    let pass = |kept: &mut Kept| kept.pass(|ui| button(ui, "Open"));
    kept.input.push(Event::Pointer(Some([1., 1.])));
    pass(&mut kept);
    kept.input.push(Event::Pressed(Button::Left));
    assert!(!pass(&mut kept));
    kept.input.push(Event::Released(Button::Left));
    // The host runs the pass after a release in the same frame.
    assert!(!pass(&mut kept));
    assert!(pass(&mut kept));
    assert!(!pass(&mut kept));
}
