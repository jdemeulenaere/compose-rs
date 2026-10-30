//! Review of D450-D453: lazy grids composed item by item.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Rect, UnitExt};

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

/// Lines of different heights (an item's height from its index), scrolled on and back by the
/// same amounts: back at the top exactly.
#[test]
fn review_scrolling_back_over_lines_of_different_heights_returns_to_the_top() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 300.dp())),
            state = given,
            vertical_arrangement = Arrangement::spaced_by(3.dp()),
            |scope: LazyGridScope| {
                scope.items(600, |index| {
                    let height = 20 + (index * 7919 % 13) as i32 * 5;
                    Box!(modifier = Modifier.height(height.dp()).test_tag(format!("{index}")));
                });
            }
        );
    });
    for _ in 0..80 {
        state.dispatch_raw_delta(37.0);
        ui.wait_for_idle();
    }
    let far = state.first_visible_item_index();
    assert!(far > 30, "{far}");
    for _ in 0..80 {
        state.dispatch_raw_delta(-37.0);
        ui.wait_for_idle();
    }
    assert_eq!(state.first_visible_item_index(), 0);
    assert_eq!(state.first_visible_item_scroll_offset(), 0);
    assert_eq!(bounds(&mut ui, "0").top, 0.0);
    // Lines contiguous: line 1 starts after line 0's tallest item and the spacing.
    let line0 = (0..3).map(|i| 20 + (i * 7919 % 13) * 5).max().unwrap() as f32;
    assert_eq!(bounds(&mut ui, "3").top, line0 + 3.0);
    // A big jump back from far.
    state.request_scroll_to_item(500, 0);
    ui.wait_for_idle();
    for _ in 0..30 {
        state.dispatch_raw_delta(-1000.0);
        ui.wait_for_idle();
    }
    assert_eq!(state.first_visible_item_index(), 0);
    assert_eq!(bounds(&mut ui, "0").top, 0.0);
    assert!(!state.can_scroll_backward());
}

/// Scrolled past the end: the last line ends at the viewport's end, inside the padding.
#[test]
fn review_the_end_clamps_with_padding_and_spacing() {
    for vertical in [true, false] {
        let state = LazyGridState::new(0, 0);
        let given = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            let content = |scope: LazyGridScope| {
                scope.items(31, |index| {
                    Box!(modifier = Modifier.size((30.dp(), 30.dp())).test_tag(format!("{index}")));
                });
            };
            if vertical {
                LazyVerticalGrid!(
                    GridCells::Fixed(3),
                    modifier = Modifier.size((300.dp(), 200.dp())),
                    state = given,
                    content_padding = PaddingValues::new(0.dp(), 7.dp(), 0.dp(), 11.dp()),
                    vertical_arrangement = Arrangement::spaced_by(5.dp()),
                    content = content
                );
            } else {
                LazyHorizontalGrid!(
                    GridCells::Fixed(3),
                    modifier = Modifier.size((200.dp(), 300.dp())),
                    state = given,
                    content_padding = PaddingValues::new(7.dp(), 0.dp(), 11.dp(), 0.dp()),
                    horizontal_arrangement = Arrangement::spaced_by(5.dp()),
                    content = content
                );
            }
        });
        state.dispatch_raw_delta(100_000.0);
        ui.wait_for_idle();
        let last = bounds(&mut ui, "30");
        let end = if vertical { last.bottom } else { last.right };
        assert_eq!(end, 200.0 - 11.0, "vertical {vertical}");
        assert!(!state.can_scroll_forward());
        state.dispatch_raw_delta(-100_000.0);
        ui.wait_for_idle();
        let first = bounds(&mut ui, "0");
        assert_eq!(if vertical { first.top } else { first.left }, 7.0);
    }
}

