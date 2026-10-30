//! Adversarial review of 33e6428 (D575-D579).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::Optimizations;
use compose_ui::{IntSize, UnitExt};

thread_local! {
    static LIVE: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static PATTERN: Cell<u32> = const { Cell::new(0) };
    static TICKS: Cell<u64> = const { Cell::new(0) };
}

fn live_items() -> Vec<usize> {
    let mut items: Vec<usize> = LIVE.with(|live| live.borrow().iter().copied().collect());
    items.sort_unstable();
    items
}

fn later() -> Instant {
    Instant::now() + Duration::from_secs(60)
}

fn height(pattern: u32, index: usize) -> f32 {
    match pattern {
        0 => 20.0,
        1 => {
            if index.is_multiple_of(4) {
                80.0
            } else {
                10.0
            }
        }
        2 => {
            if index.is_multiple_of(7) {
                120.0
            } else {
                15.0
            }
        }
        3 => 5.0 + ((index * 37) % 90) as f32,
        4 => {
            if index < 10 {
                10.0
            } else {
                60.0
            }
        }
        _ => {
            if (index / 5).is_multiple_of(2) {
                10.0
            } else {
                70.0
            }
        }
    }
}

#[composable]
fn Row(index: usize) {
    DisposableEffect(index, move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(index));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&index));
        })
    });
    let h = height(PATTERN.get(), index);
    Box!(modifier = Modifier.size((100.dp(), h.dp())));
}

fn list(pattern: u32) -> (ComposeTestRule, Owned<LazyListState>) {
    LIVE.with(|live| live.borrow_mut().clear());
    PATTERN.set(pattern);
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, Row);
            }
        );
    });
    (ui, state)
}

fn idle(ui: &mut ComposeTestRule) {
    for _ in 0..10 {
        if !ui.host().prefetch(later()) {
            break;
        }
    }
}

/// After a forward scroll, a measurement (what the window's bound before the scroll kept goes with
/// it, D894) and all the idle time they want, nothing before the first visible item stays composed
/// at parity.
#[test]
fn x_items_that_left_are_deactivated_once_idle() {
    let mut failures = Vec::new();
    for pattern in 0..6 {
        for step in [3.0f32, 7.0, 15.0, 33.0, 64.0] {
            let (mut ui, state) = list(pattern);
            for frame in 0..40 {
                state.dispatch_raw_delta(step);
                ui.wait_for_idle();
                idle(&mut ui);
                state.force_remeasure();
                ui.wait_for_idle();
                idle(&mut ui);
                let first = state.first_visible_item_index();
                let live = live_items();
                let stale: Vec<usize> = live.iter().copied().filter(|i| *i < first).collect();
                if !stale.is_empty() {
                    failures.push(format!(
                        "pattern {pattern} step {step} frame {frame}: first {first}, stale {stale:?}, live {live:?}, prefetch work {}",
                        ui.host().has_prefetch_work()
                    ));
                    break;
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

thread_local! {
    static ANIMATE: Cell<bool> = const { Cell::new(false) };
}

#[composable]
fn AnimatedRow(index: usize) {
    DisposableEffect(index, move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(index));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&index));
        })
    });
    if index == 0 {
        LaunchedEffect((), async move {
            loop {
                with_frame_nanos(|_| ()).await;
                if ANIMATE.get() {
                    TICKS.set(TICKS.get() + 1);
                }
            }
        });
    }
    let h = height(5, index);
    Box!(modifier = Modifier.size((100.dp(), h.dp())));
}

/// Row 0 runs an infinite animation (a progress indicator). Scrolled out (pattern 5, 64 px: a
/// case where the request's item was composed already, so its composition deactivated
/// nothing), it is deactivated in the next idle time, and its animation stops.
#[test]
fn x_an_infinite_animation_in_a_row_that_left_runs_forever() {
    LIVE.with(|live| live.borrow_mut().clear());
    TICKS.set(0);
    ANIMATE.set(false);
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, AnimatedRow);
            }
        );
    });
    for _ in 0..3 {
        ui.host().frame();
    }
    idle(&mut ui);
    state.dispatch_raw_delta(64.0);
    for _ in 0..3 {
        ui.host().frame();
    }
    idle(&mut ui);
    assert!(state.first_visible_item_index() > 0);
    // The window's bound before the scroll kept row 0: it stays, as Compose's `keepAroundItems`,
    // until the list is measured again (D894). Then it goes, and its animation stops.
    assert!(live_items().contains(&0));
    state.force_remeasure();
    for _ in 0..3 {
        ui.host().frame();
    }
    idle(&mut ui);
    ANIMATE.set(true);
    for _ in 0..200 {
        ui.host().frame();
        idle(&mut ui);
    }
    let live = live_items();
    let needs = ui.host().needs_frame();
    let work = ui.host().has_prefetch_work();
    assert!(
        !live.contains(&0) && TICKS.get() == 0,
        "row 0 still live: {live:?}, ticks {}, needs_frame {needs}, prefetch work {work}",
        TICKS.get()
    );
}

