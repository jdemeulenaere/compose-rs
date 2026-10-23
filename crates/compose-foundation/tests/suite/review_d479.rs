//! Review of D479-D485 (d301767), fixed by D486-D490. Each states the input, what Jetpack
//! Compose does, and what happened instead (or that it holds).

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{
    IntSize, NestedScrollConnection, NestedScrollSource, Offset, PointerEvent, PointerId,
    PointerType, UnitExt,
};

fn touch(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// Records what's left after the scroll (an app bar already expanded, taking none).
#[derive(Default)]
struct Records {
    available: RefCell<Vec<Offset>>,
    flung: RefCell<Vec<Offset>>,
}

impl NestedScrollConnection for Records {
    fn on_post_scroll(&self, _: Offset, available: Offset, _: NestedScrollSource) -> Offset {
        self.available.borrow_mut().push(available);
        Offset::ZERO
    }
    fn on_post_fling(&self, _: Offset, available: Offset) -> Offset {
        self.flung.borrow_mut().push(available);
        Offset::ZERO
    }
}

fn left_y(records: &Records) -> f32 {
    records.available.borrow().iter().map(|offset| offset.y).sum()
}

/// A press, one move past the slop by `by`, a release; then one frame.
fn drag_one_frame(ui: &mut ComposeTestRule, from: Offset, by: Offset) {
    let slop = 8.0;
    let unit = Offset::new(by.x.signum() * slop, by.y.signum() * slop);
    ui.host().send_pointer(touch(PointerEvent::press(from)));
    ui.host().send_pointer(touch(PointerEvent::moved(from + unit + by, true)));
    ui.host().send_pointer(touch(PointerEvent::release(from + unit + by)));
    ui.host().frame();
}

/// A 500 px list of 50 px rows at `state`, in a nested-scroll parent recording what it's left.
fn nested_rows(state: LazyListState, records: Rc<Records>) -> ComposeTestRule {
    let connection: Rc<dyn NestedScrollConnection> = records;
    ComposeTestRule::sized(IntSize::new(100, 500), move || {
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = state,
                |scope: LazyListScope| {
                    scope.items(200, |_| Box!(modifier = Modifier.size((100.dp(), 50.dp()))));
                }
            );
        });
    })
}

/// Input: a 500 px list of 50 px rows at row 5 (250 px down, the start not laid out: the room
/// back an estimate) in a nested-scroll parent; a drag down by 280 px.
///
/// Compose: the list moves 250 px to its top; the parent's post-scroll gets the other 30 px,
/// in the frame. (Holds.)
#[test]
fn a_nested_list_passing_its_estimated_start_offers_the_rest_in_the_frame() {
    let state = LazyListState::new(5, 0);
    let records = Rc::new(Records::default());
    let mut ui = nested_rows(*state, records.clone());
    ui.wait_for_idle();
    drag_one_frame(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 280.0));
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            left_y(&records)
        ),
        (0, 0, 30.0),
    );
}

/// Input: as above, rows 10 px apart, the first item 1,000 px, the list at item 3; a drag down
/// by 1,250 px (the list is 1,000 + 2 × 50 + 3 × 10 = 1,130 px down).
///
/// Compose: the list moves 1,130 px; the parent gets 120 px. (Holds: the distance from the
/// sizes measured is exact, spacing included.)
#[test]
fn the_rest_offered_counts_spacing_and_items_measured_on_the_way() {
    let state = LazyListState::new(3, 0);
    let given = *state;
    let records = Rc::new(Records::default());
    let connection: Rc<dyn NestedScrollConnection> = records.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = given,
                vertical_arrangement = Arrangement::spaced_by(10.dp()),
                |scope: LazyListScope| {
                    scope.item(|| Box!(modifier = Modifier.size((100.dp(), 1000.dp()))));
                    scope.items(200, |_| Box!(modifier = Modifier.size((100.dp(), 50.dp()))));
                }
            );
        });
    });
    ui.wait_for_idle();
    drag_one_frame(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 1250.0));
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            left_y(&records)
        ),
        (0, 0, 120.0),
    );
}

