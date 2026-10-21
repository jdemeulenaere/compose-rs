//! Headless UI frames (docs/09-testing.md targets): a counter click under 1 ms, and toggling
//! one of 1,000 todos under 4 ms frame p99. Each iteration is a press and release on a node
//! followed by a frame (recompose, measure, draw).

use std::time::{Duration, Instant};

use compose::test::*;
use compose::ui::{IntSize, PointerEvent, Role};
use criterion::{Criterion, criterion_group, criterion_main};

/// Clicks the center of `matcher`'s `index`th node, then runs frames until idle.
fn click_frame(ui: &mut ComposeTestRule, matcher: &Matcher, index: usize) -> Duration {
    let center = ui.on_all_nodes(matcher.clone(), index).fetch_semantics_node().center();
    let start = Instant::now();
    ui.host().send_pointer(PointerEvent::press(center));
    ui.host().send_pointer(PointerEvent::release(center));
    ui.wait_for_idle();
    start.elapsed()
}

fn report_p99(name: &str, mut samples: Vec<Duration>) {
    samples.sort();
    let p99 = samples[samples.len() * 99 / 100];
    let median = samples[samples.len() / 2];
    println!("{name}: median {median:?}, p99 {p99:?} over {} frames", samples.len());
}

fn ui(criterion: &mut Criterion) {
    let mut counter = compose_test(counter::Counter);
    let count = has_text_containing("Count");
    criterion.bench_function("counter click frame", |bench| {
        bench.iter_custom(|iterations| {
            (0..iterations).map(|_| click_frame(&mut counter, &count, 0)).sum()
        })
    });

    let mut todos = ComposeTestRule::sized(IntSize::new(400, 100_000), || todo::TodoAppWith(1000));
    let checkbox = has_role(Role::Checkbox);
    criterion.bench_function("toggle 1 of 1000 todos frame", |bench| {
        bench.iter_custom(|iterations| {
            (0..iterations).map(|_| click_frame(&mut todos, &checkbox, 500)).sum()
        })
    });
    // The frame p99 the roadmap targets.
    let samples = (0..500).map(|_| click_frame(&mut todos, &checkbox, 500)).collect();
    report_p99("toggle 1 of 1000 todos", samples);
}

criterion_group!(benches, ui);
criterion_main!(benches);
