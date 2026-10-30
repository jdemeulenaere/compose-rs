//! Animation frames: many infinite transitions at once, as docs/15's `stress_animation` (140
//! cells, each with an infinite transition of 4 values), and the cost of resuming the tasks that
//! wait for a frame. The hosts run at parity (`none`, Compose's work: D610).

use std::hint::black_box;
use std::rc::Rc;
use std::time::{Duration, Instant};

use compose::animation::*;
use compose::foundation::layout::*;
use compose::runtime::{composable, platform};
use compose::ui::platform::{FixedTextMeasurer, Optimizations, UiHost};
use compose::ui::*;
use criterion::{Criterion, criterion_group, criterion_main};

/// The cells: 140, as `stress_animation`'s grid.
const CELLS: u32 = 140;

thread_local! {
    /// Whether the cells draw their values (or only animate them).
    static DRAWN: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// A cell turning, scaling, fading and changing color forever, as `stress_animation`'s.
#[composable]
fn Cell(index: u32) {
    let transition = remember_infinite_transition("Cell");
    let rotation = transition.animate_float(
        0.0,
        360.0,
        infinite_repeatable(
            tween(1500 + index % 5 * 250).easing(LINEAR_EASING),
            RepeatMode::Restart,
        ),
    );
    let scale = transition.animate_float(
        0.6,
        1.0,
        infinite_repeatable(tween(700 + index % 7 * 100), RepeatMode::Reverse),
    );
    let alpha = transition.animate_float(
        0.35,
        1.0,
        infinite_repeatable(tween(900 + index % 3 * 300), RepeatMode::Reverse),
    );
    let fill = transition.animate_color(
        Color::RED,
        Color::BLUE,
        infinite_repeatable(tween(1200 + index % 4 * 200), RepeatMode::Reverse),
    );
    if !DRAWN.get() {
        return;
    }
    Box!(
        modifier = Modifier
            .size(22.dp())
            .graphics_layer(move |layer| {
                layer.rotation_z = rotation.get();
                layer.scale_x = scale.get();
                layer.scale_y = scale.get();
                layer.alpha = alpha.get();
            })
            .draw_behind(move |scope| {
                scope.draw_rect(scope.bounds(), Paint::fill(fill.get()));
            })
    );
}

#[composable]
fn Cells() {
    Column!({
        for row in 0..10 {
            Row!({
                for column in 0..CELLS / 10 {
                    let index = row * (CELLS / 10) + column;
                    compose::runtime::key(index, move || Cell(index));
                }
            });
        }
    });
}

fn report(name: &str, mut samples: Vec<Duration>) {
    samples.sort();
    let at = |percent: usize| samples[samples.len() * percent / 100];
    println!(
        "{name}: p50 {:?}, p90 {:?}, p99 {:?} over {} frames",
        at(50),
        at(90),
        at(99),
        samples.len()
    );
}

/// Frames 16 ms apart of `content` on a phone-sized host.
fn frames(criterion: &mut Criterion, title: &str, content: fn()) {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1080, 2400),
        Density::new(2.625),
        Box::new(FixedTextMeasurer::new()),
    );
    // Parity, Compose's work, as before the hosts' default became the recommended set (D610).
    host.set_optimizations(Optimizations::NONE);
    host.set_content(content);
    let frame = |host: &mut UiHost| {
        platform::advance_clock(Duration::from_millis(16));
        let start = Instant::now();
        host.frame();
        start.elapsed()
    };
    for _ in 0..30 {
        frame(&mut host);
    }
    criterion.bench_function(title, |bench| {
        bench.iter_custom(|iterations| (0..iterations).map(|_| frame(&mut host)).sum())
    });
    let samples = (0..1000).map(|_| frame(&mut host)).collect();
    report(title, samples);
}

