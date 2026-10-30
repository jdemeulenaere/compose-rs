//! Compose's `RangeSlider` tests of `SliderTest.kt`, adapted, and its goldens of
//! `SliderScreenshotTest.kt` as the display list's shapes. Compose's `onAllNodes(isFocusable())`
//! are the thumbs' tags here.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, IntSize, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Offset,
    ProgressBarRangeInfo, SemanticsActions, SemanticsProperties, UnitExt, platform::DrawOp,
};

const TAG: &str = "slider";
const SLOP: f32 = 8.0;

fn rule(state: RangeSliderState, direction: LayoutDirection) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                RangeSlider!(
                    state,
                    move |(start, end)| {
                        state.set_start_value(start);
                        state.set_end_value(end);
                    },
                    modifier = Modifier.test_tag(TAG)
                );
            });
        });
    })
}

/// The slider's left, right and center x, y.
fn frame(ui: &mut ComposeTestRule) -> (f32, f32, f32, f32) {
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().bounds;
    (
        bounds.left,
        bounds.right,
        (bounds.left + bounds.right) / 2.0,
        (bounds.top + bounds.bottom) / 2.0,
    )
}

fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.01, "{actual} is not {expected}");
}

/// As `calculateFraction`: where the value would be for a pointer at `x`, thumb widths left out.
fn value_at(left: f32, right: f32, x: f32) -> f32 {
    ((x - left - 2.0) / (right - left - 4.0)).clamp(0.0, 1.0)
}

