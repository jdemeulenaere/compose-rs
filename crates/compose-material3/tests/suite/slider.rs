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
//! `slider_label_staysVisible_whenHoverExitsWhileStillPressedOrDragging`
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
    Dp, DragInteraction, Interaction, Key, LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier,
    MutableInteractionSource, Offset, PointerEvent, PointerId, PointerType, PressInteraction,
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
    let touch = |event: PointerEvent| event.with_pointer(PointerId(0), PointerType::Touch);
    let mut at = from;
    ui.host().send_pointer(touch(PointerEvent::press(at)));
    for (dx, dy) in moves {
        at = Offset::new(at.x + dx, at.y + dy);
        ui.host().send_pointer(touch(PointerEvent::moved(at, true)));
    }
    ui.host().send_pointer(touch(PointerEvent::release(at)));
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

/// The thumb is measured with the slider's constraints, as Compose's `SliderImpl`
/// (`measure(constraints)`): a custom thumb smaller than the slider's minimum (4 by 16 dp) sits
/// in a box of that height (its own width kept by the thumb's `wrapContentWidth`, centred in the
/// handle's 4 dp), at the box's top: at the slider's top, not 7 dp down (centred) as a loosened
/// thumb would be.
#[test]
fn a_small_custom_thumb_is_measured_with_the_sliders_constraints() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        Slider!(
            handle,
            |_| {},
            modifier = Modifier.test_tag(TAG),
            thumb = Some(|_: SliderState| Box!(modifier = Modifier.size(2.dp()).test_tag("thumb"))),
            track =
                Some(|_: SliderState| Box!(modifier = Modifier.fill_max_width().height(2.dp()))),
        );
    }));
    let slider = layout_bounds(&mut ui);
    let thumb = ui.on_node_with_tag_unmerged("thumb").fetch_semantics_node().bounds;
    assert_eq!(thumb.top - slider.top, 0.0, "{slider:?} {thumb:?}");
    // Centred in the 4 dp wrapper at the start.
    assert_eq!(thumb.left - slider.left, 1.0, "{slider:?} {thumb:?}");
}

/// `SetProgress` looks for the nearest stop from a distance of the target itself, as Compose's
/// `sliderSemantics` (`var distance: Float = newValue`): below zero no stop is that near, so the
/// value asked is the target as is (the state's setter snaps it then), not its nearest stop.
#[test]
fn set_progress_below_zero_asks_for_the_target_as_composes() {
    let asked = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = asked.clone();
    let state = SliderState::new(0.0, 3, (-50.0, 50.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        let seen = seen.clone();
        Slider!(
            handle,
            move |new| {
                seen.borrow_mut().push(new);
                handle.set_value(new);
            },
            modifier = Modifier.test_tag(TAG),
        );
    }));
    for target in [-20.0, 20.0] {
        ui.on_node_with_tag(TAG).perform_semantics_action_with(
            &SemanticsActions::SET_PROGRESS,
            |set| {
                set(target);
            },
        );
    }
    // Above zero, the nearest stop.
    assert_eq!(*asked.borrow(), [-20.0, 25.0]);
}

/// Every key down asks for a value, as Compose's `slideOnKeyEvents` calls `onValueChange` on
/// each, the same value at an end too.
#[test]
fn every_key_down_asks_for_a_value_even_at_the_ends() {
    let asked = Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = asked.clone();
    let state = SliderState::new(1.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        let seen = seen.clone();
        Slider!(
            handle,
            move |new| {
                seen.borrow_mut().push(new);
                handle.set_value(new);
            },
            modifier = Modifier.test_tag(TAG),
        );
    }));
    ui.press_key(compose_ui::Key::Tab);
    ui.press_key(compose_ui::Key::MoveEnd);
    ui.press_key(compose_ui::Key::DirectionRight);
    ui.press_key(compose_ui::Key::MoveHome);
    ui.press_key(compose_ui::Key::MoveHome);
    assert_eq!(*asked.borrow(), [1.0, 1.0, 0.0, 0.0]);
}

