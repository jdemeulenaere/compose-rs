//! Review of D475-D478 (e124993): tests that failed there, fixed by D479-D483 (the "Instead"
//! paragraphs say what e124993 did). Each states the input, what Jetpack Compose
//! does, and what happens instead.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    FocusDirection, IntSize, Key, NestedScrollConnection, NestedScrollSource, Offset, PointerEvent,
    PointerId, PointerType, UnitExt,
};

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// Records what's left after the scroll (as an app bar that's already expanded, taking none).
#[derive(Default)]
struct Records {
    available: RefCell<Vec<Offset>>,
}

impl NestedScrollConnection for Records {
    fn on_post_scroll(&self, _: Offset, available: Offset, _: NestedScrollSource) -> Offset {
        self.available.borrow_mut().push(available);
        Offset::ZERO
    }
}

/// A drag from `from` by `by` past the touch slop (8 px at density 1), in one move.
fn drag(ui: &mut ComposeTestRule, from: Offset, by: Offset) {
    let slop = 8.0;
    let unit = Offset::new(by.x.signum() * slop, by.y.signum() * slop);
    ui.host().send_pointer(touch(PointerEvent::press(from)));
    ui.host().send_pointer(touch(PointerEvent::moved(from + unit + by, true)));
    ui.host().send_pointer(touch(PointerEvent::release(from + unit + by)));
    ui.wait_for_idle();
}

/// Input: a 500 px list, a 1,000 px first item then 50 px rows, at item 3 (1,100 px down),
/// inside a nested-scroll parent (a Scaffold's app bar connection, here one that takes
/// nothing); a drag down by 600 px.
///
/// Compose: the list's measurement consumes the 600 px (into the header, 500 px down); the
/// parent's post-scroll gets nothing left.
///
/// Instead (D476): with a nested-scroll parent around (`has_outer`), the delta is cut at the
/// estimated room back (3 × 50 px), the parent is offered the other 450 px, and the list stops
/// 150 px back: the review_d470 bug, for any list in a Scaffold with a scroll behavior.
#[test]
fn a_drag_in_a_nested_list_onto_a_big_item_moves_the_whole_delta() {
    let state = LazyListState::new(3, 0);
    let given = *state;
    let records = Rc::new(Records::default());
    let connection: Rc<dyn NestedScrollConnection> = records.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 1000), move || {
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.item(|| Box!(modifier = Modifier.size((100.dp(), 1000.dp()))));
                    scope.items(200, |_| Box!(modifier = Modifier.size((100.dp(), 50.dp()))));
                }
            );
        });
    });
    ui.wait_for_idle();
    drag(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 600.0));
    let left: f32 = records.available.borrow().iter().map(|offset| offset.y).sum();
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset(), left),
        (0, 500, 0.0),
    );
}

/// Input: a carousel (a `LazyRow`, 500 px wide: a 1,000 px first tile then 50 px tiles, at
/// tile 3) in a `LazyColumn` (the stress list's carousels); a drag right by 600 px.
///
/// Compose: the row moves 600 px back (into the first tile, 500 px in).
///
/// Instead (D476): the column around is a nested-scroll parent (`has_outer`), so the row cuts
/// the delta at its estimated room (150 px) for the column to take the rest, which it can't
/// (it scrolls vertically): 450 px are lost, the row stops at tile 0, 850 px in.
#[test]
fn a_drag_in_a_carousel_in_a_lazy_column_moves_the_whole_delta() {
    let state = LazyListState::new(3, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyColumn!(modifier = Modifier.size((500.dp(), 500.dp())), move |scope: LazyListScope| {
            scope.item(move || {
                LazyRow!(
                    modifier = Modifier.size((500.dp(), 100.dp())),
                    state = given,
                    |scope: LazyListScope| {
                        scope.item(|| Box!(modifier = Modifier.size((1000.dp(), 100.dp()))));
                        scope.items(200, |_| Box!(modifier = Modifier.size((50.dp(), 100.dp()))));
                    }
                );
            });
            scope.items(20, |_| Box!(modifier = Modifier.size((500.dp(), 100.dp()))));
        });
    });
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 3);
    drag(&mut ui, Offset::new(10.0, 50.0), Offset::new(600.0, 0.0));
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 500),
    );
}

