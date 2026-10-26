//! `ModalBottomSheet` (showing, anchors, drags, the scrim, Escape) and `SwipeToDismissBox`.

#![allow(non_snake_case)]

use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Key, Modifier, Offset, PointerEvent, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds
}

#[composable]
fn SheetApp(state: SheetState, shown: MutableState<bool>, content_height: f32) {
    MaterialTheme!({
        Box!(modifier = Modifier.fill_max_size(), {
            if shown.get() {
                ModalBottomSheet!(move || shown.set(false), sheet_state = state, {
                    Box!(
                        modifier = Modifier
                            .fill_max_width()
                            .height(content_height.dp())
                            .test_tag("content")
                    );
                });
            }
        });
    });
}

/// A 400 by 600 window showing a sheet of `content_height` dp (and its 48 dp drag handle).
fn sheet(
    content_height: f32,
) -> (ComposeTestRule, Owned<SheetState>, MutableState<bool>, Rc<dyn std::any::Any>) {
    let state = SheetState::new(false, SheetValue::Hidden, |_| true, false);
    let owner = mutable_state_of(true);
    let shown = owner.handle();
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || {
        SheetApp(given, shown, content_height)
    });
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(600));
    (ui, state, shown, Rc::new(owner))
}

#[test]
fn a_short_sheet_shows_expanded_and_a_tall_one_half_way() {
    let (mut ui, state, _shown, _owner) = sheet(200.0);
    assert_eq!(state.current_value(), SheetValue::Expanded);
    // The handle's 48 dp (22 + 4 + 22) above the content, at the bottom.
    assert_eq!(bounds(&mut ui, "content").top, 600.0 - 200.0);
    ui.on_node_with_content_description("Drag handle").assert_exists();
    assert!(!state.has_partially_expanded_state());

    let (mut ui, state, _shown, _owner) = sheet(500.0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    assert_eq!(bounds(&mut ui, "content").top, 300.0 + 48.0);
    let scope = CoroutineScope::new();
    let expanding = *state;
    scope.launch(async move { expanding.expand().await });
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert_eq!(bounds(&mut ui, "content").top, 600.0 - 500.0);
}

#[test]
fn the_sheet_slides_up_as_it_shows() {
    let state = SheetState::new(true, SheetValue::Hidden, |_| true, false);
    let owner = mutable_state_of(false);
    let shown = owner.handle();
    let given = *state;
    let mut ui =
        ComposeTestRule::sized(IntSize::new(400, 600), move || SheetApp(given, shown, 200.0));
    ui.set_auto_advance(false);
    shown.set(true);
    ui.advance_time_by(Duration::from_millis(100));
    let sliding = bounds(&mut ui, "content").top;
    assert!(sliding > 400.0 && sliding < 600.0 + 48.0, "{sliding}");
    assert_eq!(state.target_value(), SheetValue::Expanded);
    ui.advance_time_by(Duration::from_millis(500));
    assert_eq!(bounds(&mut ui, "content").top, 400.0);
}

#[test]
fn dragging_the_sheet_down_dismisses_it_and_a_short_drag_comes_back() {
    let (mut ui, state, shown, _owner) = sheet(200.0);
    // 40 dp down (past the touch slop): less than the 56 dp threshold, slowly.
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 450.0)));
    for y in [460.0, 470.0, 480.0, 498.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(200.0, y), true));
        ui.advance_time_by(Duration::from_millis(200));
    }
    assert!(bounds(&mut ui, "content").top > 420.0);
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 498.0)));
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(bounds(&mut ui, "content").top, 400.0);
    assert!(shown.get());
    // Now 100 dp down: hidden, then dismissed.
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 450.0)));
    for y in [470.0, 500.0, 530.0, 558.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(200.0, y), true));
        ui.advance_time_by(Duration::from_millis(200));
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 558.0)));
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::Hidden);
    assert!(!shown.get());
    ui.on_node_with_tag("content").assert_does_not_exist();
}

#[test]
fn the_scrim_and_escape_dismiss_the_sheet() {
    let (mut ui, state, shown, _owner) = sheet(200.0);
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 100.0)));
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 100.0)));
    ui.advance_time_by(Duration::from_millis(600));
    assert!(!shown.get());
    assert!(!state.is_visible());

    let (mut ui, _state, shown, _owner) = sheet(200.0);
    ui.press_key(Key::Escape);
    ui.advance_time_by(Duration::from_millis(600));
    assert!(!shown.get());
}

/// Android's back (not only Escape) takes an expanded sheet half way, then hides it and
/// dismisses it; while the sheet shows, back never leaves the app.
#[test]
fn back_partially_expands_then_dismisses_the_sheet() {
    let (mut ui, state, shown, _owner) = sheet(500.0);
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    let scope = CoroutineScope::new();
    let expanding = *state;
    scope.launch(async move { expanding.expand().await });
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert!(ui.host().has_enabled_back_handlers());
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    assert!(shown.get());
    assert!(ui.host().on_back_pressed());
    ui.advance_time_by(Duration::from_millis(600));
    assert!(!shown.get());
    assert!(!ui.host().has_enabled_back_handlers());
}

