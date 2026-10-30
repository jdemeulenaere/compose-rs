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
use compose::runtime::{composable, derived_state_of, key, mutable_state_of, platform, remember};
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

/// A column of 3,000 clickables: a first frame's cost per focus target, and its growth with them.
#[composable]
fn Clickables() {
    Column!({
        for _ in 0..3000 {
            Box!(modifier = Modifier.size(2.dp()).clickable(|| {}));
        }
    });
}

/// 300 scrolling columns: a first frame's cost per scrollable, each a focus group.
#[composable]
fn Scrollables() {
    Column!({
        for _ in 0..300 {
            Column!(modifier = Modifier.size(20.dp()).vertical_scroll(remember_scroll_state(0)), {
                Box!(modifier = Modifier.size(2.dp()));
            });
        }
    });
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
    let center = ui.on_all_nodes(matcher).get(index).fetch_semantics_snapshot().center();
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

/// A first frame of a hundred plain list items.
#[composable]
fn ManyListItems() {
    use compose::material3::*;
    MaterialTheme!({
        Column!({
            for index in 0..100usize {
                key(index, || {
                    ListItem!({
                        Text!("Item");
                    });
                });
            }
        });
    });
}

/// A first frame of a hundred selectable list items.
#[composable]
fn ManySelectableItems() {
    use compose::material3::*;
    MaterialTheme!({
        Column!({
            for index in 0..100usize {
                key(index, || {
                    SelectableListItem!(index == 0, || {}, {
                        Text!("Item");
                    });
                });
            }
        });
    });
}

/// A first frame of a hundred rows of the benchmark screens' three chips.
#[composable]
fn ManyChips() {
    use compose::material3::*;
    MaterialTheme!({
        Column!({
            for index in 0..100usize {
                key(index, || {
                    Row!({
                        AssistChip!(|| {}, || Text!("Assist"));
                        FilterChip!(index.is_multiple_of(3), || {}, || Text!("Filter"));
                        SuggestionChip!(|| {}, || Text!("Hint"));
                    });
                });
            }
        });
    });
}

thread_local! {
    static PICKED: compose::runtime::OwnedState<usize> = mutable_state_of(0);
    static VALUE: compose::runtime::OwnedState<String> = mutable_state_of(String::new());
}

/// Five navigation items, the picked one selected.
#[composable]
fn NavigationItems() {
    use compose::material3::*;
    let picked = PICKED.with(|it| it.handle()).get();
    MaterialTheme!({
        NavigationBar!(|row: RowScope| {
            for index in 0..5usize {
                key(index, || {
                    NavigationBarItem!(
                        row,
                        index == picked,
                        || {},
                        {
                            Box!(modifier = Modifier.size(24.dp()));
                        },
                        label = Some(|| Text!("Item"))
                    );
                });
            }
        });
    });
}

/// A hundred list items, the picked one selected.
#[composable]
fn SelectedListItem() {
    use compose::material3::*;
    let picked = PICKED.with(|it| it.handle()).get();
    MaterialTheme!({
        Column!({
            for index in 0..100usize {
                key(index, || {
                    SelectableListItem!(index == picked, || {}, {
                        Text!("Item");
                    });
                });
            }
        });
    });
}

/// Two text fields with labels, the first with what's typed in it.
#[composable]
fn Fields() {
    use compose::material3::*;
    let value = VALUE.with(|it| it.handle());
    MaterialTheme!({
        Column!({
            TextField!(
                value.get(),
                move |new| value.set(new),
                modifier = Modifier.test_tag("first"),
                label = Some(|| Text!("Label")),
            );
            TextField!(
                String::new(),
                |_| {},
                modifier = Modifier.test_tag("second"),
                label = Some(|| Text!("Label")),
            );
        });
    });
}

/// `n` selections, each until idle, among `count` items.
/// A sheet open, a drag moving it (D1466): the anchored state, the sheet's relayout.
#[composable]
fn OpenSheet() {
    compose::material3::MaterialTheme!({
        compose::material3::ModalBottomSheet!(|| {}, {
            Box!(modifier = Modifier.fill_max_width().height(300.dp()));
        });
    });
}

fn sheet_drag(n: usize) {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 800), OpenSheet);
    ui.wait_for_idle();
    let finger = |event: PointerEvent| event.with_pointer(PointerId(1), PointerType::Touch);
    ui.host().send_pointer(finger(PointerEvent::press(Offset::new(200.0, 700.0))));
    ui.advance_time_by(Duration::from_millis(16));
    let (mut y, mut way) = (700.0, 8.0);
    for step in 0..n {
        if step % 20 == 0 {
            way = -way;
        }
        y += way;
        ui.host().send_pointer(finger(PointerEvent::moved(Offset::new(200.0, y), true)));
        ui.advance_time_by(Duration::from_millis(16));
    }
}

fn selections(content: fn(), count: usize, n: usize) {
    let mut ui = compose_test(content);
    let picked = PICKED.with(|it| it.handle());
    for turn in 0..n {
        picked.set((turn + 1) % count);
        ui.wait_for_idle();
    }
}

