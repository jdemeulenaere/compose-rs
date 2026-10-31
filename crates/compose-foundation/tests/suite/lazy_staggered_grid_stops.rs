//! What a staggered grid does where Compose's composes as it measures and this one stops and
//! measures again in the frame's next round (D1469, D1470): that a stop changes nothing, that a
//! frame shows something, how much is composed, and what's kept.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{Color, Dp, IntSize, UnitExt};

use crate::scroll_support::run;

type Live = Rc<RefCell<BTreeSet<usize>>>;

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

/// The requests between frames, trims among them, until they ended.
fn idle(ui: &mut ComposeTestRule) {
    for _ in 0..50 {
        ui.wait_for_idle();
        if !ui.host().has_prefetch_work() {
            return;
        }
        ui.host().prefetch(later());
    }
    panic!("prefetching never ended");
}

fn noting(index: usize, live: Live) {
    DisposableEffect(index, move |effects| {
        live.borrow_mut().insert(index);
        let live = live.clone();
        effects.on_dispose(move || {
            live.borrow_mut().remove(&index);
        })
    });
}

struct Grid {
    ui: ComposeTestRule,
    state: LazyStaggeredGridState,
    live: Live,
}

/// A vertical grid of `lanes` in `size`, of `count` items of `main(index)`, each with a background,
/// noting what's composed.
fn grid(
    lanes: usize,
    size: (i32, i32),
    count: usize,
    padding: (i32, i32),
    main: impl Fn(usize) -> i32 + Clone + 'static,
) -> Grid {
    spanned_from(0, lanes, size, count, padding, main, |_| false)
}

fn spanned(
    lanes: usize,
    size: (i32, i32),
    count: usize,
    padding: (i32, i32),
    main: impl Fn(usize) -> i32 + Clone + 'static,
    full: impl Fn(usize) -> bool + Clone + 'static,
) -> Grid {
    spanned_from(0, lanes, size, count, padding, main, full)
}

/// `grid` at item `first`, with some items full lines.
fn spanned_from(
    first: usize,
    lanes: usize,
    size: (i32, i32),
    count: usize,
    padding: (i32, i32),
    main: impl Fn(usize) -> i32 + Clone + 'static,
    full: impl Fn(usize) -> bool + Clone + 'static,
) -> Grid {
    use_optimizations(Optimizations::NONE);
    let owned = LazyStaggeredGridState::new(first, 0);
    let state = *owned;
    std::mem::forget(owned);
    state.set_prefetching_enabled(false);
    let live: Live = Rc::default();
    let noted = live.clone();
    let ui = ComposeTestRule::sized(IntSize::new(2000, 2000), move || {
        let (noted, main, full) = (noted.clone(), main.clone(), full.clone());
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(lanes),
            modifier = Modifier.size((Dp(size.0 as f32), Dp(size.1 as f32))),
            state = state,
            content_padding = PaddingValues {
                top: Dp(padding.0 as f32),
                bottom: Dp(padding.1 as f32),
                ..PaddingValues::ZERO
            },
            move |scope: LazyStaggeredGridScope| {
                let (noted, main, full) = (noted.clone(), main.clone(), full.clone());
                scope.items_keyed_with_span(
                    count,
                    |index| index,
                    |_| (),
                    move |index| {
                        if full(index) {
                            StaggeredGridItemSpan::FullLine
                        } else {
                            StaggeredGridItemSpan::SingleLane
                        }
                    },
                    move |index| {
                        Box!(
                            modifier = Modifier
                                .fill_max_width()
                                .height(Dp(main(index) as f32))
                                .background(Color::BLUE)
                                .test_tag(index.to_string())
                        );
                        noting(index, noted.clone());
                    },
                );
            }
        );
    });
    Grid { ui, state, live }
}

fn scroll_to(grid: &mut Grid, index: usize, offset: i32) {
    let state = grid.state;
    let _ = run(&mut grid.ui, async move { state.scroll_to_item(index, offset).await });
}

/// What's in view: each item, its lane and where.
fn laid(state: LazyStaggeredGridState) -> Vec<(usize, usize, i32)> {
    state
        .layout_info()
        .visible_items_info
        .iter()
        .map(|it| (it.index, it.lane, it.offset.y))
        .collect()
}

