//! What drawing graphics layers, clips and shadows as Compose's render nodes (D522-D525) costs
//! headless: the frame (recompose, layout, draw) and its serialization for the Kotlin replayer,
//! per frame of the benchmark's screens at the Pixel 8 Pro's size, with the bytes, the groups,
//! and the groups whose records changed (new or drawn again: what the replayer records again). Ignored: run with `cargo test --release -p compose-benchmarks --test layer_cost --
//! --ignored --nocapture` (the same file runs on an earlier commit to compare, D527).

use std::time::{Duration, Instant};

use compose::runtime::platform;
use compose::ui::platform::{FixedTextMeasurer, UiHost};
use compose::ui::*;
use compose_android::serialize::{GroupCache, Resources, Writer, write_display_list};

/// Per-frame medians over `frames` frames of `content`, `input` sent before each.
fn measure(
    name: &str,
    content: fn(),
    frames: usize,
    input: impl Fn(usize) -> Option<PointerEvent>,
) {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1344, 2780),
        Density::new(3.0),
        std::boxed::Box::new(FixedTextMeasurer::new()),
    );
    host.set_keep_unplaced_groups(true);
    host.set_content(content);
    host.settle(10);
    let (mut resources, mut cache, mut out) =
        (Resources::default(), GroupCache::default(), Writer::default());
    let (mut frame_times, mut serialize_times, mut bytes, mut groups, mut written) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for frame in 0..frames + 30 {
        platform::advance_clock(Duration::from_nanos(8_333_333));
        // The input's dispatch and the frame it asks for.
        let start = Instant::now();
        if let Some(event) = input(frame) {
            host.send_pointer(event);
        }
        host.frame();
        let framed = start.elapsed();
        let start = Instant::now();
        host.with_tree(|tree| {
            write_display_list(tree.display_list(), &mut resources, &mut cache, &mut out)
        });
        resources.collect();
        let serialized = start.elapsed();
        // The first 30 warm up.
        if frame >= 30 {
            frame_times.push(framed);
            serialize_times.push(serialized);
            bytes.push(out.bytes.len());
            groups.push(host.with_tree(|tree| tree.display_list().groups.len()));
            // Written with new bytes: what the replayer records again.
            written.push(cache.written() - cache.unchanged());
        }
    }
    let median = |values: &mut Vec<Duration>| {
        values.sort();
        values[values.len() / 2]
    };
    let median_count = |values: &mut Vec<usize>| {
        values.sort();
        values[values.len() / 2]
    };
    let total_written: usize = written.iter().sum();
    println!(
        "{name}: frame {:?}, serialize {:?}, {} bytes, {} groups, {} recorded again (medians of \
         {frames}; {:.2} a frame on average)",
        median(&mut frame_times),
        median(&mut serialize_times),
        median_count(&mut bytes),
        median_count(&mut groups),
        median_count(&mut written),
        total_written as f64 / frames as f64,
    );
}

fn scroll(frame: usize) -> Option<PointerEvent> {
    // The wheel at the screen's center, 60 px a frame, down then (after 300 frames) up.
    let delta = if (frame / 300).is_multiple_of(2) { 60.0 } else { -60.0 };
    Some(PointerEvent::scroll(Offset::new(672.0, 1390.0), Offset::new(0.0, delta)))
}

/// A list with a background, in a boundary: its clip is an inner layer (D522).
#[compose::runtime::composable]
#[allow(non_snake_case)]
fn BackgroundList() {
    use compose::foundation::layout::*;
    use compose::foundation::*;
    use compose::material3::*;
    MaterialTheme!({
        Box!(modifier = Modifier.fill_max_size().repaint_boundary(), {
            LazyColumn!(
                modifier = Modifier.fill_max_size().background(Color::LIGHT_GRAY),
                |scope: LazyListScope| {
                    scope.items(10_000, |index| {
                        Card!(modifier = Modifier.fill_max_width().padding(8.dp()), {
                            Text!("Item {}", index, modifier = Modifier.padding(16.dp()));
                        });
                    });
                }
            );
        });
    });
}

#[test]
#[ignore]
fn layer_cost() {
    measure("background list scroll", BackgroundList, 600, scroll);
    measure("feed scroll", compose_rs_feed::Feed, 600, scroll);
    measure("stress scroll", compose_rs_feed::stress::StressScroll, 600, scroll);
    measure("stress animation", compose_rs_feed::stress::StressAnimation, 600, |_| None);
    measure("feed animation", compose_rs_feed::FeedAnimated, 600, |_| None);
}