/// A slider given `source`, shown while `shown`, tagged.
fn with_source(
    state: SliderState,
    source: MutableInteractionSource,
    shown: MutableState<bool>,
) -> impl Fn() + 'static {
    move || {
        if shown.get() {
            Slider!(
                state,
                move |new| state.set_value(new),
                modifier = Modifier.test_tag(TAG),
                interaction_source = Some(source.clone()),
            );
        }
    }
}

/// The slider's interactions, as `sliderTapModifier`'s and `draggable`'s: a press until the drag
/// takes it, then a drag until the release.
#[test]
fn a_press_becomes_a_drag_in_the_interaction_source() {
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let shown = mutable_state_of(true);
    let mut ui = compose_test(material(with_source(*state, source.clone(), shown.handle())));
    let at = ui.on_node_with_tag(TAG).fetch_semantics_node().center();
    ui.host().send_pointer(PointerEvent::press(at));
    ui.wait_for_idle();
    assert!(source.is_pressed() && !source.is_dragged());
    let moved = at + Offset::new(SLOP + 10.0, 0.0);
    ui.host().send_pointer(PointerEvent::moved(moved, true));
    ui.wait_for_idle();
    assert!(!source.is_pressed() && source.is_dragged());
    ui.host().send_pointer(PointerEvent::release(moved));
    ui.wait_for_idle();
    assert!(!source.is_pressed() && !source.is_dragged());
}

/// A tap ends its press with a release, and focus goes through the source too.
#[test]
fn a_tap_and_a_focus_go_through_the_interaction_source() {
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let shown = mutable_state_of(true);
    let mut ui = compose_test(material(with_source(*state, source.clone(), shown.handle())));
    let at = ui.on_node_with_tag(TAG).fetch_semantics_node().center();
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
    assert!(!source.is_pressed());
    ui.on_node_with_tag(TAG).request_focus();
    ui.wait_for_idle();
    assert!(source.is_focused());
    shown.set(false);
    ui.wait_for_idle();
    assert!(!source.is_focused());
}

/// `SetProgress` calls through on a disabled slider, as Compose's: refusing a disabled node's
/// actions is the platform bridges' (D1255).
#[test]
fn a_disabled_sliders_set_progress_calls_through() {
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let handle = *state;
    let mut ui = compose_test(material(move || {
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            enabled = false,
        );
    }));
    ui.on_node_with_tag(TAG).assert_is_not_enabled();
    ui.on_node_with_tag(TAG)
        .perform_semantics_action_with(&SemanticsActions::SET_PROGRESS, |set| assert!(set(0.5)));
    assert_near(state.value(), 0.5);
}

/// A slider on a new state, given `source`, while `enabled` and `shown`: what the tests below
/// change under a gesture.
struct Rig {
    ui: ComposeTestRule,
    source: MutableInteractionSource,
    state: compose_runtime::Owned<SliderState>,
    enabled: OwnedState<bool>,
    shown: OwnedState<bool>,
    finished: Rc<Cell<u32>>,
}

fn rig() -> Rig {
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let enabled = mutable_state_of(true);
    let shown = mutable_state_of(true);
    let finished = Rc::new(Cell::new(0));
    let (given, handle, on, visible, count) =
        (source.clone(), *state, enabled.handle(), shown.handle(), finished.clone());
    let ui = compose_test(material(move || {
        if visible.get() {
            let count = count.clone();
            Slider!(
                handle,
                move |new| handle.set_value(new),
                modifier = Modifier.test_tag(TAG),
                enabled = on.get(),
                on_value_change_finished = Some(move || count.set(count.get() + 1)),
                interaction_source = Some(given.clone()),
            );
        }
    }));
    Rig { ui, source, state, enabled, shown, finished }
}

fn center_of(ui: &mut ComposeTestRule) -> Offset {
    ui.on_node_with_tag(TAG).fetch_semantics_node().center()
}

/// A second pointer, a finger.
fn second_finger(event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(1), PointerType::Touch)
}

