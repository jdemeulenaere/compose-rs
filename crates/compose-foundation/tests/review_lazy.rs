//! Lazy list review: failing tests for `LazyColumn`, `LazyRow` and `LazyListState`
//! (docs/06-ui.md, "Lazy lists"; D125), the in-frame recompose-and-relayout loop of
//! `UiHost::frame`, and `DragScroller`. Each test states the input, what Jetpack Compose does,
//! and what happens instead.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Key, Offset, PointerEvent, UnitExt};

thread_local! {
    static COUNT: Cell<usize> = const { Cell::new(0) };
}

fn count() {
    COUNT.with(|count| count.set(count.get() + 1));
}

fn take_count() -> usize {
    COUNT.with(|count| count.replace(0))
}

fn top(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds.top
}

/// A 100 x 100 list of `count` rows, 20 px tall, tagged `row {index}`.
#[composable]
fn Rows(state: LazyListState, count: usize) {
    LazyColumn!(
        modifier = Modifier.size_wh(100.dp(), 100.dp()),
        state = state,
        |scope: LazyListScope| {
            scope.items(count, |index| {
                Box!(
                    modifier = Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("row {index}"))
                );
            });
        }
    );
}

/// Input: a 100 px `LazyColumn` of 100 keyed rows (20 px, keys `0..100`), scrolled to the row
/// keyed 10 (`scroll_to_item(10, 0)`); then an item keyed 1000 is inserted at index 0.
///
/// Compose: `LazyListState` remembers the key of the first visible item and, when the data
/// changes, finds it again (`updateScrollPositionIfTheFirstItemWasMoved`): the row keyed 10
/// stays at the top, `firstVisibleItemIndex` becomes 11, and nothing on screen moves.
///
/// Instead: the position is an index only, so the list stays at index 10, now the row keyed 9:
/// every row on screen jumps down by one. The same happens on removals before the first
/// visible item (the todo list removing an item above the ones shown).
#[test]
fn the_first_visible_item_is_kept_by_key_when_items_are_inserted_before_it() {
    let keys = mutable_state_of((0..100u32).collect::<Vec<_>>());
    let handle = keys.handle();
    let state = LazyListState::new(10, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let keys = std::rc::Rc::new(handle.get());
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                let (by_index, labels) = (keys.clone(), keys.clone());
                scope.items_keyed(
                    keys.len(),
                    move |index| by_index[index],
                    move |index| {
                        let key = labels[index];
                        Box!(
                            modifier =
                                Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("key {key}"))
                        );
                    },
                );
            }
        );
    });
    assert_eq!(top(&mut ui, "key 10"), 0.0);
    keys.edit(|keys| keys.insert(0, 1000));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 11, "the first visible item, by key");
    assert_eq!(top(&mut ui, "key 10"), 0.0, "the row at the top moved");
}

/// Input: a 100 px `LazyColumn` with `vertical_arrangement = Arrangement::BOTTOM` and two
/// 20 px items (a chat-style list).
///
/// Compose: the arrangement places the items when they don't fill the viewport
/// (`LazyColumn`'s `verticalArrangement` is used to arrange them): at the bottom, at 60 and 80.
/// `Arrangement.Center` would put them at 30 and 50.
///
/// Instead: only the arrangement's spacing is used; the items are at the top, at 0 and 20.
#[test]
fn the_arrangement_places_items_that_do_not_fill_the_list() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            vertical_arrangement = Arrangement::BOTTOM,
            |scope: LazyListScope| {
                scope.items(2, |index| {
                    Box!(
                        modifier =
                            Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    assert_eq!(top(&mut ui, "row 0"), 60.0);
    assert_eq!(top(&mut ui, "row 1"), 80.0);
}

/// Input: a 100 px `LazyColumn` of 1000 rows whose heights vary (50 px for every fourth index,
/// 10 px otherwise), scrolled to item 100; then `scroll_by(-200.0)`.
///
/// Compose: measuring composes the items it scrolls back over, so the content moves exactly
/// 200 px: item 88 (spanning 240 to 190 px above item 100) is first, 40 px scrolled off, and
/// item 100's top is 200 px below the viewport's top.
///
/// Instead: the items before the composed ones are skipped with an estimated size (the average
/// of the sizes measured so far), and the position found with the estimate is kept: the list
/// lands on a different item and offset, so a drag or `scroll_by` backward moves the content
/// by a different distance than asked, and the content under the finger slips.
#[test]
fn scrolling_back_past_the_composed_items_moves_by_the_distance_asked() {
    let state = LazyListState::new(100, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    let height = if index % 4 == 0 { 50 } else { 10 };
                    Box!(
                        modifier = Modifier
                            .size_wh(100.dp(), height.dp())
                            .test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    assert_eq!(top(&mut ui, "row 100"), 0.0);
    assert!(state.scroll_by(-200.0));
    ui.wait_for_idle();
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (88, 40),
        "where 200 px back lands"
    );
}

/// Input: a `LazyColumn` of 1000 rows; ten wheel events of 0.4 px each (a trackpad's fine
/// pixel deltas), a frame after each.
///
/// Compose: `LazyListState` keeps the fraction it couldn't consume (`scrollToBeConsumed`), so
/// the deltas add up: the list is 4 px scrolled.
///
/// Instead: each frame rounds the offset plus the pending delta (0.4 rounds to 0), so the
/// fractions are lost and the list never moves; a slow trackpad scroll does nothing (and deltas
/// of 0.6 px would scroll 10 px instead of 6).
#[test]
fn sub_pixel_wheel_deltas_add_up() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Rows(given.clone(), 1000));
    for _ in 0..10 {
        ui.host()
            .send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 0.4)));
        ui.wait_for_idle();
    }
    assert_eq!(state.first_visible_item_scroll_offset(), 4);
}