fn drag(ui: &mut ComposeTestRule, from: Offset, moves: &[f32]) {
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.down(from);
        for by in moves {
            touch.move_by(Offset::new(*by, 0.0));
        }
        touch.up();
    });
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_dragThumb
#[test]
fn a_drag_moves_the_nearer_thumb() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x + 10.0, y), &[SLOP, 100.0]);
    assert_eq!(state.start_value(), 0.0);
    near(state.end_value(), value_at(left, right, x + 10.0 + SLOP + 100.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_drag_out_of_bounds
#[test]
fn a_drag_out_of_bounds_and_back_keeps_its_place() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    let width = right - left;
    drag(&mut ui, Offset::new(x + 10.0, y), &[SLOP, width, -width, -width, width + 100.0]);
    assert_eq!(state.start_value(), 0.0);
    near(state.end_value(), value_at(left, right, x + 10.0 + SLOP + 100.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_drag_overlap_thumbs
#[test]
fn thumbs_stop_at_each_other() {
    let state = RangeSliderState::new(0.5, 1.0, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    let width = right - left;
    drag(&mut ui, Offset::new(right - 1.0, y), &[-SLOP, -width]);
    assert_eq!((state.start_value(), state.end_value()), (0.5, 0.5));
    drag(&mut ui, Offset::new(x, y), &[-SLOP, -width]);
    assert_eq!((state.start_value(), state.end_value()), (0.0, 0.5));
}

/// What the callback hears stops at the track's ends and the other thumb, whatever the pixels round
/// to, as `onDrag`'s coercions: before the state's setters, which would hide it.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Slider.kt, onDrag
#[test]
fn what_a_drag_asks_stops_at_the_ends_and_the_other_thumb() {
    let run = |start: f32, end: f32, from_end: bool, by: f32| {
        let state = RangeSliderState::new(start, end, 0, (0.0, 1.0));
        let handle = *state;
        let heard = Rc::new(Cell::new((start, end)));
        let last = heard.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            let last = last.clone();
            MaterialTheme!({
                let last = last.clone();
                RangeSlider!(
                    handle,
                    move |range: (f32, f32)| {
                        last.set(range);
                        handle.set_start_value(range.0);
                        handle.set_end_value(range.1);
                    },
                    modifier = Modifier.test_tag(TAG)
                );
            });
        });
        let (left, right, _, y) = frame(&mut ui);
        let at = if from_end { end } else { start };
        let x = left + 2.0 + at * (right - left - 4.0);
        drag(&mut ui, Offset::new(x, y), &[by.signum() * SLOP, by * (right - left)]);
        heard.get()
    };
    for hundredth in 20..90 {
        let at = hundredth as f32 / 100.0;
        // Past the other thumb.
        let (start, end) = run(0.1, at, false, 1.0);
        assert!(start <= end, "{start} past {end}");
        near(start, at);
        let (start, end) = run(at, 1.0, true, -1.0);
        assert!(start <= end, "{start} past {end}");
        near(end, at);
        // Past its end.
        assert_eq!(run(at, 1.0, false, -1.0).0, 0.0);
        assert_eq!(run(0.0, at, true, 1.0).1, 1.0);
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_tap
#[test]
fn a_tap_moves_the_nearer_thumb_there() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x + 50.0, y), &[]);
    near(state.end_value(), value_at(left, right, x + 50.0));
    assert_eq!(state.start_value(), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_drag_rtl
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_drag_out_of_bounds_rtl
#[test]
fn right_to_left_a_drag_runs_the_other_way() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Rtl);
    let (left, right, x, y) = frame(&mut ui);
    let width = right - left;
    // From left of the center, the end thumb in this direction, away from its place at the left.
    drag(&mut ui, Offset::new(x - 10.0, y), &[-SLOP, width, -width, -width, width - 100.0]);
    assert_eq!(state.start_value(), 0.0);
    near(state.end_value(), value_at(left, right, x + 10.0 + SLOP + 100.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_closeThumbs_dragRight
#[test]
fn thumbs_on_each_other_a_drag_right_takes_the_end() {
    let state = RangeSliderState::new(0.5, 0.5, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x, y), &[SLOP, 100.0]);
    assert_eq!(state.start_value(), 0.5);
    near(state.end_value(), value_at(left, right, x + SLOP + 100.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_closeThumbs_dragLeft
#[test]
fn thumbs_on_each_other_a_drag_left_takes_the_start() {
    let state = RangeSliderState::new(0.5, 0.5, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x, y), &[-SLOP, -100.0]);
    assert_eq!(state.end_value(), 0.5);
    near(state.start_value(), value_at(left, right, x - SLOP - 100.0));
}

/// Right to left, a drag right is towards the start: the start thumb's.
#[test]
fn right_to_left_thumbs_on_each_other_a_drag_right_takes_the_start() {
    let state = RangeSliderState::new(0.5, 0.5, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Rtl);
    let (_, _, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x, y), &[SLOP, 100.0]);
    assert_eq!(state.end_value(), 0.5);
    assert!(state.start_value() < 0.5, "{}", state.start_value());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_overlappingThumbs_tapUpdatesValue
#[test]
fn a_tap_beside_overlapping_thumbs_moves_the_one_on_its_side() {
    let state = RangeSliderState::new(0.5, 0.8, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (left, right, x, y) = frame(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.down(Offset::new(x, y));
        touch.move_to(Offset::new(right, y));
        touch.up();
    });
    ui.wait_for_idle();
    near(state.start_value(), 0.8);
    near(state.end_value(), 0.8);
    let tap = left + (right - left) * 0.9;
    drag(&mut ui, Offset::new(tap, y), &[]);
    near(state.start_value(), 0.8);
    near(state.end_value(), value_at(left, right, tap));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_weightModifier
#[test]
fn a_weight_on_it_shares_the_row() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            Row!(|row: RowScope| {
                Box!(modifier = Modifier.size(100.dp()));
                RangeSlider!(handle, |_| {}, modifier = Modifier.weight(row, 1.0).test_tag(TAG));
            });
        });
    });
    let (left, right, _, _) = frame(&mut ui);
    assert_eq!((left, right), (100.0, 400.0));
}

/// The thumbs by tag, a thumb's own node being its tag's parent.
fn thumbs_rule(state: RangeSliderState) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            RangeSlider!(
                state,
                move |(start, end)| {
                    state.set_start_value(start);
                    state.set_end_value(end);
                },
                modifier = Modifier.test_tag(TAG),
                start_thumb = Some(|_: RangeSliderState| {
                    SliderDefaults::thumb(
                        remember(compose_ui::MutableInteractionSource::new),
                        Modifier.test_tag("start"),
                        SliderDefaults::colors(),
                        true,
                        SliderDefaults::THUMB_SIZE,
                    );
                }),
                end_thumb = Some(|_: RangeSliderState| {
                    SliderDefaults::thumb(
                        remember(compose_ui::MutableInteractionSource::new),
                        Modifier.test_tag("end"),
                        SliderDefaults::colors(),
                        true,
                        SliderDefaults::THUMB_SIZE,
                    );
                }),
            );
        });
    })
}

fn info(ui: &mut ComposeTestRule, tag: &str) -> ProgressBarRangeInfo {
    let node = ui.on_node_unmerged(has_test_tag(tag)).on_parent().fetch_semantics_snapshot();
    *node.config.get(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO).expect("a range")
}

fn range(current: f32, range: (f32, f32), steps: u32) -> ProgressBarRangeInfo {
    ProgressBarRangeInfo { current, range, steps }
}

