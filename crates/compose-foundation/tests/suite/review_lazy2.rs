//! Second lazy list review: failing tests for `LazyColumn`, `LazyRow` and `LazyListState`
//! (docs/06-ui.md, "Lazy lists"; D125, D129), `DragScroller`, `bring_into_view` on focus and the
//! relayout loop of `UiHost::frame`, after the first review's fixes (`review_lazy.rs`). Each
//! test states the input, what Jetpack Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Key, KeyEvent, Offset, PointerEvent, UnitExt};

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
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds.top
}

/// A 100 x 100 list of rows keyed by `keys`, 20 px tall, tagged `key {key}`; clickable (so
/// focusable) when `clickable`.
#[composable]
fn KeyedRows(state: LazyListState, keys: Rc<Vec<u32>>, clickable: bool) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            let (by_index, labels) = (keys.clone(), keys.clone());
            scope.items_keyed(
                keys.len(),
                move |index| by_index[index],
                move |index| {
                    let key = labels[index];
                    let modifier = Modifier.size((100.dp(), 20.dp()));
                    let modifier = if clickable { modifier.clickable(|| {}) } else { modifier };
                    Box!(modifier = modifier.test_tag(format!("key {key}")));
                },
            );
        }
    );
}

/// Input: `LazyListState::new(10, 5)` (a restored position, as
/// `rememberLazyListState(10, 5)`), given to a `LazyColumn` whose data is still loading: no
/// items in the first frame, 100 rows in the next.
///
/// Compose: `LazyListScrollPosition.updateFromMeasureResult` ignores the index and offset of
/// layouts until the first one with items ("otherwise the initial index and scroll passed to
/// the state would be lost"): once the rows arrive, row 10 is first, 5 px scrolled off.
///
/// Instead: the empty layout resets the position to (0, 0), so the restored position is lost
/// and the list shows its top.
#[test]
fn an_initial_position_survives_an_empty_first_layout() {
    let keys = mutable_state_of(Vec::<u32>::new());
    let handle = keys.handle();
    let state = LazyListState::new(10, 5);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        KeyedRows(given, Rc::new(handle.get()), false)
    });
    keys.set((0..100).collect());
    ui.wait_for_idle();
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (10, 5),
        "the restored position"
    );
}

/// Input: a `LazyColumn` of 100 keyed rows scrolled to the row keyed 50; the data becomes empty
/// (a search filter matching nothing), then the 100 rows come back (the filter cleared).
///
/// Compose: an empty layout sets `lastKnownFirstItemKey` to null and the index to 0, so the
/// refilled list starts at its top: row 0 first.
///
/// Instead: an empty layout keeps the stale first key, and the next layout finds it again: the
/// list jumps back to the row keyed 50.
#[test]
fn a_list_emptied_and_refilled_starts_at_its_top() {
    let keys = mutable_state_of((0..100u32).collect::<Vec<_>>());
    let handle = keys.handle();
    let state = LazyListState::new(50, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        KeyedRows(given, Rc::new(handle.get()), false)
    });
    assert_eq!(top(&mut ui, "key 50"), 0.0);
    keys.set(Vec::new());
    ui.wait_for_idle();
    keys.set((0..100).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 0, "the refilled list didn't start at its top");
}

/// Input: a `LazyColumn` of 1000 keyed rows whose first item (key 0) is a `BasicTextField`,
/// focused and typed into; an item is inserted before it (key 5000, so the field is now item
/// 1), then the list scrolls 300 px down and more text is typed.
///
/// Compose: the focused item is pinned as an item (`LazyLayoutPinnableItem` follows its key):
/// it stays composed and focused while scrolled away, and typing goes to it.
///
/// Instead: the pin is an index (`pinned: Some(0)`), set when focus came and never moved with
/// the data: after the insertion it pins the new item 0, the field leaves the composition when
/// scrolled out of range, focus is lost and the typed text goes nowhere.
#[test]
fn a_focused_item_stays_pinned_when_items_are_inserted_before_it() {
    let keys = mutable_state_of((0..1000u32).collect::<Vec<_>>());
    let handle = keys.handle();
    let text = mutable_state_of(String::new());
    let field = text.handle();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let keys = Rc::new(handle.get());
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                let (by_index, labels) = (keys.clone(), keys.clone());
                scope.items_keyed(
                    keys.len(),
                    move |index| by_index[index],
                    move |index| {
                        let key = labels[index];
                        if key == 0 {
                            BasicTextField!(
                                value = field.get(),
                                on_value_change = move |value| field.set(value),
                                modifier = Modifier.size((100.dp(), 20.dp())).test_tag("field"),
                            );
                        } else {
                            Box!(
                                modifier = Modifier
                                    .size((100.dp(), 20.dp()))
                                    .test_tag(format!("key {key}"))
                            );
                        }
                    },
                );
            }
        );
    });
    ui.on_node_with_tag("field").request_focus();
    ui.type_text("a");
    keys.edit(|keys| keys.insert(0, 5000));
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_focused();
    state.dispatch_raw_delta(300.0);
    ui.wait_for_idle();
    ui.type_text("x");
    assert_eq!(text.get(), "ax", "the focused field didn't take the text");
}

