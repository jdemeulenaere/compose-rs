//! Scrolling lazy lists through many items: what an item coming into view costs (composing it,
//! creating or reusing its nodes, measuring it), in the feed and the stress list of
//! `examples/feed` and a photo grid (`Gallery`, below), on a phone-sized host (1080 × 2400 px
//! at 2.625) with a fixed text measurer.
//! There's no prefetch (the platform's idle time runs it): each frame composes the items it
//! scrolls to, so a frame's time is mostly theirs. The hosts run at parity (`none`, Compose's
//! work) unless `LAZY_BENCH_OPTIMIZATIONS` names a set (D610).
//!
//! Besides criterion's frame time, each screen prints the time per item composed: the frames'
//! total over the items whose body ran (`LazyItem`, a grid's `Tile`), scrolling through `ITEMS`
//! items.

#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::runtime::{MutableState, composable, mutable_state_of, platform, remember};
use compose::ui::platform::{FixedTextMeasurer, UiHost};
use compose::ui::{Color, Density, IntSize, Modifier, Offset, PointerEvent, UnitExt};
use criterion::{Criterion, criterion_group, criterion_main};

/// The items scrolled through for the time per item.
const ITEMS: u64 = 300;

/// What a frame scrolls, in pixels: about half a feed post.
const STEP: f32 = 200.0;

/// A phone-sized host showing `content`, laid out.
fn host(content: fn()) -> UiHost {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1080, 2400),
        Density::new(2.625),
        Box::new(FixedTextMeasurer::new()),
    );
    // `LAZY_BENCH_OPTIMIZATIONS`: compose-rs's optimizations (D520); without it parity, none,
    // Compose's work, not the hosts' default (the recommended set, D610).
    host.set_optimizations(match std::env::var("LAZY_BENCH_OPTIMIZATIONS") {
        Ok(list) => {
            compose::ui::platform::Optimizations::parse(&list).expect("known optimizations")
        }
        Err(_) => compose::ui::platform::Optimizations::NONE,
    });
    host.set_content(content);
    host.settle(10);
    host
}

/// A frame scrolling the list by `pixels` (a wheel's turn, at the screen's center), timed. Turns
/// each frame join one scroll, a large one moving by that much a frame once its animation has
/// caught up.
fn scroll_frame(host: &mut UiHost, pixels: f32) -> Duration {
    platform::advance_clock(Duration::from_millis(16));
    let start = Instant::now();
    host.send_pointer(PointerEvent::scroll(Offset::new(540.0, 1200.0), Offset::new(0.0, pixels)));
    host.frame();
    start.elapsed()
}

/// Back to the top, untimed: a wheel's turn animates over 100 ms, and its scroll stays held for
/// 50 ms more.
fn rewind(host: &mut UiHost) {
    scroll_frame(host, -1.0e7);
    for _ in 0..12 {
        platform::advance_clock(Duration::from_millis(16));
        host.frame();
    }
    host.settle(10);
}

fn report(name: &str, mut samples: Vec<Duration>) {
    samples.sort();
    let percentile = |p: usize| samples[(samples.len() * p / 100).min(samples.len() - 1)];
    println!(
        "{name}: frame median {:?}, p90 {:?}, p99 {:?} over {} frames",
        percentile(50),
        percentile(90),
        percentile(99),
        samples.len()
    );
}

/// The tiles in the grid.
const TILES: usize = 10_000;

/// A photo grid: 3 columns of tiles (a picture, a title and a count, clickable) 4 dp apart.
#[composable]
fn Gallery() {
    LazyVerticalGrid!(
        GridCells::Adaptive(120.dp()),
        modifier = Modifier.fill_max_size(),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        horizontal_arrangement = Arrangement::spaced_by(4.dp()),
        |scope: LazyGridScope| {
            scope.items_keyed(TILES, |index| index, Tile);
        }
    );
}

#[composable]
fn Tile(index: usize) {
    let liked = remember(|| mutable_state_of(false));
    let color = Color::from_argb(0xFF00_0000 | ((index as u32).wrapping_mul(2_654_435_761) >> 8));
    Column!(
        modifier =
            Modifier.fill_max_width().height(140.dp()).clickable(move || liked.update(|on| !on)),
        {
            Box!(modifier = Modifier.fill_max_width().height(100.dp()).background(color));
            BasicText!("Photo {index}");
            BasicText!("{} likes", index % 97 + usize::from(liked.get()));
        }
    );
}