fn set_progress(ui: &mut ComposeTestRule, tag: &str, value: f32) {
    let node = ui.on_node_unmerged(has_test_tag(tag)).on_parent().fetch_semantics_snapshot();
    let action = node.config.get(&SemanticsActions::SET_PROGRESS).expect("set progress").clone();
    assert!((action.action.expect("its action"))(value));
    ui.wait_for_idle();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_semantics_continuous
#[test]
fn each_thumb_reports_the_range_it_can_reach_and_takes_set_progress() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = thumbs_rule(*state);
    assert_eq!(info(&mut ui, "start"), range(0.0, (0.0, 1.0), 0));
    assert_eq!(info(&mut ui, "end"), range(1.0, (0.0, 1.0), 0));
    state.set_start_value(0.5);
    state.set_end_value(0.75);
    ui.wait_for_idle();
    assert_eq!(info(&mut ui, "start"), range(0.5, (0.0, 0.75), 0));
    assert_eq!(info(&mut ui, "end"), range(0.75, (0.5, 1.0), 0));
    set_progress(&mut ui, "start", 0.6);
    set_progress(&mut ui, "end", 0.8);
    assert_eq!(info(&mut ui, "start"), range(0.6, (0.0, 0.8), 0));
    assert_eq!(info(&mut ui, "end"), range(0.8, (0.6, 1.0), 0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_semantics_stepped
#[test]
fn each_thumb_reports_its_own_steps() {
    let state = RangeSliderState::new(0.0, 20.0, 3, (0.0, 20.0));
    let mut ui = thumbs_rule(*state);
    state.set_start_value(5.0);
    state.set_end_value(10.0);
    ui.wait_for_idle();
    assert_eq!(info(&mut ui, "start"), range(5.0, (0.0, 10.0), 1));
    assert_eq!(info(&mut ui, "end"), range(10.0, (5.0, 20.0), 2));
    set_progress(&mut ui, "start", 10.0);
    set_progress(&mut ui, "end", 15.0);
    assert_eq!(info(&mut ui, "start"), range(10.0, (0.0, 15.0), 2));
    assert_eq!(info(&mut ui, "end"), range(15.0, (10.0, 20.0), 1));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_thumbs_semanticsNodeBounds
#[test]
fn a_thumbs_node_is_wider_than_the_thumb() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = thumbs_rule(*state);
    for tag in ["start", "end"] {
        let node = ui.on_node_unmerged(has_test_tag(tag)).on_parent().fetch_semantics_snapshot();
        // Compose's is 14: its `boundsInRoot` is clipped by the slider's edge, a thumb being at its
        // end. A node's bounds here aren't clipped, so 10 dp on both sides: 24.
        assert_eq!((node.bounds.width(), node.bounds.height()), (4.0 + 20.0, 44.0), "{tag}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_thumbs_visualBounds
#[test]
fn a_thumb_is_4_by_44_dp() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = thumbs_rule(*state);
    for tag in ["start", "end"] {
        let bounds = ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds;
        assert_eq!((bounds.width(), bounds.height()), (4.0, 44.0), "{tag}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_parentWithInfiniteWidth_minWidth
#[test]
fn with_infinite_width_it_takes_its_minimum() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Row!(modifier = Modifier.horizontal_scroll(remember_scroll_state!()), {
                RangeSlider!(handle, |_| {}, modifier = Modifier.test_tag(TAG));
            });
        });
    });
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_snapshot().bounds;
    assert_eq!(bounds.width(), 48.0);
}

/// In a row with room without end, a weight has nothing to share: it lays out.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_rowWithInfiniteWidth
#[test]
fn a_weighted_slider_in_an_infinite_row_lays_out() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Box!(modifier = Modifier.horizontal_scroll(remember_scroll_state!()), {
                Row!(|row: RowScope| {
                    RangeSlider!(
                        handle,
                        |_| {},
                        modifier = Modifier.weight(row, 1.0).test_tag(TAG)
                    );
                });
            });
        });
    });
    ui.on_node_with_tag(TAG).assert_exists();
}

/// A state made for a new range, a tap goes by it.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_tap_rangeChange
#[test]
fn a_tap_goes_by_a_new_states_range() {
    let range_end = mutable_state_of(0.25f32);
    let read = range_end.handle();
    let held = Rc::new(RefCell::new(None));
    let current = held.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let current = current.clone();
        MaterialTheme!({
            let end = read.get();
            let state =
                remember_keyed(end.to_bits(), || RangeSliderState::new(0.0, 25.0, 0, (0.0, end)));
            let handle = state;
            *current.borrow_mut() = Some(handle);
            RangeSlider!(
                handle,
                move |(start, end)| {
                    handle.set_start_value(start);
                    handle.set_end_value(end);
                },
                modifier = Modifier.test_tag(TAG)
            );
        });
    });
    range_end.set(1.0);
    ui.wait_for_idle();
    let (left, right, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x + 50.0, y), &[]);
    let state = held.borrow().expect("composed");
    near(state.end_value(), value_at(left, right, x + 50.0));
}