/// 140 infinite transitions of 4 values: only animating (nothing reads the values), and drawn
/// as `stress_animation`'s cells. Then the whole `stress_animation` screen (text measured by a
/// fixed measurer).
fn infinite_transitions(criterion: &mut Criterion) {
    DRAWN.set(false);
    frames(criterion, "140 infinite transitions of 4 values, frame", Cells);
    DRAWN.set(true);
    frames(criterion, "140 infinite transitions of 4 values drawn, frame", Cells);
    frames(criterion, "stress_animation, frame", compose_rs_feed::stress::StressAnimation);
    serialized_frames(criterion);
}

/// `stress_animation`'s frames with what Android's bridge does after each: the display list
/// written for Kotlin (`write_display_list`, the resources collected) and taken with the
/// resource changes (`write_pending`), as `frame` and `takeDrawing` do. Prints the bytes Kotlin
/// takes a frame, and of them the path definitions' (the 50 waves' paths, D532).
fn serialized_frames(criterion: &mut Criterion) {
    use compose_android::serialize::{GroupCache, Resources, Writer, write_display_list};
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1080, 2400),
        Density::new(2.625),
        Box::new(FixedTextMeasurer::new()),
    );
    // Parity, host and records, as before the default became the recommended set (D610).
    host.set_optimizations(Optimizations::NONE);
    host.set_content(compose_rs_feed::stress::StressAnimation);
    let (mut resources, mut groups, mut list, mut taken) = (
        Resources::default(),
        GroupCache::new(Optimizations::NONE),
        Writer::default(),
        Writer::default(),
    );
    let mut frame = |host: &mut UiHost| {
        platform::advance_clock(Duration::from_millis(16));
        let start = Instant::now();
        host.frame();
        host.with_tree(|tree| {
            write_display_list(tree.display_list(), &mut resources, &mut groups, &mut list)
        });
        resources.collect();
        taken.bytes.clear();
        resources.write_pending(&mut taken);
        let resource_bytes = taken.bytes.len();
        taken.bytes.extend_from_slice(&list.bytes);
        (start.elapsed(), taken.bytes.len(), resource_bytes)
    };
    for _ in 0..30 {
        frame(&mut host);
    }
    let title = "stress_animation, frame and serialize";
    criterion.bench_function(title, |bench| {
        bench.iter_custom(|iterations| (0..iterations).map(|_| frame(&mut host).0).sum())
    });
    let runs: Vec<_> = (0..1000).map(|_| frame(&mut host)).collect();
    let mean = |pick: fn(&(Duration, usize, usize)) -> usize| {
        runs.iter().map(pick).sum::<usize>() as f64 / runs.len() as f64
    };
    println!(
        "{title}: {:.0} bytes taken a frame, {:.0} of them resource changes",
        mean(|run| run.1),
        mean(|run| run.2)
    );
    report(title, runs.iter().map(|run| run.0).collect());
}

/// Resuming 568 tasks waiting for a frame (`dispatch_frame`), against 568 calls of a boxed
/// closure doing the same work (a counter): the executor's cost per frame awaiter.
fn frame_awaiters(criterion: &mut Criterion) {
    const TASKS: usize = 568;
    platform::set_manual_clock(platform::now_nanos());
    let counted = Rc::new(std::cell::Cell::new(0u64));
    let scope = compose::runtime::CoroutineScope::new();
    for _ in 0..TASKS {
        let counted = counted.clone();
        scope.launch(async move {
            loop {
                let time = compose::runtime::with_frame_nanos(|time| time).await;
                counted.set(counted.get().wrapping_add(time));
            }
        });
    }
    platform::run_ready();
    criterion.bench_function("resume 568 frame awaiters (dispatch_frame)", |bench| {
        bench.iter(|| {
            platform::advance_clock(Duration::from_millis(16));
            platform::dispatch_frame()
        })
    });
    scope.cancel();
    let mut calls: Vec<Box<dyn FnMut(u64)>> = (0..TASKS)
        .map(|_| {
            let counted = counted.clone();
            Box::new(move |time: u64| counted.set(counted.get().wrapping_add(time)))
                as Box<dyn FnMut(u64)>
        })
        .collect();
    criterion.bench_function("568 boxed closure calls", |bench| {
        bench.iter(|| {
            for call in &mut calls {
                call(black_box(16));
            }
        })
    });
}