/// Items removed at the end while scrolled to it: the grid clamps, no panic, and the last
/// line ends at the viewport's end.
#[test]
fn review_items_removed_at_the_end_while_scrolled_there() {
    let count = mutable_state_of(300usize);
    let handle = count.handle();
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let count = handle.get();
        LazyVerticalGrid!(
            GridCells::Fixed(4),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            move |scope: LazyGridScope| {
                scope.items_keyed(
                    count,
                    |index| index,
                    |index| {
                        Box!(modifier = Modifier.height(30.dp()).test_tag(format!("{index}")));
                    },
                );
            }
        );
    });
    state.dispatch_raw_delta(100_000.0);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "299").bottom, 200.0);
    count.set(250);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "249").bottom, 200.0);
    count.set(5);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 0);
    assert_eq!(bounds(&mut ui, "0").top, 0.0);
}

/// `scroll_to_item` onto an item mid-line with an offset: its line at the top, the offset into
/// it, `first_visible_item_index` the line's first item.
#[test]
fn review_scroll_to_an_item_mid_line_with_an_offset() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            |scope: LazyGridScope| {
                scope.items(300, |index| {
                    Box!(modifier = Modifier.height(50.dp()).test_tag(format!("{index}")));
                });
            }
        );
    });
    state.request_scroll_to_item(101, 20);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 99);
    assert_eq!(state.first_visible_item_scroll_offset(), 20);
    assert_eq!(bounds(&mut ui, "101").top, -20.0);
}

/// An Adaptive grid scrolled into an item mid-way, made narrower then wider: the first
/// visible item's line stays first, with its offset.
#[test]
fn review_adaptive_resize_keeps_the_first_item_and_offset() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 300), move || {
        LazyVerticalGrid!(
            GridCells::Adaptive(100.dp()),
            modifier = Modifier.fill_max_size(),
            state = given,
            |scope: LazyGridScope| {
                scope.items_keyed(
                    1000,
                    |index| index,
                    |index| {
                        Box!(modifier = Modifier.height(50.dp()).test_tag(format!("{index}")));
                    },
                );
            }
        );
    });
    state.request_scroll_to_item(400, 10);
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 400);
    ui.host().resize(IntSize::new(300, 300));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 399);
    assert_eq!(state.first_visible_item_scroll_offset(), 10);
    assert_eq!(bounds(&mut ui, "400").top, -10.0);
    ui.host().resize(IntSize::new(500, 300));
    ui.wait_for_idle();
    // Item 399's line (5 columns): items 395 to 399.
    assert_eq!(state.first_visible_item_index(), 395);
    assert_eq!(bounds(&mut ui, "399").top, -10.0);
}

thread_local! {
    static REMEMBERED: Cell<usize> = const { Cell::new(0) };
}

#[composable]
fn Counted(key: usize) {
    remember(|| REMEMBERED.with(|count| count.set(count.get() + 1)));
    Box!(modifier = Modifier.height(20.dp()).test_tag(format!("{key}")));
}

/// D453 in a grid: items removed far before those composed. Compose keeps the compositions of
/// the items in view (found by key).
#[test]
fn review_grid_items_removed_far_before_keep_their_composition() {
    let keys = mutable_state_of((0..3000usize).collect::<Vec<_>>());
    let handle = keys.handle();
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let keys = Rc::new(handle.get());
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            move |scope: LazyGridScope| {
                let (k, c) = (keys.clone(), keys.clone());
                scope.items_keyed(
                    keys.len(),
                    move |index| k[index],
                    move |index| Counted(c[index]),
                );
            }
        );
    });
    state.request_scroll_to_item(900, 0);
    ui.wait_for_idle();
    let before = REMEMBERED.with(Cell::get);
    keys.set((60..3000).collect());
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_index(), 840);
    assert_eq!(bounds(&mut ui, "900").top, 0.0);
    let after = REMEMBERED.with(Cell::get);
    assert_eq!(after - before, 0, "items composed again");
}