/// Input: a `Column` with `vertical_scroll`, 100 px tall, holding 300 px of content; a slow
/// drag up by 20 px in moves of 0.4 px.
///
/// Compose: the scrollable's drag deltas reach `ScrollState`, which accumulates fractions
/// (`accumulator`): after the 8 dp slop, the content follows the finger, 12 px scrolled.
///
/// Instead: `scroll_state_by` rounds each 0.4 px delta to no movement and drops it, so a slow
/// drag never scrolls.
#[test]
fn a_slow_drag_scrolls() {
    let state = ScrollState::new(0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Column!(modifier = Modifier.size_wh(100.dp(), 100.dp()).vertical_scroll(given.clone()), {
            Spacer(Modifier.size_wh(100.dp(), 300.dp()));
        });
    });
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 80.0)));
    let mut y = 80.0;
    for _ in 0..50 {
        y -= 0.4;
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, y), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(50.0, y)));
    ui.wait_for_idle();
    assert!(state.value() >= 11, "scrolled {} px by a 20 px drag", state.value());
}

/// Input: a 100 px `Column` with `vertical_scroll` holding a 100 px `LazyColumn` of 1000 rows
/// and 200 px below it; a drag up by 50 px inside the lazy list.
///
/// Compose: nested scrolling gives the drag to the innermost scrollable first and only what it
/// leaves to the parent: the lazy list scrolls, the column stays at 0.
///
/// Instead: `DragScroller` takes drags in the Initial pass, outermost first, and doesn't look
/// at whether the event was consumed: both the column and the lazy list scroll by the drag.
#[test]
fn a_drag_in_a_nested_lazy_list_scrolls_only_the_list() {
    let outer = ScrollState::new(0);
    let state = LazyListState::new(0, 0);
    let (given_outer, given) = (outer.clone(), state.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let given = given.clone();
        Column!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()).vertical_scroll(given_outer.clone()),
            {
                Rows(given.clone(), 1000);
                Spacer(Modifier.size_wh(100.dp(), 200.dp()));
            }
        );
    });
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 90.0)));
    for y in [80.0, 70.0, 60.0, 50.0, 40.0] {
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, y), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(50.0, 40.0)));
    ui.wait_for_idle();
    assert!(state.first_visible_item_index() >= 2, "the lazy list scrolled");
    assert_eq!(outer.value(), 0, "the outer column scrolled too");
}