/// What `source` emits from now on, in short.
fn recorded(source: &MutableInteractionSource) -> Rc<std::cell::RefCell<Vec<&'static str>>> {
    let recorded: Rc<std::cell::RefCell<Vec<&'static str>>> = Rc::default();
    let sink = recorded.clone();
    let interactions = source.interactions();
    compose_runtime::platform::launch(async move {
        interactions
            .collect(move |interaction| {
                sink.borrow_mut().push(match interaction {
                    Interaction::Press(PressInteraction::Press(_)) => "Press",
                    Interaction::Press(PressInteraction::Release(_)) => "PressRelease",
                    Interaction::Press(PressInteraction::Cancel(_)) => "PressCancel",
                    Interaction::Drag(DragInteraction::Start(_)) => "DragStart",
                    Interaction::Drag(DragInteraction::Stop(_)) => "DragStop",
                    Interaction::Drag(DragInteraction::Cancel(_)) => "DragCancel",
                    _ => "Other",
                })
            })
            .await
    });
    recorded
}

/// The handle's width: the round rectangle 44 dp high.
fn handle_width(ui: &mut ComposeTestRule) -> f32 {
    let list = ui.host().display_list();
    let widths: Vec<f32> = list
        .ops
        .iter()
        .filter_map(|op| match op {
            compose_ui::platform::DrawOp::DrawRoundRect(rect, _, _)
                if rect.bottom - rect.top == 44.0 =>
            {
                Some(rect.right - rect.left)
            }
            _ => None,
        })
        .collect();
    assert_eq!(widths.len(), 1, "{:#?}", list.ops);
    widths[0]
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SliderTest.kt, slider_interactionSource_resetWhenDisposed
#[test]
fn the_interactions_end_when_the_slider_leaves() {
    let mut rig = rig();
    let interactions = recorded(&rig.source);
    rig.ui.wait_for_idle();
    assert!(interactions.borrow().is_empty());
    let at = center_of(&mut rig.ui);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(at + Offset::new(100.0, 0.0), true));
    rig.ui.wait_for_idle();
    assert_eq!(*interactions.borrow(), ["Press", "DragStart", "PressCancel"]);
    rig.shown.set(false);
    rig.ui.wait_for_idle();
    assert_eq!(*interactions.borrow(), ["Press", "DragStart", "PressCancel", "DragCancel"]);
}

/// Disabled under a press, the press ends then, and its release adds nothing, as
/// `sliderTapModifier` dropping off.
#[test]
fn a_press_ends_when_the_slider_is_disabled() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.wait_for_idle();
    assert!(rig.source.is_pressed());
    rig.enabled.set(false);
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed());
    rig.ui.host().send_pointer(PointerEvent::release(at));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed());
    assert_eq!(rig.state.value(), 0.0);
    assert_eq!(rig.finished.get(), 0);
}

/// Disabled under a drag, the drag ends then, as `DragGestureNode.update`, on the source and on
/// the state, and leaves nothing a hover after it could move the value with.
#[test]
fn a_drag_ends_when_the_slider_is_disabled() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let moved = at + Offset::new(30.0, 0.0);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(moved, true));
    rig.ui.wait_for_idle();
    assert!(rig.source.is_dragged() && rig.state.is_dragging());
    rig.enabled.set(false);
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_dragged() && !rig.state.is_dragging());
    // `processDragCancel` calls `onDragStopped`, which finishes the value's change.
    assert_eq!(rig.finished.get(), 1);
    rig.ui.host().send_pointer(PointerEvent::release(moved));
    rig.ui.wait_for_idle();
    assert_eq!(rig.finished.get(), 1, "its release is nothing now");
    rig.enabled.set(true);
    rig.ui.wait_for_idle();
    let value = rig.state.value();
    // A mouse over it, nothing pressed.
    rig.ui.host().send_pointer(PointerEvent::moved(at, false));
    rig.ui.wait_for_idle();
    assert_eq!(rig.state.value(), value, "a hover moved the value");
}

/// Disabled while focused, its focus ends, as `focusable(enabled = false)` removing its node.
#[test]
fn a_focus_ends_when_the_slider_is_disabled() {
    let mut rig = rig();
    rig.ui.on_node_with_tag(TAG).request_focus();
    rig.ui.wait_for_idle();
    assert!(rig.source.is_focused());
    rig.enabled.set(false);
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_focused());
}

