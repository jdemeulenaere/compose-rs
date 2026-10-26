//! Headless UI frames (docs/09-testing.md targets): a counter click under 1 ms, and toggling
//! one of 1,000 todos under 4 ms frame p99. Each iteration is a press and release on a node
//! followed by a frame (recompose, measure, draw).
//!
//! Every host runs at parity (`Optimizations::NONE`, Compose's work), as before the hosts'
//! default became the recommended set (D610).

use std::time::{Duration, Instant};

use compose::test::*;
use compose::ui::platform::Optimizations;
use compose::ui::{IntSize, PointerEvent, Role};
use criterion::{Criterion, criterion_group, criterion_main};

/// The rules made from now on, on this thread, run at parity.
fn parity() {
    use_optimizations(Optimizations::NONE);
}

/// Clicks the center of `matcher`'s `index`th node, then runs frames until idle.
fn click_frame(ui: &mut ComposeTestRule, matcher: &Matcher, index: usize) -> Duration {
    let center = ui.on_all_nodes(matcher.clone()).get(index).fetch_semantics_node().center();
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
    parity();
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
    parity();
    let mut ui = ComposeTestRule::with_host(host, || todo::TodoAppWith(1000));
    let mut renderer = compose_skia::RasterRenderer::new(960, 1280);
    renderer.set_optimizations(Optimizations::NONE);
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
        host.set_optimizations(Optimizations::NONE);
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

thread_local! {
    static LIST: std::cell::Cell<Option<compose::foundation::LazyListState>> =
        const { std::cell::Cell::new(None) };
}

/// A 100,000-item list, 50 dp items in 400 dp, each in its `SaveableStateProvider` (D290).
#[compose::runtime::composable]
#[allow(non_snake_case)]
fn BigList() {
    use compose::foundation::layout::*;
    use compose::foundation::*;
    use compose::ui::{Modifier, UnitExt};
    let state = remember_lazy_list_state!();
    LIST.set(Some(state));
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 400.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items_keyed(
                100_000,
                |index| index as u32,
                |index| {
                    Box!(modifier = Modifier.size((100.dp(), 50.dp())), {
                        BasicText!("item {index}");
                    });
                },
            );
        }
    );
}

/// Scrolling a long lazy list: each frame scrolls 100 px, composing about two items and
/// releasing two.
fn lazy_scroll_frame(criterion: &mut Criterion) {
    parity();
    let mut ui = compose_test(BigList);
    let list = LIST.get().expect("the list composed");
    let mut frame = move || {
        // Back to the top, untimed, before the end.
        if list.first_visible_item_index() > 90_000 {
            list.scroll_to_item(0, 0);
            ui.wait_for_idle();
        }
        let start = Instant::now();
        list.scroll_by(100.0);
        ui.wait_for_idle();
        start.elapsed()
    };
    for _ in 0..500 {
        frame();
    }
    criterion.bench_function("scroll a 100,000-item lazy list, frame", |bench| {
        bench.iter_custom(|iterations| (0..iterations).map(|_| frame()).sum())
    });
}

/// The first frame of a new phone-sized host: composing, measuring and drawing everything in
/// view. The feed (a lazy list's posts in view) and its big screen (200 posts in a scrolling
/// column, all composed: thousands of layout nodes reading the same composition locals, the
/// case docs/14's #5 made linear). The host's creation and drop aren't timed.
fn first_frame(criterion: &mut Criterion) {
    use compose::runtime::platform;
    use compose::ui::{
        Density,
        platform::{FixedTextMeasurer, UiHost},
    };
    let screens: [(&str, fn()); 2] =
        [("feed", compose_rs_feed::Feed), ("big screen", compose_rs_feed::BigScreen)];
    for (name, content) in screens {
        platform::set_manual_clock(platform::now_nanos());
        let frame = || {
            let mut host = UiHost::new(
                IntSize::new(1080, 2400),
                Density::new(2.625),
                Box::new(FixedTextMeasurer::new()),
            );
            host.set_optimizations(Optimizations::NONE);
            host.set_content(content);
            let start = Instant::now();
            host.frame();
            let elapsed = start.elapsed();
            // Untimed: what effects launched, then the drop.
            host.settle(10);
            drop(host);
            elapsed
        };
        criterion.bench_function(&format!("first frame, {name}"), |bench| {
            bench.iter_custom(|iterations| (0..iterations).map(|_| frame()).sum())
        });
    }
}

/// The navigation screen's first frame (docs/17), on the Pixel 8 Pro's screen: from the
/// launcher, a tap on its button opens the heavy screen of a new visit (its content drawn from
/// the visit's number), and the timed frame composes, lays it out and draws it, with what
/// Android's bridge does after it (the display list written for Kotlin, the resources collected
/// and taken, as `frame` and `takeDrawing` do). Back to the launcher, untimed, before the next.
/// Text is measured by a fixed measurer: the device's paragraphs are not in it.
fn navigation_frame(criterion: &mut Criterion) {
    use compose::runtime::platform;
    use compose::ui::{
        Density, Offset,
        platform::{FixedTextMeasurer, UiHost},
    };
    use compose_android::serialize::{GroupCache, Resources, Writer, write_display_list};
    platform::set_manual_clock(platform::now_nanos());
    let mut host = UiHost::new(
        IntSize::new(1008, 2244),
        Density::new(2.25),
        Box::new(FixedTextMeasurer::new()),
    );
    host.set_optimizations(Optimizations::NONE);
    host.set_content(compose_rs_feed::navigation::NavigationApp);
    let (mut resources, mut groups, mut list, mut taken) = (
        Resources::default(),
        GroupCache::new(Optimizations::NONE),
        Writer::default(),
        Writer::default(),
    );
    let mut frame = |host: &mut UiHost| {
        platform::advance_clock(Duration::from_millis(8));
        let start = Instant::now();
        let stats = host.frame();
        host.with_tree(|tree| {
            write_display_list(tree.display_list(), &mut resources, &mut groups, &mut list)
        });
        resources.collect();
        taken.bytes.clear();
        resources.write_pending(&mut taken);
        taken.bytes.extend_from_slice(&list.bytes);
        (start.elapsed(), stats.drew)
    };
    // The screen's center, which the launcher's big button holds, as measure.py taps it.
    let center = Offset::new(504.0, 1122.0);
    frame(&mut host);
    let mut navigate = |host: &mut UiHost| {
        host.send_pointer(PointerEvent::press(center));
        host.send_pointer(PointerEvent::release(center));
        let (elapsed, drew) = frame(host);
        assert!(drew, "the heavy screen's frame draws");
        // Untimed: its effects and frames settle, then back to the launcher.
        for _ in 0..3 {
            frame(host);
        }
        assert!(host.on_back_pressed(), "the heavy screen takes back");
        for _ in 0..3 {
            frame(host);
        }
        elapsed
    };
    // A warm-up visit, as measure.py's.
    navigate(&mut host);
    let title = "navigation: the heavy screen's first frame and serialize";
    criterion.bench_function(title, |bench| {
        bench.iter_custom(|iterations| (0..iterations).map(|_| navigate(&mut host)).sum())
    });
    let samples = (0..30).map(|_| navigate(&mut host)).collect();
    report_p99(title, samples);
}

criterion_group!(
    benches,
    ui,
    desktop_frame,
    animation_frame,
    lazy_scroll_frame,
    first_frame,
    navigation_frame
);
criterion_main!(benches);