/// A few pseudo-random operations, the same each run.
struct Rand(u32);

impl Rand {
    fn next(&mut self, below: u32) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 % below
    }
}

/// Measuring with stops gives what measuring with everything composed gives, for jumps and scrolls.
fn stops_change_nothing(trims: bool) {
    let main = |index: usize| 20 + (index as i32 * 37) % 160;
    let full = |index: usize| index % 23 == 7;
    for seed in 1..=12u32 {
        let mut stopping = spanned(3, (600, 1000), 300, (30, 15), main, full);
        let mut whole = spanned(3, (600, 1000), 300, (30, 15), main, full);
        whole.state.compose_items(0, 300);
        whole.ui.wait_for_idle();
        stopping.ui.wait_for_idle();
        let mut rand = Rand(seed.wrapping_mul(2654435761));
        for step in 0..40 {
            let kind = rand.next(3);
            let (index, offset) = (rand.next(300) as usize, rand.next(120) as i32 - 20);
            let delta = rand.next(3000) as f32 - 1500.0;
            for grid in [&mut stopping, &mut whole] {
                if kind == 0 {
                    scroll_to(grid, index, 0);
                } else if kind == 1 {
                    scroll_to(grid, index, offset);
                } else {
                    let state = grid.state;
                    let _ = run(&mut grid.ui, async move { state.scroll_by(delta).await });
                }
                grid.ui.wait_for_idle();
            }
            if trims {
                idle(&mut stopping.ui);
            }
            assert_eq!(
                laid(stopping.state),
                laid(whole.state),
                "seed {seed}, step {step}: {kind} {index} {offset} {delta}"
            );
        }
    }
}

#[test]
fn a_stop_changes_nothing_of_what_a_measurement_finds() {
    stops_change_nothing(false);
}

#[test]
fn a_stop_changes_nothing_with_trims_between() {
    stops_change_nothing(true);
}

/// A jump composes around where it lands, not all that's between.
#[test]
fn a_jump_composes_around_its_item_only() {
    let mut grid = grid(2, (400, 800), 100_000, (0, 0), |_| 100);
    grid.ui.wait_for_idle();
    scroll_to(&mut grid, 50_000, 0);
    assert!(grid.live.borrow().len() < 100, "{} composed", grid.live.borrow().len());
    idle(&mut grid.ui);
    scroll_to(&mut grid, 90_000, 0);
    idle(&mut grid.ui);
    scroll_to(&mut grid, 100, 0);
    assert!(grid.live.borrow().len() < 100, "{} composed", grid.live.borrow().len());
    idle(&mut grid.ui);
    let state = grid.state;
    let _ = run(&mut grid.ui, async move { state.animate_scroll_to_item(30_000, 0).await });
    assert_eq!(state.first_visible_item_index(), 30_000);
    idle(&mut grid.ui);
    assert!(grid.live.borrow().len() < 100, "{} composed", grid.live.borrow().len());
}

/// Rects in the display list: what a frame shows.
fn drawn(ui: &mut ComposeTestRule) -> usize {
    ui.host().display_list().dump().lines().filter(|line| line.contains("rect")).count()
}

fn no_frame_is_blank(mut grid: Grid, frames: usize, delta: f32) {
    grid.ui.wait_for_idle();
    grid.ui.set_auto_advance(false);
    assert!(drawn(&mut grid.ui) > 0, "the first frame shows nothing");
    let mut before = laid(grid.state);
    for frame in 0..frames {
        let _ = grid.state.dispatch_raw_delta(delta);
        grid.ui.host().frame();
        assert!(drawn(&mut grid.ui) > 0, "frame {frame} shows nothing");
        // Its rounds were enough: laid out, not a stop's partial view.
        let now = laid(grid.state);
        assert_ne!(now, before, "frame {frame} didn't finish");
        before = now;
    }
}

#[test]
fn a_scroll_from_large_items_to_small_ones_shows_each_frame() {
    no_frame_is_blank(
        grid(2, (400, 800), 2000, (0, 0), |index| if index < 20 { 400 } else { 16 }),
        80,
        150.0,
    );
}