/// The items of the grids a caller recomposes, far into which they're scrolled.
const LONG_GRID: usize = 100_000;
/// Items between two headers of those grids.
const SECTION: usize = 20;

thread_local! {
    /// What the composable of `recompose_a_long_grid` reads at the grid's call site.
    static TICK: std::cell::Cell<Option<MutableState<u64>>> = const { std::cell::Cell::new(None) };
}

/// A grid of `LONG_GRID` items, near the last of them, whose content is new with each recomposition
/// of its caller, as a closure capturing anything is: with headers, a span per item to walk again.
#[composable]
fn LongGrid(headers: bool) {
    let tick = remember(|| mutable_state_of(0u64));
    TICK.with(|cell| cell.set(Some(tick)));
    let tick = tick.get();
    let state = remember_lazy_grid_state(LONG_GRID - 100, 0);
    LazyVerticalGrid!(
        GridCells::Fixed(3),
        modifier = Modifier.fill_max_size(),
        state = state,
        move |scope: LazyGridScope| {
            if headers {
                for _ in 0..LONG_GRID / SECTION {
                    scope.sticky_header(|_| Spacer(Modifier.height(20.dp())));
                    scope.items(SECTION - 1, move |_| {
                        let _ = tick;
                        Spacer(Modifier.height(100.dp()));
                    });
                }
            } else {
                scope.items(LONG_GRID, move |_| {
                    let _ = tick;
                    Spacer(Modifier.height(100.dp()));
                });
            }
        }
    );
}

#[composable]
fn LongGridWithHeaders() {
    LongGrid(true);
}

#[composable]
fn LongGridOfItems() {
    LongGrid(false);
}

/// A caller recomposing a long grid far from its start: the spans walked again, the items in view
/// composed anew (D1454). The grid without headers has no spans to walk: the difference is the walk.
fn recompose_a_long_grid(criterion: &mut Criterion) {
    let screens: [(&str, fn()); 2] =
        [("headers", LongGridWithHeaders), ("no headers", LongGridOfItems)];
    for (name, content) in screens {
        let mut host = host(content);
        let tick = TICK.with(|cell| cell.get()).expect("composed");
        criterion.bench_function(&format!("recompose a long grid, {name}, frame"), |bench| {
            bench.iter_custom(|iterations| {
                (0..iterations)
                    .map(|_| {
                        platform::advance_clock(Duration::from_millis(16));
                        tick.update(|value| value + 1);
                        let start = Instant::now();
                        host.frame();
                        start.elapsed()
                    })
                    .sum()
            })
        });
        // A scroll there, a header sticking: a window's walk over the lines before it (D1451).
        let mut frame = 0u64;
        criterion.bench_function(&format!("scroll a long grid, {name}, frame"), |bench| {
            bench.iter_custom(|iterations| {
                (0..iterations)
                    .map(|_| {
                        frame += 1;
                        let way = if frame % 80 < 40 { 1.0 } else { -1.0 };
                        scroll_frame(&mut host, way * STEP)
                    })
                    .sum()
            })
        });
    }
}

