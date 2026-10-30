//! Scrolling: the wheel and drags move the content, within its range, and clicks still work.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, UnitExt};

thread_local! {
    static CLICKS: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
    static STATE: RefCell<Option<ScrollState>> = const { RefCell::new(None) };
}

#[composable]
fn List() {
    let state = remember_scroll_state!();
    STATE.with(|cell| *cell.borrow_mut() = Some(state));
    Column!(modifier = Modifier.fill_max_size().vertical_scroll(state), {
        for index in 0..20u32 {
            Spacer(
                Modifier
                    .fill_max_width()
                    .height(50.dp())
                    .test_tag(format!("item {index}"))
                    .clickable(move || CLICKS.with(|clicks| clicks.borrow_mut().push(index))),
            );
        }
    });
}

fn top(ui: &mut ComposeTestRule, index: u32) -> f32 {
    ui.on_node_with_tag(&format!("item {index}")).fetch_semantics_snapshot().bounds.top
}

fn state() -> ScrollState {
    STATE.with(|cell| (*cell.borrow()).expect("composed"))
}

#[test]
fn the_wheel_scrolls_within_the_content() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), List);
    assert_eq!(top(&mut ui, 1), 50.0);
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 120.0)));
    ui.wait_for_idle();
    assert_eq!(state().value(), 120);
    assert_eq!(top(&mut ui, 3), 30.0);
    // 20 items of 50 in 200: 800 at most.
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 5000.0)));
    ui.wait_for_idle();
    assert_eq!(state().max_value(), 800);
    assert_eq!(state().value(), 800);
    assert_eq!(top(&mut ui, 19), 150.0);
    ui.host()
        .send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, -5000.0)));
    ui.wait_for_idle();
    assert_eq!(state().value(), 0);
}

#[test]
fn a_drag_scrolls_and_cancels_the_click_under_it() {
    CLICKS.with(|clicks| clicks.borrow_mut().clear());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), List);
    let host = ui.host();
    host.send_pointer(
        PointerEvent::press(Offset::new(50.0, 150.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    host.send_pointer(
        PointerEvent::moved(Offset::new(50.0, 140.0), true)
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    host.send_pointer(
        PointerEvent::moved(Offset::new(50.0, 50.0), true)
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    host.send_pointer(
        PointerEvent::release(Offset::new(50.0, 50.0))
            .with_pointer(compose_ui::PointerId(0), compose_ui::PointerType::Touch),
    );
    ui.wait_for_idle();
    // What moved past the 8 px slop scrolls, as Compose's.
    assert_eq!(state().value(), 92);
    assert!(CLICKS.with(|clicks| clicks.borrow().is_empty()), "the drag isn't a click");
    // A tap clicks the item now under it.
    ui.on_node_with_tag("item 3").perform_click();
    assert_eq!(CLICKS.with(|clicks| clicks.borrow().clone()), [3]);
}

#[test]
fn content_scrolled_out_of_the_viewport_is_not_hit() {
    CLICKS.with(|clicks| clicks.borrow_mut().clear());
    #[composable]
    fn Screen() {
        Column!({
            Box!(modifier = Modifier.height(100.dp()).fill_max_width(), |_| {
                List();
            });
            Spacer(Modifier.height(100.dp()).fill_max_width().test_tag("below"));
        });
    }
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), Screen);
    // Item 2 is laid out at y = 100..150, under the viewport's bottom edge: below it is the
    // spacer, not the item.
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 120.0)));
    host.send_pointer(PointerEvent::release(Offset::new(50.0, 120.0)));
    ui.wait_for_idle();
    assert!(CLICKS.with(|clicks| clicks.borrow().is_empty()));
}

#[test]
fn setting_the_scroll_state_scrolls() {
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), List);
    CoroutineScope::new().launch(async {
        let _ = state().scroll_to(250).await;
    });
    ui.wait_for_idle();
    assert_eq!(top(&mut ui, 5), 0.0);
    CoroutineScope::new().launch(async {
        let _ = state().scroll_to(10_000).await;
    });
    ui.wait_for_idle();
    assert_eq!(state().value(), 800, "clamped at once");
}

/// `rememberScrollState(initial)` through its call macro, and `Spacer()` without a modifier.
#[test]
fn a_scroll_state_starts_at_its_initial_value() {
    thread_local! {
        static INITIAL: RefCell<Option<ScrollState>> = const { RefCell::new(None) };
    }
    let _ui = ComposeTestRule::sized(IntSize::new(100, 100), || {
        let state = remember_scroll_state!(initial = 30);
        INITIAL.with(|cell| *cell.borrow_mut() = Some(state));
        Column!(modifier = Modifier.fill_max_size().vertical_scroll(state), {
            Spacer!();
            Spacer!(Modifier.height(500.dp()));
        });
    });
    assert_eq!(INITIAL.with(|cell| (*cell.borrow()).unwrap().value()), 30);
}