#[test]
fn a_scroll_from_very_large_items_to_very_small_ones_shows_each_frame() {
    no_frame_is_blank(
        grid(2, (400, 800), 4000, (0, 0), |index| if index < 20 { 400 } else { 10 }),
        60,
        200.0,
    );
}

#[test]
fn a_scroll_over_items_of_no_size_shows_each_frame() {
    no_frame_is_blank(
        grid(2, (400, 100), 200, (0, 0), |index| if (20..40).contains(&index) { 0 } else { 30 }),
        12,
        20.0,
    );
}

/// A frame composes about 2,250 at most (the rounds' 1, 1, 2, 4 … 128, 200 …); more wait for the
/// next, and what's measured is shown meanwhile.
#[test]
fn a_frame_composes_a_bounded_number_of_items_and_shows_those() {
    let mut grid = grid(4, (400, 1990), 20_000, (0, 0), |_| 1);
    grid.ui.wait_for_idle();
    idle(&mut grid.ui);
    let live = grid.live.clone();
    grid.ui.set_auto_advance(false);
    grid.state.request_scroll_to_item(10_000, 0);
    grid.ui.host().frame();
    let new = live.borrow().iter().filter(|item| **item >= 9_000).count();
    assert!((1_000..=2_300).contains(&new), "{new} composed in the frame");
    assert!(drawn(&mut grid.ui) > 0);
    grid.ui.set_auto_advance(true);
    grid.ui.wait_for_idle();
    grid.ui.on_node_with_tag("17900").assert_exists();
}

/// What's in view under the start padding stays, and at rest the trims leave nothing to do.
#[test]
fn the_idle_trim_keeps_whats_under_the_padding() {
    for padding in [100, 2000] {
        let mut grid = grid(2, (400, 800 + padding), 1000, (padding, 0), |_| 50);
        grid.ui.wait_for_idle();
        scroll_to(&mut grid, 100, 0);
        idle(&mut grid.ui);
        let before = grid.live.borrow().clone();
        let visible = laid(grid.state);
        assert!(visible.iter().any(|item| item.2 < 0), "items under the padding");
        for _ in 0..4 {
            idle(&mut grid.ui);
        }
        assert_eq!(*grid.live.borrow(), before);
        for (index, _, _) in visible {
            assert!(grid.live.borrow().contains(&index), "{index} left");
        }
        assert!(drawn(&mut grid.ui) > 0);
    }
}

/// A very long item in a lane doesn't keep all that the other lanes pass: a set is composed.
#[test]
fn items_between_a_long_item_and_those_in_view_leave() {
    let mut grid = grid(4, (400, 800), 8000, (0, 0), |index| if index == 0 { 200_000 } else { 40 });
    grid.ui.wait_for_idle();
    let state = grid.state;
    let _ = run(&mut grid.ui, async move { state.scroll_by(40_000.0).await });
    idle(&mut grid.ui);
    let visible = laid(state).len();
    assert!(visible >= 60, "{visible} in view");
    assert!(grid.live.borrow().contains(&0));
    let live = grid.live.borrow().len();
    assert!(live < visible + 30, "{live} composed for {visible} in view");
}

/// An item scrolled out and focused past the last in view is placed past it, not over the first.
#[test]
fn a_focused_item_past_those_in_view_is_placed_past_them() {
    use_optimizations(Optimizations::NONE);
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(2),
            modifier = Modifier.size(200.dp()),
            state = state,
            |scope: LazyStaggeredGridScope| {
                scope.items(100, |index| {
                    Box!(
                        modifier = Modifier.height(50.dp()).test_tag(index.to_string()).focusable()
                    );
                });
            }
        );
    });
    let _ = run(&mut ui, async move { state.scroll_to_item(20, 0).await });
    ui.on_node_with_tag("22").request_focus();
    ui.wait_for_idle();
    let _ = run(&mut ui, async move { state.scroll_to_item(0, 0).await });
    let bounds = ui.on_node_with_tag("22").fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.top, 200.0);
    assert!(!ui.on_node_with_tag("22").is_displayed());
}