/// A callback that opens a snackbar doesn't get in the way of the drag it ends.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_onValueChangeFinishedWithSnackbar
#[test]
fn a_drags_end_can_show_a_snackbar() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let host = SnackbarHostState::new();
    let shown = *host;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            let scope = remember_coroutine_scope();
            Scaffold!(
                snackbar_host = || {
                    SnackbarHost!(shown);
                },
                |_| {
                    RangeSlider!(
                        handle,
                        move |(start, end)| {
                            handle.set_start_value(start);
                            handle.set_end_value(end);
                        },
                        modifier = Modifier.test_tag(TAG),
                        on_value_change_finished = Some(move || {
                            scope.launch(async move {
                                shown.show_snackbar("Snackbar Description").await;
                            });
                        })
                    );
                }
            );
        });
    });
    let (left, right, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x, y), &[SLOP, 100.0]);
    assert_eq!(state.start_value(), 0.0);
    near(state.end_value(), value_at(left, right, x + SLOP + 100.0));
    ui.on_node_with_text("Snackbar Description").assert_exists();
}

thread_local! {
    static START_OUTER: Cell<u32> = const { Cell::new(0) };
    static START_INNER: Cell<u32> = const { Cell::new(0) };
    static END_OUTER: Cell<u32> = const { Cell::new(0) };
    static END_INNER: Cell<u32> = const { Cell::new(0) };
}

#[composable]
fn OuterContent(state: RangeSliderState) {
    SideEffect(|| START_OUTER.with(|count| count.set(count.get() + 1)));
    Column!({
        Text!("OuterContent");
        InnerContent(state);
    });
}

#[composable]
fn InnerContent(state: RangeSliderState) {
    SideEffect(|| START_INNER.with(|count| count.set(count.get() + 1)));
    Text!("InnerContent: {}..{}", state.start_value(), state.end_value());
}

#[composable]
fn EndOuterContent(state: RangeSliderState) {
    SideEffect(|| END_OUTER.with(|count| count.set(count.get() + 1)));
    Column!({
        Text!("OuterContent");
        EndInnerContent(state);
    });
}

#[composable]
fn EndInnerContent(state: RangeSliderState) {
    SideEffect(|| END_INNER.with(|count| count.set(count.get() + 1)));
    Text!("InnerContent: {}..{}", state.start_value(), state.end_value());
}

fn reset_counts() {
    for count in [&START_OUTER, &START_INNER, &END_OUTER, &END_INNER] {
        count.with(|count| count.set(0));
    }
}

/// Down at its center, and three moves back and forth, each composed.
fn drag_back_and_forth(ui: &mut ComposeTestRule) {
    let (_, _, x, y) = frame(ui);
    let mut at = Offset::new(x, y);
    ui.host().send_pointer(compose_ui::PointerEvent::press(at));
    for dx in [100.0, -100.0, 100.0] {
        at = Offset::new(at.x + dx, at.y);
        ui.host().send_pointer(compose_ui::PointerEvent::moved(at, true));
        ui.wait_for_idle();
    }
}

/// The thumbs' slots compose once from the outside; what reads the values each time they change.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_thumb_recomposition
#[test]
fn a_drag_recomposes_only_what_reads_the_values_in_the_thumbs() {
    reset_counts();
    let state = RangeSliderState::new(0.0, 100.0, 0, (0.0, 100.0));
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            RangeSlider!(
                handle,
                move |(start, end)| {
                    handle.set_start_value(start);
                    handle.set_end_value(end);
                },
                modifier = Modifier.test_tag(TAG),
                start_thumb = Some(OuterContent),
                end_thumb = Some(EndOuterContent),
            );
        });
    });
    drag_back_and_forth(&mut ui);
    assert_eq!(START_OUTER.with(Cell::get), 1);
    assert_eq!(END_OUTER.with(Cell::get), 1);
    // The first composition and each move, composed before the next: Compose's count of 2 has its
    // moves come together, as `slider.rs`'s single slider counts them too.
    assert_eq!(START_INNER.with(Cell::get), 4);
    assert_eq!(END_INNER.with(Cell::get), 4);
}

/// The same for the track, 3 as Compose's: it's its text that sizes the slider here, too small
/// for the first move to change the end, already at the last place.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_track_recomposition
#[test]
fn a_drag_recomposes_only_what_reads_the_values_in_the_track() {
    reset_counts();
    let state = RangeSliderState::new(0.0, 100.0, 0, (0.0, 100.0));
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            RangeSlider!(
                handle,
                move |(start, end)| {
                    handle.set_start_value(start);
                    handle.set_end_value(end);
                },
                modifier = Modifier.test_tag(TAG),
                track = Some(OuterContent),
            );
        });
    });
    drag_back_and_forth(&mut ui);
    assert_eq!(START_OUTER.with(Cell::get), 1);
    assert_eq!(START_INNER.with(Cell::get), 3);
}