/// Input: a `LazyColumn` of 1000 keyed, clickable rows; Tab focuses the first (key 0), which
/// is then removed from the data (focus goes with it); the list scrolls 300 px down.
///
/// Compose: the pin goes with the removed item (its `PinnableItem` is released when the item
/// leaves the composition): 300 px down, the rows near the top (key 1, now item 0) are no
/// longer composed.
///
/// Instead: the removed item never told the list it lost focus, so `pinned` stays `Some(0)`
/// and pins whatever item 0 is now (key 1) for as long as nothing else takes focus: it stays
/// composed, placed at a made-up position out of view, and is in the Tab order after the
/// rows in view (focusing it brings that made-up position into view, not the row).
#[test]
fn removing_the_focused_item_releases_its_pin() {
    let keys = mutable_state_of((0..1000u32).collect::<Vec<_>>());
    let handle = keys.handle();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        KeyedRows(given, Rc::new(handle.get()), true)
    });
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("key 0").assert_is_focused();
    keys.edit(|keys| {
        keys.remove(0);
    });
    ui.wait_for_idle();
    state.dispatch_raw_delta(300.0);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 15);
    ui.on_node_with_tag("key 1").assert_does_not_exist();
}

/// Input: a `LazyColumn` of 1,000,000 keyed rows scrolled to item 500,000, whose key function
/// counts its calls; the first visible row is removed from the data (the user deletes it).
///
/// Compose: the first visible item's key is looked up near its last known index only
/// (`LazyLayoutKeyIndexMap` covers a sliding window around the first visible item,
/// `NearestRangeKeyIndexMap`); not found, the index is kept: a few hundred key lookups.
///
/// Instead: the measurement searches every index from 0 for the key
/// (`(0..count).find(...)`), calling the key function for all 999,999 items, in the frame:
/// deleting a row of a large list stalls a frame, and the cost grows with the list.
#[test]
fn removing_the_first_visible_of_a_million_rows_does_not_look_up_every_key() {
    let removed = mutable_state_of(false);
    let handle = removed.handle();
    let state = LazyListState::new(500_000, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        // The data: 0..1_000_000, without 500_000 once removed.
        let removed = handle.get();
        let rows = if removed { 999_999 } else { 1_000_000 };
        let key_of = move |index: usize| {
            if removed && index >= 500_000 { index as u64 + 1 } else { index as u64 }
        };
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items_keyed(
                    rows,
                    move |index| {
                        count();
                        key_of(index)
                    },
                    move |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 20.dp()))
                                .test_tag(format!("key {}", key_of(index)))
                        );
                    },
                );
            }
        );
    });
    assert_eq!(top(&mut ui, "key 500000"), 0.0);
    take_count();
    removed.set(true);
    ui.wait_for_idle();
    let looked_up = take_count();
    assert_eq!(top(&mut ui, "key 500001"), 0.0);
    assert!(looked_up < 10_000, "{looked_up} key lookups for removing one row");
}