/// Input: a 500 px grid (2 columns) of 50 px lines at item 10 (line 5, 250 px down) in a
/// nested-scroll parent (an app bar's, say); a drag down by 280 px.
///
/// Compose: the grid moves 250 px to its top; the parent's post-scroll gets the other 30 px.
///
/// Instead (D479): a grid's rest is dropped (`overshoot.take()` in the grid's measurement), and
/// never offered later either: the parent gets nothing. At e124993 (D476) the delta was cut at
/// the estimated room back (250 px, exact here) and the parent got 30 px; this test passes
/// there and at 727d266: a regression for a grid in a Scaffold with a collapsing bar.
#[test]
fn a_nested_grid_passing_its_estimated_start_offers_the_rest() {
    let state = LazyGridState::new(10, 0);
    let given = *state;
    let records = Rc::new(Records::default());
    let connection: Rc<dyn NestedScrollConnection> = records.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            LazyVerticalGrid!(
                GridCells::Fixed(2),
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = given,
                |scope: LazyGridScope| {
                    scope.items(400, |_| Box!(modifier = Modifier.height(50.dp())));
                }
            );
        });
    });
    ui.wait_for_idle();
    drag_one_frame(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 280.0));
    ui.wait_for_idle();
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            left_y(&records)
        ),
        (0, 0, 30.0),
    );
}

/// Input: a 200 px list of 50 px rows at row 5 under a 300 px header in a scrolling column
/// scrolled 300 px (the list at the top of the window); a drag down by 280 px on the list.
///
/// Compose: the list moves 250 px to its top, the column the other 30 px back, in the frame.
/// (Holds.)
#[test]
fn a_list_in_a_scrolling_column_passes_the_rest_to_it_in_the_frame() {
    let state = LazyListState::new(5, 0);
    let given = *state;
    let outer = ScrollState::new(300);
    let scroll = *outer;
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        Column!(modifier = Modifier.size((100.dp(), 500.dp())).vertical_scroll(scroll), {
            Spacer(Modifier.size((100.dp(), 300.dp())));
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 200.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(200, |_| Box!(modifier = Modifier.size((100.dp(), 50.dp()))));
                }
            );
            Spacer(Modifier.size((100.dp(), 1000.dp())));
        });
    });
    ui.wait_for_idle();
    assert_eq!(scroll.value(), 300);
    drag_one_frame(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 280.0));
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            scroll.value()
        ),
        (0, 0, 270),
    );
}

/// Input: a carousel (a `LazyRow` of 50 px tiles at tile 5) in a `LazyColumn` scrolled to row
/// 3; a vertical drag on the carousel by 120 px down.
///
/// Compose: the column scrolls back 120 px; the row doesn't move. (Holds.)
#[test]
fn a_vertical_drag_on_a_carousel_scrolls_the_column() {
    let row = LazyListState::new(5, 0);
    let row_given = *row;
    let column = LazyListState::new(3, 0);
    let column_given = *column;
    let mut ui = ComposeTestRule::sized(IntSize::new(500, 500), move || {
        LazyColumn!(
            modifier = Modifier.size((500.dp(), 500.dp())),
            state = column_given,
            move |scope: LazyListScope| {
                scope.items(20, move |index| {
                    if index == 3 {
                        LazyRow!(
                            modifier = Modifier.size((500.dp(), 100.dp())),
                            state = row_given,
                            |scope: LazyListScope| {
                                scope.items(200, |_| {
                                    Box!(modifier = Modifier.size((50.dp(), 100.dp())))
                                });
                            }
                        );
                    } else {
                        Box!(modifier = Modifier.size((500.dp(), 100.dp())));
                    }
                });
            }
        );
    });
    ui.wait_for_idle();
    drag_one_frame(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 120.0));
    ui.wait_for_idle();
    assert_eq!(
        (
            column.first_visible_item_index(),
            column.first_visible_item_scroll_offset(),
            row.first_visible_item_index(),
            row.first_visible_item_scroll_offset()
        ),
        (1, 80, 5, 0),
    );
}

/// Input: as the first test, two moves in one frame: down by 280 px, then back up by 20 px
/// (a finger's jitter; Android batches several moves a frame). The list is 250 px down.
///
/// Compose: each move is measured: the first moves 250 px and offers 30, the second scrolls
/// 20 px back on: the parent got 30 px, the list is 20 px down.
///
/// Here: the moves add up before the layout: the list ends at its top and the parent gets
/// 10 px (the net 260 less 250). Conserved, but not Compose's split (a nit).
#[test]
fn moves_both_ways_in_a_frame_offer_what_the_first_left() {
    let state = LazyListState::new(5, 0);
    let records = Rc::new(Records::default());
    let mut ui = nested_rows(*state, records.clone());
    ui.wait_for_idle();
    let from = Offset::new(50.0, 10.0);
    ui.host().send_pointer(touch(PointerEvent::press(from)));
    ui.host().send_pointer(touch(PointerEvent::moved(from + Offset::new(0.0, 288.0), true)));
    ui.host().send_pointer(touch(PointerEvent::moved(from + Offset::new(0.0, 268.0), true)));
    ui.host().frame();
    let in_frame = left_y(&records);
    ui.host().send_pointer(touch(PointerEvent::release(from + Offset::new(0.0, 268.0))));
    ui.wait_for_idle();
    assert!(in_frame <= 30.0 + 0.5, "offered {in_frame}");
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 0)
    );
}