thread_local! {
    static CLICKED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// The rows that left (0 to 4), waiting for the next composition to deactivate them: out of
/// semantics, hit testing and drawing.
#[test]
fn x_stuck_rows_are_out_of_semantics_hits_and_drawing() {
    LIVE.with(|live| live.borrow_mut().clear());
    CLICKED.with(|c| c.borrow_mut().clear());
    use_optimizations(Optimizations::NONE);
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| {
                scope.items(1000, |index| {
                    DisposableEffect(index, move |scope| {
                        LIVE.with(|live| live.borrow_mut().insert(index));
                        scope.on_dispose(move || {
                            LIVE.with(|live| live.borrow_mut().remove(&index));
                        })
                    });
                    let h = height(5, index);
                    Box!(
                        modifier = Modifier
                            .size((100.dp(), h.dp()))
                            .test_tag(format!("row {index}"))
                            .clickable(move || CLICKED.with(|c| c.borrow_mut().push(index)))
                    );
                });
            }
        );
    });
    idle(&mut ui);
    state.dispatch_raw_delta(64.0);
    ui.wait_for_idle();
    // Before the idle time that deactivates them (D575): rows 0 to 4 wait, composed.
    let live = live_items();
    assert!(live.contains(&0), "precondition: row 0 waiting: {live:?}");
    ui.on_node_with_tag("row 0").assert_does_not_exist();
    ui.on_node_with_tag("row 4").assert_does_not_exist();
    for y in [2.0, 5.0, 25.0, 45.0] {
        ui.host().send_pointer(compose_ui::PointerEvent::press(compose_ui::Offset::new(50.0, y)));
        ui.host().send_pointer(compose_ui::PointerEvent::release(compose_ui::Offset::new(50.0, y)));
        ui.wait_for_idle();
    }
    let clicked = CLICKED.with(|c| c.borrow().clone());
    assert!(clicked.iter().all(|i| *i >= 5), "{clicked:?}");
    let display = ui.dump_display_list();
    let tree = ui.dump_tree();
    eprintln!("DISPLAY\n{display}\nTREE\n{tree}");
}

#[composable]
fn GridCell(index: usize) {
    DisposableEffect(index, move |scope| {
        LIVE.with(|live| live.borrow_mut().insert(index));
        scope.on_dispose(move || {
            LIVE.with(|live| live.borrow_mut().remove(&index));
        })
    });
    let h = height(PATTERN.get(), index / 3);
    Box!(modifier = Modifier.size((100.dp(), h.dp())));
}