/// Input: a 100 px `LazyColumn` of 2000 items alternating 49 px rows and 1 px dividers (an
/// `items` with dividers as items), scrolled to item 1000, whose contents count their runs; a
/// wheel scroll of 150 px, then one of -300 px.
///
/// Compose: measuring composes the items it reaches, one at a time: each scroll composes the
/// half-dozen items it brings into view (and disposes those it leaves).
///
/// Instead: the items lacking are estimated from the smallest size measured (1 px, a divider):
/// the room left (up to the viewport, or the distance scrolled back) divided by 1 px, so each
/// scroll step composes about a hundred items (and disposes them again in the same frame, once
/// measured: `too_many`). The same happens on the first frame of a list whose first item is
/// small (an 8 px spacer as `item`: room / 8 items composed).
#[test]
fn small_items_do_not_make_a_scroll_compose_a_hundred_items() {
    let state = LazyListState::new(1000, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(2000, |index| {
                    count();
                    let height = if index % 2 == 0 { 49 } else { 1 };
                    Box!(
                        modifier =
                            Modifier.size((100.dp(), height.dp())).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    take_count();
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 150.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 1006);
    let forward = take_count();
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, -300.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 994);
    let backward = take_count();
    assert!(
        forward <= 20 && backward <= 30,
        "{forward} item contents ran for a 150 px scroll, {backward} for 300 px back"
    );
}

/// Input: a `LazyColumn` of 1000 clickable rows counting clicks; a press on row 2, then moves
/// of 3 px down and up four times (a finger's tremor on a touch screen, never more than 3 px
/// from where it pressed), and a release where it pressed.
///
/// Compose: the touch slop is measured on the displacement from the press
/// (`awaitTouchSlopOrCancellation` sums the signed position changes): it is never crossed, so
/// the list doesn't scroll and the row is clicked.
///
/// Instead: `DragScroller` adds up the distance travelled (`travelled += along.abs()`), so
/// 24 px of back-and-forth crosses the 8 px slop: the list scrolls and the click is cancelled.
#[test]
fn a_tremor_within_the_slop_is_a_click() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), 20.dp()))
                            .clickable(count)
                            .test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    take_count();
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 50.0)));
    for _ in 0..4 {
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, 53.0), true));
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, 50.0), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(50.0, 50.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_scroll_offset(), 0, "the list scrolled");
    assert_eq!(take_count(), 1, "the row wasn't clicked");
}

/// Input: a 100 px `LazyColumn` of 1000 clickable 20 px rows; items 5, 6 and 7 are plain
/// (not focusable) rows, a label and its spacing, say. Tab is pressed six times.
///
/// Compose: focus search asks the lazy list to lay out items beyond its bounds, one more at a
/// time, until a focusable one is found (`LazyLayoutBeyondBoundsModifierLocal`): the sixth Tab
/// focuses item 8, brought into view.
///
/// Instead: only two items beyond the viewport are composed and placed; neither is focusable,
/// so the focus search wraps around: the sixth Tab focuses row 0 again, and keyboard users
/// can't get past three consecutive non-focusable items.
#[test]
fn tab_reaches_a_row_past_three_unfocusable_items() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            scope.items(1000, |index| {
                let modifier = Modifier.size((100.dp(), 20.dp()));
                let modifier =
                    if (5..8).contains(&index) { modifier } else { modifier.clickable(|| {}) };
                Box!(modifier = modifier.test_tag(format!("row {index}")));
            });
        });
    });
    for _ in 0..6 {
        ui.press_key(Key::Tab);
    }
    ui.on_node_with_tag("row 8").assert_is_focused();
}

/// Input: a 100 px `LazyColumn` of 1000 clickable 20 px rows, row 4 focused (the last in
/// view); Tab pressed twice before the next frame (a key repeat, or two key events queued
/// behind a slow frame).
///
/// Compose: each bring-into-view request is relative to the current scroll, pending scrolls
/// included (`ContentInViewNode` tracks the requests): row 6 ends at the viewport's bottom,
/// 40 px scrolled.
///
/// Instead: the second Tab measures row 6 where the last layout put it, ignoring the 20 px the
/// first Tab asked for and not yet laid out: the list scrolls 60 px, and row 6 is 20 px above
/// the bottom. (A third Tab before the frame would find no row placed after row 6, as only
/// two are composed beyond the viewport.)
#[test]
fn two_tabs_before_a_frame_scroll_the_second_row_just_into_view() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        KeyedRows(given, Rc::new((0..1000).collect()), true)
    });
    ui.on_node_with_tag("key 4").request_focus();
    ui.wait_for_idle();
    for _ in 0..2 {
        ui.host().send_key(KeyEvent::down(Key::Tab));
        ui.host().send_key(KeyEvent::up(Key::Tab));
    }
    ui.wait_for_idle();
    ui.on_node_with_tag("key 6").assert_is_focused();
    assert_eq!(top(&mut ui, "key 6"), 80.0);
}