/// Input: a 500 px list of 50 px rows at row 5 (start estimated) in a nested-scroll parent; a
/// fling back towards the start, fast enough to pass it.
///
/// Compose: the fling's frame that reaches the top posts its rest (SideEffect), and the
/// parent's post-fling gets the velocity left.
///
/// Here: the frame crossing the estimated start takes its whole delta (the flinger dispatches
/// with no overshoot) and its rest is lost; the next frame's delta goes to the parent, and
/// the post-fling comes a frame later. Passes (rest and velocity both reach the parent).
#[test]
fn a_fling_past_an_estimated_start_offers_its_rest() {
    let state = LazyListState::new(5, 0);
    let records = Rc::new(Records::default());
    let mut ui = nested_rows(*state, records.clone());
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let from = Offset::new(50.0, 10.0);
    let mut event = touch(PointerEvent::press(from));
    event.time_nanos = 1;
    ui.host().send_pointer(event);
    for step in 1..=5u64 {
        let mut event =
            touch(PointerEvent::moved(from + Offset::new(0.0, 10.0 + 8.0 * step as f32), true));
        event.time_nanos = 1 + step * 4_000_000;
        ui.host().send_pointer(event);
    }
    let mut event = touch(PointerEvent::release(from + Offset::new(0.0, 50.0)));
    event.time_nanos = 1 + 6 * 4_000_000;
    ui.host().send_pointer(event);
    for _ in 0..120 {
        ui.advance_time_by(std::time::Duration::from_millis(16));
    }
    assert_eq!(
        (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
        (0, 0)
    );
    assert!(left_y(&records) > 0.0, "the fling's rest went nowhere");
    assert!(records.flung.borrow().last().is_some_and(|velocity| velocity.y > 0.0));
}

/// Input: a mouse wheel over a nested list at row 5 (start estimated): one turn of 280 px back.
///
/// Compose (desktop): the list moves 250 px; the parent gets 30. (Holds.)
#[test]
fn a_wheel_past_an_estimated_start_offers_the_rest() {
    let state = LazyListState::new(5, 0);
    let records = Rc::new(Records::default());
    let mut ui = nested_rows(*state, records.clone());
    ui.wait_for_idle();
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, -280.0)));
    ui.host().frame();
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            left_y(&records)
        ),
        (0, 0, 30.0),
    );
}

/// A 100 px `LazyColumn` of 50 px rows (two in view), each a `LazyRow` of two focusable 50 px
/// tiles (both in view) then `plain` unfocusable ones (out of view: a trailing spacer, a "see
/// all" label), then a focusable box after the column.
fn rows_of_rows_with_plain_tiles(plain: usize) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(200, 300), move || {
        Column!({
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 100.dp())),
                move |scope: LazyListScope| {
                    scope.items(50, move |row| {
                        LazyRow!(
                            modifier = Modifier.size((100.dp(), 50.dp())),
                            move |scope: LazyListScope| {
                                scope.items(2 + plain, move |tile| {
                                    let modifier = Modifier
                                        .size((50.dp(), 50.dp()))
                                        .test_tag(format!("tile {row} {tile}"));
                                    let modifier =
                                        if tile < 2 { modifier.clickable(|| {}) } else { modifier };
                                    Box!(modifier = modifier);
                                });
                            }
                        );
                    });
                }
            );
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    })
}