/// Disabled while focused then enabled again, it isn't focused, as `focusable(enabled = false)`
/// removing its node: nothing's heard again, and the handle is whole.
#[test]
fn a_slider_enabled_again_has_lost_its_focus() {
    let mut rig = rig();
    rig.ui.on_node_with_tag(TAG).request_focus();
    rig.ui.wait_for_idle();
    assert!(rig.source.is_focused());
    rig.enabled.set(false);
    rig.ui.wait_for_idle();
    rig.enabled.set(true);
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_focused());
    rig.ui.on_node_with_tag(TAG).assert_is_not_focused();
    assert_eq!(handle_width(&mut rig.ui), 4.0);
    rig.ui.on_node_with_tag(TAG).request_focus();
    rig.ui.wait_for_idle();
    assert!(rig.source.is_focused(), "and can take it again");
}

/// A disabled slider takes no press.
#[test]
fn a_disabled_slider_takes_no_press() {
    let mut rig = rig();
    rig.enabled.set(false);
    rig.ui.wait_for_idle();
    let at = center_of(&mut rig.ui);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed());
    rig.ui.host().send_pointer(PointerEvent::release(at));
    rig.ui.wait_for_idle();
    assert_eq!(rig.state.value(), 0.0);
}

/// `Slider.kt` asks for no focus: a drag takes none, and the handle is whole again after it.
#[test]
fn a_drag_takes_no_focus_and_the_handle_is_whole_after_it() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let moved = at + Offset::new(30.0, 0.0);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(moved, true));
    rig.ui.host().send_pointer(PointerEvent::release(moved));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_focused(), "focused by a drag");
    assert_eq!(handle_width(&mut rig.ui), 4.0);
}

/// `SliderDefaults.Thumb`: 4 dp wide at rest, half that while a press, a drag or a focus lasts.
#[test]
fn the_handle_narrows_for_a_press_a_drag_and_a_focus() {
    let mut rig = rig();
    assert_eq!(handle_width(&mut rig.ui), 4.0);
    let at = center_of(&mut rig.ui);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.wait_for_idle();
    assert_eq!(handle_width(&mut rig.ui), 2.0, "pressed");
    // The drag has taken the press: only the drag narrows it now.
    let moved = at + Offset::new(30.0, 0.0);
    rig.ui.host().send_pointer(PointerEvent::moved(moved, true));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed() && rig.source.is_dragged());
    assert_eq!(handle_width(&mut rig.ui), 2.0, "dragged");
    rig.ui.host().send_pointer(PointerEvent::release(moved));
    rig.ui.wait_for_idle();
    assert_eq!(handle_width(&mut rig.ui), 4.0, "released");
    rig.ui.on_node_with_tag(TAG).request_focus();
    rig.ui.wait_for_idle();
    assert_eq!(handle_width(&mut rig.ui), 2.0, "focused");
}

/// Out of bounds a press is cancelled, and there's no tap, as `waitForUpOrCancellation`.
#[test]
fn a_move_out_of_bounds_cancels_the_press_and_the_tap() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let away = at + Offset::new(0.0, 300.0);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(away, true));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed());
    rig.ui.host().send_pointer(PointerEvent::release(away));
    rig.ui.wait_for_idle();
    assert_eq!(rig.state.value(), 0.0, "a tap after leaving the slider");
    assert_eq!(rig.finished.get(), 0);
}

/// Inside the minimum touch target, though past the node's own bounds, a press goes on, as
/// `isOutOfBounds(size, extendedTouchPadding)`: the slider's 44 dp height (the handle's) has 2 dp
/// more on each side up to 48.
#[test]
fn a_move_inside_the_touch_target_keeps_the_press() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let top = at.y - 22.0;
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(Offset::new(at.x, top - 1.0), true));
    rig.ui.wait_for_idle();
    assert!(rig.source.is_pressed(), "inside the target");
    rig.ui.host().send_pointer(PointerEvent::moved(Offset::new(at.x, top - 3.0), true));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed(), "past the target");
}

