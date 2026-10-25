//! Second review of D597-D599: a headless time per item composed while scrolling (frames, no
//! prefetch), for a list and a grid; run in release with `--ignored --nocapture`.

#![allow(non_snake_case)]

extern crate compose_runtime as compose;

use std::time::{Duration, Instant};

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::{composable, mutable_state_of, platform, remember};
use compose_ui::platform::{FixedTextMeasurer, UiHost};
use compose_ui::{Color, Density, IntSize, Modifier, Offset, PointerEvent, UnitExt};

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

fn scroll_frame(host: &mut UiHost, pixels: f32) -> Duration {
    platform::advance_clock(Duration::from_millis(16));
    let start = Instant::now();
    host.send_pointer(PointerEvent::scroll(Offset::new(540.0, 1200.0), Offset::new(0.0, pixels)));
    host.frame();
    start.elapsed()
}

#[composable]
fn Gallery() {
    LazyVerticalGrid!(
        GridCells::Adaptive(120.dp()),
        modifier = Modifier.fill_max_size(),
        vertical_arrangement = Arrangement::spaced_by(4.dp()),
        horizontal_arrangement = Arrangement::spaced_by(4.dp()),
        |scope: LazyGridScope| {
            scope.items_keyed(10_000, |index| index, Tile);
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

#[composable]
fn Rows() {
    LazyColumn!(modifier = Modifier.fill_max_size(), |scope: LazyListScope| {
        scope.items_keyed(10_000, |index| index, RowItem);
    });
}

#[composable]
fn RowItem(index: usize) {
    let liked = remember(|| mutable_state_of(false));
    Row!(
        modifier =
            Modifier.fill_max_width().height(72.dp()).clickable(move || liked.update(|on| !on)),
        {
            Box!(
                modifier =
                    Modifier.size((48.dp(), 48.dp())).background(Color::from_argb(0xFF336699))
            );
            Column!({
                BasicText!("Title {index}");
                BasicText!("Subtitle {}", index % 13 + usize::from(liked.get()));
            });
        }
    );
}

fn per_item(name: &str, content: fn()) {
    let mut host = host(content);
    for _ in 0..200 {
        scroll_frame(&mut host, 200.0);
    }
    let mut medians = Vec::new();
    for _ in 0..5 {
        let mut samples = Vec::new();
        for _ in 0..400 {
            samples.push(scroll_frame(&mut host, 200.0));
        }
        samples.sort();
        medians.push(samples[samples.len() / 2]);
    }
    medians.sort();
    println!("PERF {name}: frame median of medians {:?} ({medians:?})", medians[2]);
}

#[test]
#[ignore]
fn perf_review2_frames_composing_items() {
    per_item("rows", Rows);
    per_item("grid", Gallery);
}