/// Input: focus on the last focusable tile (then `plain` unfocusable ones) of the last row in
/// view of a `LazyColumn` of `LazyRow`s; Tab, then 60 frames.
///
/// Compose: the row's beyond-bounds layout lays out its other tiles, finds nothing, the search
/// goes to the column's, which lays out row 2: Tab focuses its first tile.
///
/// Instead (D483): each search starts again at the innermost container: once the column laid
/// out row 2, the row's turn comes first again (its Tab target, tile 2 0, isn't inside it), and
/// `lay_out_beyond` for it ends the column's search (row 2 goes) and restarts the row's count:
/// the row lays out its tiles again, then the column row 2, and so on, forever (still waiting
/// after 3,000 frames). The focus never moves, and every frame runs 8 searches with their
/// layouts and compositions (`has_pending_work` stays true: the app never idles).
fn tab_from_a_carousel_with_plain_tiles(plain: usize) {
    let mut ui = rows_of_rows_with_plain_tiles(plain);
    ui.on_node_with_tag("tile 1 1").request_focus();
    ui.wait_for_idle();
    ui.host().send_key(compose_ui::KeyEvent::down(compose_ui::Key::Tab));
    for _ in 0..60 {
        ui.host().frame();
    }
    let waiting = ui.host().with_tree(|tree| tree.has_beyond_bounds_moves());
    assert!(!waiting, "the Tab still waits after 60 frames");
    ui.on_node_with_tag("tile 2 0").assert_is_focused();
}

#[test]
fn tab_from_a_carousel_whose_last_tiles_are_unfocusable_moves_to_the_next_row() {
    tab_from_a_carousel_with_plain_tiles(3);
}

/// As above with a single trailing unfocusable item (a carousel's end spacer).
#[test]
fn tab_from_a_carousel_ending_in_a_spacer_moves_to_the_next_row() {
    tab_from_a_carousel_with_plain_tiles(1);
}

thread_local! {
    static BODIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A 2,244 px list (the Pixel 8 Pro's screen) whose items' heights repeat `pattern` (offset by
/// `parity`), each its own item.
fn patterned(state: LazyListState, pattern: &'static [f32], parity: usize) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(100, 2244), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 2244.dp())),
            state = state,
            move |scope: LazyListScope| {
                scope.items(4000, move |index| {
                    BODIES.with(|cell| cell.set(cell.get() + 1));
                    let size = pattern[(index + parity) % pattern.len()];
                    Box!(modifier = Modifier.size((100.dp(), size.dp())));
                });
            }
        );
    })
}

/// Scrolled to rest 3,000 + `start` px down (prefetch run), then scrolled `by` in a frame:
/// the items that frame composed.
fn composed_by_a_scroll(pattern: &'static [f32], parity: usize, start: i32, by: i32) -> usize {
    let state = LazyListState::new(0, 0);
    let mut ui = patterned(*state, pattern, parity);
    ui.wait_for_idle();
    state.scroll_by(3000.0 + start as f32);
    ui.wait_for_idle();
    for _ in 0..3 {
        ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
        ui.wait_for_idle();
    }
    BODIES.with(|cell| cell.set(0));
    state.scroll_by(by as f32);
    ui.host().frame();
    BODIES.with(std::cell::Cell::get)
}

/// Sweep: starts every 53 px, scrolls of 200 to 2,400 px each way: the most items a frame
/// composed.
fn sweep(pattern: &'static [f32]) {
    let mut worst = (0, 0, 0, 0);
    let mut over = 0;
    for parity in 0..pattern.len() {
        for start in (0..1300).step_by(53) {
            for by in (-2400..2400).step_by(97) {
                if (-200..200).contains(&by) {
                    continue;
                }
                let composed = composed_by_a_scroll(pattern, parity, start, by);
                if composed > 40 {
                    over += 1;
                }
                if composed > worst.0 {
                    worst = (composed, parity, start, by);
                }
            }
        }
    }
    assert!(worst.0 <= 40, "{over} scrolls composed over 40 items; worst {worst:?}");
}

/// Compose: a frame composes what comes into view (at most about 17 items). (Holds: 16.)
#[test]
fn a_scroll_onto_cards_and_dividers_never_composes_hundreds() {
    sweep(&[300.0, 1.0]);
}

/// As above, a card then two thin items (an 8 px spacer and a 1 px divider) each time.
///
/// Compose: at most about 24 items a frame.
///
/// Instead (D481): 197 of the sweep's scrolls (all of 1,965 px or more: a page) compose over 40
/// items, up to 800. Under 40 in every case at 727d266.
#[test]
fn a_scroll_onto_cards_spacers_and_dividers_never_composes_hundreds() {
    sweep(&[300.0, 8.0, 1.0]);
}