/// A tap takes the value where it pressed, as `state.onPress`, not where it was released.
#[test]
fn a_tap_takes_the_value_of_where_it_pressed() {
    let mut tapped = rig();
    let at = center_of(&mut tapped.ui);
    tapped.ui.host().send_pointer(PointerEvent::press(at));
    tapped.ui.host().send_pointer(PointerEvent::release(at));
    tapped.ui.wait_for_idle();
    let mut wandered = rig();
    let at = center_of(&mut wandered.ui);
    let near = at + Offset::new(6.0, 0.0);
    wandered.ui.host().send_pointer(PointerEvent::press(at));
    wandered.ui.host().send_pointer(PointerEvent::moved(near, true));
    wandered.ui.host().send_pointer(PointerEvent::release(near));
    wandered.ui.wait_for_idle();
    assert_eq!(wandered.state.value(), tapped.state.value());
}

/// As `detectTapGestures`, the slider consumes the down: a clickable around it isn't pressed.
#[test]
fn a_clickable_around_the_slider_takes_no_press() {
    let parent = MutableInteractionSource::new();
    let clicks = Rc::new(Cell::new(0));
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let (given, count, handle) = (parent.clone(), clicks.clone(), *state);
    let mut ui = compose_test(material(move || {
        let count = count.clone();
        ClickableSurface!(
            move || count.set(count.get() + 1),
            interaction_source = Some(given.clone()),
            {
                Slider!(
                    handle,
                    move |new| handle.set_value(new),
                    modifier = Modifier.test_tag(TAG)
                );
            }
        );
    }));
    let at = ui.on_node_unmerged(has_test_tag(TAG)).fetch_semantics_node().center();
    ui.host().send_pointer(PointerEvent::press(at));
    ui.advance_time_by(std::time::Duration::from_millis(200));
    assert!(!parent.is_pressed());
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
    assert_eq!(clicks.get(), 0);
    assert!(state.value() > 0.0, "the slider took the tap");
}

/// A scrolling parent takes the gesture: the press ends, and there's no tap.
#[test]
fn a_scrolling_parent_cancels_the_press() {
    use compose_foundation::ScrollModifiers;
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let (given, handle) = (source.clone(), *state);
    let mut ui = compose_test(move || {
        let given = given.clone();
        let scroll = compose_foundation::remember_scroll_state!(1000);
        Column!(modifier = Modifier.vertical_scroll(scroll), {
            Spacer!(modifier = Modifier.height(1000.dp()));
            Slider!(
                handle,
                move |new| handle.set_value(new),
                modifier = Modifier.test_tag(TAG),
                interaction_source = Some(given.clone()),
            );
            Spacer!(modifier = Modifier.height(2000.dp()));
        });
    });
    let at = ui.on_node_unmerged(has_test_tag(TAG)).fetch_semantics_node().center();
    let touch = |event: PointerEvent| event.with_pointer(PointerId(0), PointerType::Touch);
    ui.host().send_pointer(touch(PointerEvent::press(at)));
    assert!(source.is_pressed());
    let mut position = at;
    for _ in 0..3 {
        position = position + Offset::new(0.0, 15.0);
        ui.host().send_pointer(touch(PointerEvent::moved(position, true)));
        ui.wait_for_idle();
    }
    assert!(!source.is_pressed(), "still pressed while the parent scrolls");
    ui.host().send_pointer(touch(PointerEvent::release(position)));
    ui.wait_for_idle();
    assert!(!source.is_pressed());
    assert_eq!(state.value(), 0.0);
}

/// The state's value is what the input goes on from, whatever else changed it (D1024): tap, set
/// the value back, tap the same place, and it moves again.
#[test]
fn a_tap_after_the_state_was_reset_moves_it_again() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let tap = |ui: &mut ComposeTestRule| {
        ui.host().send_pointer(PointerEvent::press(at));
        ui.host().send_pointer(PointerEvent::release(at));
        ui.wait_for_idle();
    };
    tap(&mut rig.ui);
    let tapped = rig.state.value();
    assert!(tapped > 0.0);
    rig.state.set_value(0.0);
    rig.ui.wait_for_idle();
    tap(&mut rig.ui);
    assert_eq!(rig.state.value(), tapped, "the second tap did nothing");
}

/// A key goes on from the state's value, not the one it last asked for (D1024).
#[test]
fn a_key_after_the_state_was_reset_goes_on_from_the_state() {
    let mut rig = rig();
    rig.ui.on_node_with_tag(TAG).request_focus();
    rig.ui.wait_for_idle();
    rig.ui.press_key(Key::MoveEnd);
    rig.ui.wait_for_idle();
    assert_eq!(rig.state.value(), 1.0);
    rig.state.set_value(0.5);
    rig.ui.wait_for_idle();
    rig.ui.press_key(Key::DirectionLeft);
    rig.ui.wait_for_idle();
    assert_near(rig.state.value(), 0.49);
}

