//! `ScrollState` with `vertical_scroll` / `horizontal_scroll`: the offset, its range, reversed
//! scrolling, `scroll_to` clamping at once, animated scrolls, flings stopped by a touch, and
//! what the state says about its scrolling.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_animation::spring;
use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, LayoutDirection, Offset, UnitExt};

use crate::scroll_support::{Touch, launch, run, swipe};

const TAG: &str = "scroller";
const CELL: i32 = 5;
const CELLS: i32 = 8;
const CROSS: i32 = 45;

/// The configurations Compose's `ScrollTest` runs in.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Config {
    Vertical,
    HorizontalLtr,
    HorizontalRtl,
}

const CONFIGS: [Config; 3] = [Config::Vertical, Config::HorizontalLtr, Config::HorizontalRtl];

/// Eight 5 px cells in a scroller `main` px long (density 1), as Compose's `composeScroller`.
fn scroller(config: Config, state: ScrollState, reversed: bool, main: i32) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let cell = |index: i32, size: Modifier| {
            Box!(modifier = size.test_tag(format!("cell {index}")));
        };
        Box!({
            if config == Config::Vertical {
                Column!(
                    modifier = Modifier
                        .size((CROSS.dp(), main.dp()))
                        .test_tag(TAG)
                        .vertical_scroll_with(state, true, None, reversed),
                    {
                        for index in 0..CELLS {
                            cell(index, Modifier.size((CROSS.dp(), CELL.dp())));
                        }
                    }
                );
            } else {
                Row!(
                    modifier = Modifier
                        .size((main.dp(), CROSS.dp()))
                        .test_tag(TAG)
                        .horizontal_scroll_with(state, true, None, reversed),
                    {
                        for index in 0..CELLS {
                            cell(index, Modifier.size((CELL.dp(), CROSS.dp())));
                        }
                    }
                );
            }
        });
    });
    if config == Config::HorizontalRtl {
        ui.host().set_layout_direction(LayoutDirection::Rtl);
        ui.wait_for_idle();
    }
    ui
}

/// Where cell `index` starts along the scroller's axis, from the scroller's start (its right,
/// right to left).
fn cell_start(ui: &mut ComposeTestRule, config: Config, index: i32) -> f32 {
    let scroller = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    let cell = ui.on_node_with_tag(&format!("cell {index}")).fetch_semantics_node().bounds;
    match config {
        Config::Vertical => cell.top - scroller.top,
        Config::HorizontalLtr => cell.left - scroller.left,
        Config::HorizontalRtl => scroller.right - cell.right,
    }
}

/// The cells show the content `offset` px in, as Compose's `validateScroller` checks pixels.
fn validate(ui: &mut ComposeTestRule, config: Config, offset: i32) {
    for index in 0..CELLS {
        assert_eq!(
            cell_start(ui, config, index),
            (index * CELL - offset) as f32,
            "{config:?}: cell {index} with the content {offset} px in"
        );
    }
}