/// A 100 px `LazyColumn` of 50 px rows (two in view), each a `LazyRow` of two focusable 50 px
/// tiles (both in view), then a focusable box after the column.
fn rows_of_rows() -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(200, 300), || {
        Column!({
            LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
                scope.items(50, |row| {
                    LazyRow!(
                        modifier = Modifier.size((100.dp(), 50.dp())),
                        move |scope: LazyListScope| {
                            scope.items(2, move |tile| {
                                Box!(
                                    modifier = Modifier
                                        .size((50.dp(), 50.dp()))
                                        .clickable(|| {})
                                        .test_tag(format!("tile {row} {tile}"))
                                );
                            });
                        }
                    );
                });
            });
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    })
}

/// Input: focus on the last tile of the last row in view of a `LazyColumn` of `LazyRow`s; Tab.
///
/// Compose: the row's focus group finds nothing more (its `BeyondBoundsLayout` has no more
/// tiles), the search goes up to the column's, which lays out the next row: Tab focuses its
/// first tile.
///
/// Instead (D477): only the innermost container around the focused node is asked
/// (`beyond_bounds_container`); the row has no more, so the focus leaves the column for the box
/// after it, skipping its 48 other rows.
#[test]
fn tab_from_the_last_tile_of_a_carousel_moves_to_the_next_row_of_the_column() {
    let mut ui = rows_of_rows();
    ui.on_node_with_tag("tile 1 1").request_focus();
    ui.wait_for_idle();
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("tile 2 0").assert_is_focused();
}

/// Input: as above, focus on the first tile of the last row in view; the focus moved down
/// (the two-dimensional search, a D-pad).
///
/// Compose: the column lays out the next row, whose first tile takes the focus.
///
/// Instead (D477): the row, asked, can't lay out more downwards (it's horizontal), and the
/// column isn't asked: the focus moves to the box after the column.
#[test]
fn moving_down_from_a_carousel_moves_to_the_next_row_of_the_column() {
    let mut ui = rows_of_rows();
    ui.on_node_with_tag("tile 1 0").request_focus();
    ui.wait_for_idle();
    ui.host().with_tree(|tree| tree.move_focus_in(FocusDirection::Down));
    ui.wait_for_idle();
    ui.on_node_with_tag("tile 2 0").assert_is_focused();
}

/// Down repeatedly through a grid, past the lines placed beyond its bounds (D456's two): each
/// move finds the next line.
#[test]
fn moving_down_through_a_grid_past_the_lines_laid_out() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), || {
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((100.dp(), 100.dp())),
            |scope: LazyGridScope| {
                scope.items(1000, |index| {
                    Box!(
                        modifier = Modifier
                            .height(20.dp())
                            .clickable(|| {})
                            .test_tag(format!("tile {index}"))
                    );
                });
            }
        );
    });
    ui.on_node_with_tag("tile 0").request_focus();
    ui.wait_for_idle();
    for _ in 0..12 {
        ui.host().with_tree(|tree| tree.move_focus_in(FocusDirection::Down));
        ui.wait_for_idle();
    }
    ui.on_node_with_tag("tile 24").assert_is_focused();
}

/// A 100 px list: rows 0 to 4 focusable, then `gap` unfocusable rows, then a focusable row, all
/// 20 px; a focusable box after the list.
fn gap_list(gap: usize, shown: Rc<dyn Fn() -> bool>) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(200, 300), move || {
        let shown = shown.clone();
        Column!({
            if shown() {
                LazyColumn!(
                    modifier = Modifier.size((100.dp(), 100.dp())),
                    move |scope: LazyListScope| {
                        scope.items(gap + 6, move |index| {
                            let focusable = index < 5 || index == gap + 5;
                            let modifier = Modifier.size((100.dp(), 20.dp()));
                            let modifier = if focusable {
                                modifier.clickable(|| {}).test_tag(format!("row {index}"))
                            } else {
                                modifier.test_tag(format!("row {index}"))
                            };
                            Box!(modifier = modifier);
                        });
                    }
                );
            }
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    })
}

fn lazy_items(ui: &ComposeTestRule) -> usize {
    ui.dump_tree().lines().filter(|line| line.trim_start().starts_with("LazyItem")).count()
}