/// Input: a composable that counts its body runs and calls a `LazyColumn` of 1000 rows; the
/// list scrolls by 60 px (three rows), so the composed range changes.
///
/// Compose: scrolling composes the new items inside the lazy layout (subcomposition); the
/// composable that called `LazyColumn` doesn't recompose.
///
/// Instead: `LazyList` is non-restartable and reads the composed range (`state.composed`) in
/// its caller's scope, so each change of the range recomposes the caller. In the todo example
/// that caller is the screen's column content, which filters every todo into a new `Vec`: each
/// scroll step that reaches a new row is O(n) (measured in release: about 0.65 ms per wheel
/// scroll with 200,000 todos, 10 ms with 2,000,000).
#[test]
fn scrolling_does_not_recompose_the_caller_of_the_list() {
    #[composable]
    fn Screen(state: LazyListState) {
        count();
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = state,
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    Box!(
                        modifier =
                            Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    }
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Screen(given.clone()));
    take_count();
    state.scroll_by(60.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 3);
    assert_eq!(take_count(), 0, "the caller of LazyColumn recomposed on a scroll");
}

/// Input: a `LazyColumn` of 1000 rows whose item content counts its runs; the list scrolls by
/// three rows (60 px).
///
/// Compose: an item's content runs when the item is first composed; scrolling composes the
/// three items coming into range and leaves the others alone (their content lambdas aren't
/// re-run).
///
/// Instead: every recomposition of the list (the caller's, see above) builds new item content
/// closures, never compared, so every item in the composed range runs its content again
/// whenever the range changes: nine contents instead of three, six of them for items that
/// didn't change (and item content that isn't a skippable composable recomposes in full).
#[test]
fn scrolling_one_row_composes_only_the_rows_coming_into_range() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    count();
                    Box!(
                        modifier =
                            Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    take_count();
    state.scroll_by(60.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 3);
    let ran = take_count();
    assert!(ran <= 3, "{ran} item contents ran for a three-row scroll");
}

/// Input: a 100 px `LazyColumn` of 1000 items: the first ten 100 px tall, the rest 2 px; a
/// `scroll_to_item(500, 0)` and one frame.
///
/// Compose: measurement composes items until the viewport is full, in the same frame: items
/// 500 to 549 are shown.
///
/// Instead: each round of the frame's relayout loop composes as many items as the room left
/// divided by the average size measured so far (about 50 px), a few at a time, and the loop
/// stops after three rounds: the frame is drawn with most of the viewport empty, filled over
/// the next frames.
#[test]
fn a_jump_fills_the_viewport_in_one_frame() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    let height = if index < 10 { 100 } else { 2 };
                    Box!(
                        modifier = Modifier
                            .size_wh(100.dp(), height.dp())
                            .test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    state.scroll_to_item(500, 0);
    ui.host().frame();
    ui.on_node_with_tag("row 500").assert_exists();
    ui.on_node_with_tag("row 549").assert_exists();
}

/// Input: a 100 px `LazyColumn` of 1000 clickable (focusable) 20 px rows; Tab pressed seven
/// times.
///
/// Compose: focus moves from row to row; the lazy list lays out items beyond its bounds for the
/// focus search and the focused item is brought into view, so the seventh Tab focuses row 6,
/// scrolled into the viewport.
///
/// Instead: only the five rows in view are placed (the two composed beyond aren't), so focus
/// search can't reach row 5: Tab from row 4 wraps to row 0 (the seventh Tab focuses row 1), and
/// keyboard users can't get past the first screen of a list.
#[test]
fn tab_moves_focus_past_the_rows_in_view() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.size_wh(100.dp(), 100.dp()), |scope: LazyListScope| {
            scope.items(1000, |index| {
                Box!(
                    modifier = Modifier
                        .size_wh(100.dp(), 20.dp())
                        .clickable(|| {})
                        .test_tag(format!("row {index}"))
                );
            });
        });
    });
    for _ in 0..7 {
        ui.press_key(Key::Tab);
    }
    ui.on_node_with_tag("row 6").assert_is_focused();
    let row = ui.on_node_with_tag("row 6").fetch_semantics_node().bounds;
    assert!(row.top >= 0.0 && row.bottom <= 100.0, "the focused row is out of view: {row:?}");
}