/// `SetProgress` compares with the state's value, not the one it last asked for (D1024).
#[test]
fn set_progress_after_the_state_was_reset_moves_it_again() {
    let mut rig = rig();
    let set = |ui: &mut ComposeTestRule| {
        ui.on_node_with_tag(TAG)
            .perform_semantics_action_with(&SemanticsActions::SET_PROGRESS, |set| {
                assert!(set(0.75))
            });
        ui.wait_for_idle();
    };
    set(&mut rig.ui);
    assert_eq!(rig.state.value(), 0.75);
    rig.state.set_value(0.25);
    rig.ui.wait_for_idle();
    set(&mut rig.ui);
    assert_eq!(rig.state.value(), 0.75);
}

/// A second finger is ignored: nothing stays pressed after both are up, and the tap is the first
/// finger's.
#[test]
fn a_second_finger_leaves_nothing_pressed() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let second = at + Offset::new(20.0, 0.0);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(second_finger(PointerEvent::press(second)));
    rig.ui.wait_for_idle();
    assert!(rig.source.is_pressed());
    rig.ui.host().send_pointer(second_finger(PointerEvent::release(second)));
    rig.ui.wait_for_idle();
    assert!(rig.source.is_pressed(), "the first finger is still down");
    rig.ui.host().send_pointer(PointerEvent::release(at));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed(), "stuck pressed after two fingers");
    assert_eq!(rig.finished.get(), 1);
}

/// A second finger during a drag is ignored too.
#[test]
fn a_second_finger_during_a_drag_leaves_nothing_dragged() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let moved = at + Offset::new(30.0, 0.0);
    let second = at + Offset::new(-20.0, 0.0);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(moved, true));
    rig.ui.host().send_pointer(second_finger(PointerEvent::press(second)));
    rig.ui.wait_for_idle();
    let value = rig.state.value();
    rig.ui.host().send_pointer(second_finger(PointerEvent::moved(second, true)));
    rig.ui.host().send_pointer(second_finger(PointerEvent::release(second)));
    rig.ui.wait_for_idle();
    assert_eq!(rig.state.value(), value, "the second finger moved the value");
    assert!(rig.source.is_dragged() && rig.state.is_dragging(), "the first finger is still down");
    rig.ui.host().send_pointer(PointerEvent::release(moved));
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_pressed() && !rig.source.is_dragged() && !rig.state.is_dragging());
    assert_eq!(rig.finished.get(), 1);
}

/// Removed under a drag, the state isn't left dragging, and its release does nothing.
#[test]
fn a_slider_removed_during_a_drag_leaves_the_state_not_dragging() {
    let mut rig = rig();
    let at = center_of(&mut rig.ui);
    let moved = at + Offset::new(30.0, 0.0);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(moved, true));
    rig.ui.wait_for_idle();
    assert!(rig.state.is_dragging());
    rig.shown.set(false);
    rig.ui.wait_for_idle();
    assert!(!rig.source.is_dragged());
    assert!(!rig.state.is_dragging());
    rig.ui.host().send_pointer(PointerEvent::release(moved));
    rig.ui.wait_for_idle();
    assert!(!rig.state.is_dragging());
}

/// Deactivated under a drag (its host inactive), nothing stays on the source or the state.
#[test]
fn a_deactivated_slider_ends_its_interactions() {
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let active = mutable_state_of(true);
    let (given, handle, on) = (source.clone(), *state, active.handle());
    let mut ui = compose_test(material(move || {
        let given = given.clone();
        ReusableContentHost(on.get(), move || {
            Slider!(
                handle,
                move |new| handle.set_value(new),
                modifier = Modifier.test_tag(TAG),
                interaction_source = Some(given.clone()),
            );
        });
    }));
    let at = center_of(&mut ui);
    let moved = at + Offset::new(30.0, 0.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::moved(moved, true));
    ui.wait_for_idle();
    assert!(source.is_dragged() && state.is_dragging());
    active.set(false);
    ui.wait_for_idle();
    assert!(!source.is_dragged() && !state.is_dragging());
    active.set(true);
    ui.wait_for_idle();
    assert!(!source.is_dragged() && !state.is_dragging());
}