/// Input: Tab from row 4 of a list whose next focusable row comes after 50 unfocusable ones.
///
/// Compose: the search lays out items beyond the bounds until it finds row 55.
///
/// Here (D477): it does, a frame at a time (8 items a frame); but once over, the rows it laid
/// out behind the one found stay composed (not placed): 14 items composed where scrolling
/// there leaves 7 (`baseline_composed_after_scrolling_in_steps`), within the frame's two
/// viewports' slack, and prefetch never trims them.
#[test]
fn tab_past_unfocusable_rows_finds_the_next_and_leaves_few_composed() {
    let mut ui = gap_list(50, Rc::new(|| true));
    ui.on_node_with_tag("row 4").request_focus();
    ui.wait_for_idle();
    ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
    ui.wait_for_idle();
    let before = lazy_items(&ui);
    ui.press_key(Key::Tab);
    ui.on_node_with_tag("row 55").assert_is_focused();
    for _ in 0..5 {
        ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
        ui.wait_for_idle();
    }
    let after = lazy_items(&ui);
    assert!(after <= before + 4, "{before} items composed before, {after} after");
}

/// Input: a Tab from the last row in view (it waits for the next row, D477), then the list
/// leaves composition before the frame.
///
/// Expected: no panic; the Tab's search ends (nothing waits).
#[test]
fn a_waiting_search_whose_list_leaves_ends() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = gap_list(50, Rc::new(move || handle.get()));
    ui.on_node_with_tag("row 4").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::down(Key::Tab));
    assert!(ui.host().with_tree(|tree| tree.has_beyond_bounds_moves()));
    shown.set(false);
    ui.wait_for_idle();
    assert!(!ui.host().with_tree(|tree| tree.has_beyond_bounds_moves()));
}

thread_local! {
    static BODIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A `height` px list of 1 px dividers each followed by a 300 px card, each an item (a common
/// pattern: dividers, spacers or section gaps as items of their own).
fn cards_and_dividers(height: f32) -> impl Fn() {
    move || {
        LazyColumn!(modifier = Modifier.size((100.dp(), height.dp())), |scope: LazyListScope| {
            scope.items(4000, |index| {
                BODIES.with(|cell| cell.set(cell.get() + 1));
                let size = if index % 2 == 1 { 300.0 } else { 1.0 };
                Box!(modifier = Modifier.size((100.dp(), size.dp())));
            });
        });
    }
}

/// Input: a 2,244 px list (the Pixel 8 Pro's screen) of 300 px cards and 1 px dividers, each an
/// item; one frame.
///
/// Compose: its measurement composes items until the viewport is full: 15 items (8 cards and 7
/// dividers).
///
/// Instead (D475): once 4 sizes are known, the items missing are estimated at the smaller of
/// the in-view average and the last item's size: the last is a 1 px divider, so the round asks
/// for the room left (about 1,600 px) in 1 px items, up to 800: the first frame composes and
/// measures 805 items to show 15 (at 987162f, without the last item's cap: under 30).
#[test]
fn a_first_frame_of_cards_and_dividers_composes_what_it_shows() {
    let mut host = compose_ui::platform::UiHost::headless(100, 2244);
    host.set_content(cards_and_dividers(2244.0));
    BODIES.with(|cell| cell.set(0));
    host.frame();
    let composed = BODIES.with(std::cell::Cell::get);
    assert!(composed <= 30, "the first frame composed {composed} items to show 15");
}

/// Input: as above, scrolled to rest, then a scroll of 600 px onto items not composed (no
/// prefetch ran: a fling's first frame).
///
/// Compose: the frame composes the items that come into view (about 4).
///
/// Instead (D475): the estimate from a 1 px last item asks for an item a pixel: the frame
/// composes 96 items (without the last item's cap: under 10).
#[test]
fn a_scroll_onto_cards_and_dividers_composes_what_comes_into_view() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 1000), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 1000.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(4000, |index| {
                    BODIES.with(|cell| cell.set(cell.get() + 1));
                    let size = if index % 2 == 1 { 300.0 } else { 1.0 };
                    Box!(modifier = Modifier.size((100.dp(), size.dp())));
                });
            }
        );
    });
    ui.wait_for_idle();
    BODIES.with(|cell| cell.set(0));
    state.dispatch_raw_delta(601.0);
    ui.host().frame();
    let composed = BODIES.with(std::cell::Cell::get);
    assert!(composed <= 10, "the frame composed {composed} items");
}