/// A move in the up event itself isn't a drag's, as `drag`, which returns on an up first.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt, drag
#[test]
fn an_up_somewhere_else_moves_nothing() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    let (_, _, x, y) = frame(&mut ui);
    let at = Offset::new(x + 20.0, y);
    ui.host().send_pointer(compose_ui::PointerEvent::press(Offset::new(x, y)));
    ui.host().send_pointer(compose_ui::PointerEvent::moved(at, true));
    ui.wait_for_idle();
    let before = state.end_value();
    ui.host().send_pointer(compose_ui::PointerEvent::release(Offset::new(x + 90.0, y)));
    ui.wait_for_idle();
    assert_eq!(state.end_value(), before);
}

/// The finger dragging lifts with another down: that one goes on with it, and the drag ends with
/// it, as `awaitDragOrUp`.
// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/DragGestureDetector.kt, awaitDragOrUp
#[test]
fn another_finger_takes_over_a_drag() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let finished = Rc::new(Cell::new(0));
    let heard = finished.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let heard = heard.clone();
        MaterialTheme!({
            let heard = heard.clone();
            RangeSlider!(
                handle,
                move |(start, end)| {
                    handle.set_start_value(start);
                    handle.set_end_value(end);
                },
                modifier = Modifier.test_tag(TAG),
                on_value_change_finished = Some(move || heard.set(heard.get() + 1))
            );
        });
    });
    let (left, right, x, y) = frame(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.down_pointer(0, Offset::new(x, y));
        touch.move_pointer_by(0, Offset::new(20.0 + SLOP, 0.0), 0);
        touch.down_pointer(1, Offset::new(x + 100.0, y));
        touch.up_pointer(0);
    });
    ui.wait_for_idle();
    let lifted = state.end_value();
    assert_eq!(finished.get(), 0, "the drag goes on");
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.move_pointer_by(1, Offset::new(30.0, 0.0), 0);
    });
    ui.wait_for_idle();
    near(state.end_value(), lifted + 30.0 / (right - left - 4.0));
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.up_pointer(1));
    ui.wait_for_idle();
    assert_eq!(finished.get(), 1);
}

/// A tap with a little horizontal jitter under the slop is a tap: released, not cancelled.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Slider.kt, rangeSliderPressDragModifier
#[test]
fn a_tap_that_moved_a_little_is_a_release() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let source = compose_ui::MutableInteractionSource::new();
    let end_source = source.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let end_source = end_source.clone();
        MaterialTheme!({
            RangeSlider!(
                handle,
                |_| {},
                modifier = Modifier.test_tag(TAG),
                end_thumb_interaction_source = Some(end_source.clone())
            );
        });
    });
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = seen.clone();
    let interactions = source.interactions();
    compose_runtime::platform::launch(async move {
        interactions.collect(|interaction| log.borrow_mut().push(format!("{interaction:?}"))).await;
    });
    let (_, _, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x + 50.0, y), &[1.0]);
    let seen = seen.borrow();
    assert!(seen.iter().any(|it| it.contains("Release")), "{seen:?}");
    assert!(!seen.iter().any(|it| it.contains("Cancel")), "{seen:?}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_valueUpdatedByLaunchEffectAndInteraction
#[test]
fn values_set_by_an_effect_are_what_a_gesture_starts_from() {
    let state = RangeSliderState::new(0.0, 100.0, 0, (0.0, 100.0));
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            RangeSlider!(
                handle,
                move |(start, end)| {
                    handle.set_start_value(start);
                    handle.set_end_value(end);
                },
                modifier = Modifier.test_tag(TAG)
            );
            LaunchedEffect((), async move {
                handle.set_end_value(50.0);
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!((handle.start_value(), handle.end_value()), (0.0, 50.0));
    let (_, _, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x - 50.0, y), &[]);
    assert_ne!(handle.end_value(), 50.0);
    assert_eq!(handle.start_value(), 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeslider_initialValueOutsideOfRange_doesNotCrash
#[test]
fn values_outside_the_range_are_kept_in_it() {
    let state = RangeSliderState::new(-1.0, -1.0, 0, (0.0, 1.0));
    let _ui = rule(*state, LayoutDirection::Ltr);
    assert_eq!((state.start_value(), state.end_value()), (0.0, 0.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_directCallbacks_invokedDuringDrag
#[test]
fn callbacks_hear_a_drag_and_its_end() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let changes = Rc::new(RefCell::new(Vec::new()));
    let finished = Rc::new(Cell::new(0));
    let (heard, done) = (changes.clone(), finished.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (heard, done) = (heard.clone(), done.clone());
        MaterialTheme!({
            let (heard, done) = (heard.clone(), done.clone());
            RangeSlider!(
                handle,
                move |range| heard.borrow_mut().push(range),
                modifier = Modifier.test_tag(TAG),
                on_value_change_finished = Some(move || done.set(done.get() + 1))
            );
        });
    });
    let (_, _, x, y) = frame(&mut ui);
    drag(&mut ui, Offset::new(x + 10.0, y), &[100.0]);
    assert!(!changes.borrow().is_empty());
    assert_eq!(finished.get(), 1);
}

/// A press goes to the thumb it would take, a drag converts it, on that thumb's source.
#[test]
fn interactions_go_to_the_thumbs_source() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let start = compose_ui::MutableInteractionSource::new();
    let end = compose_ui::MutableInteractionSource::new();
    let (start_source, end_source) = (start.clone(), end.clone());
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let (start_source, end_source) = (start_source.clone(), end_source.clone());
        MaterialTheme!({
            RangeSlider!(
                handle,
                |_| {},
                modifier = Modifier.test_tag(TAG),
                start_thumb_interaction_source = Some(start_source.clone()),
                end_thumb_interaction_source = Some(end_source.clone())
            );
        });
    });
    let (_, _, x, y) = frame(&mut ui);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.down(Offset::new(x + 10.0, y)));
    ui.wait_for_idle();
    assert!(end.is_pressed() && !start.is_pressed());
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.move_by(Offset::new(50.0, 0.0)));
    ui.wait_for_idle();
    assert!(end.is_dragged() && !end.is_pressed() && !start.is_dragged());
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| touch.up());
    ui.wait_for_idle();
    assert!(!end.is_dragged());
}

