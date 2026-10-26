//! Review of D470-D474: lists place only the items in view; frame rounds ask for no frame. The
//! "Instead" paragraphs say what 987162f did; D475-D477 fixed each (the tests pass).

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::platform;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, PointerId, PointerType, UnitExt};

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// A 500 px list: a `header` px tall first item, then 50 px rows.
fn header_list(state: LazyListState, header: f32, row: f32) -> impl Fn() {
    move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 500.dp())),
            state = state,
            move |scope: LazyListScope| {
                scope.item(move || {
                    Box!(modifier = Modifier.size((100.dp(), header.dp())).test_tag("header"));
                });
                scope.items(200, move |index| {
                    Box!(
                        modifier =
                            Modifier.size((100.dp(), row.dp())).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    }
}

/// Input: a 500 px list whose first item is 1,000 px tall, then 50 px rows, at item 3 (1,100 px
/// down); `scroll_by(-600.0)`.
///
/// Compose: the list moves 600 px back (into the header, 500 px down).
///
/// Instead (since D470): the header isn't composed (nothing beyond the view), so the room back
/// is estimated from the rows' average size (3 × 50 px): the list moves 150 px only.
#[test]
fn scrolling_back_onto_a_big_item_out_of_view_moves_the_whole_delta() {
    let state = LazyListState::new(3, 0);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), header_list(*state, 1000.0, 50.0));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 3);
    state.dispatch_raw_delta(-600.0);
    ui.wait_for_idle();
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 500),
    );
}

/// Input: the end of a 500 px list of 50 px rows, then a 1,000 px footer; at row 10 (rows 10 to
/// 19 fill the view, the footer just below); `scroll_by(600.0)`.
///
/// Compose: the list moves 600 px on (into the footer).
///
/// Instead (since D470): the footer isn't composed, the room on is estimated from the average
/// row: the list moves 50 px.
#[test]
fn scrolling_on_onto_a_big_item_out_of_view_moves_the_whole_delta() {
    let state = LazyListState::new(10, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 500.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(20, |_| Box!(modifier = Modifier.size((100.dp(), 50.dp()))));
                scope.item(|| Box!(modifier = Modifier.size((100.dp(), 1000.dp()))));
            }
        );
    });
    state.dispatch_raw_delta(600.0);
    ui.wait_for_idle();
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (20, 100),
    );
}

/// Input: as above, a 600 px header and 20 px rows, at a restored position (row 30); then a
/// fast downward swipe flings back up.
///
/// Compose: the fling reaches the top (item 0, offset 0) and stops there.
///
/// Instead (since D470): one frame's delta exceeds the estimated room back (the average size
/// and the offset), the fling takes it as the edge and ends at the header's last 20 px (an
/// overscroll then shows on Android).
#[test]
fn a_fling_back_to_a_big_first_item_reaches_the_top() {
    // A restored position (`remember_saveable`, `initial_first_visible_item_index`).
    let state = LazyListState::new(30, 0);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), header_list(*state, 600.0, 20.0));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    // A quick downward swipe: 300 px in 60 ms, every 10 ms.
    let start = platform::now_nanos();
    let x = 50.0;
    ui.host().send_pointer(touch(PointerEvent::press(Offset::new(x, 100.0))).at_time(start));
    for step in 1..=6u64 {
        let at = start + step * 10_000_000;
        let y = 100.0 + step as f32 * 50.0;
        ui.host().send_pointer(touch(PointerEvent::moved(Offset::new(x, y), true)).at_time(at));
        ui.advance_time_by(Duration::from_millis(10));
    }
    ui.host().send_pointer(
        touch(PointerEvent::release(Offset::new(x, 400.0))).at_time(start + 60_000_000),
    );
    ui.advance_time_by(Duration::from_millis(3000));
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 0),
    );
}

