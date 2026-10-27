//! Compose's `SliderTest.kt`, adapted: the single slider's value coercion, semantics, taps and
//! drags (right to left too, in a scrolling container, past its ends), sizes, callbacks, slots'
//! recompositions and stops.
//!
//! Touch input is a private helper over `send_pointer` (the test harness has no gesture DSL
//! yet): Compose's `moveBy`s are one move event each. Where Compose's `calculateFraction` takes
//! the node's left and right, these take the layout's (the semantics bounds less the 10 dp
//! `HorizontalSemanticsBoundsPadding` on each side): Compose's tolerance (0.003) absorbs the
//! difference on a phone's width, not on this 400 px root.
//!
//! Not ported: `sliderPosition_stepsThrowWhenLessThanZero` (`steps` is a `u32`),
//! `slider_interactionSource_resetWhenDisposed` (compose-rs's interaction source keeps states,
//! not an interaction stream), `slider_label_staysVisible_whenHoverExitsWhileStillPressedOrDragging`
//! (no `Label`), `slider_onValueChangeFinishedWithSnackbar` (a snackbar shown from the callback;
//! the callback is covered by the others), `slider_thumbPosition_staysSameWhenFocused` (ignored
//! in Compose, b/447508701), the inset focus ring tests, the vertical and range sliders' tests
//! (not in compose-rs yet).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Dp, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Offset, PointerEvent,
    ProgressBarRangeInfo, Rect, SemanticsActions, SizeSpec, UnitExt,
};

const TAG: &str = "slider";
const TOLERANCE: f32 = 0.003;
/// The touch slop at density 1, as `LocalViewConfiguration.current.touchSlop` (8 dp).
const SLOP: f32 = 8.0;
/// Half the handle, as Compose's `ThumbWidth / 2`: the track's ends are that far in.
const HALF_THUMB: f32 = 2.0;
/// `HorizontalSemanticsBoundsPadding`.
const SEMANTICS_PADDING: f32 = 10.0;

fn material(content: impl Fn() + 'static) -> impl Fn() + 'static {
    let content = Rc::new(content);
    move || {
        let content = content.clone();
        MaterialTheme!({ content() });
    }
}

/// `Modifier.requiredWidth`.
fn required_width(modifier: Modifier, width: f32) -> Modifier {
    modifier.size_in(SizeSpec::exact(width.dp(), Dp::UNSPECIFIED, true))
}

/// The slider's layout bounds: its semantics bounds less the padding on each side.
fn layout_bounds(ui: &mut ComposeTestRule) -> Rect {
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    Rect::new(
        bounds.left + SEMANTICS_PADDING,
        bounds.top,
        bounds.right - SEMANTICS_PADDING,
        bounds.bottom,
    )
}

fn center(bounds: Rect) -> Offset {
    Offset::new((bounds.left + bounds.right) / 2.0, (bounds.top + bounds.bottom) / 2.0)
}

/// As the test's `calculateFraction`: where `position` is along the track, 0 to 1.
fn fraction(bounds: Rect, position: f32) -> f32 {
    let start = bounds.left + HALF_THUMB;
    let end = bounds.right - HALF_THUMB;
    ((position - start) / (end - start)).clamp(0.0, 1.0)
}

fn range_info(ui: &mut ComposeTestRule) -> ProgressBarRangeInfo {
    ui.on_node_with_tag(TAG)
        .fetch_semantics_node()
        .config
        .get(&compose_ui::SemanticsProperties::PROGRESS_BAR_RANGE_INFO)
        .copied()
        .expect("a progress bar range")
}

fn info(current: f32, steps: u32) -> ProgressBarRangeInfo {
    ProgressBarRangeInfo { current, range: (0.0, 1.0), steps }
}

/// A press at `from`, a move by each of `moves` (one event each), and the release, as
/// `performTouchInput { down(); moveBy()…; up() }`.
fn drag(ui: &mut ComposeTestRule, from: Offset, moves: &[(f32, f32)]) {
    let mut at = from;
    ui.host().send_pointer(PointerEvent::press(at));
    for (dx, dy) in moves {
        at = Offset::new(at.x + dx, at.y + dy);
        ui.host().send_pointer(PointerEvent::moved(at, true));
    }
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
}

fn tap(ui: &mut ComposeTestRule, at: Offset) {
    drag(ui, at, &[]);
}

/// A slider of `state` that takes the values it's given, tagged.
fn plain(state: SliderState) -> impl Fn() + 'static {
    move || {
        Slider!(state, move |new| state.set_value(new), modifier = Modifier.test_tag(TAG));
    }
}

