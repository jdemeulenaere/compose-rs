//! Scrolling lazy lists through many items: what an item coming into view costs (composing it,
//! creating or reusing its nodes, measuring it), in the feed and the stress list of
//! `examples/feed` and a photo grid (`Gallery`, below), on a phone-sized host (1080 × 2400 px
//! at 2.625) with a fixed text measurer.
//! There's no prefetch (the platform's idle time runs it): each frame composes the items it
//! scrolls to, so a frame's time is mostly theirs.
//!
//! Besides criterion's frame time, each screen prints the time per item composed: the frames'
//! total over the items whose body ran (`LazyItem`, a grid's `Tile`), scrolling through `ITEMS`
//! items.

#![allow(non_snake_case)]

use std::time::{Duration, Instant};

use compose::foundation::layout::*;
use compose::foundation::*;
use compose::runtime::{composable, mutable_state_of, platform, remember};
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
    host.set_content(content);
    host.settle(10);
    host
}

/// A frame scrolling the list by `pixels` (the wheel, at the screen's center), timed.
fn scroll_frame(host: &mut UiHost, pixels: f32) -> Duration {
    platform::advance_clock(Duration::from_millis(16));
    let start = Instant::now();
    host.send_pointer(PointerEvent::scroll(Offset::new(540.0, 1200.0), Offset::new(0.0, pixels)));
    host.frame();
    start.elapsed()
}

/// Back to the top, untimed.
fn rewind(host: &mut UiHost) {
    scroll_frame(host, -1.0e7);
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

criterion_group!(benches, scroll_through);
criterion_main!(benches);