thread_local! {
    static BOTTOM: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

/// A `height` px list: a `header` px first item, then `row` px rows, each noting how far down
/// it's placed.
fn header_rows(height: f32, header: f32, row: f32) -> impl Fn() {
    move || {
        let note = |coordinates: compose_ui::LayoutCoordinates| {
            let bottom = coordinates.bounds_in_root().bottom;
            BOTTOM.with(|cell| cell.set(cell.get().max(bottom)));
        };
        LazyColumn!(
            modifier = Modifier.size((100.dp(), height.dp())),
            move |scope: LazyListScope| {
                scope.item(move || {
                    Box!(
                        modifier =
                            Modifier.size((100.dp(), header.dp())).on_globally_positioned(note)
                    );
                });
                scope.items(400, move |_| {
                    Box!(
                        modifier = Modifier.size((100.dp(), row.dp())).on_globally_positioned(note)
                    );
                });
            }
        );
    }
}

/// How far down the first frame's items reach.
fn first_frame_bottom(height: f32, header: f32, row: f32) -> f32 {
    let mut host = compose_ui::platform::UiHost::headless(100, height as i32);
    host.set_content(header_rows(height, header, row));
    BOTTOM.with(|cell| cell.set(0.0));
    host.frame();
    BOTTOM.with(|cell| cell.get())
}

/// Input: a 2,244 px list (the Pixel 8 Pro's screen) of a 300 px header (a hero image) and
/// 120 px rows; one frame.
///
/// Compose: the first frame shows the list full (its measurement composes items until the
/// viewport is filled).
///
/// Instead (since D472): the first layout asks for 4 more items whatever its estimate, then
/// each round estimates from the sizes known (the header's weighs on them): the three rounds
/// run out with the list short (2,220 px), and the first frame shows its bottom empty; the
/// next frame fills it. 727d266 filled it.
#[test]
fn the_first_frame_of_a_list_under_a_header_is_full() {
    let bottom = first_frame_bottom(2244.0, 300.0, 120.0);
    assert!(bottom >= 2244.0, "the first frame's items end at {bottom} px");
}

/// The same with an 81 px header (the stress list's) and 20 px rows in 800 px (781 px), or
/// 5 px rows in 500 px (391 px).
#[test]
fn the_first_frame_of_a_list_of_small_rows_under_a_header_is_full() {
    let bottom = first_frame_bottom(800.0, 81.0, 20.0);
    assert!(bottom >= 800.0, "the first frame's items end at {bottom} px");
    let bottom = first_frame_bottom(500.0, 81.0, 5.0);
    assert!(bottom >= 500.0, "the first frame's items end at {bottom} px");
}

/// Input: a 100 px list of focusable 20 px rows (five in view); six Tabs before a frame runs
/// (queued while the main thread was busy).
///
/// Compose: focus search lays out items beyond the bounds when it asks: the sixth Tab
/// focuses row 5.
///
/// Instead (since D470): the rows beyond aren't composed until a frame after the focus came
/// into the list (`pinned`): the sixth Tab finds no row after row 4 and leaves the list.
#[test]
fn two_tabs_in_a_frame_move_past_the_rows_in_view() {
    use compose_ui::{Key, KeyEvent};
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.items(1000, |index| {
                Box!(
                    modifier = Modifier
                        .size((100.dp(), 20.dp()))
                        .clickable(|| {})
                        .test_tag(format!("row {index}"))
                );
            });
        });
    });
    // Six Tabs queued while the main thread was busy: rows 0 to 5.
    for _ in 0..6 {
        ui.host().send_key(KeyEvent::down(Key::Tab));
        ui.host().send_key(KeyEvent::up(Key::Tab));
    }
    ui.wait_for_idle();
    ui.on_node_with_tag("row 5").assert_is_focused();
}

/// Input: a 2,244 px list of 120 px rows (19 in view), the first a text field; a tap on a row
/// (touch), then focus on the field (a tap on it) and one frame.
///
/// Compose: focus composes nothing (items beyond the bounds are laid out only while a focus
/// search asks for them).
///
/// Instead (since D470): while an item holds the focus, a viewport's worth of items is wanted
/// beyond each side: the frame after the field took the focus composes them (16 rows here), in
/// the frame.
#[test]
fn focusing_a_field_in_a_list_composes_no_rows_in_a_frame() {
    let text = compose_runtime::mutable_state_of(String::new());
    let handle = text.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 2244), move || {
        LazyColumn!(modifier = Modifier.size((300.dp(), 2244.dp())), |scope: LazyListScope| {
            scope.item(move || {
                BasicTextField!(
                    value = handle.get(),
                    on_value_change = move |value| handle.set(value),
                    modifier = Modifier.size((300.dp(), 120.dp())).test_tag("field"),
                );
            });
            scope.items(1000, |index| {
                Box!(
                    modifier = Modifier
                        .size((300.dp(), 120.dp()))
                        .clickable(|| {})
                        .test_tag(format!("row {index}"))
                );
            });
        });
    });
    ui.host().prefetch(std::time::Instant::now() + Duration::from_secs(60));
    // A tap on a row: no focus in touch mode, nothing composed.
    let at = Offset::new(100.0, 500.0);
    ui.host().send_pointer(touch(PointerEvent::press(at)));
    ui.host().send_pointer(touch(PointerEvent::release(at)));
    let stats = ui.host().frame();
    assert_eq!(stats.bodies_run, 0, "a tap on a row composed");
    ui.wait_for_idle();
    let before = platform::recomposition_counts();
    ui.on_node_with_tag("field").request_focus();
    ui.wait_for_idle();
    let rows = platform::recomposition_counts().since(&before).scopes_named("LazyItem");
    assert_eq!(rows, 0, "focusing the field composed {rows} items in frames");
}

/// D474: what the items a round composes start (size callbacks writing state, effects waiting
/// for frames) still gets its frames, by the signals the hosts read.
#[test]
fn what_items_composed_in_a_round_start_gets_its_frames() {
    use compose_runtime::*;
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 200.dp())), |scope: LazyListScope| {
            scope.items(100, |index| {
                let size = remember(|| mutable_state_of(0));
                let steps = remember(|| mutable_state_of(0));
                let handle = steps;
                LaunchedEffect((), async move {
                    for _ in 0..3 {
                        with_frame_nanos(|_| ()).await;
                        handle.update(|steps| steps + 1);
                    }
                });
                let sized = size;
                Box!(
                    modifier = Modifier
                        .size((100.dp(), 20.dp()))
                        .on_size_changed(move |s: IntSize| sized.set(s.height)),
                    {
                        BasicText!("row {} {} {}", index, size.get(), steps.get());
                    }
                );
            });
        });
    });
    for index in 0..10 {
        ui.on_node_with_text(&format!("row {index} 20 3")).assert_exists();
    }
}