/// Items resized out of view: what's remembered of them isn't how far it can go back.
#[test]
fn a_grid_with_content_left_takes_all_of_a_scroll_back() {
    let sizes = mutable_state_of(50);
    let read = sizes.handle();
    let mut grid =
        grid(2, (200, 300), 200, (0, 0), move |index| if index < 14 { read.get() } else { 50 });
    grid.ui.wait_for_idle();
    let state = grid.state;
    let _ = run(&mut grid.ui, async move { state.scroll_by(350.0).await });
    assert_eq!(state.first_visible_item_index(), 14);
    sizes.set(200);
    grid.ui.wait_for_idle();
    let consumed = run(&mut grid.ui, async move { state.scroll_by(-501.0).await });
    assert_eq!(consumed.expect("not interrupted"), -501.0);
}

/// Items of no size at the start: past them it can scroll back as `first[0] == 0 && offset <= 0`
/// says, where no room shows.
#[test]
fn items_of_no_size_at_the_start_are_room_to_go_back() {
    let mut grid = grid(2, (200, 200), 100, (0, 0), |index| if index < 10 { 0 } else { 50 });
    grid.ui.wait_for_idle();
    let state = grid.state;
    assert!(!state.can_scroll_backward());
    let _ = run(&mut grid.ui, async move { state.scroll_by(1.0).await });
    let _ = run(&mut grid.ui, async move { state.scroll_by(-1.0).await });
    assert_eq!(
        state.can_scroll_backward(),
        !(state.first_visible_item_index() == 0 && state.first_visible_item_scroll_offset() <= 0)
    );
}

#[test]
fn a_first_item_going_to_the_second_lane_is_put_back() {
    let mut grid = grid(2, (200, 200), 40, (0, 0), |_| 50);
    grid.ui.wait_for_idle();
    scroll_to(&mut grid, 1, 0);
    let state = grid.state;
    let _ = run(&mut grid.ui, async move { state.scroll_by(-200.0).await });
    assert_eq!(state.first_visible_item_index(), 0);
    let items = laid(state);
    assert_eq!(items[0], (0, 0, 0));
    assert_eq!(items[1], (1, 1, 0));
}

#[test]
fn a_full_line_at_the_position_and_more_lanes_after() {
    let lanes = mutable_state_of(2usize);
    let read = lanes.handle();
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(read.get()),
            modifier = Modifier.size(300.dp()),
            state = state,
            |scope: LazyStaggeredGridScope| {
                scope.items_with_span(
                    60,
                    |_| (),
                    |index| {
                        if index == 6 {
                            StaggeredGridItemSpan::FullLine
                        } else {
                            StaggeredGridItemSpan::SingleLane
                        }
                    },
                    |index| Spacer(Modifier.height(50.dp()).test_tag(index.to_string())),
                );
            }
        );
    });
    let _ = run(&mut ui, async move { state.scroll_to_item(6, 0).await });
    assert_eq!(state.first_visible_item_index(), 6);
    lanes.set(3);
    ui.wait_for_idle();
    // The lanes given out again: an item before the full line, in the last lane, starts it.
    assert_eq!(state.first_visible_item_index(), 5);
    assert!(state.layout_info().visible_items_info.iter().any(|it| it.index == 6 && it.lane == 0));
}

#[test]
fn fewer_lanes_at_the_end_and_a_scroll_back() {
    let lanes = mutable_state_of(3usize);
    let read = lanes.handle();
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(read.get()),
            modifier = Modifier.size(300.dp()),
            state = state,
            |scope: LazyStaggeredGridScope| {
                scope.items(60, |index| {
                    Spacer(Modifier.height(50.dp()).test_tag(index.to_string()));
                });
            }
        );
    });
    let _ = run(&mut ui, async move { state.scroll_by(2000.0).await });
    lanes.set(2);
    ui.wait_for_idle();
    let _ = run(&mut ui, async move { state.scroll_by(-500.0).await });
    // Nothing lost: every item in view has its neighbours, and the lanes alternate.
    let items = state.layout_info().visible_items_info;
    assert!(items.len() >= 12);
    for pair in items.windows(2) {
        assert_eq!(pair[1].index, pair[0].index + 1);
    }
    assert_eq!(
        items.last().map(|it| it.index),
        Some(state.first_visible_item_index() + items.len() - 1)
    );
}