#[composable]
fn VisibleBoxes() {
    Column!({
        for index in 0..100u32 {
            compose::runtime::key(index, || {
                AnimatedVisibility!(true, {
                    Box!(modifier = Modifier.size(4.dp()));
                });
            });
        }
    });
}

#[composable]
fn EnteringBoxes() {
    Column!({
        for index in 0..100u32 {
            compose::runtime::key(index, || {
                let state = compose::runtime::remember(|| {
                    let state = MutableTransitionState::new(false);
                    state.set_target_state(true);
                    state
                });
                AnimatedVisibility!(state, {
                    Box!(modifier = Modifier.size(4.dp()));
                });
            });
        }
    });
}

/// The first frame of 100 `AnimatedVisibility`s, at rest and entering: what each costs to set up,
/// its transitions, tracker and modifiers (the cost of D828 to those without a deferred phase,
/// D834).
fn animated_visibility_first_frame(criterion: &mut Criterion) {
    platform::set_manual_clock(platform::now_nanos());
    let cases: [(&str, fn()); 2] = [
        ("100 AnimatedVisibility, first frame", VisibleBoxes),
        ("100 AnimatedVisibility entering, first frame", EnteringBoxes),
    ];
    for (title, content) in cases {
        criterion.bench_function(title, |bench| {
            bench.iter_custom(|iterations| {
                (0..iterations)
                    .map(|_| {
                        let mut host = UiHost::new(
                            IntSize::new(1080, 2400),
                            Density::new(2.625),
                            Box::new(FixedTextMeasurer::new()),
                        );
                        host.set_optimizations(Optimizations::NONE);
                        let start = Instant::now();
                        host.set_content(content);
                        host.frame();
                        let elapsed = start.elapsed();
                        drop(host);
                        elapsed
                    })
                    .sum()
            })
        });
    }
}

/// A read of a shared spec, with the start and end values of one animation or of two in turns: its
/// splines are kept by those values, Compose vectorizing a spec for each animation (D846).
fn shared_specs(criterion: &mut Criterion) {
    let arc = KeyframesSpec::from(ArcAnimationSpec::new(ArcMode::ArcBelow));
    let spline = keyframes_with_spline(|frames: &mut KeyframesWithSplineBuilder<Offset>| {
        frames.at(150, Offset::new(50.0, 80.0));
    });
    let zero = AnimationVector::zero(2);
    let ends = [Offset::new(100.0, 100.0).to_vector(), Offset::new(200.0, 50.0).to_vector()];
    let start = Offset::ZERO.to_vector();
    let mut group = criterion.benchmark_group("shared_specs");
    group.bench_function("arc_one_animation", |b| {
        b.iter(|| black_box(arc.value_from_millis(150, start, ends[0], zero)))
    });
    group.bench_function("arc_two_animations", |b| {
        let mut turn = 0;
        b.iter(|| {
            turn = (turn + 1) % 2;
            black_box(arc.value_from_millis(150, start, ends[turn], zero))
        })
    });
    group.bench_function("spline_one_animation", |b| {
        b.iter(|| black_box(spline.value_from_millis(75, start, ends[0], zero)))
    });
    group.bench_function("spline_two_animations", |b| {
        let mut turn = 0;
        b.iter(|| {
            turn = (turn + 1) % 2;
            black_box(spline.value_from_millis(75, start, ends[turn], zero))
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    frame_awaiters,
    infinite_transitions,
    animated_visibility_first_frame,
    shared_specs
);
criterion_main!(benches);