#[test]
fn x_grid_items_that_left_are_deactivated_once_idle() {
    let mut failures = Vec::new();
    for pattern in 0..6 {
        for step in [3.0f32, 7.0, 15.0, 33.0, 64.0] {
            LIVE.with(|live| live.borrow_mut().clear());
            PATTERN.set(pattern);
            use_optimizations(Optimizations::NONE);
            let state = LazyGridState::new(0, 0);
            let given = *state;
            let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
                LazyVerticalGrid!(
                    GridCells::Fixed(3),
                    modifier = Modifier.size((300.dp(), 100.dp())),
                    state = given,
                    |scope: LazyGridScope| {
                        scope.items(900, GridCell);
                    }
                );
            });
            for frame in 0..40 {
                state.dispatch_raw_delta(step);
                ui.wait_for_idle();
                idle(&mut ui);
                state.force_remeasure();
                ui.wait_for_idle();
                idle(&mut ui);
                let first = state.first_visible_item_index();
                let live = live_items();
                let stale: Vec<usize> = live.iter().copied().filter(|i| *i < first).collect();
                if !stale.is_empty() {
                    failures.push(format!(
                        "pattern {pattern} step {step} frame {frame}: first {first}, stale {stale:?}, live {live:?}"
                    ));
                    break;
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

thread_local! {
    static SWEEP_PATTERN: RefCell<Vec<f32>> = const { RefCell::new(Vec::new()) };
}

fn sweep_height(index: usize) -> f32 {
    SWEEP_PATTERN.with(|p| {
        let p = p.borrow();
        p[index % p.len()]
    })
}

/// A frame's scroll lands where asked (no short scroll), both ways, over mixed sizes.
#[test]
fn x_a_frames_scroll_lands_where_asked() {
    let patterns: [&[f32]; 5] = [
        &[300.0, 1.0],
        &[300.0, 8.0, 1.0],
        &[10.0, 10.0, 10.0, 200.0],
        &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 300.0],
        &[50.0, 2.0, 2.0, 2.0, 2.0, 120.0],
    ];
    let mut failures = Vec::new();
    let mut runs = 0;
    for pattern in patterns {
        SWEEP_PATTERN.with(|p| *p.borrow_mut() = pattern.to_vec());
        for start in (0..1300).step_by(211) {
            for by in (-2400i32..2400).step_by(173) {
                use_optimizations(Optimizations::NONE);
                let state = LazyListState::new(0, 0);
                let given = *state;
                let mut ui = ComposeTestRule::sized(IntSize::new(100, 1000), move || {
                    LazyColumn!(
                        modifier = Modifier.size((100.dp(), 900.dp())),
                        state = given,
                        |scope: LazyListScope| {
                            scope.items(20000, |index| {
                                let h = sweep_height(index);
                                Box!(modifier = Modifier.size((100.dp(), h.dp())));
                            });
                        }
                    );
                });
                state.dispatch_raw_delta(5000.0 + start as f32);
                ui.wait_for_idle();
                idle(&mut ui);
                ui.wait_for_idle();
                let absolute = |state: &LazyListState| {
                    let index = state.first_visible_item_index();
                    (0..index).map(sweep_height).sum::<f32>()
                        + state.first_visible_item_scroll_offset() as f32
                };
                let before = absolute(&state);
                state.dispatch_raw_delta(by as f32);
                ui.host().frame();
                let after = absolute(&state);
                runs += 1;
                if (after - before - by as f32).abs() > 1.5 {
                    failures.push(format!(
                        "{pattern:?} start {start} by {by}: moved {}",
                        after - before
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} of {runs}:\n{}", failures.len(), failures.join("\n"));
}

fn focus_search_grid(cells: usize, target: Option<usize>) -> ComposeTestRule {
    LIVE.with(|live| live.borrow_mut().clear());
    use_optimizations(Optimizations::NONE);
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 300), move || {
        Column!({
            LazyVerticalGrid!(
                GridCells::Fixed(3),
                modifier = Modifier.size((300.dp(), 100.dp())),
                |scope: LazyGridScope| {
                    scope.items(cells, move |index| {
                        DisposableEffect(index, move |scope| {
                            LIVE.with(|live| live.borrow_mut().insert(index));
                            scope.on_dispose(move || {
                                LIVE.with(|live| live.borrow_mut().remove(&index));
                            })
                        });
                        let modifier =
                            Modifier.size((100.dp(), 20.dp())).test_tag(format!("cell {index}"));
                        let modifier = if index == 0 || Some(index) == target {
                            modifier.clickable(|| {})
                        } else {
                            modifier
                        };
                        Box!(modifier = modifier);
                    });
                }
            );
            Box!(modifier = Modifier.size((100.dp(), 20.dp())).clickable(|| {}).test_tag("after"));
        });
    });
    ui.on_node_with_tag("cell 0").request_focus();
    ui.wait_for_idle();
    ui.press_key(compose_ui::Key::Tab);
    for _ in 0..60 {
        ui.host().frame();
    }
    ui.wait_for_idle();
    idle(&mut ui);
    ui.wait_for_idle();
    ui
}

/// D577 (2) for grids: a focus search's lines let go once it ended?
#[test]
fn x_a_grids_focus_search_lets_the_lines_it_laid_out_go() {
    let mut ui = focus_search_grid(600, Some(150));
    ui.on_node_with_tag("cell 150").assert_is_focused();
    let found = live_items();
    let mut ui = focus_search_grid(123, None);
    ui.on_node_with_tag("after").assert_is_focused();
    let left = live_items();
    assert!(
        found.len() <= 24 && left.len() <= 24,
        "found: {} {found:?}\nleft: {} {left:?}",
        found.len(),
        left.len()
    );
}

/// The last item (a grid: line) showing in a 100 px viewport when the list starts at `first`,
/// `offset` px in, each sized by `unit`.
fn last_visible(first: usize, offset: f32, unit: impl Fn(usize) -> f32) -> usize {
    let mut end = -offset;
    let mut index = first;
    loop {
        end += unit(index);
        if end >= 100.0 {
            return index;
        }
        index += 1;
    }
}

/// After a scroll and the idle time it wants, a still screen asks for nothing more (no frame,
/// no prefetch), and what stays composed is at most what Compose's would hold: the items in
/// view and those in the ahead window (at most half the viewport, its last item over it), for mixed
/// item sizes (a grid: lines).
#[test]
fn x_a_still_screen_goes_idle_holding_what_compose_would() {
    let mut failures = Vec::new();
    for pattern in 0..6 {
        for step in [3.0f32, 7.0, 15.0, 33.0, 64.0] {
            // A list.
            let (mut ui, state) = list(pattern);
            for _ in 0..12 {
                state.dispatch_raw_delta(step);
                ui.wait_for_idle();
                idle(&mut ui);
            }
            state.force_remeasure();
            ui.wait_for_idle();
            idle(&mut ui);
            let first = state.first_visible_item_index();
            let offset = state.first_visible_item_scroll_offset() as f32;
            let last = last_visible(first, offset, |index| height(pattern, index));
            let live = live_items();
            if ui.host().needs_frame() || ui.host().has_prefetch_work() {
                failures.push(format!("list {pattern} {step}: not idle"));
            }
            let window: f32 =
                (last + 1..*live.last().unwrap()).map(|index| height(pattern, index)).sum();
            if live.first() != Some(&first) || window >= 50.0 {
                failures
                    .push(format!("list {pattern} {step}: shows {first}..={last}, holds {live:?}"));
            }
            // A grid of lines sized by `pattern`.
            LIVE.with(|live| live.borrow_mut().clear());
            use_optimizations(Optimizations::NONE);
            let state = LazyGridState::new(0, 0);
            let given = *state;
            let mut ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
                LazyVerticalGrid!(
                    GridCells::Fixed(3),
                    modifier = Modifier.size((300.dp(), 100.dp())),
                    state = given,
                    |scope: LazyGridScope| {
                        scope.items(900, GridCell);
                    }
                );
            });
            for _ in 0..12 {
                state.dispatch_raw_delta(step);
                ui.wait_for_idle();
                idle(&mut ui);
            }
            state.force_remeasure();
            ui.wait_for_idle();
            idle(&mut ui);
            let first_line = state.first_visible_item_index() / 3;
            let offset = state.first_visible_item_scroll_offset() as f32;
            let last_line = last_visible(first_line, offset, |line| height(pattern, line));
            let live = live_items();
            if ui.host().needs_frame() || ui.host().has_prefetch_work() {
                failures.push(format!("grid {pattern} {step}: not idle"));
            }
            let window: f32 =
                (last_line + 1..*live.last().unwrap() / 3).map(|line| height(pattern, line)).sum();
            if live.first() != Some(&(first_line * 3)) || window >= 50.0 {
                failures.push(format!(
                    "grid {pattern} {step}: shows lines {first_line}..={last_line}, holds {live:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A first layout composes about what it shows, before any idle time: a list's and a grid's
/// estimates ask for a few at a time while they know few sizes and double from there (D472,
/// D487, D578), so neither composes more than about twice what it shows (the grid composed 15
/// lines for 6 before D578). What it composed beyond the next one goes in the next idle time
/// (`x_a_still_screen_goes_idle_holding_what_compose_would`).
#[test]
fn x_a_first_layout_composes_about_what_it_shows() {
    let mut failures = Vec::new();
    for pattern in 0..6 {
        let (_ui, _state) = list(pattern);
        let shown = last_visible(0, 0.0, |index| height(pattern, index));
        let live = live_items();
        if *live.last().unwrap() > 2 * shown + 2 {
            failures.push(format!("list {pattern}: shows 0..={shown}, holds {live:?}"));
        }
        LIVE.with(|live| live.borrow_mut().clear());
        PATTERN.set(pattern);
        use_optimizations(Optimizations::NONE);
        let _ui = ComposeTestRule::sized(IntSize::new(400, 200), move || {
            LazyVerticalGrid!(
                GridCells::Fixed(3),
                modifier = Modifier.size((300.dp(), 100.dp())),
                |scope: LazyGridScope| {
                    scope.items(900, GridCell);
                }
            );
        });
        let lines = last_visible(0, 0.0, |line| height(pattern, line));
        let live = live_items();
        if *live.last().unwrap() >= (2 * lines + 3) * 3 {
            failures.push(format!("grid {pattern}: shows lines 0..={lines}, holds {live:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