/// Reused for another key under a drag, its node staying attached, nothing stays on the source
/// or the state (`on_reset`), and the old gesture's next move does nothing.
#[test]
fn a_reused_slider_ends_its_interactions() {
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let key = mutable_state_of(0);
    let (given, handle, current) = (source.clone(), *state, key.handle());
    let mut ui = compose_test(material(move || {
        let given = given.clone();
        ReusableContent(current.get(), move || {
            Slider!(
                handle,
                move |new| handle.set_value(new),
                modifier = Modifier.test_tag(TAG),
                interaction_source = Some(given.clone()),
            );
        });
    }));
    let at = center_of(&mut ui);
    let moved = at + Offset::new(30.0, 0.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::moved(moved, true));
    ui.wait_for_idle();
    assert!(source.is_dragged() && state.is_dragging());
    key.set(1);
    ui.wait_for_idle();
    assert!(!source.is_dragged() && !state.is_dragging());
}

/// A source swapped under a drag: it ends on the old one, as `DragGestureNode.update`; it goes on
/// unreported, the new one hearing nothing; its release finishes.
#[test]
fn a_new_source_during_a_drag_hears_nothing_and_the_drag_goes_on() {
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let use_second = mutable_state_of(false);
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let finished = Rc::new(Cell::new(0));
    let (a, b, flag, handle, count) =
        (first.clone(), second.clone(), use_second.handle(), *state, finished.clone());
    let mut ui = compose_test(material(move || {
        let source = if flag.get() { b.clone() } else { a.clone() };
        let count = count.clone();
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            on_value_change_finished = Some(move || count.set(count.get() + 1)),
            interaction_source = Some(source),
        );
    }));
    let at = center_of(&mut ui);
    let moved = at + Offset::new(30.0, 0.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::moved(moved, true));
    ui.wait_for_idle();
    assert!(first.is_dragged());
    let before = state.value();
    use_second.set(true);
    ui.wait_for_idle();
    assert!(!first.is_dragged() && !first.is_pressed());
    let further = moved + Offset::new(30.0, 0.0);
    ui.host().send_pointer(PointerEvent::moved(further, true));
    ui.wait_for_idle();
    assert!(state.value() > before, "the drag stopped");
    assert!(state.is_dragging());
    assert!(!second.is_dragged());
    ui.host().send_pointer(PointerEvent::release(further));
    ui.wait_for_idle();
    assert!(!state.is_dragging());
    assert_eq!(finished.get(), 1);
}

/// A source swapped under a press ends it on the old one.
#[test]
fn a_new_source_during_a_press_ends_it_on_the_old_one() {
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let use_second = mutable_state_of(false);
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let (a, b, flag, handle) = (first.clone(), second.clone(), use_second.handle(), *state);
    let mut ui = compose_test(material(move || {
        let source = if flag.get() { b.clone() } else { a.clone() };
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            interaction_source = Some(source),
        );
    }));
    let at = center_of(&mut ui);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.wait_for_idle();
    assert!(first.is_pressed());
    use_second.set(true);
    ui.wait_for_idle();
    assert!(!first.is_pressed() && !second.is_pressed());
}

/// A source swapped while focused: `FocusableNode.update` disposes the old one's, and tells the
/// new one nothing until the next change.
#[test]
fn a_new_source_while_focused_hears_nothing() {
    let (first, second) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let use_second = mutable_state_of(false);
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let (a, b, flag, handle) = (first.clone(), second.clone(), use_second.handle(), *state);
    let mut ui = compose_test(material(move || {
        let source = if flag.get() { b.clone() } else { a.clone() };
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            interaction_source = Some(source),
        );
    }));
    ui.on_node_with_tag(TAG).request_focus();
    ui.wait_for_idle();
    assert!(first.is_focused());
    use_second.set(true);
    ui.wait_for_idle();
    assert!(!first.is_focused());
    assert!(!second.is_focused());
}