/// The display list of `content`'s first frame, then after `n` frames 16 ms apart and a wheel's
/// turn, for comparing two checkouts' drawing: `compare_instructions.py --dumps`.
fn dump(content: fn(), n: usize) {
    platform::set_manual_clock(platform::now_nanos());
    let mut host = phone(content);
    host.frame();
    print!("{}", host.display_list().dump());
    for _ in 0..n {
        platform::advance_clock(Duration::from_millis(16));
        host.frame();
    }
    println!("---");
    print!("{}", host.display_list().dump());
    let at = Offset::new(540.0, 1200.0);
    host.send_pointer(PointerEvent::scroll(at, Offset::new(0.0, 200.0)));
    for _ in 0..n {
        platform::advance_clock(Duration::from_millis(16));
        host.frame();
    }
    println!("---");
    print!("{}", host.display_list().dump());
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(scenario), Some(n)) = (args.next(), args.next()) else {
        panic!("usage: instructions <scenario> <iterations>");
    };
    let n: usize = n.parse().expect("a number of iterations");
    use_optimizations(Optimizations::NONE);
    match scenario.as_str() {
        "state_read" => {
            let state = mutable_state_of(1u64);
            for _ in 0..n {
                std::hint::black_box(state.get());
            }
        }
        "state_write" => {
            let state = mutable_state_of(0u64);
            for value in 0..n {
                state.set(value as u64);
            }
        }
        "derived_read" => {
            let state = mutable_state_of(1u64);
            let handle = state.handle();
            let derived = derived_state_of(move || handle.get() + 1);
            for _ in 0..n {
                std::hint::black_box(derived.get());
            }
        }
        "derived_after_write" => {
            let (state, unrelated) = (mutable_state_of(1u64), mutable_state_of(0u64));
            let handle = state.handle();
            let derived = derived_state_of(move || handle.get() + 1);
            for value in 0..n {
                unrelated.set(value as u64);
                std::hint::black_box(derived.get());
            }
        }
        "derived_recompute" => {
            let state = mutable_state_of(0u64);
            let handle = state.handle();
            let derived = derived_state_of(move || handle.get() + 1);
            for value in 0..n {
                state.set(value as u64);
                std::hint::black_box(derived.get());
            }
        }
        "counter_click" => {
            let mut ui = compose_test(counter::Counter);
            clicks(&mut ui, has_text_containing("Count"), 0, n);
        }
        // A finger: no hover, as on a phone.
        "counter_click_touch" => {
            let mut ui = compose_test(counter::Counter);
            let center = ui
                .on_all_nodes(has_text_containing("Count"))
                .get(0)
                .fetch_semantics_snapshot()
                .center();
            let finger = |event: PointerEvent| event.with_pointer(PointerId(1), PointerType::Touch);
            for _ in 0..n {
                ui.host().send_pointer(finger(PointerEvent::press(center)));
                ui.host().send_pointer(finger(PointerEvent::release(center)));
                ui.wait_for_idle();
            }
        }
        "todo_toggle" => {
            let mut ui =
                ComposeTestRule::sized(IntSize::new(400, 100_000), || todo::TodoAppWith(1000));
            clicks(&mut ui, has_role(Role::Checkbox), 500, n);
        }
        "first_frame_feed" => first_frames(compose_rs_feed::Feed, n),
        "first_frame_big" => first_frames(compose_rs_feed::BigScreen, n),
        "first_frame_clickables" => first_frames(Clickables, n),
        "first_frame_scrollables" => first_frames(Scrollables, n),
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
        "sheet_drag" => sheet_drag(n),
        "cells_animated_positioned" => {
            POSITIONED.set(true);
            frames(AnimatedCells, n);
        }
        "first_frame_list_items" => first_frames(ManyListItems, n),
        "first_frame_selectable_items" => first_frames(ManySelectableItems, n),
        "first_frame_chips" => first_frames(ManyChips, n),
        "nav_selection" => selections(NavigationItems, 5, n),
        "list_selection" => selections(SelectedListItem, 2, n),
        "field_focus" => {
            let mut ui = compose_test(Fields);
            for turn in 0..n {
                let tag = if turn % 2 == 0 { "first" } else { "second" };
                ui.on_node_with_tag(tag).request_focus();
                ui.wait_for_idle();
            }
        }
        "field_keystroke" => {
            let mut ui = compose_test(Fields);
            ui.on_node_with_tag("first").request_focus();
            ui.wait_for_idle();
            let value = VALUE.with(|it| it.handle());
            for turn in 0..n {
                ui.on_node_with_tag("first").perform_text_input("x");
                if turn % 20 == 19 {
                    value.set(String::new());
                    ui.wait_for_idle();
                }
            }
        }
        // A key as a phone's: no semantics tree looked through to find the field.
        "field_keystroke_direct" => {
            let mut ui = compose_test(Fields);
            ui.on_node_with_tag("first").request_focus();
            ui.wait_for_idle();
            let value = VALUE.with(|it| it.handle());
            for turn in 0..n {
                ui.host().send_text("x");
                ui.wait_for_idle();
                if turn % 20 == 19 {
                    value.set(String::new());
                    ui.wait_for_idle();
                }
            }
        }
        "dump_feed" => dump(compose_rs_feed::Feed, n),
        "dump_big" => dump(compose_rs_feed::BigScreen, n),
        "dump_feed_animated" => dump(compose_rs_feed::FeedAnimated, n),
        "dump_stress" => dump(compose_rs_feed::stress::StressScroll, n),
        "dump_stress_animation" => dump(compose_rs_feed::stress::StressAnimation, n),
        "dump_grid" => dump(Gallery, n),
        "dump_cells" => dump(AnimatedCells, n),
        other => panic!("no scenario {other}"),
    }
}