/// A span of 0 or larger than the line, and `max_current_line_span` passed back: no panic,
/// the items clamped to 1 and the line.
#[test]
fn review_spans_out_of_range_are_clamped() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 300.dp())),
            |scope: LazyGridScope| {
                scope.items_with_span(
                    6,
                    |line: LazyGridItemSpanScope, index| match index {
                        0 => GridItemSpan(0),
                        1 => GridItemSpan(99),
                        2 => GridItemSpan(line.max_current_line_span()),
                        _ => GridItemSpan(1),
                    },
                    |_| (),
                    |index| {
                        Box!(modifier = Modifier.height(20.dp()).test_tag(format!("{index}")));
                    },
                );
            }
        );
    });
    ui.wait_for_idle();
    let rects: Vec<Rect> = (0..6).map(|i| bounds(&mut ui, &i.to_string())).collect();
    assert_eq!((rects[0].left, rects[0].top, rects[0].width()), (0.0, 0.0, 100.0));
    assert_eq!((rects[1].left, rects[1].top, rects[1].width()), (0.0, 20.0, 300.0));
    assert_eq!((rects[2].left, rects[2].top, rects[2].width()), (0.0, 40.0, 300.0));
    assert_eq!((rects[3].left, rects[3].top), (0.0, 60.0));
}

/// An item's content type changing with its key the same keeps its composition, as Compose's.
#[test]
fn review_an_items_content_type_changing_keeps_its_state() {
    let kind = mutable_state_of(0u32);
    let handle = kind.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let kind = handle.get();
        LazyVerticalGrid!(
            GridCells::Fixed(2),
            modifier = Modifier.size((300.dp(), 300.dp())),
            move |scope: LazyGridScope| {
                scope.items_keyed_with_span(
                    4,
                    |index| index,
                    |_, _| GridItemSpan(1),
                    move |index| if index == 1 { kind } else { 9 },
                    Counted,
                );
            }
        );
    });
    ui.wait_for_idle();
    let before = REMEMBERED.with(Cell::get);
    kind.set(1);
    ui.wait_for_idle();
    assert_eq!(REMEMBERED.with(Cell::get), before);
}

thread_local! {
    static SPANS_ASKED: RefCell<usize> = const { RefCell::new(0) };
}

/// A grid with spans whose caller recomposes (another state it reads) while scrolled far: the
/// grid's content lambda is the same (its captures are), so the grid skips and asks no span
/// again, as Compose keeps its lines while the content is the same. (Called from the test's root
/// closure, which isn't a composable's body, the lambda was a new one each time: every span to
/// item 90,000 asked again, 0.54 ms. A content that changed asks them again, as in Compose: the
/// `_with_span` forms say so.)
#[test]
fn review_spans_asked_again_on_each_recomposition_of_the_caller() {
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || SpanCaller(handle, given));
    state.request_scroll_to_item(90_000, 0);
    ui.wait_for_idle();
    SPANS_ASKED.with(|asked| *asked.borrow_mut() = 0);
    let start = std::time::Instant::now();
    tick.set(1);
    ui.wait_for_idle();
    let elapsed = start.elapsed();
    let asked = SPANS_ASKED.with(|asked| *asked.borrow());
    println!("spans asked after one recomposition of the caller: {asked} in {elapsed:?}");
    assert!(asked < 1000, "{asked} spans asked again");
}

#[composable]
fn SpanCaller(tick: MutableState<u32>, state: LazyGridState) {
    let _ = tick.get();
    LazyVerticalGrid!(
        GridCells::Fixed(3),
        modifier = Modifier.size((300.dp(), 300.dp())),
        state = state,
        |scope: LazyGridScope| {
            scope.items_with_span(
                100_000,
                |line: LazyGridItemSpanScope, index| {
                    SPANS_ASKED.with(|asked| *asked.borrow_mut() += 1);
                    GridItemSpan(if index % 10 == 0 { line.max_line_span() } else { 1 })
                },
                |_| (),
                |index| {
                    Box!(modifier = Modifier.height(20.dp()).test_tag(format!("{index}")));
                },
            );
        }
    );
}