/// The pointers cancelled under a drag, it ends as `processDragCancel`: a cancellation, not
/// dragging, and `on_value_change_finished`, as `onDragStopped`.
#[test]
fn a_cancelled_drag_finishes_the_value_change() {
    let mut rig = rig();
    let interactions = recorded(&rig.source);
    rig.ui.wait_for_idle();
    let at = center_of(&mut rig.ui);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.host().send_pointer(PointerEvent::moved(at + Offset::new(30.0, 0.0), true));
    rig.ui.wait_for_idle();
    assert_eq!(rig.finished.get(), 0);
    rig.ui.host().with_tree(|tree| tree.cancel_pointers());
    rig.ui.wait_for_idle();
    assert!(!rig.state.is_dragging());
    assert_eq!(*interactions.borrow(), ["Press", "DragStart", "PressCancel", "DragCancel"]);
    assert_eq!(rig.finished.get(), 1);
}

/// The pointers cancelled under a press: a cancellation, nothing finished.
#[test]
fn a_cancelled_press_finishes_nothing() {
    let mut rig = rig();
    let interactions = recorded(&rig.source);
    rig.ui.wait_for_idle();
    let at = center_of(&mut rig.ui);
    rig.ui.host().send_pointer(PointerEvent::press(at));
    rig.ui.wait_for_idle();
    rig.ui.host().with_tree(|tree| tree.cancel_pointers());
    rig.ui.wait_for_idle();
    assert_eq!(*interactions.borrow(), ["Press", "PressCancel"]);
    assert_eq!(rig.finished.get(), 0);
    assert_eq!(rig.state.value(), 0.0);
}

/// Consumes every release that reaches it.
#[derive(Clone)]
struct ConsumeRelease;

struct ConsumeReleaseNode;

impl compose_ui::ModifierNodeElement for ConsumeRelease {
    type Node = ConsumeReleaseNode;
    fn create(&self) -> ConsumeReleaseNode {
        ConsumeReleaseNode
    }
    fn update(&self, _: &mut ConsumeReleaseNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
    fn name(&self) -> &'static str {
        "consume_release"
    }
}

impl compose_ui::ModifierNode for ConsumeReleaseNode {
    fn as_pointer_input(&mut self) -> Option<&mut dyn compose_ui::PointerInputModifierNode> {
        Some(self)
    }
}

impl compose_ui::PointerInputModifierNode for ConsumeReleaseNode {
    fn on_pointer_event(
        &mut self,
        event: &mut PointerEvent,
        pass: compose_ui::PointerEventPass,
        _: compose_ui::IntSize,
        _: &mut compose_ui::EventContext<'_>,
    ) {
        if pass == compose_ui::PointerEventPass::Main
            && event.kind == compose_ui::PointerEventType::Release
        {
            event.consume();
        }
    }
}

/// A release a thumb inside consumed is a cancellation, as `tryAwaitRelease`: no tap.
#[test]
fn a_consumed_release_cancels_the_press() {
    let source = MutableInteractionSource::new();
    let state = SliderState::new(0.0, 0, (0.0, 1.0));
    let finished = Rc::new(Cell::new(0));
    let (given, handle, count) = (source.clone(), *state, finished.clone());
    let mut ui = compose_test(material(move || {
        let count = count.clone();
        Slider!(
            handle,
            move |new| handle.set_value(new),
            modifier = Modifier.test_tag(TAG),
            on_value_change_finished = Some(move || count.set(count.get() + 1)),
            interaction_source = Some(given.clone()),
            thumb = Some(|_: SliderState| {
                Box!(modifier = Modifier.size(48.dp()).then_custom(ConsumeRelease));
            }),
        );
    }));
    let interactions = recorded(&source);
    ui.wait_for_idle();
    let bounds = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    // On the thumb, near the slider's start.
    let at = Offset::new(bounds.left + SEMANTICS_PADDING + 30.0, bounds.top + 24.0);
    ui.host().send_pointer(PointerEvent::press(at));
    ui.host().send_pointer(PointerEvent::release(at));
    ui.wait_for_idle();
    assert_eq!(*interactions.borrow(), ["Press", "PressCancel"]);
    assert_eq!(state.value(), 0.0);
    assert_eq!(finished.get(), 0);
}
