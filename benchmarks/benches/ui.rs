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

/// A toggle in a 480 × 640 dp window at 2×, as the desktop host does it: dispatch, recompose,
/// measure, record and render with Skia's CPU raster backend. Input to rendered pixels, without
/// the window system's present.
fn desktop_frame(criterion: &mut Criterion) {
    use compose::ui::{Color, Density, platform::Renderer};
    let host = compose::ui::platform::UiHost::new(
        IntSize::new(960, 1280),
        Density::new(2.0),
        Box::new(compose_skia::SkiaTextMeasurer::new()),
    );
    let mut ui = ComposeTestRule::with_host(host, || todo::TodoAppWith(1000));
    let mut renderer = compose_skia::RasterRenderer::new(960, 1280);
    let checkbox = has_role(Role::Checkbox);
    let mut frame = |ui: &mut ComposeTestRule| {
        let elapsed = click_frame(ui, &checkbox, 3);
        let start = Instant::now();
        ui.host().with_tree(|tree| {
            renderer.render(tree.display_list(), Color::WHITE, 1.0, tree.text_measurer_ref())
        });
        elapsed + start.elapsed()
    };
    criterion.bench_function("toggle 1 of 1000 todos, rendered at 2x", |bench| {
        bench.iter_custom(|iterations| (0..iterations).map(|_| frame(&mut ui)).sum())
    });
    let samples = (0..300).map(|_| frame(&mut ui)).collect();
    report_p99("toggle 1 of 1000 todos, rendered at 2x", samples);
}

/// An animation frame of the feed's animated header (a spinner drawn again, a pulsing dot's alpha
/// and a turning star's rotation, each read while drawing) over the posts a lazy list shows, as
/// docs/12's `feed_animated` on a phone, and in a window four phones tall (four times the posts
/// on screen, the same header): the frame's work besides the header (drawing the posts) is what
/// retained drawing per repaint boundary skips.
fn animation_frame(criterion: &mut Criterion) {
    use compose::runtime::platform;
    use compose::ui::{
        Density,
        platform::{FixedTextMeasurer, UiHost},
    };
    for (name, height) in [("phone", 2400), ("four phones tall", 9600)] {
        platform::set_manual_clock(platform::now_nanos());
        let mut host = UiHost::new(
            IntSize::new(1080, height),
            Density::new(2.625),
            Box::new(FixedTextMeasurer::new()),
        );
        host.set_content(compose_rs_feed::FeedAnimated);
        let frame = |host: &mut UiHost| {
            platform::advance_clock(Duration::from_millis(16));
            let start = Instant::now();
            host.frame();
            start.elapsed()
        };
        for _ in 0..10 {
            frame(&mut host);
        }
        platform::advance_clock(Duration::from_millis(16));
        assert!(host.frame().drew, "an animation frame draws");
        let title = format!("feed_animated frame, {name}");
        criterion.bench_function(&title, |bench| {
            bench.iter_custom(|iterations| (0..iterations).map(|_| frame(&mut host)).sum())
        });
        let samples = (0..500).map(|_| frame(&mut host)).collect();
        report_p99(&title, samples);
    }
}

criterion_group!(benches, ui, desktop_frame, animation_frame);
criterion_main!(benches);