/// Baseline: scrolling a row at a time to row 47, then prefetch: what stays composed.
#[test]
fn baseline_composed_after_scrolling_in_steps() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 300), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(100, |index| {
                    Box!(
                        modifier =
                            Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}"))
                    );
                });
            }
        );
    });
    for _ in 0..47 {
        state.dispatch_raw_delta(20.0);
        ui.wait_for_idle();
    }
    for _ in 0..5 {
        ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
        ui.wait_for_idle();
    }
    let composed = lazy_items(&ui);
    assert_eq!(state.first_visible_item_index(), 47);
    assert!(composed <= 9, "{composed} composed");
}

/// Input: a 100 px list of focusable 20 px rows 16 px apart (rows 0 to 2 in view, row 3 starts
/// at 108 px), then a focusable box right under the list (at 100 px); focus on row 2, moved
/// down (a D-pad).
///
/// Compose: the two-dimensional search looks inside the list's focus group first; finding
/// nothing there, it lays out row 3 beyond the bounds and finds it: row 3 takes the focus.
///
/// Instead (D477): the search is the window's: the box, nearer than row 3, wins each time, so
/// the move is taken as leaving the list, which lays out one more row, again and again (8 a
/// frame, 200 in all), until the search gives up and the box takes the focus, 25 frames later,
/// having composed 200 rows.
#[test]
fn moving_down_in_a_spaced_list_moves_to_the_next_row() {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 300), || {
        Column!({
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                vertical_arrangement = Arrangement::spaced_by(16.dp()),
                |scope: LazyListScope| {
                    scope.items(1000, |index| {
                        Box!(
                            modifier = Modifier
                                .size((100.dp(), 20.dp()))
                                .clickable(|| {})
                                .test_tag(format!("row {index}"))
                        );
                    });
                }
            );
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("row 2").request_focus();
    ui.wait_for_idle();
    ui.host().with_tree(|tree| tree.move_focus_in(FocusDirection::Down));
    let mut frames = 0;
    while ui.host().needs_frame() && frames < 100 {
        ui.host().frame();
        frames += 1;
    }
    let row_3 = ui.count_nodes(&has_test_tag("row 3")) == 1
        && ui
            .on_node_with_tag("row 3")
            .fetch_semantics_snapshot()
            .config
            .get(&compose_ui::SemanticsProperties::FOCUSED)
            .copied()
            .unwrap_or(false);
    assert!(row_3, "row 3 not focused after {frames} frames:\n{}", ui.dump_semantics());
}

/// Input: a list of 100 rows scrolled to row 10 (neither end laid out), then emptied (a
/// refresh, a filter matching nothing); `scroll_by(500.0)` (a wheel turn, a fling going on);
/// then the rows come back.
///
/// Compose: an empty list consumes no scroll (`scroll_by` says it can't move) and starts at
/// its top once filled again.
///
/// Instead (D476): the empty list's measurement sets the room each way to 0 but leaves
/// `room_estimated` as the last layout with items set it (both estimated), so `dispatch` takes
/// the whole delta (an estimated room is no limit); it waits as pending (the empty list keeps
/// what's scrolled for the first items), and the rows come back scrolled 500 px down.
#[test]
fn an_emptied_list_takes_no_scroll() {
    let state = LazyListState::new(10, 0);
    let given = *state;
    let count = mutable_state_of(100usize);
    let handle = count.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 200), move || {
        let count = handle.get();
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            move |scope: LazyListScope| {
                scope.items(count, |_| Box!(modifier = Modifier.size((100.dp(), 20.dp()))));
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 10);
    count.set(0);
    ui.wait_for_idle();
    let moved = state.dispatch_raw_delta(500.0);
    ui.wait_for_idle();
    count.set(100);
    ui.wait_for_idle();
    assert_eq!((moved != 0.0, state.first_visible_item_index()), (false, 0));
}