/// The items lost size at the end: the scroll back finds lanes off, and starts them again.
#[test]
fn a_scroll_back_at_the_end_with_lanes_off_starts_them_again() {
    let shrunk = mutable_state_of(false);
    let read = shrunk.handle();
    let mut grid =
        grid(
            2,
            (200, 400),
            30,
            (0, 0),
            move |index| {
                if read.get() && index % 2 == 1 { 20 } else { 100 }
            },
        );
    grid.ui.wait_for_idle();
    let state = grid.state;
    scroll_to(&mut grid, 29, 0);
    let before = laid(state);
    shrunk.set(true);
    grid.ui.wait_for_idle();
    let after = laid(state);
    assert_ne!(before, after);
    // Whatever it found, the last ends the grid and each lane holds on from the other's order.
    let last = after.last().expect("items");
    assert_eq!(last.0, 29);
    assert!(after.iter().all(|item| item.2 < 400));
    let _ = run(&mut grid.ui, async move { state.scroll_by(-1_000_000.0).await });
    let start = laid(state);
    assert_eq!(start[0], (0, 0, 0));
    assert_eq!(start[1], (1, 1, 0));
    assert_eq!(start[2], (2, 1, 20));
}
/// `layout_info` read where the grid is called: it stays idle, its items' new closures saying
/// nothing new (D1470).
#[test]
fn reading_layout_info_where_the_grid_is_called_goes_idle() {
    use_optimizations(Optimizations::NONE);
    let reads = Rc::new(Cell::new(0));
    let counted = reads.clone();
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let _ = state.layout_info();
        counted.set(counted.get() + 1);
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(2),
            modifier = Modifier.size(300.dp()),
            state = state,
            |scope: LazyStaggeredGridScope| {
                scope.items(60, |_| Spacer(Modifier.height(50.dp())));
            }
        );
    });
    ui.wait_for_idle();
    assert!(reads.get() < 8, "{} compositions", reads.get());
}

/// Going back over many items is composed a few at first, then more, in the frame's rounds.
#[test]
fn a_long_scroll_back_finishes_in_a_frame() {
    let mut grid = grid(2, (400, 400), 2000, (0, 0), |_| 50);
    grid.ui.wait_for_idle();
    scroll_to(&mut grid, 1000, 0);
    idle(&mut grid.ui);
    let state = grid.state;
    grid.ui.set_auto_advance(false);
    let _ = state.dispatch_raw_delta(-5000.0);
    grid.ui.host().frame();
    assert_eq!(state.first_visible_item_index(), 800);
}

/// A jump where lanes are far apart: what's far comes apart from the items composed, or each lane
/// would replace the other's for ever.
#[test]
fn a_jump_with_lanes_far_apart_settles() {
    let mut grid =
        grid(4, (400, 800), 12_000, (0, 0), |index| if index == 0 { 60_000 } else { 40 });
    grid.ui.wait_for_idle();
    let state = grid.state;
    // Past the long item, which leaves: its lane's known, and nothing's composed near it.
    let _ = run(&mut grid.ui, async move { state.scroll_by(70_000.0).await });
    idle(&mut grid.ui);
    assert!(!grid.live.borrow().contains(&0));
    // Back into it: the first lane's first is item 0, the others' far from it.
    scroll_to(&mut grid, 2000, 30_000);
    idle(&mut grid.ui);
    assert!(grid.live.borrow().contains(&0), "{:?}", laid(state).first());
    assert!(grid.live.borrow().len() < 150, "{} composed", grid.live.borrow().len());
}

/// Item 0 of no size going to the second lane on the way back, the lanes starting together: only
/// that it's in the wrong lane starts them again (`misalignedStart`'s case 3).
#[test]
fn a_first_item_of_no_size_in_the_second_lane_is_put_back() {
    // At item 1 from the first, so item 0's lane isn't known.
    let main = |index: usize| if index == 0 { 0 } else { 50 };
    let mut grid = spanned_from(1, 2, (200, 200), 40, (0, 0), main, |_| false);
    grid.ui.wait_for_idle();
    let state = grid.state;
    let _ = run(&mut grid.ui, async move { state.scroll_by(-50.0).await });
    assert_eq!(laid(state)[0].1, 0, "{:?}", laid(state));
}