/// A gesture moving across it isn't ours: the press ends, and nothing moves.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_insideHorizontalScroll_dragMovesThumbNotScroll
#[test]
fn a_drag_across_leaves_the_values_and_a_drag_along_takes_a_scroll() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let handle = *state;
    let scroll = compose_foundation::ScrollState::new(0);
    let scrolled = *scroll;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            Row!(modifier = Modifier.horizontal_scroll(scrolled), {
                RangeSlider!(
                    handle,
                    move |(start, end)| {
                        handle.set_start_value(start);
                        handle.set_end_value(end);
                    },
                    modifier = Modifier.test_tag(TAG).width(500.dp())
                );
            });
        });
    });
    let (_, _, x, y) = frame(&mut ui);
    // One move past the slop: the event crossing it is the thumb's, for a scroll around too.
    drag(&mut ui, Offset::new(x + 10.0, y), &[-60.0, -140.0]);
    assert_eq!(scrolled.value(), 0, "the thumb's drag isn't a scroll");
    assert!(handle.end_value() < 1.0);
    let before = handle.end_value();
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        touch.down(Offset::new(x + 10.0, y));
        touch.move_by(Offset::new(0.0, 30.0));
        touch.up();
    });
    ui.wait_for_idle();
    assert_eq!(handle.end_value(), before);
}

/// Keys, as `rangeSliderOnKeyEvents`: each thumb stopped by the other. Compose has none for these:
/// they mirror `SlideUsingKeysTest`.
#[test]
fn keys_move_the_focused_thumb_up_to_the_other() {
    use compose_ui::Key;
    let state = RangeSliderState::new(0.25, 0.75, 0, (0.0, 1.0));
    let mut ui = rule(*state, LayoutDirection::Ltr);
    ui.press_key(Key::Tab);
    ui.press_key(Key::DirectionRight);
    near(state.start_value(), 0.26);
    ui.press_key(Key::MoveHome);
    assert_eq!(state.start_value(), 0.0);
    ui.press_key(Key::MoveEnd);
    assert_eq!(state.start_value(), 0.75);
    ui.press_key(Key::Tab);
    ui.press_key(Key::DirectionLeft);
    near(state.end_value(), 0.75);
    ui.press_key(Key::MoveEnd);
    assert_eq!(state.end_value(), 1.0);
    ui.press_key(Key::PageDown);
    near(state.end_value(), 0.9);
}