fn scroll_through(criterion: &mut Criterion) {
    // Each screen, and the composable counted as an item.
    let screens: [(&str, fn(), &str); 3] = [
        ("feed", compose_rs_feed::Feed, "LazyItem"),
        ("stress", compose_rs_feed::stress::StressScroll, "LazyItem"),
        ("grid", Gallery, "Tile"),
    ];
    for (name, content, item) in screens {
        let mut host = host(content);
        // Warm up: the pool of reusable items fills, caches warm.
        for _ in 0..100 {
            scroll_frame(&mut host, STEP);
        }
        rewind(&mut host);
        // The time per item: frames until `ITEMS` items were composed.
        platform::track_recompositions(true);
        let before = platform::recomposition_counts();
        let mut total = Duration::ZERO;
        let mut samples = Vec::new();
        let mut composed = 0;
        while composed < ITEMS {
            let elapsed = scroll_frame(&mut host, STEP);
            total += elapsed;
            samples.push(elapsed);
            composed = platform::recomposition_counts().since(&before).scopes_named(item);
        }
        platform::track_recompositions(false);
        println!(
            "{name}: {composed} items composed in {} frames, {:?} per item",
            samples.len(),
            total / composed as u32
        );
        report(name, samples);
        rewind(&mut host);
        let mut frames = 0;
        criterion.bench_function(&format!("scroll the {name} list {STEP} px, frame"), |bench| {
            bench.iter_custom(|iterations| {
                (0..iterations)
                    .map(|_| {
                        frames += 1;
                        // Back to the top before the end, untimed.
                        if frames % 1000 == 0 {
                            rewind(&mut host);
                        }
                        scroll_frame(&mut host, STEP)
                    })
                    .sum()
            })
        });
        // A frame that composes nothing: a pixel down or back up, in the middle of the list.
        rewind(&mut host);
        for _ in 0..30 {
            scroll_frame(&mut host, STEP);
        }
        let mut nudges = 0u64;
        let mut nudge = move |host: &mut UiHost| {
            nudges += 1;
            scroll_frame(host, if nudges.is_multiple_of(2) { 1.0 } else { -1.0 })
        };
        report(&format!("{name}, a pixel"), (0..500).map(|_| nudge(&mut host)).collect());
        criterion.bench_function(&format!("scroll the {name} list a pixel, frame"), |bench| {
            bench.iter_custom(|iterations| (0..iterations).map(|_| nudge(&mut host)).sum())
        });
    }
}

/// The stress list's items prefetched in idle times of a given length, as Compose's pausable
/// composition splits an item's composition across them (D590): per item a request composed,
/// the time spent in prefetch (composing, applying, measuring, the strategy) and the idle times
/// it took. `one idle time` is a paused composition never short of time (one resume, then apply).
fn prefetch_in_pauses(criterion: &mut Criterion) {
    let budgets: [(&str, Option<Duration>); 4] = [
        ("one idle time", None),
        ("20 µs idle times", Some(Duration::from_micros(20))),
        ("12 µs idle times", Some(Duration::from_micros(12))),
        ("8 µs idle times", Some(Duration::from_micros(8))),
    ];
    for (label, budget) in budgets {
        let mut host = host(compose_rs_feed::stress::StressScroll);
        // A frame scrolling a third of a screen, then idle times until its requests are done.
        let step = |host: &mut UiHost| -> (Duration, u64, u64) {
            scroll_frame(host, 300.0);
            let before = platform::recomposition_counts();
            let (mut spent, mut idle_times) = (Duration::ZERO, 0u64);
            while host.has_prefetch_work() && idle_times < 100_000 {
                if host.has_pending_work() {
                    host.frame();
                }
                let start = Instant::now();
                let deadline = start + budget.unwrap_or(Duration::from_secs(1));
                // A request whose stretch doesn't fit these idle times would wait for ever here,
                // up to the cap, which made the shortest budgets' times bimodal (D899): no frame for
                // two intervals, the third has no limit for the first request, as Compose's
                // `isFrameIdle`.
                host.prefetch_with(deadline, idle_times % 3 == 2);
                spent += start.elapsed();
                idle_times += 1;
            }
            let items = platform::recomposition_counts().since(&before).scopes_named("LazyItem");
            (spent, idle_times, items)
        };
        platform::track_recompositions(true);
        // Warm up: the averages learn, the pool of reusable items fills.
        for _ in 0..60 {
            step(&mut host);
        }
        rewind(&mut host);
        let (mut spent, mut idle_times, mut items) = (Duration::ZERO, 0, 0);
        for _ in 0..120 {
            let (time, idle, composed) = step(&mut host);
            spent += time;
            idle_times += idle;
            items += composed;
        }
        platform::track_recompositions(false);
        let items = items.max(1);
        println!(
            "stress prefetch, {label}: {items} items composed by requests, {:?} in prefetch and \
             {:.1} idle times per item",
            spent / items as u32,
            idle_times as f64 / items as f64
        );
        rewind(&mut host);
        criterion.bench_function(&format!("prefetch the stress list's items, {label}"), |bench| {
            bench.iter_custom(|iterations| {
                (0..iterations)
                    .map(|index| {
                        if index % 200 == 199 {
                            rewind(&mut host);
                        }
                        step(&mut host).0
                    })
                    .sum()
            })
        });
    }
}

criterion_group!(benches, scroll_through, prefetch_in_pauses, recompose_a_long_grid);
criterion_main!(benches);
