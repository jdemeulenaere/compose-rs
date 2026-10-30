//! A review's scenarios of `draggable` and the scrollables' keys and gestures: what Compose's tests
//! don't try, each from the source it's about.
#![allow(non_snake_case)]
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Key, Modifier, MutableInteractionSource, Offset, PointerEvent, UnitExt};

fn consuming() -> (DefaultScrollableState, Rc<Cell<f32>>) {
    let total = Rc::new(Cell::new(0.0f32));
    let adding = total.clone();
    let state = scrollable_state(move |delta| {
        adding.set(adding.get() + delta);
        delta
    });
    (state, total)
}

fn counter() -> (Rc<Cell<f32>>, impl Fn(f32) + Clone + 'static) {
    let total = Rc::new(Cell::new(0.0f32));
    let adding = total.clone();
    (total, move |delta: f32| adding.set(adding.get() + delta))
}

fn window(content: impl Fn() + 'static) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(600, 600), content);
    ui.wait_for_idle();
    ui
}

fn focusable_child() {
    Box!(modifier = Modifier.size(50.dp()).test_tag("focus").focusable());
}

fn page_down(ui: &mut ComposeTestRule) {
    ui.on_node_with_tag("focus").request_focus();
    ui.wait_for_idle();
    ui.press_key(Key::PageDown);
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt: onKeyEvent, ContentInViewNode.viewportSize
#[test]
fn page_excludes_padding_written_before_scrollable() {
    let (state, total) = consuming();
    let mut ui = window(move || {
        Box!(
            modifier = Modifier
                .size(300.dp())
                .padding(20.dp())
                .scrollable(state.clone(), Orientation::Vertical),
            {
                focusable_child();
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(total.get(), -260.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/ContentInViewNode.kt: onRemeasured
#[test]
fn page_excludes_size_of_outer_padding_when_size_is_inside() {
    let (state, total) = consuming();
    let mut ui = window(move || {
        Box!(
            modifier = Modifier
                .padding(20.dp())
                .size(100.dp())
                .scrollable(state.clone(), Orientation::Vertical),
            {
                focusable_child();
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(total.get(), -100.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/ContentInViewNode.kt: onRemeasured
#[test]
fn page_includes_padding_written_after_scrollable() {
    let (state, total) = consuming();
    let mut ui = window(move || {
        Box!(
            modifier = Modifier
                .size(300.dp())
                .scrollable(state.clone(), Orientation::Vertical)
                .padding(20.dp()),
            {
                focusable_child();
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(total.get(), -300.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/ContentInViewNode.kt: onRemeasured
#[test]
fn page_on_vertical_scroll_inside_padding_is_the_viewport() {
    let scroll = ScrollState::new(0);
    let given = *scroll;
    let mut ui = window(move || {
        Column!(modifier = Modifier.size(300.dp()).padding(20.dp()).vertical_scroll(given), {
            focusable_child();
            Box!(modifier = Modifier.size(2000.dp()));
        });
    });
    page_down(&mut ui);
    assert_eq!(scroll.value(), 260);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/ContentInViewNode.kt: onRemeasured
#[test]
fn page_on_lazy_column_inside_padding_is_the_viewport() {
    let list = LazyListState::new(0, 0);
    let handle = *list;
    let mut ui = window(move || {
        LazyColumn!(
            modifier = Modifier.size(300.dp()).padding(20.dp()),
            state = handle,
            |scope: LazyListScope| {
                scope.item(|| {
                    Box!(modifier = Modifier.size(100.dp()).test_tag("focus").focusable());
                });
                scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(
        (handle.first_visible_item_index(), handle.first_visible_item_scroll_offset()),
        (2, 60)
    );
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt: onKeyEvent through ScrollingLogic.reverseIfNeeded
#[test]
fn page_reverse_direction_on_scrollable() {
    let (state, total) = consuming();
    let mut ui = window(move || {
        Box!(
            modifier = Modifier.size(300.dp()).scrollable_with(
                state.clone(),
                Orientation::Vertical,
                true,
                true,
                None,
                None
            ),
            {
                focusable_child();
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(total.get(), 300.0);
}

// ---- pressed kept when disabled ----

fn enabled_drag(
    enabled: impl Fn() -> bool + 'static,
    add: impl Fn(f32) + Clone + 'static,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let add = add.clone();
        let state = remember_draggable_state!(add);
        Box!(
            modifier = Modifier.size(100.dp()).draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions { enabled: enabled(), ..DraggableOptions::default() }
            )
        );
    });
    ui.wait_for_idle();
    ui
}

const CENTER: Offset = Offset::new(50.0, 50.0);

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isAnyChangedToDown reads the event, not a count of its own
#[test]
fn two_fingers_lifted_while_disabled_leave_the_next_drag_working() {
    let (total, add) = counter();
    let enabled = mutable_state_of(true);
    let mut ui = {
        let read = enabled.handle();
        enabled_drag(move || read.get(), add)
    };
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(0, CENTER);
        touch.down_pointer(1, CENTER + Offset::new(10.0, 0.0));
    });
    ui.wait_for_idle();
    enabled.set(false);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.up_pointer(1);
        touch.up_pointer(0);
    });
    ui.wait_for_idle();
    enabled.set(true);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(0, CENTER);
        touch.move_pointer_by(0, Offset::new(0.0, 30.0), 10);
        touch.up_pointer(0);
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 22.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isAnyChangedToDown
#[test]
fn one_finger_lifted_while_disabled_leaves_the_next_drag_working() {
    let (total, add) = counter();
    let enabled = mutable_state_of(true);
    let mut ui = {
        let read = enabled.handle();
        enabled_drag(move || read.get(), add)
    };
    ui.on_root().perform_touch_input(|touch| touch.down(CENTER));
    ui.wait_for_idle();
    enabled.set(false);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    enabled.set(true);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(0.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 22.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isAnyChangedToDown
#[test]
fn another_finger_id_after_one_lifted_while_disabled_drags() {
    let (total, add) = counter();
    let enabled = mutable_state_of(true);
    let mut ui = {
        let read = enabled.handle();
        enabled_drag(move || read.get(), add)
    };
    ui.on_root().perform_touch_input(|touch| touch.down_pointer(1, CENTER));
    ui.wait_for_idle();
    enabled.set(false);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| touch.up_pointer(1));
    ui.wait_for_idle();
    enabled.set(true);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(0, CENTER);
        touch.move_pointer_by(0, Offset::new(0.0, 30.0), 10);
        touch.up_pointer(0);
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 22.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: processInitialDownState
#[test]
fn press_made_while_disabled_then_enabled_does_not_lock() {
    let (total, add) = counter();
    let enabled = mutable_state_of(false);
    let mut ui = {
        let read = enabled.handle();
        enabled_drag(move || read.get(), add)
    };
    ui.on_root().perform_touch_input(|touch| touch.down(CENTER));
    ui.wait_for_idle();
    enabled.set(true);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::new(0.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 0.0, "no press seen: no drag");
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(0.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 22.0);
}

// ---- mouse ----

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt: CanDragCalculation, a mouse doesn't drag a scrollable
#[test]
fn mouse_click_moving_a_pixel_clicks_inside_a_scrollable() {
    let clicks = Rc::new(Cell::new(0));
    let told = clicks.clone();
    let (state, total) = consuming();
    let mut ui = window(move || {
        let told = told.clone();
        Box!(
            modifier = Modifier.size(300.dp()).scrollable(state.clone(), Orientation::Vertical),
            {
                let told = told.clone();
                Box!(
                    modifier = Modifier.size(300.dp()).clickable(move || told.set(told.get() + 1))
                );
            }
        );
    });
    ui.host().send_pointer(PointerEvent::press(CENTER));
    ui.host().send_pointer(PointerEvent::moved(CENTER + Offset::new(0.0, 1.0), true));
    ui.host().send_pointer(PointerEvent::release(CENTER + Offset::new(0.0, 1.0)));
    ui.wait_for_idle();
    assert_eq!(clicks.get(), 1);
    assert_eq!(total.get(), 0.0);
}

// Compose: the mouse's slop is crossed, the draggable consumes, the clickable inside cancels.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: pointerSlop
#[test]
fn mouse_click_moving_a_pixel_is_a_drag_inside_a_draggable() {
    let clicks = Rc::new(Cell::new(0));
    let told = clicks.clone();
    let (total, add) = counter();
    let mut ui = window(move || {
        let told = told.clone();
        let add = add.clone();
        let state = remember_draggable_state!(add);
        Box!(
            modifier = Modifier.size(300.dp()).draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions::default()
            ),
            {
                let told = told.clone();
                Box!(
                    modifier = Modifier.size(300.dp()).clickable(move || told.set(told.get() + 1))
                );
            }
        );
    });
    ui.host().send_pointer(PointerEvent::press(CENTER));
    ui.host().send_pointer(PointerEvent::moved(CENTER + Offset::new(0.0, 1.0), true));
    ui.host().send_pointer(PointerEvent::release(CENTER + Offset::new(0.0, 1.0)));
    ui.wait_for_idle();
    assert_eq!(clicks.get(), 0, "the drag took it");
    assert!((total.get() - (1.0 - 8.0 / 144.0)).abs() < 1e-4);
}

// Compose: awaitTouchSlopOrCancellation goes through pointerSlop(type): a mouse's 1/144.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: detectDragGestures, pointerSlop(down.type)
#[test]
fn detect_drag_gestures_uses_the_mouse_slop() {
    use compose_ui::DragGestures;
    let (total, add) = counter();
    let mut ui = window(move || {
        let add = add.clone();
        Box!(
            modifier = Modifier.size(300.dp()).pointer_input((), move |scope| {
                let add = add.clone();
                async move {
                    scope
                        .detect_drag_gestures(
                            DragGestures::default().on_drag(move |_, amount| add(amount.y)),
                        )
                        .await;
                }
            })
        );
    });
    ui.host().send_pointer(PointerEvent::press(CENTER));
    ui.host().send_pointer(PointerEvent::moved(CENTER + Offset::new(0.0, 1.0), true));
    ui.host().send_pointer(PointerEvent::release(CENTER + Offset::new(0.0, 1.0)));
    ui.wait_for_idle();
    assert!(total.get() > 0.0, "a mouse moving a pixel is past its slop");
}

// ---- the angle ----

fn touch_moves(ui: &mut ComposeTestRule, at: Offset, by: Offset) {
    ui.on_root().perform_touch_input(|touch| {
        touch.down(at);
        touch.move_by(by);
    });
    ui.wait_for_idle();
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isDragAngleAlignedWithOrientation
#[test]
fn same_chain_scrollable_outer_draggable_inner_at_50_degrees_goes_to_the_scrollable() {
    let (state, scrolled) = consuming();
    let (dragged, add) = counter();
    let mut ui = window(move || {
        let add = add.clone();
        let drag = remember_draggable_state!(add);
        Box!(
            modifier = Modifier
                .size(300.dp())
                .scrollable(state.clone(), Orientation::Vertical)
                .draggable_with(drag, Orientation::Horizontal, DraggableOptions::default())
        );
    });
    // Both past the slop: 50 degrees isn't the inner's, it's the outer's, waiting.
    touch_moves(&mut ui, Offset::new(50.0, 50.0), Offset::new(10.0, 12.0));
    assert_eq!(dragged.get(), 0.0);
    assert_eq!(scrolled.get(), 4.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isDragAngleAlignedWithOrientation
#[test]
fn same_chain_draggable_outer_scrollable_inner_at_24_degrees_goes_to_the_draggable() {
    let (state, scrolled) = consuming();
    let (dragged, add) = counter();
    let mut ui = window(move || {
        let add = add.clone();
        let drag = remember_draggable_state!(add);
        Box!(
            modifier = Modifier
                .size(300.dp())
                .draggable_with(drag, Orientation::Horizontal, DraggableOptions::default())
                .scrollable(state.clone(), Orientation::Vertical)
        );
    });
    touch_moves(&mut ui, Offset::new(50.0, 50.0), Offset::new(20.0, 9.0));
    assert_eq!(scrolled.get(), 0.0);
    assert_eq!(dragged.get(), 12.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isDragAngleAlignedWithOrientation, its example 4
#[test]
fn same_axis_nested_inner_takes_at_any_angle() {
    let (outer_state, outer) = consuming();
    let (inner_state, inner) = consuming();
    let mut ui = window(move || {
        let inner_state = inner_state.clone();
        Box!(
            modifier =
                Modifier.size(300.dp()).scrollable(outer_state.clone(), Orientation::Vertical),
            {
                let inner_state = inner_state.clone();
                Box!(
                    modifier = Modifier
                        .size(300.dp())
                        .scrollable(inner_state.clone(), Orientation::Vertical)
                );
            }
        );
    });
    // 24 degrees: not vertical at all, but no one around is horizontal: the inner takes it.
    touch_moves(&mut ui, Offset::new(50.0, 50.0), Offset::new(20.0, 9.0));
    assert_eq!(inner.get(), 1.0);
    assert_eq!(outer.get(), 0.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: traverseParentDraggableGestures
#[test]
fn a_lazy_row_inside_a_lazy_column_leaves_45_degrees_to_the_column() {
    let (column, row) = (LazyListState::new(0, 0), LazyListState::new(0, 0));
    let (column_handle, row_handle) = (*column, *row);
    let mut ui = window(move || {
        LazyColumn!(
            modifier = Modifier.size(300.dp()),
            state = column_handle,
            |scope: LazyListScope| {
                scope.item(move || {
                    LazyRow!(
                        modifier = Modifier.size((300.dp(), 200.dp())),
                        state = row_handle,
                        |scope: LazyListScope| {
                            scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
                        }
                    );
                });
                scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
            }
        );
    });
    touch_moves(&mut ui, Offset::new(150.0, 150.0), Offset::new(-20.0, -20.0));
    assert_eq!(row_handle.first_visible_item_scroll_offset(), 0);
    assert_eq!(column_handle.first_visible_item_scroll_offset(), 12);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: traverseParentDraggableGestures
#[test]
fn a_lazy_column_inside_a_lazy_row_leaves_24_degrees_to_the_row() {
    let (column, row) = (LazyListState::new(0, 0), LazyListState::new(0, 0));
    let (column_handle, row_handle) = (*column, *row);
    let mut ui = window(move || {
        LazyRow!(modifier = Modifier.size(300.dp()), state = row_handle, |scope: LazyListScope| {
            scope.item(move || {
                LazyColumn!(
                    modifier = Modifier.size((200.dp(), 300.dp())),
                    state = column_handle,
                    |scope: LazyListScope| {
                        scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
                    }
                );
            });
            scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
        });
    });
    touch_moves(&mut ui, Offset::new(100.0, 150.0), Offset::new(-20.0, -9.0));
    assert_eq!(column_handle.first_visible_item_scroll_offset(), 0);
    assert_eq!(row_handle.first_visible_item_scroll_offset(), 12);
}

// A disabled draggable around doesn't hold a clickable's press back, and takes nothing.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: gestureState, Idle when disabled
#[test]
fn a_disabled_draggable_doesnt_delay_a_press_inside() {
    let source = MutableInteractionSource::new();
    let (given, recorded) =
        (source.clone(), crate::interaction_helpers::record(source.as_source()));
    let mut ui = window(move || {
        let state = remember_draggable_state!(|_| {});
        let given = given.clone();
        Box!(
            modifier = Modifier.size(300.dp()).draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions { enabled: false, ..DraggableOptions::default() }
            ),
            {
                let given = given.clone();
                Box!(
                    modifier = Modifier.size(300.dp()).clickable_with(
                        compose_ui::ClickableOptions {
                            interaction_source: Some(given.clone()),
                            ..compose_ui::ClickableOptions::DEFAULT
                        },
                        || {}
                    )
                );
            }
        );
    });
    ui.on_root().perform_touch_input(|touch| touch.down(CENTER));
    ui.wait_for_idle();
    assert!(!recorded.borrow().is_empty(), "the press shows at once");
    ui.on_root().perform_touch_input(|touch| touch.up());
}

// ---- detach mid drag ----

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: onDetach, disposeInteractionSource; onDragStopped checks isAttached
#[test]
fn a_draggable_item_scrolled_away_mid_drag_cancels_without_a_stop_and_frees_its_state() {
    let list = LazyListState::new(0, 0);
    let handle = *list;
    let stops = Rc::new(Cell::new(0));
    let told = stops.clone();
    let source = MutableInteractionSource::new();
    let given = source.clone();
    let recorded = crate::interaction_helpers::record(source.as_source());
    let (dragged, add) = counter();
    let drag_state = draggable_state(add);
    let state = drag_state.clone();
    let mut ui = window(move || {
        let (told, given, state) = (told.clone(), given.clone(), state.clone());
        LazyColumn!(modifier = Modifier.size(300.dp()), state = handle, |scope: LazyListScope| {
            let (told, given, state) = (told.clone(), given.clone(), state.clone());
            scope.item(move || {
                let told = told.clone();
                Box!(
                    modifier = Modifier.size(100.dp()).draggable_with(
                        state.clone(),
                        Orientation::Horizontal,
                        DraggableOptions {
                            interaction_source: Some(given.clone()),
                            on_drag_stopped: Some(Rc::new(move |_, _| told.set(told.get() + 1))),
                            ..DraggableOptions::default()
                        }
                    )
                );
            });
            scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
        });
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(30.0, 0.0));
    });
    ui.wait_for_idle();
    assert_eq!(dragged.get(), 22.0);
    let _ = crate::scroll_support::run(&mut ui, async move { handle.scroll_to_item(20, 0).await });
    ui.wait_for_idle();
    assert_eq!(stops.get(), 0, "gone: no callback");
    let kinds = crate::interaction_helpers::kinds(&recorded);
    assert!(format!("{kinds:?}").contains("Cancel"), "{kinds:?}");
    // Its state's free: a drag of the same priority takes it, and returns.
    let ran = Rc::new(Cell::new(false));
    let set = ran.clone();
    crate::scroll_support::run(&mut ui, async move {
        drag_state
            .drag(compose_foundation::MutatePriority::Default, move |scope| async move {
                scope.drag_by(1.0);
                set.set(true);
            })
            .await
            .unwrap();
    });
    assert!(ran.get());
    assert_eq!(dragged.get(), 23.0);
    // The finger still down moves nothing, its release neither.
    ui.on_root().perform_touch_input(|touch| {
        touch.move_by(Offset::new(30.0, 0.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(dragged.get(), 23.0);
}

// ---- callback order ----

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: DraggableNode.onDragStarted, launched undispatched
#[test]
fn a_start_callback_runs_before_the_first_delta_and_what_it_launches_after() {
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let (for_delta, for_start) = (log.clone(), log.clone());
    let mut ui = window(move || {
        let (for_delta, for_start) = (for_delta.clone(), for_start.clone());
        let state = remember_draggable_state!(move |_| for_delta.borrow_mut().push("delta"));
        Box!(
            modifier = Modifier.size(300.dp()).draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions {
                    on_drag_started: Some(Rc::new(move |scope: CoroutineScope, _| {
                        for_start.borrow_mut().push("started");
                        let log = for_start.clone();
                        scope.launch(async move { log.borrow_mut().push("launched") });
                    })),
                    ..DraggableOptions::default()
                }
            )
        );
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(0.0, 30.0));
    });
    ui.wait_for_idle();
    assert_eq!(*log.borrow(), ["started", "delta", "launched"]);
    ui.on_root().perform_touch_input(|touch| touch.up());
}

// ---- batch 2 ----

// A finger down since before it was enabled counts: `isAnyChangedToDown` refuses a press beside it.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: isAnyChangedToDown, hasOtherActivePressedPointer
#[test]
fn a_press_beside_a_finger_down_since_disabled_starts_nothing() {
    let (total, add) = counter();
    let enabled = mutable_state_of(false);
    let mut ui = {
        let read = enabled.handle();
        enabled_drag(move || read.get(), add)
    };
    ui.on_root().perform_touch_input(|touch| touch.down_pointer(0, CENTER));
    ui.wait_for_idle();
    enabled.set(true);
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down_pointer(1, CENTER + Offset::new(10.0, 0.0));
        touch.move_pointer_by(1, Offset::new(0.0, 30.0), 10);
    });
    ui.wait_for_idle();
    assert_eq!(total.get(), 0.0);
}

// PointerInputModifierNode.onViewConfigurationChange's default cancels: Draggable.kt doesn't override it.
// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/PointerInputModifierNode.kt: onViewConfigurationChange
#[test]
fn a_new_view_configuration_mid_drag_stops_it() {
    let stops = Rc::new(Cell::new(0));
    let told = stops.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let told = told.clone();
        let state = remember_draggable_state!(|_| {});
        Box!(
            modifier = Modifier.size(100.dp()).draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions {
                    on_drag_stopped: Some(Rc::new(move |_, _| told.set(told.get() + 1))),
                    ..DraggableOptions::default()
                }
            )
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(0.0, 30.0));
    });
    ui.wait_for_idle();
    ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
        touch_slop: 20.0,
        ..compose_ui::ViewConfiguration::default()
    }));
    ui.wait_for_idle();
    assert_eq!(stops.get(), 1);
    ui.on_root().perform_touch_input(|touch| touch.up());
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/PointerInputModifierNode.kt: onDensityChange
#[test]
fn a_new_density_mid_drag_stops_it() {
    let stops = Rc::new(Cell::new(0));
    let told = stops.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let told = told.clone();
        let state = remember_draggable_state!(|_| {});
        Box!(
            modifier = Modifier.size(100.dp()).draggable_with(
                state,
                Orientation::Vertical,
                DraggableOptions {
                    on_drag_stopped: Some(Rc::new(move |_, _| told.set(told.get() + 1))),
                    ..DraggableOptions::default()
                }
            )
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(0.0, 30.0));
    });
    ui.wait_for_idle();
    ui.host().set_density(compose_ui::Density::new(2.0));
    ui.wait_for_idle();
    assert_eq!(stops.get(), 1);
    ui.on_root().perform_touch_input(|touch| touch.up());
}

fn remember_owned_text() -> Rc<OwnedState<String>> {
    remember(|| Rc::new(mutable_state_of(String::from("one\ntwo\nthree"))))
}

// Compose: a text field takes Page keys (KeyCommand.PAGE_DOWN moves the cursor) and consumes them.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/text/input/internal/TextFieldKeyEventHandler.kt: PAGE_DOWN
#[test]
fn page_down_in_a_text_field_inside_a_scroll_doesnt_scroll_it() {
    let scroll = ScrollState::new(0);
    let given = *scroll;
    let mut ui = window(move || {
        Column!(modifier = Modifier.size(300.dp()).vertical_scroll(given), {
            let text = remember_owned_text();
            let handle = text.clone();
            BasicTextField!(
                value = text.get(),
                on_value_change = move |value| handle.set(value),
                modifier = Modifier.size((300.dp(), 120.dp())).test_tag("focus"),
            );
            Box!(modifier = Modifier.size(2000.dp()));
        });
    });
    ui.on_node_with_tag("focus").request_focus();
    ui.wait_for_idle();
    ui.press_key(Key::PageDown);
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 0);
}

// A sibling under a disabled draggable: Compose's node stays a pointer target, so it's not hit.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: the node stays a pointer target
#[test]
fn a_disabled_draggable_over_a_sibling_blocks_it_as_compose_s_does() {
    let clicks = Rc::new(Cell::new(0));
    let told = clicks.clone();
    let mut ui = window(move || {
        let told = told.clone();
        let state = remember_draggable_state!(|_| {});
        Box!(modifier = Modifier.size(300.dp()), {
            let told = told.clone();
            Box!(modifier = Modifier.size(300.dp()).clickable(move || told.set(told.get() + 1)));
            Box!(
                modifier = Modifier.size(300.dp()).draggable_with(
                    state.clone(),
                    Orientation::Vertical,
                    DraggableOptions { enabled: false, ..DraggableOptions::default() }
                )
            );
        });
    });
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(clicks.get(), 0);
}

// The Compose detector needs inDirection > 0: a slop of 0 isn't crossed by a move across.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: TouchSlopDetector, inDirection > 0
#[test]
fn a_scrollable_with_a_slop_of_zero_isnt_taken_by_a_move_across() {
    let (state, _total) = consuming();
    let inside = Rc::new(Cell::new(false));
    let seen = inside.clone();
    let mut ui = window(move || {
        let seen = seen.clone();
        Box!(
            modifier = Modifier.size(300.dp()).scrollable(state.clone(), Orientation::Vertical),
            {
                let seen = seen.clone();
                Box!(
                    modifier = Modifier.size(300.dp()).pointer_input((), move |scope| {
                        let seen = seen.clone();
                        async move {
                            loop {
                                let event = scope
                                    .await_pointer_event(compose_ui::PointerEventPass::Final)
                                    .await;
                                let change = &event.changes[0];
                                if change.is_consumed() {
                                    seen.set(true);
                                }
                            }
                        }
                    })
                );
            }
        );
    });
    ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
        touch_slop: 0.0,
        ..compose_ui::ViewConfiguration::default()
    }));
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(30.0, 0.0));
    });
    ui.wait_for_idle();
    assert!(!inside.get(), "a move across isn't past a slop along the axis");
    ui.on_root().perform_touch_input(|touch| touch.up());
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: TouchSlopDetector, inDirection >= touchSlop
#[test]
fn a_draggable_is_taken_at_exactly_the_slop_and_not_short_of_it() {
    let slop = compose_ui::ViewConfiguration::default().touch_slop;
    let cases = [
        (Orientation::Vertical, Offset::new(0.0, slop), true),
        (Orientation::Vertical, Offset::new(0.0, slop - 0.25), false),
        (Orientation::Horizontal, Offset::new(slop, 0.0), true),
        (Orientation::Horizontal, Offset::new(slop - 0.25, 0.0), false),
    ];
    for (orientation, by, taken) in cases {
        let starts = Rc::new(Cell::new(0));
        let told = starts.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            let told = told.clone();
            let state = remember_draggable_state!(|_| {});
            Box!(
                modifier = Modifier.size(100.dp()).draggable_with(
                    state,
                    orientation,
                    DraggableOptions {
                        on_drag_started: Some(Rc::new(move |_, _| told.set(told.get() + 1))),
                        ..DraggableOptions::default()
                    }
                )
            );
        });
        ui.wait_for_idle();
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(by);
        });
        ui.wait_for_idle();
        assert_eq!(starts.get() == 1, taken, "{by:?}");
        ui.on_root().perform_touch_input(|touch| touch.up());
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: TouchSlopDetector, the distance with no orientation
#[test]
fn a_2d_draggable_is_taken_at_exactly_the_slop_and_not_short_of_it() {
    let slop = compose_ui::ViewConfiguration::default().touch_slop;
    for (by, taken) in [(slop, true), (slop - 0.25, false)] {
        let starts = Rc::new(Cell::new(0));
        let told = starts.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            let told = told.clone();
            let state = remember_draggable_2d_state!(|_| {});
            Box!(
                modifier = Modifier.size(100.dp()).draggable_2d(
                    state,
                    Draggable2DOptions {
                        on_drag_started: Some(Rc::new(move |_, _| told.set(told.get() + 1))),
                        ..Draggable2DOptions::default()
                    }
                )
            );
        });
        ui.wait_for_idle();
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(Offset::new(by, 0.0));
        });
        ui.wait_for_idle();
        assert_eq!(starts.get() == 1, taken, "{by}");
        ui.on_root().perform_touch_input(|touch| touch.up());
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: TouchSlopDetector, inDirection > 0
#[test]
fn with_a_slop_of_zero_a_move_across_takes_no_draggable() {
    for (orientation, across) in [
        (Orientation::Vertical, Offset::new(30.0, 0.0)),
        (Orientation::Horizontal, Offset::new(0.0, 30.0)),
    ] {
        let starts = Rc::new(Cell::new(0));
        let told = starts.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            let told = told.clone();
            let state = remember_draggable_state!(|_| {});
            Box!(
                modifier = Modifier.size(100.dp()).draggable_with(
                    state,
                    orientation,
                    DraggableOptions {
                        on_drag_started: Some(Rc::new(move |_, _| told.set(told.get() + 1))),
                        ..DraggableOptions::default()
                    }
                )
            );
        });
        ui.host().set_view_configuration(Some(compose_ui::ViewConfiguration {
            touch_slop: 0.0,
            ..compose_ui::ViewConfiguration::default()
        }));
        ui.wait_for_idle();
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(across);
        });
        ui.wait_for_idle();
        assert_eq!(starts.get(), 0, "{orientation:?}");
        ui.on_root().perform_touch_input(|touch| touch.up());
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt: detectDragGestures, crossed at exactly the slop
#[test]
fn detect_drag_gestures_starts_at_exactly_the_slop() {
    use compose_ui::DragGestures;
    let slop = compose_ui::ViewConfiguration::default().touch_slop;
    for (by, started) in [(slop, true), (slop - 0.25, false)] {
        let starts = Rc::new(Cell::new(0));
        let told = starts.clone();
        let mut ui = window(move || {
            let told = told.clone();
            Box!(
                modifier = Modifier.size(300.dp()).pointer_input((), move |scope| {
                    let told = told.clone();
                    async move {
                        scope
                            .detect_drag_gestures(
                                DragGestures::default()
                                    .on_drag_start(move |_| told.set(told.get() + 1)),
                            )
                            .await;
                    }
                })
            );
        });
        ui.on_root().perform_touch_input(|touch| {
            touch.down(CENTER);
            touch.move_by(Offset::new(0.0, by));
        });
        ui.wait_for_idle();
        assert_eq!(starts.get() == 1, started, "{by}");
        ui.on_root().perform_touch_input(|touch| touch.up());
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: DraggableNode.drag, of the priority UserInput
#[test]
fn a_drag_holds_a_state_of_your_own_at_the_user_input_priority() {
    #[derive(Clone)]
    struct OwnState {
        priorities: Rc<RefCell<Vec<MutatePriority>>>,
        total: Rc<Cell<f32>>,
    }
    impl compose_foundation::gestures::DraggableState for OwnState {
        fn drag_boxed<'a>(
            &'a self,
            priority: MutatePriority,
            block: compose_foundation::gestures::DragBlock<'a>,
        ) -> compose_foundation::gestures::LocalBoxFuture<'a, Result<(), MutationInterrupted>>
        {
            self.priorities.borrow_mut().push(priority);
            let total = self.total.clone();
            let scope = DragScope::new(move |delta| total.set(total.get() + delta));
            Box::pin(async move {
                block(scope).await;
                Ok(())
            })
        }
        fn dispatch_raw_delta(&self, delta: f32) {
            self.total.set(self.total.get() + delta);
        }
    }
    impl ComposeArg for OwnState {
        fn same(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.total, &other.total)
        }
    }
    let state = OwnState { priorities: Rc::default(), total: Rc::default() };
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        Box!(
            modifier = Modifier.size(100.dp()).draggable_with(
                given.clone(),
                Orientation::Vertical,
                DraggableOptions::default()
            )
        );
    });
    ui.wait_for_idle();
    ui.on_root().perform_touch_input(|touch| {
        touch.down(CENTER);
        touch.move_by(Offset::new(0.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(*state.priorities.borrow(), [MutatePriority::UserInput]);
    assert_eq!(state.total.get(), 22.0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Scrollable.kt: onKeyEvent, gated on enabled
#[test]
fn page_keys_do_nothing_on_a_disabled_scrollable_of_each_kind() {
    let (state, total) = consuming();
    let mut ui = window(move || {
        Box!(
            modifier = Modifier.size(300.dp()).scrollable_with(
                state.clone(),
                Orientation::Vertical,
                false,
                false,
                None,
                None
            ),
            {
                focusable_child();
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(total.get(), 0.0);

    let scroll = ScrollState::new(0);
    let given = *scroll;
    let mut ui = window(move || {
        Column!(
            modifier = Modifier.size(300.dp()).vertical_scroll_with(given, false, None, false),
            {
                focusable_child();
                Box!(modifier = Modifier.size(2000.dp()));
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(scroll.value(), 0);

    let list = LazyListState::new(0, 0);
    let handle = *list;
    let mut ui = window(move || {
        LazyColumn!(
            modifier = Modifier.size(300.dp()),
            state = handle,
            user_scroll_enabled = false,
            |scope: LazyListScope| {
                scope.item(focusable_child);
                scope.items(40, |_| Box!(modifier = Modifier.size(100.dp())));
            }
        );
    });
    page_down(&mut ui);
    assert_eq!(handle.first_visible_item_index(), 0);
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: traverseParentDraggableGestures, a vertical scroll around
#[test]
fn a_gesture_past_30_degrees_goes_past_a_horizontal_scrollable_to_a_vertical_scroll_around() {
    let scroll = ScrollState::new(0);
    let given = *scroll;
    let (state, inner) = consuming();
    let mut ui = window(move || {
        let state = state.clone();
        Column!(modifier = Modifier.size(300.dp()).vertical_scroll(given), {
            let state = state.clone();
            Box!(
                modifier =
                    Modifier.size(300.dp()).scrollable(state.clone(), Orientation::Horizontal)
            );
            Box!(modifier = Modifier.size(2000.dp()));
        });
    });
    touch_moves(&mut ui, Offset::new(150.0, 150.0), Offset::new(10.0, -20.0));
    assert_eq!(inner.get(), 0.0);
    assert_eq!(scroll.value(), 12);
}

fn press_shown_at_once(wrap: impl Fn() -> Modifier + 'static) -> bool {
    let source = MutableInteractionSource::new();
    let (given, recorded) =
        (source.clone(), crate::interaction_helpers::record(source.as_source()));
    let mut ui = window(move || {
        let given = given.clone();
        Box!(modifier = wrap(), {
            let given = given.clone();
            Box!(
                modifier = Modifier.size(100.dp()).clickable_with(
                    compose_ui::ClickableOptions {
                        interaction_source: Some(given.clone()),
                        ..compose_ui::ClickableOptions::DEFAULT
                    },
                    || {}
                )
            );
        });
    });
    ui.set_auto_advance(false);
    ui.on_root().perform_touch_input(|touch| touch.down(CENTER));
    !recorded.borrow().is_empty()
}

// hasWaitingParent: a ScrollableNode is a DragGestureNode, Waiting once it saw a touch's press.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: gestureState, hasWaitingParent
#[test]
fn a_press_inside_a_plain_scrollable_is_delayed() {
    let (state, _total) = consuming();
    assert!(!press_shown_at_once(move || {
        Modifier.size(300.dp()).scrollable(state.clone(), Orientation::Vertical)
    }));
}

// Disabled, gestureState is Idle.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/Draggable.kt: gestureState, Idle when disabled
#[test]
fn a_press_inside_a_disabled_vertical_scroll_is_shown_at_once() {
    let scroll = ScrollState::new(0);
    let given = *scroll;
    assert!(press_shown_at_once(move || {
        Modifier.size(300.dp()).vertical_scroll_with(given, false, None, false)
    }));
}