fn scroll_to(ui: &mut ComposeTestRule, state: ScrollState, value: i32) {
    let _ = run(ui, async move { state.scroll_to(value).await });
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, smallContent
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, smallContent_Unscrollable
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, canNotScrollForwardOrBackward
#[test]
fn content_that_fits_does_not_scroll() {
    for config in CONFIGS {
        let state = ScrollState::new(0);
        let mut ui = scroller(config, *state, false, 40);
        validate(&mut ui, config, 0);
        assert_eq!(state.max_value(), 0);
        assert!(!state.can_scroll_forward() && !state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, largeContent_NoScroll
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, largeContent_ScrollToEnd
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, canScrollForward
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, canScrollBackward
#[test]
fn large_content_scrolls_to_its_end() {
    for config in CONFIGS {
        let state = ScrollState::new(0);
        let mut ui = scroller(config, *state, false, 30);
        validate(&mut ui, config, 0);
        assert_eq!(state.max_value(), 10);
        assert!(state.can_scroll_forward() && !state.can_scroll_backward());
        scroll_to(&mut ui, *state, 10);
        validate(&mut ui, config, 10);
        assert_eq!(state.value(), 10);
        assert!(!state.can_scroll_forward() && state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, canScrollForwardAndBackward
#[test]
fn halfway_it_scrolls_both_ways() {
    for config in CONFIGS {
        let state = ScrollState::new(0);
        let mut ui = scroller(config, *state, false, 30);
        assert_eq!(state.max_value() / 2, 5);
        scroll_to(&mut ui, *state, 5);
        assert_eq!((state.value(), state.max_value()), (5, 10));
        assert!(state.can_scroll_forward() && state.can_scroll_backward());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, reversed
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, largeContent_Reversed_ScrollToEnd
#[test]
fn reversed_scrolling_starts_at_the_end() {
    for config in CONFIGS {
        let state = ScrollState::new(0);
        let mut ui = scroller(config, *state, true, 30);
        validate(&mut ui, config, CELLS * CELL - 30);
        let state = ScrollState::new(0);
        let mut ui = scroller(config, *state, true, 20);
        scroll_to(&mut ui, *state, 10);
        validate(&mut ui, config, CELLS * CELL - 20 - 10);
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, viewPortSize_shouldRepresentScrollableLayoutSize_contentFits
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, viewPortSize_shouldRepresentScrollableLayoutSize_contentDoesNotFit
#[test]
fn the_viewport_size_is_the_scrollers() {
    for (config, main) in [(Config::Vertical, CELLS * CELL), (Config::HorizontalLtr, 30)] {
        let state = ScrollState::new(0);
        let _ui = scroller(config, *state, false, main);
        assert_eq!(state.viewport_size(), main, "{config:?}");
    }
}

/// A hundred lines of text in a 100 dp box, as Compose's `createScrollableContent`.
fn scrollable_content(
    config: Config,
    state: ScrollState,
    count: impl Fn() -> usize + 'static,
    enabled: bool,
) -> ComposeTestRule {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
        let count = count();
        Box!({
            Box!(modifier = Modifier.size(100.dp()), {
                if config == Config::Vertical {
                    Column!(
                        modifier = Modifier
                            .test_tag(TAG)
                            .vertical_scroll_with(state, enabled, None, false),
                        {
                            for index in 0..count {
                                BasicText!("{}", index);
                            }
                        }
                    );
                } else {
                    Row!(
                        modifier = Modifier
                            .test_tag(TAG)
                            .horizontal_scroll_with(state, enabled, None, false),
                        {
                            for index in 0..count {
                                BasicText!("{}", index);
                            }
                        }
                    );
                }
            });
        });
    });
    if config == Config::HorizontalRtl {
        ui.host().set_layout_direction(LayoutDirection::Rtl);
        ui.wait_for_idle();
    }
    ui
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scroller_coerce_whenScrollTo
#[test]
fn scrolls_are_clamped_to_the_range_at_once() {
    for config in CONFIGS {
        let owner = ScrollState::new(0);
        let state = *owner;
        let mut ui = scrollable_content(config, state, || 100, true);
        assert_eq!(state.value(), 0);
        assert!(state.max_value() > 0);
        let by = |ui: &mut ComposeTestRule, delta: f32| {
            let _ = run(ui, async move { state.scroll_by(delta).await });
        };
        by(&mut ui, -100.0);
        assert_eq!(state.value(), 0, "{config:?}");
        by(&mut ui, -100.0);
        assert_eq!(state.value(), 0);
        scroll_to(&mut ui, state, state.max_value());
        assert_eq!(state.value(), state.max_value());
        // Never past it, not even until the next layout.
        let _ = launch(async move {
            let _ = state.scroll_to(state.max_value() + 1000).await;
        });
        compose_runtime::platform::run_ready();
        assert_eq!(state.value(), state.max_value());
        ui.wait_for_idle();
        by(&mut ui, 100.0);
        assert_eq!(state.value(), state.max_value());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, largeContent_coerceWhenMaxChanges
#[test]
fn the_offset_follows_a_shrinking_range() {
    let state = ScrollState::new(0);
    let count = mutable_state_of(100usize);
    let count_handle = count.handle();
    let mut ui = scrollable_content(Config::Vertical, *state, move || count_handle.get(), true);
    assert_eq!(state.value(), 0);
    let max = state.max_value();
    assert!(max > 0);
    scroll_to(&mut ui, *state, max);
    count_handle.set(98);
    ui.wait_for_idle();
    let new_max = state.max_value();
    assert!(new_max < max);
    assert_eq!(state.value(), new_max);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scroller_coerce_whenScrollSmoothTo
#[test]
fn animated_scrolls_are_clamped_to_the_range() {
    for config in CONFIGS {
        let owner = ScrollState::new(0);
        let state = *owner;
        let mut ui = scrollable_content(config, state, || 100, true);
        let max = state.max_value();
        assert!(max > 0);
        let to = |ui: &mut ComposeTestRule, value: i32| {
            let _ = run(ui, async move { state.animate_scroll_to(value, spring()).await });
        };
        let by = |ui: &mut ComposeTestRule, delta: f32| {
            let _ = run(ui, async move { state.animate_scroll_by(delta, spring()).await });
        };
        to(&mut ui, -100);
        assert_eq!(state.value(), 0, "{config:?}");
        by(&mut ui, -100.0);
        assert_eq!(state.value(), 0);
        to(&mut ui, max);
        assert_eq!(state.value(), max);
        to(&mut ui, max + 1000);
        assert_eq!(state.value(), max);
        by(&mut ui, 100.0);
        assert_eq!(state.value(), max);
    }
}

/// The middle of the scroller across its axis (a column of text is as wide as its widest line).
fn scroller_center_x(ui: &mut ComposeTestRule) -> f32 {
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    (bounds.left + bounds.right) / 2.0
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scroller_whenFling_stopsByTouchDown
#[test]
fn a_touch_down_stops_a_fling() {
    let owner = ScrollState::new(0);
    let state = *owner;
    let mut ui = scrollable_content(Config::Vertical, state, || 100, true);
    ui.set_auto_advance(false);
    assert_eq!(state.value(), 0);
    assert!(!state.is_scroll_in_progress());
    // A quick swipe up from the bottom: a fling follows.
    let x = scroller_center_x(&mut ui);
    swipe(&mut ui, Offset::new(x, 90.0), Offset::new(x, 10.0), 50);
    compose_runtime::platform::run_ready();
    assert!(state.is_scroll_in_progress(), "flinging");
    let at_fling_start = state.value();
    ui.advance_time_by(Duration::from_millis(100));
    let when_interrupted = state.value();
    assert!(when_interrupted > at_fling_start);
    let finger = Touch::down(&mut ui, Offset::new(x, 50.0));
    ui.advance_time_by(Duration::from_millis(100));
    assert_eq!(state.value(), when_interrupted, "the fling stopped");
    finger.up(&mut ui);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scroller_touchInputEnabled_shouldHaveSemanticsInfo
// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scroller_touchInputDisabled_shouldHaveSemanticsInfo
#[test]
fn the_semantics_follow_the_offset_enabled_or_not() {
    for enabled in [true, false] {
        let owner = ScrollState::new(0);
        let state = *owner;
        let mut ui = scrollable_content(Config::Vertical, state, || 100, enabled);
        let x = scroller_center_x(&mut ui);
        swipe(&mut ui, Offset::new(x, 90.0), Offset::new(x, 10.0), 200);
        ui.wait_for_idle();
        let node = ui.on_node_with_tag(TAG).fetch_semantics_node();
        let range = node.config.vertical_scroll_axis_range.expect("a vertical scroll range");
        assert_eq!(range.value, state.value() as f32);
        assert_eq!(state.value() > 0, enabled, "only an enabled scroller takes the swipe");
    }
}

/// Compose's `configAwareSwipe`: a swipe over the scroller in 200 ms, towards its content's end
/// (up; left; right to left, right) or back.
fn config_aware_swipe(ui: &mut ComposeTestRule, config: Config, forward: bool) {
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    let (center_x, center_y) =
        ((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0);
    let (from, to) = match config {
        Config::Vertical => {
            (Offset::new(center_x, bounds.bottom - 1.0), Offset::new(center_x, bounds.top))
        }
        Config::HorizontalLtr => {
            (Offset::new(bounds.right - 1.0, center_y), Offset::new(bounds.left, center_y))
        }
        Config::HorizontalRtl => {
            (Offset::new(bounds.left, center_y), Offset::new(bounds.right - 1.0, center_y))
        }
    };
    let (from, to) = if forward { (from, to) } else { (to, from) };
    swipe(ui, from, to, 200);
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, swipeForward_swipeBackward
#[test]
fn a_swipe_forward_then_back_scrolls_both_ways() {
    for config in CONFIGS {
        let owner = ScrollState::new(0);
        let state = *owner;
        let mut ui = scrollable_content(config, state, || 100, true);
        ui.set_auto_advance(false);
        assert_eq!(state.value(), 0);
        config_aware_swipe(&mut ui, config, true);
        ui.advance_time_by(Duration::from_millis(5000));
        let scrolled = state.value();
        assert!(scrolled > 0, "{config:?}");
        config_aware_swipe(&mut ui, config, false);
        ui.advance_time_by(Duration::from_millis(5000));
        assert!(state.value() < scrolled, "{config:?}: {} after {scrolled}", state.value());
    }
}

// Compose: compose/foundation/foundation/src/androidDeviceTest/kotlin/androidx/compose/foundation/ScrollTest.kt, scrollStateMaxValue_changesOnResize_beforePlacement
#[test]
fn the_maximum_is_known_once_measured_before_placement() {
    for config in CONFIGS {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let into = seen.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(300, 300), move || {
            let state = remember_scroll_state!();
            let (at_first, on_size) = (into.clone(), into.clone());
            DisposableEffect(state, move |scope| {
                at_first.borrow_mut().push(state.max_value());
                scope.on_dispose(|| {})
            });
            let scroll = if config == Config::Vertical {
                Modifier.vertical_scroll(state)
            } else {
                Modifier.horizontal_scroll(state)
            };
            // Called after the measurement, before the placement.
            let modifier = Modifier
                .size(100.dp())
                .on_size_changed(move |_| on_size.borrow_mut().push(state.max_value()))
                .then(scroll);
            Box!(modifier = modifier, {
                Spacer(Modifier.size(100.dp()));
            });
        });
        if config == Config::HorizontalRtl {
            ui.host().set_layout_direction(LayoutDirection::Rtl);
        }
        ui.wait_for_idle();
        assert_eq!(*seen.borrow(), vec![i32::MAX, 0], "{config:?}");
    }
}