/// The focus on a thumb with an inset ring: the ring on it, it and the track staying put.
#[test]
fn a_focused_thumb_shows_the_inset_ring() {
    let state = RangeSliderState::new(0.25, 0.75, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        MaterialTheme!({
            CompositionLocalProvider(
                LOCAL_RIPPLE_THEME_CONFIGURATION
                    .provides(RippleDefaults::INSET_FOCUS_RING_THEME_CONFIGURATION),
                || {
                    RangeSlider!(
                        handle,
                        |_| {},
                        modifier = Modifier.test_tag(TAG),
                        start_thumb = Some(|_: RangeSliderState| {
                            Box!(modifier = Modifier.size((4.dp(), 44.dp())).test_tag("start"));
                        }),
                        end_thumb = Some(|_: RangeSliderState| {
                            Box!(modifier = Modifier.size((4.dp(), 44.dp())).test_tag("end"));
                        })
                    );
                },
            );
        });
    });
    let bounds = |ui: &mut ComposeTestRule, tag: &str| {
        ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
    };
    let (start, end) = (bounds(&mut ui, "start"), bounds(&mut ui, "end"));
    ui.press_key(compose_ui::Key::Tab);
    ui.wait_for_idle();
    assert_eq!((bounds(&mut ui, "start"), bounds(&mut ui, "end")), (start, end));
    let secondary = light_color_scheme().secondary;
    let list = ui.host().display_list();
    assert!(
        list.ops.iter().any(|op| matches!(op,
            DrawOp::DrawRoundRect(_, _, paint) if paint.color == secondary)),
        "{:?}",
        list.ops
    );
}

/// A part of the track in `color`: left, right, its left corner and its right one.
fn parts(ui: &mut ComposeTestRule, color: Color) -> Vec<(f32, f32, f32, f32)> {
    let mut at = 0.0;
    let mut stack = Vec::new();
    let mut found = Vec::new();
    for op in &ui.host().display_list().ops {
        match op {
            DrawOp::Save => stack.push(at),
            DrawOp::Restore => at = stack.pop().unwrap_or(0.0),
            DrawOp::Translate(by) => at += by.x,
            // The track's height: the thumbs are in the same color.
            DrawOp::DrawRoundRect(rect, radii, paint)
                if paint.color == color && rect.height() == 16.0 =>
            {
                found.push((rect.left + at, rect.right + at, radii[0], radii[1]));
            }
            _ => {}
        }
    }
    found
}

/// A dot or tick in `color`: its center x.
fn dots(ui: &mut ComposeTestRule, color: Color) -> Vec<f32> {
    let mut at = 0.0;
    let mut stack = Vec::new();
    let mut found = Vec::new();
    for op in &ui.host().display_list().ops {
        match op {
            DrawOp::Save => stack.push(at),
            DrawOp::Restore => at = stack.pop().unwrap_or(0.0),
            DrawOp::Translate(by) => at += by.x,
            DrawOp::DrawArc { oval, paint, .. } if paint.color == color => {
                found.push(at + (oval.left + oval.right) / 2.0);
            }
            _ => {}
        }
    }
    found
}