/// Input: a 2,244 px list of a 300 px card, an 8 px spacer and a 1 px divider, each an item,
/// again and again; scrolled to rest (prefetch run), then a page down (2,256 px: TalkBack's
/// scroll forward is a page, D484, as is a Page Down key).
///
/// Compose: the frame composes what comes into view (about 22 items).
///
/// Instead (D481): the frame's measurement places a card, a spacer and a divider (what prefetch
/// left composed there), then lacks items; the estimate leaves out the first placed (the card)
/// and averages 8 and 1 px: 4 px an item, with three sizes in view so up to 800 a round (D481's
/// allowance from 2 sizes): the frame composes and measures 800 items to show 22.
#[test]
fn a_page_down_on_cards_spacers_and_dividers_composes_what_comes_into_view() {
    let composed = composed_by_a_scroll(&[300.0, 8.0, 1.0], 1, 0, 2256);
    assert!(composed <= 40, "the frame composed {composed} items");
}

/// Input: a keyed 500 px list of 50 px rows at row 10 (the start not laid out) in a
/// nested-scroll parent; a drag down by 20 px, and in the same frame an item inserted at the
/// top (a feed's new post).
///
/// Compose: the list follows its first visible item by key and moves 20 px back; nothing is
/// left for the parent (an app bar stays as it is).
///
/// Instead (D479): the overshoot's `from` is the index before the insertion (10, now the item
/// before the first visible one, 11): the distance moved counts that item (50 px), and the
/// 50 px "left" go to the parent's post-scroll: an exit-until-collapsed bar expands mid-list.
#[test]
fn an_item_inserted_above_during_a_drag_offers_the_parent_nothing() {
    let state = LazyListState::new(10, 0);
    let given = *state;
    let records = Rc::new(Records::default());
    let connection: Rc<dyn NestedScrollConnection> = records.clone();
    let items = compose_runtime::mutable_state_of((0..200u64).collect::<Vec<u64>>());
    let handle = items.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        let keys = Rc::new(handle.get());
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            let keys = keys.clone();
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = given,
                move |scope: LazyListScope| {
                    let ids = keys.clone();
                    scope.items_keyed(
                        keys.len(),
                        move |index| ids[index],
                        |_| Box!(modifier = Modifier.size((100.dp(), 50.dp()))),
                    );
                }
            );
        });
    });
    ui.wait_for_idle();
    let from = Offset::new(50.0, 10.0);
    ui.host().send_pointer(touch(PointerEvent::press(from)));
    ui.host().send_pointer(touch(PointerEvent::moved(from + Offset::new(0.0, 28.0), true)));
    items.edit(|items| items.insert(0, 1000));
    ui.host().frame();
    ui.host().send_pointer(touch(PointerEvent::release(from + Offset::new(0.0, 28.0))));
    ui.wait_for_idle();
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            left_y(&records)
        ),
        (10, 30, 0.0),
    );
}

/// Input: a 500 px list of 50 px rows at row 5 (start estimated) in a nested-scroll parent;
/// row 5 holds a 100 px scrolling column (at its top, 300 px of content); a drag down by 280
/// px on that column.
///
/// Compose: the column can't scroll back; its post-scroll goes to the list, which moves 250 px
/// to its top, and on to the parent with the other 30 px.
///
/// Instead: the list, as the column's nested-scroll parent (`ScrollableConnection`, its
/// `flinger` dispatching with no parents), takes the whole delta at its estimated start and
/// records no overshoot: the 30 px are lost.
#[test]
fn a_list_as_a_parent_passes_on_what_it_couldnt_use() {
    let state = LazyListState::new(5, 0);
    let given = *state;
    let records = Rc::new(Records::default());
    let connection: Rc<dyn NestedScrollConnection> = records.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(100, 500), move || {
        Box!(modifier = Modifier.size((100.dp(), 500.dp())).nested_scroll(connection.clone()), {
            LazyColumn!(
                modifier = Modifier.size((100.dp(), 500.dp())),
                state = given,
                |scope: LazyListScope| {
                    scope.items(200, |index| {
                        if index == 5 {
                            Column!(
                                modifier = Modifier
                                    .size((100.dp(), 100.dp()))
                                    .vertical_scroll(remember_scroll_state!()),
                                {
                                    Spacer(Modifier.size((100.dp(), 300.dp())));
                                }
                            );
                        } else {
                            Box!(modifier = Modifier.size((100.dp(), 50.dp())));
                        }
                    });
                }
            );
        });
    });
    ui.wait_for_idle();
    drag_one_frame(&mut ui, Offset::new(50.0, 10.0), Offset::new(0.0, 280.0));
    ui.wait_for_idle();
    assert_eq!(
        (
            state.first_visible_item_index(),
            state.first_visible_item_scroll_offset(),
            left_y(&records)
        ),
        (0, 0, 30.0),
    );
}
