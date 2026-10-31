use std::time::Duration;

use super::*;
use gui::input::{Event, Key, PointerButton};
use gui::ui::testing::Kept;
use gui::ui::{Element, Length};

use crate::theme::sizes;

fn fixed(w: f32, h: f32) -> Element {
    Element {
        size: [Length::Fixed(w), Length::Fixed(h)],
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
    assert_eq!((state.top, state.offset), (2, 5.));
}

/// Twenty rows, ten and thirty tall in turn, in a window a hundred tall.
/// Returns the rows declared.
fn fit_list_pass(kept: &mut Kept, state: &mut ListScroll) -> Vec<usize> {
    let mut shown = Vec::new();
    kept.pass(|ui| {
        fit_list(ui, state, 20, |ui, index| {
            let height = if index % 2 == 0 { 10. } else { 30. };
            ui.element(fixed(10., height), |_| {});
            shown.push(index);
        });
    });
    shown
}

#[test]
fn a_fit_list_scrolls_by_each_rows_height() {
    // What it guesses a row it hasn't measured is.
    sizes(|size| size.row = 20.);
    let mut kept = Kept::default();
    let mut state = ListScroll::TOP;
    for _ in 0..3 {
        fit_list_pass(&mut kept, &mut state);
    }
    kept.input.push(Event::Pointer(Some([5., 5.])));
    fit_list_pass(&mut kept, &mut state);
    kept.input.push(Event::Scroll([0., -15.]));
    fit_list_pass(&mut kept, &mut state);
    assert_eq!((state.top, state.offset), (1, 5.));
    kept.input.push(Event::Scroll([0., 10.]));
    // The row above the top is declared, so scrolling up knows its height.
    assert_eq!(
        fit_list_pass(&mut kept, &mut state),
        (0..6).collect::<Vec<_>>()
    );
    assert_eq!((state.top, state.offset), (0, 5.));
    kept.input.push(Event::Scroll([0., -10_000.]));
    for _ in 0..5 {
        fit_list_pass(&mut kept, &mut state);
    }
    assert_eq!((state.top, state.offset), (15, 10.));
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
    assert_eq!((state.top, state.offset), (90, 0.));
    kept.input.push(Event::Scroll([0., 10_000.]));
    list_pass(&mut kept, &mut state);
    assert_eq!((state.top, state.offset), (0, 0.));
}

#[test]
fn dragging_the_thumb_scrolls_as_far_as_it_moves() {
    let mut kept = Kept::default();
    sizes(|size| size.scroll_bar = 6.);
    // The thumb is never shorter than a row.
    sizes(|size| size.row = 10.);
    let mut state = ListScroll::TOP;
    for _ in 0..3 {
        list_pass(&mut kept, &mut state);
    }
    // The thumb is 10 tall in a track of 94, so it travels 84 for 900.
    kept.input.push(Event::Pointer(Some([294., 5.])));
    kept.input.push(Event::Pressed(PointerButton::Left));
    list_pass(&mut kept, &mut state);
    assert_eq!((state.top, state.offset), (0, 0.));
    kept.input.push(Event::Pointer(Some([294., 47.])));
    list_pass(&mut kept, &mut state);
    assert_eq!((state.top, state.offset), (45, 0.));
}

#[test]
fn a_hover_thumb_waits_out_its_hold_in_one_frame_then_fades() {
    let mut kept = Kept::default();
    let mut state = ListScroll::TOP.thumb(ThumbShow::Hover {
        hold: 1.,
        fade: 0.25,
    });
    list_pass(&mut kept, &mut state);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    list_pass(&mut kept, &mut state);
    list_pass(&mut kept, &mut state);
    assert_eq!(kept.out.redraw_in, None);
    kept.input.push(Event::Pointer(None));
    list_pass(&mut kept, &mut state);
    assert_eq!(kept.out.redraw_in, Some(Duration::from_secs(1)));
    kept.set_elapsed(1.);
    list_pass(&mut kept, &mut state);
    assert_eq!(kept.out.redraw_in, Some(Duration::ZERO));
    kept.set_elapsed(0.25);
    list_pass(&mut kept, &mut state);
    assert_eq!(kept.out.redraw_in, None);
}

#[test]
fn an_always_shown_thumb_asks_for_no_frames() {
    let mut kept = Kept::default();
    let mut state = ListScroll::TOP;
    list_pass(&mut kept, &mut state);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    list_pass(&mut kept, &mut state);
    kept.input.push(Event::Pointer(None));
    list_pass(&mut kept, &mut state);
    assert_eq!(kept.out.redraw_in, None);
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
    sizes(|size| {
        size.row = 20.;
        size.gap = 8.;
    });
    let pass = |kept: &mut Kept| kept.pass(|ui| button(ui, "Open"));
    kept.input.push(Event::Pointer(Some([1., 1.])));
    pass(&mut kept);
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert!(!pass(&mut kept));
    kept.input.push(Event::Released(PointerButton::Left));
    assert!(pass(&mut kept));
    assert!(!pass(&mut kept));
}

/// A popover 50 wide hanging under the right end of a box 100 by 20.
/// Returns whether it saw a press outside.
fn popover_pass(kept: &mut Kept) -> bool {
    kept.pass(|ui| {
        ui.element(fixed(100., 20.), |ui| {
            popover(ui, 50., |ui| ui.element(fixed(10., 10.), |_| ())).1
        })
    })
}

#[test]
fn a_popover_closes_on_a_press_outside_it() {
    let mut kept = Kept::default();
    kept.input.push(Event::Pointer(Some([5., 5.])));
    // The pass it opens, the press is the one that opened it.
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert!(!popover_pass(&mut kept));
    kept.input.push(Event::Released(PointerButton::Left));
    popover_pass(&mut kept);
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert!(popover_pass(&mut kept));
}

#[test]
fn a_popover_stays_open_on_a_press_inside_it() {
    let mut kept = Kept::default();
    popover_pass(&mut kept);
    kept.input.push(Event::Pointer(Some([75., 25.])));
    popover_pass(&mut kept);
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert!(!popover_pass(&mut kept));
}

/// A box filling the window that opens `state` and asks for the left
/// button, under a menu of three items, the middle one disabled while
/// `disabled`. Returns whether the box opened it, whether the box was
/// pressed, and what the menu clicked.
fn menu_pass(
    kept: &mut Kept,
    state: &mut ContextMenu,
    disabled: bool,
) -> (bool, bool, Option<usize>) {
    let items = [
        MenuItem::new("Copy"),
        MenuItem::new("Copy as Hex").enabled(!disabled),
        MenuItem::new("Select All"),
    ];
    kept.pass(|ui| {
        let (opened, pressed) = ui.element(fixed(300., 100.), |ui| {
            (
                state.open_on_right_press(ui),
                ui.pressed(PointerButton::Left),
            )
        });
        (opened, pressed, context_menu(ui, state, &items))
    })
}

/// A menu opened at `[20, 20]`, its rows ten tall from there, laid out.
fn open_menu(kept: &mut Kept, state: &mut ContextMenu, disabled: bool) {
    sizes(|size| {
        size.row = 10.;
        size.gap = 0.;
    });
    menu_pass(kept, state, disabled);
    kept.input.push(Event::Pointer(Some([20., 20.])));
    menu_pass(kept, state, disabled);
    kept.input.push(Event::Pressed(PointerButton::Right));
    assert!(menu_pass(kept, state, disabled).0);
    kept.input.push(Event::Released(PointerButton::Right));
    menu_pass(kept, state, disabled);
    assert!(state.is_open());
}

/// Clicks at `at`, returning what the last pass of [`menu_pass`] gave.
fn click_menu(
    kept: &mut Kept,
    state: &mut ContextMenu,
    disabled: bool,
    at: [f32; 2],
) -> (bool, bool, Option<usize>) {
    kept.input.push(Event::Pointer(Some(at)));
    menu_pass(kept, state, disabled);
    kept.input.push(Event::Pressed(PointerButton::Left));
    let pressed = menu_pass(kept, state, disabled);
    kept.input.push(Event::Released(PointerButton::Left));
    let released = menu_pass(kept, state, disabled);
    (pressed.1 || released.1, released.1, released.2)
}

#[test]
fn a_menu_item_clicked_is_returned_and_closes_it() {
    let mut kept = Kept::default();
    let mut state = ContextMenu::CLOSED;
    open_menu(&mut kept, &mut state, false);
    let (under, _, clicked) = click_menu(&mut kept, &mut state, false, [25., 35.]);
    assert_eq!(clicked, Some(1));
    assert!(!under);
    assert!(!state.is_open());
}

#[test]
fn a_disabled_menu_item_cant_be_clicked() {
    let mut kept = Kept::default();
    let mut state = ContextMenu::CLOSED;
    open_menu(&mut kept, &mut state, true);
    let (_, _, clicked) = click_menu(&mut kept, &mut state, true, [25., 35.]);
    assert_eq!(clicked, None);
    assert!(state.is_open());
}

#[test]
fn a_press_outside_a_menu_closes_it_without_reaching_under() {
    let mut kept = Kept::default();
    let mut state = ContextMenu::CLOSED;
    open_menu(&mut kept, &mut state, false);
    let (under, _, clicked) = click_menu(&mut kept, &mut state, false, [5., 90.]);
    assert_eq!(clicked, None);
    assert!(!under);
    assert!(!state.is_open());
}

#[test]
fn escape_closes_a_menu() {
    let mut kept = Kept::default();
    let mut state = ContextMenu::CLOSED;
    open_menu(&mut kept, &mut state, false);
    kept.input.push(Event::Key {
        key: gui::input::Key::Escape,
        code: None,
        repeat: false,
    });
    menu_pass(&mut kept, &mut state, false);
    assert!(!state.is_open());
}

/// Three choices in a dropdown 100 across and 10 tall, the second picked.
fn dropdown_pass(kept: &mut Kept, state: &mut Dropdown) -> Chosen {
    dropdown_typing(kept, state).0
}

/// [`dropdown_pass`], and whether a box has the keyboard after it.
fn dropdown_typing(kept: &mut Kept, state: &mut Dropdown) -> (Chosen, bool) {
    let choices = [
        Choice::new("Noto Sans"),
        Choice::new("Inter"),
        Choice::new("Geist"),
    ];
    kept.pass(|ui| {
        let chosen = ui.element(fixed(100., 10.), |ui| {
            dropdown(ui, state, &choices, 1, None)
        });
        (chosen, ui.typing())
    })
}

/// A dropdown pressed open, its list laid out with its rows ten tall from
/// `[0, 14]`, and holding the keyboard.
fn open_dropdown(kept: &mut Kept, state: &mut Dropdown) {
    sizes(|size| {
        size.row = 10.;
        size.gap = 0.;
    });
    dropdown_pass(kept, state);
    press_dropdown(kept, state, [50., 5.]);
    dropdown_pass(kept, state);
    assert!(state.is_open());
}

/// Presses and lets go at `at`, returning what the last pass gave.
fn press_dropdown(kept: &mut Kept, state: &mut Dropdown, at: [f32; 2]) -> Chosen {
    kept.input.push(Event::Pointer(Some(at)));
    dropdown_pass(kept, state);
    kept.input.push(Event::Pressed(PointerButton::Left));
    dropdown_pass(kept, state);
    kept.input.push(Event::Released(PointerButton::Left));
    dropdown_pass(kept, state)
}

fn key(kept: &mut Kept, key: Key) {
    kept.input.push(Event::Key {
        key,
        code: None,
        repeat: false,
    });
}

#[test]
fn a_dropdown_choice_under_the_pointer_is_marked_and_clicking_it_picks_it() {
    let mut kept = Kept::default();
    let mut state = Dropdown::CLOSED;
    open_dropdown(&mut kept, &mut state);
    kept.input.push(Event::Pointer(Some([50., 39.])));
    dropdown_pass(&mut kept, &mut state);
    assert_eq!(dropdown_pass(&mut kept, &mut state).marked, Some(2));
    let chosen = press_dropdown(&mut kept, &mut state, [50., 39.]);
    assert_eq!(chosen.picked, Some(2));
    assert!(!state.is_open());
}

#[test]
fn a_dropdown_closes_on_a_press_outside_it_or_on_its_button() {
    let mut kept = Kept::default();
    let mut state = Dropdown::CLOSED;
    open_dropdown(&mut kept, &mut state);
    let chosen = press_dropdown(&mut kept, &mut state, [50., 90.]);
    assert_eq!(chosen, Chosen::default());
    assert!(!state.is_open());
    open_dropdown(&mut kept, &mut state);
    press_dropdown(&mut kept, &mut state, [50., 5.]);
    assert!(!state.is_open());
}

#[test]
fn a_dropdown_marks_with_up_and_down_from_the_picked_choice_and_enter_picks() {
    let mut kept = Kept::default();
    let mut state = Dropdown::CLOSED;
    open_dropdown(&mut kept, &mut state);
    assert_eq!(dropdown_pass(&mut kept, &mut state).marked, None);
    key(&mut kept, Key::Down);
    assert_eq!(dropdown_pass(&mut kept, &mut state).marked, Some(2));
    // Wraps around the end.
    key(&mut kept, Key::Down);
    assert_eq!(dropdown_pass(&mut kept, &mut state).marked, Some(0));
    key(&mut kept, Key::Up);
    key(&mut kept, Key::Enter);
    assert_eq!(dropdown_pass(&mut kept, &mut state).picked, Some(2));
    assert!(!state.is_open());
}

#[test]
fn a_dropdown_holds_the_keyboard_only_while_open() {
    let mut kept = Kept::default();
    let mut state = Dropdown::CLOSED;
    open_dropdown(&mut kept, &mut state);
    assert!(dropdown_typing(&mut kept, &mut state).1);
    key(&mut kept, Key::Escape);
    // Still held the pass it lets go, so Escape isn't taken twice.
    assert!(dropdown_typing(&mut kept, &mut state).1);
    assert!(!state.is_open());
    assert!(!dropdown_typing(&mut kept, &mut state).1);
}

/// A slider a hundred and ten across with five notches, its knob ten across,
/// so the notches sit at 5, 30, 55, 80 and 105.
fn slider_pass(kept: &mut Kept) -> Slid {
    sizes(|size| {
        size.row = 10.;
        size.icon = 10.;
    });
    kept.pass(|ui| ui.element(fixed(110., 10.), |ui| slider(ui, 5, 2)))
}

fn point(kept: &mut Kept, at: [f32; 2]) -> Slid {
    kept.input.push(Event::Pointer(Some(at)));
    slider_pass(kept)
}

#[test]
fn a_slider_holds_the_notch_nearest_the_pointer_and_sets_it_on_letting_go() {
    let mut kept = Kept::default();
    slider_pass(&mut kept);
    point(&mut kept, [33., 5.]);
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert_eq!(slider_pass(&mut kept).held, Some(1));
    assert_eq!(point(&mut kept, [70., 5.]).held, Some(3));
    // Past the end, and off it, still the last notch.
    assert_eq!(point(&mut kept, [500., 50.]).held, Some(4));
    kept.input.push(Event::Released(PointerButton::Left));
    let slid = slider_pass(&mut kept);
    assert_eq!((slid.held, slid.set), (None, Some(4)));
    assert_eq!(slider_pass(&mut kept), Slid::default());
}

#[test]
fn a_slider_ignores_a_press_elsewhere() {
    let mut kept = Kept::default();
    slider_pass(&mut kept);
    point(&mut kept, [50., 50.]);
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert_eq!(slider_pass(&mut kept), Slid::default());
    assert_eq!(point(&mut kept, [50., 5.]), Slid::default());
    kept.input.push(Event::Released(PointerButton::Left));
    assert_eq!(slider_pass(&mut kept), Slid::default());
}

#[test]
fn a_switch_says_when_its_clicked_and_not_when_the_press_was_elsewhere() {
    let mut kept = Kept::default();
    sizes(|size| size.icon = 10.);
    let pass = |kept: &mut Kept| kept.pass(|ui| switch(ui, 0.));
    pass(&mut kept);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert!(!pass(&mut kept));
    kept.input.push(Event::Released(PointerButton::Left));
    assert!(pass(&mut kept));
    kept.input.push(Event::Pointer(Some([50., 50.])));
    kept.input.push(Event::Pressed(PointerButton::Left));
    pass(&mut kept);
    kept.input.push(Event::Pointer(Some([5., 5.])));
    kept.input.push(Event::Released(PointerButton::Left));
    assert!(!pass(&mut kept));
}

#[test]
fn a_segmented_picker_says_which_segment_was_clicked() {
    let mut kept = Kept::default();
    sizes(|size| size.row = 20.);
    // Three segments 32 across, at 2, 36 and 70.
    let pass = |kept: &mut Kept| {
        kept.pass(|ui| ui.element(fixed(104., 20.), |ui| segmented(ui, &[""; 3], 0)))
    };
    pass(&mut kept);
    kept.input.push(Event::Pointer(Some([50., 10.])));
    pass(&mut kept);
    kept.input.push(Event::Pressed(PointerButton::Left));
    assert_eq!(pass(&mut kept), None);
    kept.input.push(Event::Released(PointerButton::Left));
    assert_eq!(pass(&mut kept), Some(1));
    // Between two segments is neither.
    kept.input.push(Event::Pointer(Some([35., 10.])));
    pass(&mut kept);
    kept.input.push(Event::Pressed(PointerButton::Left));
    pass(&mut kept);
    kept.input.push(Event::Released(PointerButton::Left));
    assert_eq!(pass(&mut kept), None);
}

#[test]
fn a_dropdown_list_is_wider_than_a_narrow_button() {
    let mut kept = Kept::default();
    let mut state = Dropdown::CLOSED;
    open_dropdown(&mut kept, &mut state);
    // The button is 100 across, so this is past it, in the list's last row.
    kept.input.push(Event::Pointer(Some([150., 39.])));
    dropdown_pass(&mut kept, &mut state);
    assert_eq!(dropdown_pass(&mut kept, &mut state).marked, Some(2));
}