/// Fewer items under lanes scrolled unevenly: the scroll back at the end finds them off and gives
/// the lanes out again.
#[test]
fn fewer_items_under_uneven_lanes_give_the_lanes_out_again() {
    let count = mutable_state_of(60usize);
    let read = count.handle();
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(2),
            modifier = Modifier.size((200.dp(), 300.dp())),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                scope.items(read.get(), |index| {
                    Spacer(Modifier.height(Dp((40 + (index * 29) % 90) as f32)));
                });
            }
        );
    });
    let _ = run(&mut ui, async move { state.scroll_by(1500.0).await });
    let first = state.first_visible_item_index();
    count.set(first + 3);
    ui.wait_for_idle();
    let laid_now = laid(state);
    // The last ends the grid.
    let info = state.layout_info();
    let last = info.visible_items_info.iter().map(|it| it.offset.y + it.size.height).max();
    assert_eq!(last, Some(300), "{laid_now:?}");
    let _ = run(&mut ui, async move { state.scroll_by(-1_000_000.0).await });
    assert_eq!(laid(state)[0], (0, 0, 0));
    assert_eq!(laid(state)[1], (1, 1, 0));
}

/// However the items went, each lane's are one after the other: a lane found off is given out again
/// (`gapDetected`).
#[test]
fn fewer_items_leave_no_gap_in_a_lane() {
    for shrunk_by in 0..24usize {
        let count = mutable_state_of(60usize);
        let read = count.handle();
        let owned = LazyStaggeredGridState::new(0, 0);
        let state = *owned;
        let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
            LazyVerticalStaggeredGrid!(
                StaggeredGridCells::Fixed(2),
                modifier = Modifier.size((200.dp(), 300.dp())),
                state = state,
                move |scope: LazyStaggeredGridScope| {
                    scope.items(read.get(), |index| {
                        Spacer(Modifier.height(Dp((40 + (index * 29) % 90) as f32)));
                    });
                }
            );
        });
        let _ =
            run(&mut ui, async move { state.scroll_by(1000.0 + 13.0 * shrunk_by as f32).await });
        let shown = state.layout_info().visible_items_info;
        let last = shown.last().expect("items").index;
        count.set(last);
        ui.wait_for_idle();
        let info = state.layout_info().visible_items_info;
        for lane in 0..2 {
            let in_lane: Vec<_> = info.iter().filter(|item| item.lane == lane).collect();
            for pair in in_lane.windows(2) {
                assert_eq!(
                    pair[0].offset.y + pair[0].size.height,
                    pair[1].offset.y,
                    "{shrunk_by}: {info:?}"
                );
            }
        }
    }
}

#[test]
fn a_lane_info_goes_back_to_what_it_was() {
    let mut info = LazyStaggeredGridLaneInfo::new();
    info.set_lane(3, 1);
    info.set_gaps(3, Some(vec![1, 2]));
    let check = |info: &LazyStaggeredGridLaneInfo| {
        assert_eq!(info.get_lane(3), 1);
        assert_eq!(info.get_lane(4), LazyStaggeredGridLaneInfo::LANE_UNSET);
        assert_eq!(info.get_gaps(3), Some(&[1, 2][..]));
        assert_eq!(info.get_gaps(5), None);
        assert_eq!(info.upper_bound(), 16);
        assert_eq!(info.lower_bound(), 0);
    };
    // Writes.
    info.begin_pass();
    info.set_lane(3, 0);
    info.set_lane(4, 1);
    info.set_gaps(3, None);
    info.set_gaps(5, Some(vec![7, 8]));
    info.roll_back();
    check(&info);
    // A reset, with writes before and after.
    info.begin_pass();
    info.set_lane(4, 1);
    info.reset();
    info.set_lane(3, 0);
    info.roll_back();
    check(&info);
    // Growing, and the window moving.
    info.begin_pass();
    info.set_lane(100, 1);
    info.set_lane(300_000, 1);
    info.roll_back();
    check(&info);
    // Kept when it ends.
    info.begin_pass();
    info.set_lane(4, 0);
    info.commit_pass();
    assert_eq!(info.get_lane(4), 0);
}

