//! Frames whose root width changes every frame, as a window being resized (D550, D551): the
//! resize screen's grid (40 labels sharing the width) and its 12 nested levels of labels, laid out
//! with real text (SkParagraph, the system's fonts) on a phone-sized host whose width moves 3 px
//! a frame between half and all of 1008 px. Each is measured following Compose's rule (every new
//! width lays every text out again) and with D551's optimization (a text keeps its paragraph
//! while the width it's laid out at doesn't change), and reports its text layouts per frame.

use std::time::{Duration, Instant};

use compose::foundation::set_reuse_paragraph_at_same_layout_width;
use compose::runtime::platform;
use compose::test::{CountingTextMeasurer, TextLayoutCounter};
use compose::ui::platform::UiHost;
use compose::ui::{Density, IntSize};
use criterion::{Criterion, criterion_group, criterion_main};

/// The host's width at `frame`: down from 1008 to 504 px and back, 3 px a frame.
fn width(frame: usize) -> i32 {
    let step = (frame % 336) as i32 * 3;
    if step <= 504 { 1008 - step } else { 504 + (step - 504) }
}

fn report(name: &str, mut samples: Vec<Duration>, layouts: usize) {
    samples.sort();
    let median = samples[samples.len() / 2];
    let p99 = samples[samples.len() * 99 / 100];
    let per_frame = layouts as f64 / samples.len() as f64;
    println!(
        "{name}: median {median:?}, p99 {p99:?}, {per_frame:.1} text layouts a frame over {} frames",
        samples.len()
    );
}

fn resize(criterion: &mut Criterion) {
    let contents: [(&str, fn()); 2] = [
        ("grid", compose_rs_feed::resize::ResizeGrid),
        ("deep", || compose_rs_feed::resize::ResizeDeep(0)),
    ];
    for (name, content) in contents {
        for reuse in [false, true] {
            platform::set_manual_clock(platform::now_nanos());
            let (measurer, counter): (_, TextLayoutCounter) =
                CountingTextMeasurer::new(compose_skia::SkiaTextMeasurer::new());
            let mut host =
                UiHost::new(IntSize::new(1008, 2244), Density::new(2.25), Box::new(measurer));
            host.set_content(content);
            host.frame();
            set_reuse_paragraph_at_same_layout_width(reuse);
            let mut frame_index = 0;
            let mut frame = |host: &mut UiHost| {
                frame_index += 1;
                let start = Instant::now();
                host.resize(IntSize::new(width(frame_index), 2244));
                host.frame();
                start.elapsed()
            };
            for _ in 0..336 {
                frame(&mut host);
            }
            let title = format!(
                "resize {name}, {}",
                if reuse { "reusing paragraphs at the same width" } else { "Compose's rule" }
            );
            criterion.bench_function(&title, |bench| {
                bench.iter_custom(|iterations| (0..iterations).map(|_| frame(&mut host)).sum())
            });
            counter.clear();
            let samples = (0..1008).map(|_| frame(&mut host)).collect();
            report(&title, samples, counter.total());
            set_reuse_paragraph_at_same_layout_width(false);
        }
    }
}

criterion_group!(benches, resize);
criterion_main!(benches);
