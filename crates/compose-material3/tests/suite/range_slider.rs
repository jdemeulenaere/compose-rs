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
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
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
    let node = ui.on_node_unmerged(has_test_tag(tag)).on_parent().fetch_semantics_node();
    *node.config.get(&SemanticsProperties::PROGRESS_BAR_RANGE_INFO).expect("a range")
}

fn range(current: f32, range: (f32, f32), steps: u32) -> ProgressBarRangeInfo {
    ProgressBarRangeInfo { current, range, steps }
}

fn set_progress(ui: &mut ComposeTestRule, tag: &str, value: f32) {
    let node = ui.on_node_unmerged(has_test_tag(tag)).on_parent().fetch_semantics_node();
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
        let node = ui.on_node_unmerged(has_test_tag(tag)).on_parent().fetch_semantics_node();
        assert_eq!((node.bounds.width(), node.bounds.height()), (4.0 + 20.0, 44.0), "{tag}");
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, rangeSlider_thumbs_visualBounds
#[test]
fn a_thumb_is_4_by_44_dp() {
    let state = RangeSliderState::new(0.0, 1.0, 0, (0.0, 1.0));
    let mut ui = thumbs_rule(*state);
    for tag in ["start", "end"] {
        let bounds = ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds;
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
    let (left, right, _, _) = frame(&mut ui);
    assert!(right - left >= 4.0, "{left}..{right}");
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
        ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
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