/// Each of `frames` frames, after `start`, shows something.
fn frames_show(grid: &mut Grid, frames: usize, start: impl FnOnce(LazyStaggeredGridState)) {
    grid.ui.wait_for_idle();
    idle(&mut grid.ui);
    grid.ui.set_auto_advance(false);
    start(grid.state);
    for frame in 0..frames {
        grid.ui.host().frame();
        assert!(drawn(&mut grid.ui) > 0, "frame {frame} shows nothing");
    }
}

/// All a stop measured scrolled past: what the last whole measurement placed shows.
#[test]
fn a_scroll_past_all_that_it_measured_shows_each_frame() {
    frames_show(&mut grid(2, (400, 800), 8000, (0, 0), |_| 60), 8, |state| {
        let _ = state.dispatch_raw_delta(200_000.0);
    });
    frames_show(&mut grid(2, (400, 800), 40_000, (0, 0), |_| 10), 12, |state| {
        let _ = state.dispatch_raw_delta(100_000.0);
    });
}

/// A jump stopping before its first item: what was placed stays composed, and shows.
#[test]
fn a_jump_stopped_before_its_first_item_shows_what_was_placed() {
    frames_show(&mut grid(2, (400, 800), 8000, (0, 0), |_| 10), 8, |state| {
        state.request_scroll_to_item(3000, -40_000);
    });
    frames_show(&mut grid(4, (400, 1990), 40_000, (0, 0), |_| 1), 8, |state| {
        state.request_scroll_to_item(19_999, 0);
    });
    // Going up past a frame's rounds: it ends stopped before its first item.
    frames_show(&mut grid(4, (400, 1990), 40_000, (0, 0), |_| 1), 8, |state| {
        state.request_scroll_to_item(19_999, -20_000);
    });
}

/// Each item's composition, by id: more than one is a state lost.
type Remembered = Rc<RefCell<std::collections::HashMap<i32, usize>>>;

/// A grid of `ids`, sized by `main`, an item noting each time its `remember` runs.
fn remembering(
    lanes: usize,
    ids: Vec<i32>,
    main: impl Fn(i32) -> i32 + Clone + 'static,
) -> (ComposeTestRule, LazyStaggeredGridState, OwnedState<Vec<i32>>, Remembered) {
    use_optimizations(Optimizations::NONE);
    let list = mutable_state_of(ids);
    let read = list.handle();
    let owned = LazyStaggeredGridState::new(0, 0);
    let state = *owned;
    std::mem::forget(owned);
    state.set_prefetching_enabled(false);
    let seen: Remembered = Rc::default();
    let noted = seen.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 1000), move || {
        let (noted, main) = (noted.clone(), main.clone());
        let ids = read.get();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(lanes),
            modifier = Modifier.size((200.dp(), 400.dp())),
            state = state,
            move |scope: LazyStaggeredGridScope| {
                let (noted, main) = (noted.clone(), main.clone());
                scope.items_list_keyed(
                    ids.clone(),
                    |id: &i32| *id,
                    move |id: &i32| {
                        let id = *id;
                        let noted = noted.clone();
                        let _ = remember(move || {
                            *noted.borrow_mut().entry(id).or_default() += 1;
                            Rc::new(id)
                        });
                        Spacer(Modifier.height(Dp(main(id) as f32)));
                    },
                );
            }
        );
    });
    ui.wait_for_idle();
    (ui, state, list, seen)
}

