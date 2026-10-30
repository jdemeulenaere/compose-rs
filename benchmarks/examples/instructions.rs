//! Fixed work per scenario, for `benchmarks/compare_instructions.py` to count the instructions
//! retired by each checkout's build: timing swings too much on a loaded machine to show a change
//! of a few percent (docs/14, "Method"). Run as `instructions <scenario> <iterations>`; an
//! unknown scenario is an error, not another scenario's work.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::time::Duration;

use compose::animation::*;
use compose::foundation::layout::*;
use compose::foundation::*;
use compose::runtime::{composable, key, mutable_state_of, platform, remember};
use compose::test::*;
use compose::ui::platform::{FixedTextMeasurer, Optimizations, UiHost};
use compose::ui::*;

fn phone(content: fn()) -> UiHost {
    let mut host = UiHost::new(
        IntSize::new(1080, 2400),
        Density::new(2.625),
        Box::new(FixedTextMeasurer::new()),
    );
    host.set_optimizations(Optimizations::NONE);
    host.set_content(content);
    host
}

/// `n` first frames of `content`: the host's making and dropping counted.
fn first_frames(content: fn(), n: usize) {
    platform::set_manual_clock(platform::now_nanos());
    for _ in 0..n {
        let mut host = phone(content);
        host.frame();
        host.settle(10);
    }
}

/// `n` clicks of `matcher`'s `index`th node, each until idle.
fn clicks(ui: &mut ComposeTestRule, matcher: Matcher, index: usize, n: usize) {
    let center = ui.on_all_nodes(matcher).get(index).fetch_semantics_node().center();
    for _ in 0..n {
        ui.host().send_pointer(PointerEvent::press(center));
        ui.host().send_pointer(PointerEvent::release(center));
        ui.wait_for_idle();
    }
}

/// `n` wheel turns on `content`, by `pixels` each and back every `every`.
fn wheel(content: fn(), pixels: f32, every: usize, n: usize) {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = phone(content);
    host.settle(10);
    for turn in 0..n {
        let delta = if turn % (2 * every) < every { pixels } else { -pixels };
        platform::advance_clock(Duration::from_millis(16));
        let at = Offset::new(540.0, 1200.0);
        host.send_pointer(PointerEvent::scroll(at, Offset::new(0.0, delta)));
        host.frame();
    }
}

/// `n` frames 16 ms apart of `content`.
fn frames(content: fn(), n: usize) {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = phone(content);
    for _ in 0..30 + n {
        platform::advance_clock(Duration::from_millis(16));
        host.frame();
    }
}

thread_local! {
    static LIST: Cell<Option<LazyListState>> = const { Cell::new(None) };
    /// Whether each cell has a position-aware child.
    static POSITIONED: Cell<bool> = const { Cell::new(false) };
}

/// `lazy_scroll_frame`'s list (benches/ui.rs).
#[composable]
fn BigList() {
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

/// The grid of benches/lazy.rs.
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

/// `stress_animation`'s cell (benches/animation.rs), with a position-aware child if asked.
#[composable]
fn AnimatedCell(index: u32) {
    let transition = remember_infinite_transition("Cell");
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
    Box!(
        modifier = Modifier.size(22.dp()).graphics_layer(move |layer| {
            layer.scale_x = scale.get();
            layer.scale_y = scale.get();
            layer.alpha = alpha.get();
        }),
        {
            if POSITIONED.get() {
                Box!(
                    modifier =
                        Modifier.size(10.dp()).on_globally_positioned(|_: LayoutCoordinates| {})
                );
            }
        }
    );
}

#[composable]
fn AnimatedCells() {
    Column!({
        for row in 0..10 {
            Row!({
                for column in 0..14 {
                    let index = row * 14 + column;
                    key(index, move || AnimatedCell(index));
                }
            });
        }
    });
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(scenario), Some(n)) = (args.next(), args.next()) else {
        panic!("usage: instructions <scenario> <iterations>");
    };
    let n: usize = n.parse().expect("a number of iterations");
    use_optimizations(Optimizations::NONE);
    match scenario.as_str() {
        "counter_click" => {
            let mut ui = compose_test(counter::Counter);
            clicks(&mut ui, has_text_containing("Count"), 0, n);
        }
        "todo_toggle" => {
            let mut ui =
                ComposeTestRule::sized(IntSize::new(400, 100_000), || todo::TodoAppWith(1000));
            clicks(&mut ui, has_role(Role::Checkbox), 500, n);
        }
        "first_frame_feed" => first_frames(compose_rs_feed::Feed, n),
        "first_frame_big" => first_frames(compose_rs_feed::BigScreen, n),
        "list_scroll" => {
            let mut ui = compose_test(BigList);
            let list = LIST.get().expect("the list composed");
            for _ in 0..n {
                if list.first_visible_item_index() > 90_000 {
                    list.request_scroll_to_item(0, 0);
                    ui.wait_for_idle();
                }
                list.dispatch_raw_delta(100.0);
                ui.wait_for_idle();
            }
        }
        "feed_pixel" => wheel(compose_rs_feed::Feed, 1.0, 1, n),
        "feed_step" => wheel(compose_rs_feed::Feed, 200.0, 25, n),
        "stress_pixel" => wheel(compose_rs_feed::stress::StressScroll, 1.0, 1, n),
        "stress_step" => wheel(compose_rs_feed::stress::StressScroll, 200.0, 25, n),
        "grid_pixel" => wheel(Gallery, 1.0, 1, n),
        "grid_step" => wheel(Gallery, 200.0, 25, n),
        "feed_animated" => frames(compose_rs_feed::FeedAnimated, n),
        "stress_animation" => frames(compose_rs_feed::stress::StressAnimation, n),
        "cells_animated" => frames(AnimatedCells, n),
        "cells_animated_positioned" => {
            POSITIONED.set(true);
            frames(AnimatedCells, n);
        }
        other => panic!("no scenario {other}"),
    }
}