/// Input: a 100 px `Column` with `vertical_scroll` holding a 100 px `LazyColumn` of 20 rows
/// (400 px: 300 px of scrolling) and 200 px below it; one wheel scroll of 1000 px over the list.
///
/// Compose: nested scrolling: the list consumes the 300 px it has, the column the rest (its
/// 200 px).
///
/// Instead: the list's end isn't composed yet, so its room is unknown (infinite) and it takes
/// the whole delta, then clamps at its end when measured: the 700 px left are lost and the
/// column stays at 0 (the first review's fix only covers lists whose end is already laid out).
#[test]
fn the_wheel_delta_past_the_end_of_a_long_nested_list_scrolls_its_parent() {
    let outer = ScrollState::new(0);
    let state = LazyListState::new(0, 0);
    let (given_outer, given) = (*outer, *state);
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 100), move || {
        Column!(modifier = Modifier.size((100.dp(), 100.dp())).vertical_scroll(given_outer), {
            KeyedRows(given, Rc::new((0..20).collect()), false);
            Spacer(Modifier.size((100.dp(), 200.dp())));
        });
    });
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 1000.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 15, "the lazy list is at its end");
    assert_eq!(outer.value(), 200, "the column didn't get what the list left");
}

/// Input: a `LazyColumn` of 5000 rows; `scroll_by(30_000.0)` (1500 rows), then one frame.
///
/// Compose: `scrollBy` measures the list at once, composing the items it passes over: the
/// frame shows rows 1500 to 1504.
///
/// Instead: each round of the frame's relayout loop steps over the items composed and asks
/// for at most 200 more (`MOST_MISSING`); after three rounds the viewport is still past the
/// composed items, so the frame is drawn empty.
#[test]
fn a_long_scroll_by_is_drawn_in_one_frame() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        KeyedRows(given, Rc::new((0..5000).collect()), false)
    });
    state.dispatch_raw_delta(30_000.0);
    ui.host().frame();
    ui.on_node_with_tag("key 1500").assert_exists();
    assert_eq!(top(&mut ui, "key 1500"), 0.0);
}

/// Input: a `LazyColumn` of 1000 clickable rows counting clicks, at density 2 (a HiDPI screen);
/// a press on row 2, a move of 12 px (6 dp) up, and a release there.
///
/// Compose: the touch slop is 8 dp (`ViewConfiguration.touchSlop`), 16 px at density 2: the
/// move stays within it, the list doesn't scroll and the row is clicked.
///
/// Instead: `TOUCH_SLOP` is 8 used as pixels, whatever the density: at density 2 a drag starts
/// after 4 dp, so a slightly moving tap scrolls the list and its click is cancelled.
#[test]
fn the_touch_slop_is_in_dp() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = compose_test_with(
        IntSize::new(400, 400),
        compose_ui::Density::new(2.0),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
        move || {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(1000, |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 20.dp()))
                                .clickable(count)
                                .test_tag(format!("row {index}"))
                        );
                    });
                }
            );
        },
    );
    take_count();
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(100.0, 100.0)));
    host.send_pointer(PointerEvent::moved(Offset::new(100.0, 88.0), true));
    host.send_pointer(PointerEvent::release(Offset::new(100.0, 88.0)));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_scroll_offset(), 0, "the list scrolled");
    assert_eq!(take_count(), 1, "the row wasn't clicked");
}

/// Input: a `LazyColumn` declared with 1,000,000 `scope.item(...)` calls (one interval per
/// row, as `for row in rows { item { ... } }`); twenty scrolls of one row (20 px), a frame
/// after each, near the top, then the same at item 900,000.
///
/// Compose: an item's interval is found by binary search (`MutableIntervalList.get`), so a
/// scroll frame costs the same anywhere in the list.
///
/// Instead: `Items::find` walks the intervals from the first, and every scroll frame looks up
/// several items (the first item's key in the measurement, each composed item's key in
/// `LazyItems`, the new items' content): each frame is O(index), so scrolling at item 900,000
/// is hundreds of times slower than at the top (5 ms a frame in release, 40 ms in debug,
/// against well under 0.1 ms at the top).
#[test]
fn a_scroll_frame_does_not_walk_every_item_interval() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                for index in 0..1_000_000usize {
                    scope.item(move || {
                        Box!(
                            modifier =
                                Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}"))
                        );
                    });
                }
            }
        );
    });
    let time = |ui: &mut ComposeTestRule| {
        let start = std::time::Instant::now();
        for _ in 0..20 {
            state.dispatch_raw_delta(20.0);
            ui.wait_for_idle();
        }
        start.elapsed()
    };
    let near = time(&mut ui);
    assert_eq!(state.first_visible_item_index(), 20);
    state.request_scroll_to_item(900_000, 0);
    ui.wait_for_idle();
    let far = time(&mut ui);
    assert_eq!(state.first_visible_item_index(), 900_020);
    assert!(
        far < near * 20 + std::time::Duration::from_millis(50),
        "20 scroll frames took {near:?} at the top, {far:?} at item 900,000"
    );
}