/// Input: a `LazyColumn` whose first item is a `BasicTextField`, focused and typed into; the
/// list scrolls 300 px down, then more text is typed.
///
/// Compose: a focused item is pinned (`focusable` uses `LocalPinnableContainer`): it stays
/// composed and focused while scrolled out of view, and typing goes to it.
///
/// Instead: the field leaves the composition when it leaves the composed range, so focus is
/// lost and the typed text goes nowhere.
#[test]
fn a_focused_item_scrolled_out_of_view_keeps_focus() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let text = mutable_state_of(String::new());
    let handle = text.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()),
            state = given.clone(),
            |scope: LazyListScope| {
                scope.item(move || {
                    BasicTextField!(
                        value = handle.get(),
                        on_value_change = move |value| handle.set(value),
                        modifier = Modifier.size_wh(100.dp(), 20.dp()).test_tag("field"),
                    );
                });
                scope.items(1000, |index| {
                    Box!(
                        modifier =
                            Modifier.size_wh(100.dp(), 20.dp()).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.type_text("a");
    assert_eq!(text.get(), "a");
    state.scroll_by(300.0);
    ui.wait_for_idle();
    ui.type_text("x");
    assert_eq!(text.get(), "ax", "the focused field didn't take the text");
}

/// Input: a `LazyColumn` of 1000 rows; a wheel scroll of 200 px, then one frame.
///
/// Compose: n/a (no `FrameStats`); the frame composes the rows the scroll reached.
///
/// Instead: `FrameStats::recomposed` is false: the relayout loop's recompositions aren't
/// counted, so a frame that composed ten new rows reports it recomposed nothing.
#[test]
fn a_frame_that_composes_in_its_relayout_loop_reports_it_recomposed() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Rows(given.clone(), 1000));
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 200.0)));
    let stats = ui.host().frame();
    assert_eq!(state.first_visible_item_index(), 10);
    assert!(stats.recomposed, "{stats:?}");
}

/// Input: a `LazyColumn` of 1000 rows; a drag up by 50 px in moves of 10 px.
///
/// Compose: the drag starts once the pointer moves past the touch slop (8 dp), and scrolls by
/// what it moved past the slop: 42 px, item 2 at the top, 2 px off.
///
/// Instead: the move that crosses the slop scrolls by all of its 10 px, slop included: the
/// content jumps by the slop when a drag starts, and the list is scrolled 50 px.
#[test]
fn a_drag_scrolls_by_what_it_moved_past_the_slop() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Rows(given.clone(), 1000));
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 90.0)));
    for y in [80.0, 70.0, 60.0, 50.0, 40.0] {
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, y), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(50.0, 40.0)));
    ui.wait_for_idle();
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (2, 2)
    );
}

/// Input: a 100 px `Column` with `vertical_scroll` holding a `LazyColumn` of six 20 px rows
/// (20 px of scrolling) and 200 px below it; a wheel scroll of 100 px over the lazy list.
///
/// Compose: the wheel goes through nested scrolling: the lazy list consumes the 20 px it can
/// and the column scrolls by the 80 px left.
///
/// Instead: `scroll_by` reports the list can scroll and the whole event is consumed: the list
/// moves 20 px and the 80 px are lost; the column only scrolls on the next wheel event.
#[test]
fn the_wheel_delta_a_nested_lazy_list_leaves_scrolls_its_parent() {
    let outer = ScrollState::new(0);
    let state = LazyListState::new(0, 0);
    let (given_outer, given) = (outer.clone(), state.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        let given = given.clone();
        Column!(
            modifier = Modifier.size_wh(100.dp(), 100.dp()).vertical_scroll(given_outer.clone()),
            {
                Rows(given.clone(), 6);
                Spacer(Modifier.size_wh(100.dp(), 200.dp()));
            }
        );
    });
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 100.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 1, "the lazy list is at its end");
    assert_eq!(outer.value(), 80, "the column didn't get what the list left");
}

/// Input: a `LazyColumn` of 1000 rows, nothing reading its state in composition; a wheel scroll
/// of 5 px and one frame.
///
/// Compose: the frame measures the list at its new position; writing `firstVisibleItemIndex`
/// and the offset during measurement schedules nothing, as no composition reads them: the UI
/// is idle after that frame.
///
/// Instead: the list writes the offset state while measuring, and `UiHost::frame` takes any
/// write during the frame as a request for another (`frame_requested`, then `wake`): every
/// scroll frame is followed by an idle frame, and the platform is woken for it.
#[test]
fn a_scroll_frame_leaves_no_pending_work() {
    let state = LazyListState::new(0, 0);
    let given = state.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || Rows(given.clone(), 1000));
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 5.0)));
    ui.host().frame();
    assert_eq!(state.first_visible_item_scroll_offset(), 5);
    assert!(!ui.host().has_pending_work(), "another frame was asked for");
}