fn assert_near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() <= TOLERANCE, "{actual} is not {expected}");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, sliderPosition_valueCoercion
#[test]
fn a_value_set_out_of_the_range_is_coerced_into_it() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(plain(*state));
    state.set_value(2.0);
    ui.wait_for_idle();
    assert_eq!(range_info(&mut ui), info(1.0, 0));
    state.set_value(-123145.0);
    ui.wait_for_idle();
    assert_eq!(range_info(&mut ui), info(0.0, 0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_semantics_continuous
#[test]
fn a_continuous_slider_reports_its_value_and_takes_set_progress() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(material(plain(*state)));
    assert_eq!(range_info(&mut ui), info(0.0, 0));
    state.set_value(0.5);
    ui.wait_for_idle();
    assert_eq!(range_info(&mut ui), info(0.5, 0));
    ui.on_node_with_tag(TAG).perform_semantics_action_with(
        &SemanticsActions::SET_PROGRESS,
        |set| {
            set(0.7);
        },
    );
    ui.wait_for_idle();
    assert_eq!(range_info(&mut ui), info(0.7, 0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_semantics_stepped
#[test]
fn a_stepped_slider_reports_its_steps_and_snaps_set_progress() {
    let state = SliderState::new(0.0, 4, (0.0, 1.0));
    let mut ui = compose_test(material(plain(*state)));
    assert_eq!(range_info(&mut ui), info(0.0, 4));
    state.set_value(0.6);
    ui.wait_for_idle();
    assert_eq!(range_info(&mut ui), info(0.6, 4));
    ui.on_node_with_tag(TAG).perform_semantics_action_with(
        &SemanticsActions::SET_PROGRESS,
        |set| {
            set(0.75);
        },
    );
    ui.wait_for_idle();
    assert_eq!(range_info(&mut ui), info(0.8, 4));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_semantics_focusable
#[test]
fn a_slider_is_focusable() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(material(move || {
        Slider!(*state, |_| {}, modifier = Modifier.test_tag(TAG));
    }));
    assert!(
        ui.on_node_with_tag(TAG)
            .fetch_semantics_node()
            .config
            .contains(&compose_ui::SemanticsProperties::FOCUSED)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_semantics_disabled
#[test]
fn a_disabled_slider_reads_disabled() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        Slider!(handle, |_| {}, modifier = Modifier.test_tag(TAG), enabled = false);
    }));
    ui.on_node_with_tag(TAG).assert_is_not_enabled();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_drag
#[test]
fn a_drag_moves_the_value_less_the_slop() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(material(plain(*state)));
    assert_eq!(state.value(), 0.0);
    let bounds = layout_bounds(&mut ui);
    drag(&mut ui, center(bounds), &[(100.0, 0.0)]);
    assert_near(state.value(), fraction(bounds, center(bounds).x + 100.0 - SLOP));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_drag_out_of_bounds
#[test]
fn a_drag_past_the_ends_comes_back_with_the_pointer() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(material(plain(*state)));
    let bounds = layout_bounds(&mut ui);
    let width = bounds.width() + 2.0 * SEMANTICS_PADDING;
    let moves = [(width, 0.0), (-width, 0.0), (-width, 0.0), (width + 100.0, 0.0)];
    drag(&mut ui, center(bounds), &moves);
    assert_near(state.value(), fraction(bounds, center(bounds).x + 100.0 - SLOP));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_tap
#[test]
fn a_tap_moves_the_value_there() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(material(plain(*state)));
    let bounds = layout_bounds(&mut ui);
    let at = center(bounds) + Offset::new(50.0, 0.0);
    tap(&mut ui, at);
    assert_near(state.value(), fraction(bounds, at.x));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_scrollableContainer
#[test]
fn a_slider_in_a_scrolling_column_lets_it_scroll_and_still_takes_taps() {
    use compose_foundation::ScrollModifiers;
    thread_local! { static SCROLL: Cell<Option<compose_foundation::ScrollState>> = const { Cell::new(None) }; }
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(move || {
        // Compose's column scrolls a state taking every delta: this one starts scrolled to the
        // slider, so a drag down scrolls it back.
        let scroll = compose_foundation::remember_scroll_state!(1000);
        SCROLL.with(|cell| cell.set(Some(scroll)));
        Column!(modifier = Modifier.vertical_scroll(scroll), {
            Spacer!(modifier = Modifier.height(1000.dp()));
            Slider!(handle, move |new| handle.set_value(new), modifier = Modifier.test_tag(TAG));
            Spacer!(modifier = Modifier.height(2000.dp()));
        });
    });
    let scrolled = || SCROLL.with(Cell::get).expect("composed").value();
    assert_eq!(scrolled(), 1000);
    let bounds = ui.on_node_unmerged(has_test_tag(TAG)).fetch_semantics_node().bounds;
    drag(&mut ui, center(bounds), &[(0.0, 500.0)]);
    assert!(scrolled() < 1000, "{}", scrolled());
    assert_eq!(state.value(), 0.0);
    let bounds = layout_bounds(&mut ui);
    tap(&mut ui, center(bounds));
    assert_near(state.value(), fraction(bounds, center(bounds).x));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_tap_rangeChange
#[test]
fn a_tap_after_the_range_changed_maps_to_the_new_range() {
    thread_local! { static STATE: Cell<Option<SliderState>> = const { Cell::new(None) }; }
    let range_end = mutable_state_of(0.25f32);
    let end = range_end.handle();
    let mut ui = compose_test(material(move || {
        let end = end.get();
        let state = remember_keyed(end.to_bits(), move || SliderState::new(0.0, 0, (0.0, end)));
        STATE.with(|cell| cell.set(Some(state)));
        Slider!(state, move |new| state.set_value(new), modifier = Modifier.test_tag(TAG));
    }));
    range_end.set(1.0);
    ui.wait_for_idle();
    let bounds = layout_bounds(&mut ui);
    let at = center(bounds) + Offset::new(50.0, 0.0);
    tap(&mut ui, at);
    let value = STATE.with(Cell::get).expect("composed").value();
    assert_near(value, fraction(bounds, at.x));
}

/// A slider of `state` in a right-to-left layout.
fn right_to_left(state: SliderState) -> impl Fn() + 'static {
    material(move || {
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl), || {
            plain(state)();
        });
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_drag_rtl
#[test]
fn a_right_to_left_drag_moves_the_value_from_the_right() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(right_to_left(*state));
    let bounds = layout_bounds(&mut ui);
    drag(&mut ui, center(bounds), &[(100.0, 0.0)]);
    // Rightwards is towards the start: the mirrored position.
    assert_near(state.value(), fraction(bounds, center(bounds).x - 100.0 + SLOP));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_tap_rtl
#[test]
fn a_right_to_left_tap_moves_the_value_from_the_right() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(right_to_left(*state));
    let bounds = layout_bounds(&mut ui);
    tap(&mut ui, center(bounds) + Offset::new(50.0, 0.0));
    assert_near(state.value(), fraction(bounds, center(bounds).x - 50.0));
}

/// The size of the slider in a box at most `max` dp each way, as
/// `setMaterialContentForSizeAssertions`.
fn size_within(max: f32, content: impl Fn() + 'static) -> (f32, f32) {
    let content = Rc::new(content);
    let mut ui = compose_test(material(move || {
        let content = content.clone();
        Box!(modifier = Modifier.width_in(0.dp(), max.dp()).height_in(0.dp(), max.dp()), {
            let content = content.clone();
            Box!(modifier = Modifier.test_tag("container"), { content() });
        });
    }));
    let bounds = ui.on_node_with_tag("container").fetch_semantics_node().bounds;
    (bounds.width(), bounds.height())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_sizes
#[test]
fn a_slider_takes_the_width_and_48_dp() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let size = size_within(100.0, move || {
        Slider!(handle, |_| {});
    });
    assert_eq!(size, (100.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_sizes_within_row
#[test]
fn a_weighted_slider_in_a_row_takes_whats_left() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        Row!(modifier = required_width(Modifier, 100.0), |row: RowScope| {
            Spacer!(modifier = Modifier.width(10.dp()));
            Slider!(handle, |_| {}, modifier = Modifier.weight(row, 1.0).test_tag(TAG));
            Spacer!(modifier = Modifier.width(10.dp()));
        });
    }));
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    // The handle's 44 dp high, the semantics 10 dp wider on each side.
    assert_eq!((bounds.width(), bounds.height()), (80.0 + 2.0 * SEMANTICS_PADDING, 44.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_min_size
#[test]
fn a_slider_in_no_room_is_the_handle_wide_and_the_track_high() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        Box!(modifier = Modifier.required_size((0.dp(), 0.dp())), {
            Slider!(handle, |_| {}, modifier = Modifier.test_tag(TAG));
        });
    }));
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    assert!((bounds.width() - (4.0 + 2.0 * SEMANTICS_PADDING)).abs() <= 1.0, "{bounds:?}");
    assert_eq!(bounds.height(), 16.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_noUnwantedCallbackCalls
#[test]
fn a_slider_calls_nothing_by_itself() {
    let calls = Rc::new(Cell::new(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let counted = calls.clone();
    let mut ui = compose_test(material(move || {
        let counted = counted.clone();
        Slider!(handle, move |_| counted.set(counted.get() + 1), modifier = Modifier.test_tag(TAG));
    }));
    ui.wait_for_idle();
    assert_eq!(calls.get(), 0);
}

/// A slider of `state` taking its values, counting `on_value_change_finished` calls.
fn finishing(state: SliderState, finished: Rc<Cell<u32>>) -> impl Fn() + 'static {
    material(move || {
        let finished = finished.clone();
        Slider!(
            state,
            move |new| state.set_value(new),
            modifier = Modifier.test_tag(TAG),
            on_value_change_finished = Some(move || finished.set(finished.get() + 1)),
        );
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_valueChangeFinished_calledOnce
#[test]
fn a_drag_finishes_once() {
    let finished = Rc::new(Cell::new(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(finishing(*state, finished.clone()));
    assert_eq!(finished.get(), 0);
    let bounds = layout_bounds(&mut ui);
    drag(&mut ui, center(bounds), &[(50.0, 50.0)]);
    assert_eq!(finished.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_setProgress_callsOnValueChangeFinished
#[test]
fn set_progress_finishes() {
    let finished = Rc::new(Cell::new(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let mut ui = compose_test(finishing(*state, finished.clone()));
    assert_eq!(finished.get(), 0);
    ui.on_node_with_tag(TAG).perform_semantics_action_with(
        &SemanticsActions::SET_PROGRESS,
        |set| {
            set(0.8);
        },
    );
    ui.wait_for_idle();
    assert_eq!(finished.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_onValueChangedFinish_afterTap
#[test]
fn a_tap_finishes() {
    let finished = Rc::new(Cell::new(0));
    let counted = finished.clone();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        let state = remember_slider_state!();
        Slider!(
            state,
            |_| {},
            modifier = Modifier.test_tag(TAG),
            on_value_change_finished = Some(move || counted.set(counted.get() + 1)),
        );
    });
    let bounds = layout_bounds(&mut ui);
    tap(&mut ui, center(bounds));
    assert_eq!(finished.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_zero_width
#[test]
fn a_slider_in_a_zero_box_is_zero() {
    let size = size_within(0.0, || {
        let state = remember_slider_state!(1.0);
        Slider!(state, |_| {});
    });
    assert_eq!(size, (0.0, 0.0));
}

thread_local! {
    static OUTER: Cell<u32> = const { Cell::new(0) };
    static INNER: Cell<u32> = const { Cell::new(0) };
}

/// As the test's `SliderRecompositionCounter`: an outer slot content not reading the value, an
/// inner one reading it.
#[composable]
fn OuterContent(state: SliderState) {
    SideEffect(|| OUTER.with(|count| count.set(count.get() + 1)));
    Column!({
        Text!("OuterContent");
        InnerContent(state);
    });
}

#[composable]
fn InnerContent(state: SliderState) {
    SideEffect(|| INNER.with(|count| count.set(count.get() + 1)));
    Text!("InnerContent: {}", state.value());
}

fn drag_back_and_forth(ui: &mut ComposeTestRule) {
    let bounds = layout_bounds(ui);
    let mut at = center(bounds);
    ui.host().send_pointer(PointerEvent::press(at));
    for dx in [100.0, -100.0, 100.0] {
        at = Offset::new(at.x + dx, at.y);
        ui.host().send_pointer(PointerEvent::moved(at, true));
        ui.wait_for_idle();
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_thumb_recomposition
#[test]
fn a_drag_recomposes_only_what_reads_the_value_in_the_thumb() {
    OUTER.with(|count| count.set(0));
    INNER.with(|count| count.set(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(move || {
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            thumb = Some(OuterContent),
        );
    });
    drag_back_and_forth(&mut ui);
    assert_eq!(OUTER.with(Cell::get), 1);
    assert_eq!(INNER.with(Cell::get), 4);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_track_recomposition
#[test]
fn a_drag_recomposes_only_what_reads_the_value_in_the_track() {
    OUTER.with(|count| count.set(0));
    INNER.with(|count| count.set(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(move || {
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            track = Some(OuterContent),
        );
    });
    drag_back_and_forth(&mut ui);
    assert_eq!(OUTER.with(Cell::get), 1);
    assert_eq!(INNER.with(Cell::get), 4);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_parentWithInfiniteWidth_minWidth
#[test]
fn a_slider_with_infinite_room_is_48_dp_wide() {
    use compose_foundation::ScrollModifiers;
    let mut ui = compose_test(material(|| {
        Box!(
            modifier = Modifier.horizontal_scroll(compose_foundation::remember_scroll_state!()),
            {
                Box!(modifier = Modifier.test_tag("container"), {
                    let state = remember_slider_state!();
                    Slider!(state, |_| {});
                });
            }
        );
    }));
    let bounds = ui.on_node_with_tag("container").fetch_semantics_node().bounds;
    assert_eq!(bounds.width(), 48.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_rowWithInfiniteWidth
#[test]
fn a_weighted_slider_in_an_infinite_row_lays_out() {
    use compose_foundation::ScrollModifiers;
    let mut ui = compose_test(material(|| {
        Box!(
            modifier = Modifier.horizontal_scroll(compose_foundation::remember_scroll_state!()),
            {
                Row!(|row: RowScope| {
                    let state = remember_slider_state!();
                    Slider!(state, |_| {}, modifier = Modifier.weight(row, 1.0).test_tag(TAG));
                });
            }
        );
    }));
    ui.on_node_with_tag(TAG).assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_dragOutsideTouchArea_doesntJump
#[test]
fn a_drag_that_left_the_slider_still_moves_it_by_the_pointer() {
    let state = SliderState::new(0.5, 0, (0.0, 1.0));
    let mut ui = compose_test(material(plain(*state)));
    assert_eq!(state.value(), 0.5);
    let bounds = layout_bounds(&mut ui);
    drag(&mut ui, center(bounds), &[(0.0, 500.0), (100.0, 0.0)]);
    assert_near(state.value(), fraction(bounds, center(bounds).x + 100.0 - SLOP));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_directCallbacks_invokedDuringDrag
#[test]
fn a_drag_calls_back_while_dragging_and_finishes_once() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let finished = Rc::new(Cell::new(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let (recorded, counted) = (changes.clone(), finished.clone());
    let mut ui = compose_test(material(move || {
        let (recorded, counted) = (recorded.clone(), counted.clone());
        Slider!(
            handle,
            move |new| recorded.borrow_mut().push(new),
            modifier = Modifier.test_tag(TAG),
            on_value_change_finished = Some(move || counted.set(counted.get() + 1)),
        );
    }));
    let bounds = layout_bounds(&mut ui);
    drag(&mut ui, center(bounds), &[(100.0, 0.0)]);
    assert!(!changes.borrow().is_empty());
    assert_eq!(finished.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_valueChange_doesNotRecomposeOrRemeasure
#[test]
fn a_new_value_does_not_recompose_the_slots() {
    thread_local! {
        static MEASURES: Cell<u32> = const { Cell::new(0) };
        static THUMBS: Cell<u32> = const { Cell::new(0) };
        static TRACKS: Cell<u32> = const { Cell::new(0) };
    }
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        let source = remember(compose_ui::MutableInteractionSource::new);
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG).layout(|scope, measurable, constraints| {
                MEASURES.with(|count| count.set(count.get() + 1));
                let placeable = scope.measure(measurable, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            thumb = Some(move |_: SliderState| {
                SideEffect(|| THUMBS.with(|count| count.set(count.get() + 1)));
                SliderDefaults::thumb(
                    source.clone(),
                    Modifier,
                    SliderDefaults::colors(),
                    true,
                    SliderDefaults::THUMB_SIZE,
                );
            }),
            track = Some(|state: SliderState| {
                SideEffect(|| TRACKS.with(|count| count.set(count.get() + 1)));
                SliderDefaults::track(state, Modifier, true, SliderDefaults::colors());
            }),
        );
    }));
    let measures = MEASURES.with(Cell::get);
    assert!(measures > 0);
    assert_eq!((THUMBS.with(Cell::get), TRACKS.with(Cell::get)), (1, 1));
    for value in [0.25, 0.5, 0.75] {
        state.set_value(value);
        ui.wait_for_idle();
    }
    // Not checked: Compose's slider isn't measured again (its thumb is placed again, a
    // placement-only relayout); compose-ui measures and places in one pass, so the slider is
    // measured again (docs/parity/material3.md).
    assert_eq!((THUMBS.with(Cell::get), TRACKS.with(Cell::get)), (1, 1));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_semantics_stateDescription_onlySetForCenteredSlider
#[test]
fn only_a_slider_whose_range_goes_below_zero_describes_its_state() {
    let mut ui = compose_test(material(|| {
        Column!({
            let standard = remember_slider_state!(0.5);
            Slider!(standard, |_| {}, modifier = Modifier.test_tag("standard"));
            let custom = remember_slider_state!(0.5);
            Slider!(
                custom,
                |_| {},
                modifier = Modifier
                    .semantics(|config| config.set_state_description("0:13 of 0:18"))
                    .test_tag("custom"),
            );
            let centered = remember_slider_state!(0.0, 0, (-50.0, 50.0));
            Slider!(centered, |_| {}, modifier = Modifier.test_tag("centered"));
        });
    }));
    let standard = ui.on_node_with_tag("standard").fetch_semantics_node().config;
    let description = |config: &compose_ui::SemanticsConfiguration| {
        config
            .get(&compose_ui::SemanticsProperties::STATE_DESCRIPTION)
            .map(|text| text.as_str().to_string())
    };
    let range = |config: &compose_ui::SemanticsConfiguration| {
        config.get(&compose_ui::SemanticsProperties::PROGRESS_BAR_RANGE_INFO).copied()
    };
    assert_eq!(description(&standard), None);
    assert_eq!(range(&standard), Some(info(0.5, 0)));
    let custom = ui.on_node_with_tag("custom").fetch_semantics_node().config;
    assert_eq!(description(&custom).as_deref(), Some("0:13 of 0:18"));
    assert_eq!(range(&custom), Some(info(0.5, 0)));
    let centered = ui.on_node_with_tag("centered").fetch_semantics_node().config;
    assert_eq!(description(&centered).as_deref(), Some("0.0"));
    assert_eq!(
        range(&centered),
        Some(ProgressBarRangeInfo { current: 0.0, range: (-50.0, 50.0), steps: 0 })
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_discreteIntegerTicks_returnsExactIntegers
#[test]
fn whole_number_stops_give_whole_numbers() {
    let emitted = Rc::new(RefCell::new(Vec::new()));
    let state = SliderState::new(2.0, 13, (2.0, 16.0));
    let handle = *state;
    let recorded = emitted.clone();
    let mut ui = compose_test_with(
        compose_ui::IntSize::new(1080, 400),
        compose_ui::Density::default(),
        Box::new(compose_ui::platform::FixedTextMeasurer::new()),
        material(move || {
            let recorded = recorded.clone();
            Slider!(
                handle,
                move |new| {
                    recorded.borrow_mut().push(new);
                    handle.set_value(new);
                },
                modifier = required_width(Modifier.test_tag(TAG), 1080.0),
            );
        }),
    );
    let bounds = layout_bounds(&mut ui);
    // Compose's `centerLeft` and steps of a fiftieth of the node's width: there the slider is
    // the screen's width, its semantics bounds clipped to it, so its layout's.
    let from = Offset::new(bounds.left, center(bounds).y);
    let moves = vec![(bounds.width() / 50.0, 0.0); 50];
    drag(&mut ui, from, &moves);
    let expected: Vec<f32> = (3..=16).map(|value| value as f32).collect();
    assert_eq!(*emitted.borrow(), expected);

    let divisible_by_7 = SliderState::new(0.0, 6, (0.0, 21.0));
    for expected in (0..=21).step_by(3) {
        divisible_by_7.set_value(expected as f32 + 0.1);
        assert_eq!(divisible_by_7.value(), expected as f32);
    }
}