/// The same in a `LazyColumn` (the code the grid copied): items removed at the end while
/// scrolled there. The first measurement steps back over the composed items and leaves the
/// rest pending; a second measurement before the composition catches up applies the gap again
/// (pending -140, then -280), so the list ends 150 px back from its end, its last item out of
/// view, where Compose keeps the last item at the end.
#[test]
fn review_list_items_removed_at_the_end_while_scrolled_there() {
    let count = mutable_state_of(75usize);
    let handle = count.handle();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let count = handle.get();
        LazyColumn!(
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            move |scope: LazyListScope| {
                scope.items_keyed(
                    count,
                    |index| index,
                    |index| {
                        Box!(modifier = Modifier.height(30.dp()).test_tag(format!("{index}")));
                    },
                );
            }
        );
    });
    state.dispatch_raw_delta(100_000.0);
    ui.wait_for_idle();
    assert_eq!(bounds(&mut ui, "74").bottom, 200.0);
    count.set(62);
    ui.wait_for_idle();
    assert_eq!(
        bounds(&mut ui, "61").bottom,
        200.0,
        "first visible {}",
        state.first_visible_item_index()
    );
}

thread_local! {
    static TIMES: RefCell<std::collections::HashMap<usize, usize>> =
        RefCell::new(std::collections::HashMap::new());
}

#[composable]
fn Tracked(key: usize) {
    remember(|| TIMES.with(|times| *times.borrow_mut().entry(key).or_default() += 1));
    Box!(modifier = Modifier.height((20 + (key % 7) as i32 * 3).dp()));
}

/// Scrolling one way with prefetch between frames: each item is composed once (none composed
/// by prefetch, dropped by a frame, and composed again).
#[test]
fn review_grid_scrolling_one_way_composes_each_item_once() {
    for spans in [false, true] {
        TIMES.with(|times| times.borrow_mut().clear());
        let state = LazyGridState::new(0, 0);
        let given = *state;
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            LazyVerticalGrid!(
                GridCells::Fixed(3),
                modifier = Modifier.size((300.dp(), 200.dp())),
                state = given,
                move |scope: LazyGridScope| {
                    scope.items_with_span(
                        5000,
                        move |line: LazyGridItemSpanScope, index| {
                            GridItemSpan(if spans && index % 4 == 0 {
                                line.max_line_span()
                            } else {
                                1
                            })
                        },
                        |index| index % 4 == 0,
                        Tracked,
                    );
                }
            );
        });
        for _ in 0..300 {
            state.dispatch_raw_delta(9.0);
            ui.wait_for_idle();
            ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
        }
        let times = TIMES.with(|times| times.borrow().clone());
        let mut again: Vec<(usize, usize)> =
            times.into_iter().filter(|(_, times)| *times > 1).collect();
        again.sort_unstable();
        assert!(
            again.is_empty(),
            "spans {spans}: composed again {:?}",
            &again[..again.len().min(20)]
        );
    }
}

/// Prefetching a grid (spans, headers over a line) asks for no frame, draws nothing new, and
/// leaves the position alone.
#[test]
fn review_grid_prefetch_asks_for_no_frame_and_draws_nothing() {
    let state = LazyGridState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        LazyVerticalGrid!(
            GridCells::Fixed(3),
            modifier = Modifier.size((300.dp(), 200.dp())),
            state = given,
            |scope: LazyGridScope| {
                scope.items_with_span(
                    5000,
                    |line: LazyGridItemSpanScope, index| {
                        GridItemSpan(if index % 7 == 0 { line.max_line_span() } else { 1 })
                    },
                    |index| index % 7 == 0,
                    |index| {
                        Box!(
                            modifier = Modifier
                                .height((20 + (index % 7) as i32 * 3).dp())
                                .test_tag(format!("{index}"))
                        );
                    },
                );
            }
        );
    });
    for step in 0..200 {
        state.dispatch_raw_delta(if step < 150 { 13.0 } else { -13.0 });
        ui.wait_for_idle();
        let position = (state.first_visible_item_index(), state.first_visible_item_scroll_offset());
        let display = ui.dump_display_list();
        ui.host().prefetch(std::time::Instant::now() + std::time::Duration::from_secs(60));
        assert!(!ui.host().needs_frame(), "step {step}: prefetch asked for a frame");
        assert_eq!(ui.dump_display_list(), display, "step {step}");
        assert_eq!(
            (state.first_visible_item_index(), state.first_visible_item_scroll_offset()),
            position
        );
    }
}