/// A jump keeping a long item in view: it isn't composed again.
#[test]
fn a_jump_keeps_the_state_of_an_item_in_view_before_and_after() {
    let long = |id: i32| if id == 4 { 100_000 } else { 50 };
    let (mut ui, state, _list, seen) = remembering(2, (0..2000).collect(), long);
    // Far and back, so the lanes are known and item 4 is among the items composed in a row.
    let _ = run(&mut ui, async move { state.scroll_by(26_000.0).await });
    idle(&mut ui);
    let _ = run(&mut ui, async move { state.scroll_by(-25_900.0).await });
    idle(&mut ui);
    assert!(laid(state).iter().any(|item| item.0 == 4));
    assert_eq!(seen.borrow()[&4], 1);
    state.request_scroll_to_item(500, 0);
    ui.wait_for_idle();
    idle(&mut ui);
    assert_eq!(seen.borrow()[&4], 1, "item 4 was composed again");
}

/// Items added before a long item the position's lowest: the position follows it, and it keeps its
/// state, composed apart from the others.
#[test]
fn a_prepend_with_lanes_far_apart_follows_the_first_item() {
    let long = |id: i32| if id == 5 { 100_000 } else { 40 };
    let (mut ui, state, list, seen) = remembering(2, (0..3000).collect(), long);
    let _ = run(&mut ui, async move { state.scroll_by(30_000.0).await });
    idle(&mut ui);
    assert_eq!(state.first_visible_item_index(), 5);
    let others = laid(state).len();
    assert!(others > 5);
    list.set((-1..3000).collect());
    ui.wait_for_idle();
    idle(&mut ui);
    assert_eq!(state.first_visible_item_index(), 6);
    assert_eq!(seen.borrow()[&5], 1, "the long item was composed again");
}

/// Two long items, each lane's first far from the items composed and from each other: going back
/// among them composes each apart, or the jump would replace one by the other for ever.
fn two_long_items() -> (ComposeTestRule, LazyStaggeredGridState, usize) {
    let long = |id: i32| if id == 4 || id == 1000 { 100_000 } else { 10 };
    let (mut ui, state, _list, _seen) = remembering(3, (0..14_000).collect(), long);
    // Into both.
    let _ = run(&mut ui, async move { state.scroll_by(40_000.0).await });
    idle(&mut ui);
    let target = laid(state).last().expect("items").0;
    // Past both: nothing near them composed, their lanes known.
    let _ = run(&mut ui, async move { state.scroll_by(120_000.0).await });
    idle(&mut ui);
    assert!(state.first_visible_item_index() > 1000);
    std::mem::forget(_list);
    (ui, state, target)
}

#[test]
fn a_jump_to_lanes_far_apart_settles_and_finishes_soon() {
    let (mut ui, state, target) = two_long_items();
    ui.set_auto_advance(false);
    state.request_scroll_to_item(target, 0);
    let mut frames = 0;
    while !laid(state).iter().any(|item| item.0 == target) {
        ui.host().frame();
        frames += 1;
        // More in view than a frame's rounds, which each take as many as they ask for.
        assert!(frames < 2, "{frames} frames for the jump");
    }
    let first = state.first_visible_item_index();
    assert!(first <= 4 || first == target, "{first}");
}

fn sizes_of(count: usize, item: i32, most: i32) -> Vec<i32> {
    use_optimizations(Optimizations::NONE);
    let sizes: Rc<RefCell<Vec<i32>>> = Rc::default();
    let noted = sizes.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(1000, 2000), move || {
        let noted = noted.clone();
        LazyVerticalStaggeredGrid!(
            StaggeredGridCells::Fixed(2),
            modifier = Modifier
                .height_in(0.dp(), Dp(most as f32))
                .on_size_changed(move |size: IntSize| noted.borrow_mut().push(size.height)),
            |scope: LazyStaggeredGridScope| {
                scope.items(count, move |_| Spacer(Modifier.height(Dp(item as f32))));
            }
        );
    });
    ui.wait_for_idle();
    sizes.borrow().clone()
}

/// As large as what's measured so far, never as large as it may be. A list says `[50, 150]`: the
/// 0 is the minimum, from the stop before anything is measured, which a list doesn't have.
#[test]
fn a_stop_doesnt_say_a_false_size() {
    assert_eq!(sizes_of(6, 50, 800), [0, 50, 150]);
}

/// A first measurement taking frames says one partial size until it's whole.
#[test]
fn partial_sizes_dont_go_on_across_frames() {
    assert_eq!(sizes_of(20_000, 1, 1990), [0, 1, 1990]);
}