#[composable]
fn SwipeApp(state: SwipeToDismissBoxState) {
    MaterialTheme!({
        SwipeToDismissBox!(
            state,
            |_| {
                Box!(modifier = Modifier.fill_max_size().test_tag("background"));
            },
            modifier = Modifier.test_tag("box"),
            enable_dismiss_from_start_to_end = false,
            |_| {
                ListItem!({ Text!("Swipe me") }, modifier = Modifier.test_tag("item"));
            }
        );
    });
}

#[test]
fn a_swipe_past_the_threshold_dismisses_and_a_short_one_settles_back() {
    let state = SwipeToDismissBoxState::new(SwipeToDismissBoxValue::Settled, |_| 56.0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || SwipeApp(given));
    ui.set_auto_advance(false);
    let item = bounds(&mut ui, "item");
    assert_eq!(item.left, 0.0);
    assert_eq!(bounds(&mut ui, "background"), bounds(&mut ui, "box"));
    let y = (item.top + item.bottom) / 2.0;
    // 30 dp left, slowly: back in place.
    ui.host().send_pointer(PointerEvent::press(Offset::new(300.0, y)));
    for x in [290.0, 280.0, 262.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
        ui.advance_time_by(Duration::from_millis(200));
    }
    assert!(bounds(&mut ui, "item").left < -20.0);
    assert_eq!(state.dismiss_direction(), SwipeToDismissBoxValue::EndToStart);
    ui.host().send_pointer(PointerEvent::release(Offset::new(262.0, y)));
    ui.advance_time_by(Duration::from_secs(1));
    assert_eq!(bounds(&mut ui, "item").left, 0.0);
    assert_eq!(state.current_value(), SwipeToDismissBoxValue::Settled);
    // Rightwards is disabled: the item doesn't move.
    ui.host().send_pointer(PointerEvent::press(Offset::new(100.0, y)));
    for x in [120.0, 160.0, 200.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
        ui.advance_time_by(Duration::from_millis(16));
    }
    assert_eq!(bounds(&mut ui, "item").left, 0.0);
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, y)));
    ui.advance_time_by(Duration::from_secs(1));
    // 150 dp left: dismissed, the item gone off the start.
    ui.host().send_pointer(PointerEvent::press(Offset::new(300.0, y)));
    for x in [280.0, 220.0, 150.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(150.0, y)));
    ui.advance_time_by(Duration::from_secs(1));
    assert_eq!(state.current_value(), SwipeToDismissBoxValue::EndToStart);
    assert_eq!(bounds(&mut ui, "item").left, -400.0);
    // Reset brings it back.
    let scope = CoroutineScope::new();
    let resetting = *state;
    scope.launch(async move { resetting.reset().await });
    ui.advance_time_by(Duration::from_secs(1));
    assert_eq!(bounds(&mut ui, "item").left, 0.0);
}

#[composable]
fn ScrollingSheetApp(state: SheetState) {
    MaterialTheme!({
        ModalBottomSheet!(|| {}, sheet_state = state, {
            Column!(
                modifier = Modifier.fill_max_width().vertical_scroll(remember_scroll_state!()),
                {
                    for index in 0..40 {
                        let tag = if index == 0 { "first" } else { "other" };
                        Text!("Item {}", index, modifier = Modifier.height(40.dp()).test_tag(tag));
                    }
                }
            );
        });
    });
}

#[test]
fn the_contents_scrolls_drag_the_sheet_first() {
    let state = SheetState::new(false, SheetValue::Hidden, |_| true, false);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || ScrollingSheetApp(given));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    let first = |ui: &mut ComposeTestRule| {
        ui.on_node_unmerged(has_test_tag("first")).fetch_semantics_node().bounds.top
    };
    assert_eq!(first(&mut ui), 300.0 + 48.0);
    // Up 100 past the slop on the content: the sheet goes up, the content doesn't scroll.
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 500.0)));
    for y in [492.0, 450.0, 420.0, 392.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(200.0, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    assert_eq!(first(&mut ui), 200.0 + 48.0);
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 392.0)));
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::Expanded);
    assert_eq!(first(&mut ui), 48.0);
    // Down on the content at its top: the sheet comes down.
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 300.0)));
    for y in [308.0, 350.0, 400.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(200.0, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    assert_eq!(first(&mut ui), 48.0 + 92.0);
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 400.0)));
}

#[test]
fn a_dragged_sheet_follows_the_finger() {
    let (mut ui, _state, _shown, _owner) = sheet(200.0);
    ui.host().send_pointer(PointerEvent::press(Offset::new(200.0, 450.0)));
    for y in [470.0, 500.0, 550.0] {
        ui.host().send_pointer(PointerEvent::moved(Offset::new(200.0, y), true));
        ui.advance_time_by(Duration::from_millis(100));
    }
    // 100 down, less the 8 dp touch slop: the sheet moving under the finger doesn't count.
    assert_eq!(bounds(&mut ui, "content").top, 400.0 + 92.0);
    ui.host().send_pointer(PointerEvent::release(Offset::new(200.0, 550.0)));
}