fn screenshot(
    start: f32,
    end: f32,
    steps: u32,
    direction: LayoutDirection,
    track: Option<fn(RangeSliderState)>,
) -> ComposeTestRule {
    ComposeTestRule::sized(IntSize::new(200, 48), move || {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
                let state = remember_range_slider_state!(start, end, steps);
                match track {
                    Some(track) => RangeSlider!(state, |_| {}, track = Some(track)),
                    None => RangeSlider!(state, |_| {}),
                }
            });
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_middle_no_gap
#[test]
fn in_the_middle_three_parts_are_drawn_with_a_gap_around_each_thumb() {
    let scheme = light_color_scheme();
    let mut ui = screenshot(0.25, 0.75, 0, LayoutDirection::Ltr, None);
    // A 196 dp track from 2: thumbs at 49 and 147 of it, each with 2 dp of half and 6 dp of gap.
    assert_eq!(
        parts(&mut ui, scheme.secondary_container),
        vec![(2.0, 2.0 + 49.0 - 8.0, 8.0, 2.0), (2.0 + 147.0 + 8.0, 198.0, 2.0, 8.0)]
    );
    assert_eq!(
        parts(&mut ui, scheme.primary),
        vec![(2.0 + 49.0 + 8.0, 2.0 + 147.0 - 8.0, 2.0, 2.0)]
    );
    assert_eq!(dots(&mut ui, scheme.primary), vec![10.0, 190.0]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_middle_no_inside_corner_rtl
#[test]
fn right_to_left_the_parts_are_mirrored() {
    let scheme = light_color_scheme();
    let mut ui = screenshot(0.25, 0.75, 0, LayoutDirection::Rtl, None);
    assert_eq!(
        parts(&mut ui, scheme.secondary_container),
        vec![(157.0, 198.0, 2.0, 8.0), (2.0, 43.0, 8.0, 2.0)]
    );
    assert_eq!(parts(&mut ui, scheme.primary), vec![(59.0, 141.0, 2.0, 2.0)]);
    assert_eq!(dots(&mut ui, scheme.primary), vec![190.0, 10.0]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_middle_steps_enabled
#[test]
fn steps_draw_ticks_but_in_the_gaps_and_at_the_stop_dots() {
    let scheme = light_color_scheme();
    let mut ui = screenshot(0.0, 1.0, 9, LayoutDirection::Ltr, None);
    // No inactive part with the thumbs at the ends; 11 stops, the 2 at the ends in their gaps, the
    // rest on the active part, in the inactive track's color.
    assert!(parts(&mut ui, scheme.secondary_container).is_empty());
    assert_eq!(dots(&mut ui, scheme.secondary_container).len(), 9);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_overlappingThumbs
#[test]
fn overlapping_thumbs_draw_no_active_part() {
    let scheme = light_color_scheme();
    let mut ui = screenshot(0.5, 0.5, 0, LayoutDirection::Ltr, None);
    assert!(parts(&mut ui, scheme.primary).is_empty());
    assert_eq!(parts(&mut ui, scheme.secondary_container).len(), 2);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_fullRange
#[test]
fn a_full_range_is_the_active_part_alone() {
    let scheme = light_color_scheme();
    let mut ui = screenshot(0.0, 1.0, 0, LayoutDirection::Ltr, None);
    assert!(parts(&mut ui, scheme.secondary_container).is_empty());
    assert_eq!(parts(&mut ui, scheme.primary), vec![(2.0 + 8.0, 198.0 - 8.0, 2.0, 2.0)]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_middle_no_gap
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_middle_no_stop_indicator
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderScreenshotTest.kt, rangeSliderTest_middle_no_inside_corner
#[test]
fn a_custom_track_takes_its_gap_corners_and_stop_dots() {
    fn track(state: RangeSliderState) {
        SliderDefaults::range_track_with(
            state,
            Modifier,
            true,
            SliderDefaults::colors(),
            None,
            SliderDefaults::default_draw_tick(),
            compose_ui::Dp(0.0),
            compose_ui::Dp(0.0),
            compose_ui::Dp::UNSPECIFIED,
        );
    }
    let scheme = light_color_scheme();
    let mut ui = screenshot(0.25, 0.75, 0, LayoutDirection::Ltr, Some(track));
    assert_eq!(
        parts(&mut ui, scheme.secondary_container),
        vec![(2.0, 51.0, 8.0, 0.0), (149.0, 198.0, 0.0, 8.0)]
    );
    assert_eq!(parts(&mut ui, scheme.primary), vec![(51.0, 149.0, 0.0, 0.0)]);
    assert!(dots(&mut ui, scheme.primary).is_empty());
}

/// What a key asks for is limited by the other thumb before it reaches the state: the callback hears
/// it, as `rangeSliderOnKeyEvents`.
#[test]
fn a_keys_range_stops_at_the_other_thumb() {
    use compose_ui::Key;
    let state = RangeSliderState::new(0.25, 0.75, 0, (0.0, 1.0));
    let handle = *state;
    let heard = Rc::new(Cell::new((0.0, 0.0)));
    let last = heard.clone();
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
        let last = last.clone();
        MaterialTheme!({
            let last = last.clone();
            RangeSlider!(handle, move |range| last.set(range), modifier = Modifier.test_tag(TAG));
        });
    });
    ui.press_key(Key::Tab);
    ui.press_key(Key::MoveEnd);
    assert_eq!(heard.get(), (0.75, 0.75));
    ui.press_key(Key::Tab);
    ui.press_key(Key::MoveHome);
    assert_eq!(heard.get(), (0.25, 0.25));
}

/// At the other thumb's value, each of the keys stops there too, not only Home and End.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Slider.kt, rangeSliderOnKeyEvents
#[test]
fn every_key_stops_at_the_other_thumb() {
    use compose_ui::Key;
    let run = |start: f32, end: f32, tabs: u32, keys: [Key; 2], expected: (f32, f32)| {
        let state = RangeSliderState::new(start, end, 0, (0.0, 1.0));
        let handle = *state;
        let heard = Rc::new(Cell::new((f32::NAN, f32::NAN)));
        let last = heard.clone();
        let mut ui = ComposeTestRule::sized(IntSize::new(400, 400), move || {
            let last = last.clone();
            MaterialTheme!({
                let last = last.clone();
                RangeSlider!(
                    handle,
                    move |range| last.set(range),
                    modifier = Modifier.test_tag(TAG)
                );
            });
        });
        for _ in 0..tabs {
            ui.press_key(Key::Tab);
        }
        for key in keys {
            ui.press_key(key);
            assert_eq!(heard.get(), expected, "{key:?}");
        }
    };
    // The start thumb, right up against the end's.
    run(0.75, 0.75, 1, [Key::DirectionRight, Key::PageUp], (0.75, 0.75));
    // The end thumb, down against the start's.
    run(0.25, 0.25, 2, [Key::DirectionLeft, Key::PageDown], (0.25, 0.25));
}
