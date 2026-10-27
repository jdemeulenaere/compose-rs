//! `animate*AsState` frame by frame against Compose's evaluation of the same specs: tweens of
//! every dimension, colors, springs ending at their estimated duration within their visibility
//! thresholds (a spec's own, the type's default, or a threshold given), spec changes, and an
//! animation interrupted mid-way.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Dp, DpOffset, IntOffset, IntSize, Offset, Rect, Size};

/// Reads the animated values.
type Reader<V> = Rc<dyn Fn() -> V>;
type Slot<V> = Rc<RefCell<Option<Reader<V>>>>;

/// A rule whose content animates `target` with `animate`, and a slot holding what reads the
/// values it gives.
fn animated<T: Clone + PartialEq + 'static, V: Clone + 'static>(
    initial: T,
    animate: impl Fn(T) -> Reader<V> + 'static,
) -> (ComposeTestRule, OwnedState<T>, Slot<V>) {
    let target = mutable_state_of(initial);
    let handle = target.handle();
    let slot: Slot<V> = Rc::new(RefCell::new(None));
    let kept = slot.clone();
    let ui = ComposeTestRule::with_manual_clock(move || {
        let reader = animate(handle.get());
        *kept.borrow_mut() = Some(reader);
    });
    (ui, target, slot)
}

fn read<V: Clone + 'static>(state: State<V>) -> Reader<V> {
    Rc::new(move || state.get())
}

fn value<V: Clone + 'static>(slot: &Slot<V>) -> V {
    let reader = slot.borrow().clone().expect("composed");
    reader()
}

/// Sets the target, then runs frames until the animation is over: the values at each frame, the
/// first at play time 0 (the frame after the change composes), and the play times.
fn frames<T: Clone + PartialEq + 'static, V: Clone + PartialEq + 'static>(
    ui: &mut ComposeTestRule,
    target: &OwnedState<T>,
    slot: &Slot<V>,
    new_target: T,
    count: usize,
) -> Vec<(i64, V)> {
    target.set(new_target);
    // The change composes at the next frame; the animation's first frame is the one after.
    ui.advance_time_by_frame();
    (0..count)
        .map(|frame| {
            ui.advance_time_by_frame();
            (frame as i64 * 16, value(slot))
        })
        .collect()
}

fn tween_fraction(easing: Easing, play_millis: i64, duration: i64) -> f32 {
    easing.transform(play_millis.clamp(0, duration) as f32 / duration as f32)
}

fn lerp(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, animate1DTest
#[test]
fn a_dp_tweens_frame_by_frame() {
    let spec = tween(100);
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        read(animate_dp_as_state!(
            if enabled { Dp(50.0) } else { Dp(250.0) },
            animation_spec = spec
        ))
    });
    assert_eq!(value(&slot), Dp(250.0));
    for (play, dp) in frames(&mut ui, &target, &slot, true, 9) {
        let expected = lerp(250.0, 50.0, tween_fraction(FAST_OUT_SLOW_IN_EASING, play, 100));
        assert_eq!(dp, Dp(expected), "at {play} ms");
    }
    assert_eq!(value(&slot), Dp(50.0));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, animate1DOnCoroutineTest
#[test]
fn a_float_tweens_frame_by_frame() {
    let spec = tween(200).easing(FAST_OUT_LINEAR_IN_EASING);
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        read(animate_float_as_state!(if enabled { 50.0 } else { 250.0 }, animation_spec = spec))
    });
    for (play, float) in frames(&mut ui, &target, &slot, true, 15) {
        let expected = lerp(250.0, 50.0, tween_fraction(FAST_OUT_LINEAR_IN_EASING, play, 200));
        assert_eq!(float, expected, "at {play} ms");
    }
    assert_eq!(value(&slot), 50.0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, animate2DTest
#[test]
fn sizes_and_offsets_tween_frame_by_frame() {
    let spec = tween(100).easing(LINEAR_EASING);
    let (start, end) = ([120.0, 56.0], [0.0, 77.0]);
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        let [x, y] = if enabled { end } else { start };
        let size = animate_size_as_state!(Size::new(x, y), animation_spec = spec);
        let offset = animate_offset_as_state!(Offset::new(x, y), animation_spec = spec);
        Rc::new(move || (size.get(), offset.get()))
    });
    for (play, (size, offset)) in frames(&mut ui, &target, &slot, true, 9) {
        let fraction = play.min(100) as f32 / 100.0;
        let (x, y) = (lerp(start[0], end[0], fraction), lerp(start[1], end[1], fraction));
        assert_eq!(size, Size::new(x, y), "at {play} ms");
        assert_eq!(offset, Offset::new(x, y), "at {play} ms");
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, animate4DRectTest
#[test]
fn a_rect_tweens_frame_by_frame() {
    let spec = tween(100).easing(LINEAR_OUT_SLOW_IN_EASING);
    let (start, end) = ([30.0, -76.0, 280.0, 35.0], [-42.0, 89.0, 77.0, 100.0]);
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        let [l, t, r, b] = if enabled { end } else { start };
        read(animate_rect_as_state!(Rect::new(l, t, r, b), animation_spec = spec))
    });
    for (play, rect) in frames(&mut ui, &target, &slot, true, 9) {
        let fraction = tween_fraction(LINEAR_OUT_SLOW_IN_EASING, play, 100);
        let at = |index: usize| lerp(start[index], end[index], fraction);
        assert_eq!(rect, Rect::new(at(0), at(1), at(2), at(3)), "at {play} ms");
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, animateColorTest
#[test]
fn a_color_tweens_in_oklab_frame_by_frame() {
    let spec = tween(100).easing(FAST_OUT_LINEAR_IN_EASING);
    let cyan = Color::from_argb(0xFF00FFFF);
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        read(animate_color_as_state!(
            if enabled { cyan } else { Color::BLACK },
            animation_spec = spec
        ))
    });
    let (black, cyan_vector) = (Color::BLACK.to_vector(), cyan.to_vector());
    for (play, color) in frames(&mut ui, &target, &slot, true, 9) {
        let fraction = tween_fraction(FAST_OUT_LINEAR_IN_EASING, play, 100);
        let expected = Color::from_vector(black.map(|i, a| lerp(a, cyan_vector[i], fraction)));
        for (actual, expected) in [
            (color.red(), expected.red()),
            (color.green(), expected.green()),
            (color.blue(), expected.blue()),
            (color.alpha(), expected.alpha()),
        ] {
            assert!(actual.abs_diff(expected) <= 1, "at {play} ms: {color:?}, not {expected:?}");
        }
    }
    assert_eq!(value(&slot), cyan);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, frameByFrameInterruptionTest
#[test]
fn a_target_moved_every_frame_still_gets_there() {
    let (mut ui, target, slot) = animated(Offset::new(0.0, 0.0), move |target: Offset| {
        read(animate_offset_as_state!(target, animation_spec = __offset_default_spring()))
    });
    let mut moved = false;
    for frame in 0..80 {
        let near = if frame % 2 == 0 { 599.0 } else { 601.0 };
        target.set(Offset::new(near, near));
        ui.advance_time_by_frame();
        let offset = value(&slot);
        if offset.x > 300.0 && offset.y > 300.0 {
            moved = true;
            break;
        }
    }
    assert!(moved, "the offset froze at {:?}", value(&slot));
}

/// The value of a spring of `threshold` from 0 to 100 at `play` ms, as Compose's
/// `FloatSpringSpec`, and the target once it's over.
fn spring_value(threshold: f32, play: i64) -> f32 {
    let spec = FloatSpringSpec::new(1.0, Spring::STIFFNESS_MEDIUM, threshold);
    if play * 1_000_000 < spec.duration_nanos(0.0, 100.0, 0.0) {
        spec.value_from_millis(play, 0.0, 100.0, 0.0)
    } else {
        100.0
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, visibilityThresholdTest
#[test]
fn springs_end_within_their_own_threshold() {
    let offset_spec = spring().visibility_threshold(Offset::new(0.5, 0.5));
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        let at = if enabled { 100.0 } else { 0.0 };
        let offset = animate_offset_as_state!(Offset::new(at, at), animation_spec = offset_spec);
        let float =
            animate_float_as_state!(at, animation_spec = spring().visibility_threshold(0.01f32));
        Rc::new(move || (offset.get(), float.get()))
    });
    for (play, (offset, float)) in frames(&mut ui, &target, &slot, true, 70) {
        let expected = spring_value(0.5, play);
        assert_eq!(offset, Offset::new(expected, expected), "at {play} ms");
        assert_eq!(float, spring_value(0.01, play), "at {play} ms");
    }
    assert_eq!(value(&slot), (Offset::new(100.0, 100.0), 100.0));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, defaultVisibilityThresholdTest
#[test]
fn default_springs_end_within_their_types_threshold() {
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        let at = if enabled { 100.0 } else { 0.0 };
        let offset = animate_offset_as_state!(
            Offset::new(at, at),
            animation_spec = __offset_default_spring()
        );
        let float = animate_float_as_state!(at, animation_spec = spring());
        Rc::new(move || (offset.get(), float.get()))
    });
    let mut offset_ended = None;
    for (play, (offset, float)) in frames(&mut ui, &target, &slot, true, 70) {
        // A pixel for an offset, 0.01 for a float.
        let expected = spring_value(Offset::VISIBILITY_THRESHOLD.x, play);
        assert_eq!(offset, Offset::new(expected, expected), "at {play} ms");
        assert_eq!(
            float,
            spring_value(Spring::DEFAULT_DISPLACEMENT_THRESHOLD, play),
            "at {play} ms"
        );
        if offset_ended.is_none() && offset.x == 100.0 {
            offset_ended = Some(play);
        }
    }
    // The offset's spring ends sooner (its threshold is larger).
    let float_duration = FloatSpringSpec::default().duration_millis(0.0, 100.0, 0.0);
    assert!(offset_ended.unwrap() < float_duration, "{offset_ended:?} against {float_duration}");
}

/// Runs `animate_float_as_state(target, spec)` from `start` to `end` frame by frame until its
/// finished listener hears it: the values seen.
fn values_until_finished(start: f32, end: f32, spec: SpringSpec) -> Vec<f32> {
    let finished = Rc::new(Cell::new(false));
    let heard = finished.clone();
    let (mut ui, target, slot) = animated(start, move |target: f32| {
        let heard = heard.clone();
        read(animate_float_as_state!(
            target,
            animation_spec = spec,
            finished_listener = move |_| heard.set(true)
        ))
    });
    target.set(end);
    ui.wait_for_idle();
    let mut values = Vec::new();
    while !finished.get() && values.len() < 2000 {
        values.push(value(&slot));
        ui.advance_time_by_frame();
    }
    values.push(value(&slot));
    assert!(finished.get(), "never finished");
    values
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, customSpringSpecVisibilityThresholdTest
#[test]
fn a_spring_snaps_to_its_target_once_within_its_threshold() {
    let threshold = 0.1;
    let spec = spring_with(1.0, Spring::STIFFNESS_VERY_LOW).visibility_threshold(threshold);
    let values = values_until_finished(0.0, 1.0, spec);
    assert_eq!(*values.last().unwrap(), 1.0);
    for value in &values {
        assert!(*value == 1.0 || *value <= 1.0 - threshold, "{value} in (0.9, 1)");
    }
    assert!(values.len() > 2, "it animated: {values:?}");
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, customSpringSpecLargeVisibilityThresholdTest
#[test]
fn a_spring_of_a_large_threshold_snaps_from_that_far() {
    let threshold = 5.0;
    let spec = spring_with(1.0, Spring::STIFFNESS_VERY_LOW).visibility_threshold(threshold);
    let values = values_until_finished(200.0, 1000.0, spec);
    assert_eq!(*values.last().unwrap(), 1000.0);
    for value in &values {
        assert!(*value == 1000.0 || *value <= 1000.0 - threshold, "{value} in (995, 1000)");
    }
    assert!(values.len() > 2, "it animated: {values:?}");
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, updateAnimationSpecTest
#[test]
fn a_new_spec_applies_to_the_next_animation() {
    let duration = Rc::new(Cell::new(100));
    let duration_read = duration.clone();
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        let spec = tween(duration_read.get());
        read(animate_dp_as_state!(
            if enabled { Dp(50.0) } else { Dp(250.0) },
            animation_spec = spec
        ))
    });
    for (play, dp) in frames(&mut ui, &target, &slot, true, 9) {
        let expected = lerp(250.0, 50.0, tween_fraction(FAST_OUT_SLOW_IN_EASING, play, 100));
        assert_eq!(dp, Dp(expected), "at {play} ms");
    }
    duration.set(200);
    for (play, dp) in frames(&mut ui, &target, &slot, false, 15) {
        let expected = lerp(50.0, 250.0, tween_fraction(FAST_OUT_SLOW_IN_EASING, play, 200));
        assert_eq!(dp, Dp(expected), "at {play} ms");
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, updateVisibilityThresholdTest
#[test]
fn a_threshold_changed_mid_tween_leaves_the_tween_alone() {
    let threshold = mutable_state_of(Dp(0.0));
    let threshold_read = threshold.handle();
    let finished = Rc::new(Cell::new(false));
    let heard = finished.clone();
    let (mut ui, target, slot) = animated(false, move |enabled: bool| {
        let heard = heard.clone();
        read(animate_value_as_state!(
            if enabled { Dp(50.0) } else { Dp(250.0) },
            animation_spec = tween(100),
            visibility_threshold = Some(threshold_read.get()),
            finished_listener = move |_| heard.set(true),
        ))
    });
    target.set(true);
    // The change composes at the next frame; the tween starts at the one after.
    ui.advance_time_by_frame();
    let mut play = 0;
    loop {
        ui.advance_time_by_frame();
        let fraction = tween_fraction(FAST_OUT_SLOW_IN_EASING, play, 100);
        assert_eq!(value(&slot), Dp(lerp(250.0, 50.0, fraction)), "at {play} ms");
        if fraction > 0.5 {
            break;
        }
        play += 16;
    }
    threshold.set(Dp(10.0));
    while !finished.get() {
        ui.advance_time_by_frame();
        play += 16;
        let fraction = tween_fraction(FAST_OUT_SLOW_IN_EASING, play, 100);
        assert_eq!(value(&slot), Dp(lerp(250.0, 50.0, fraction)), "at {play} ms");
        assert!(play <= 112, "the tween ended late");
    }
    assert_eq!(value(&slot), Dp(50.0));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, interruptionAwareAnimationSpec_usesInterruptionSpecOnInterruption
// The frames of its interruption, with one spec: the new animation starts at the frame the one
// it interrupts last set the value at, not a frame later.
#[test]
fn an_interrupting_animation_starts_where_the_other_left_off() {
    let spec = tween(200).easing(LINEAR_EASING);
    let (mut ui, target, slot) = animated(false, move |on: bool| {
        read(animate_float_as_state!(if on { 100.0 } else { 0.0 }, animation_spec = spec))
    });
    target.set(true);
    // The change composes, then the animation's first frame (play time 0), then 64 ms of it.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(std::time::Duration::from_millis(64));
    assert!((value(&slot) - 32.0).abs() < 0.1, "{}", value(&slot));
    // Back: the old animation takes one more frame (80 ms: 40), the new one starts there.
    target.set(false);
    ui.advance_time_by_frame();
    let at_interruption = value(&slot);
    assert!((at_interruption - 40.0).abs() < 0.1, "{at_interruption}");
    // 64 ms into its 200 ms: 32 % of the way back.
    ui.advance_time_by(std::time::Duration::from_millis(64));
    assert!((value(&slot) - at_interruption * 0.68).abs() < 0.1, "{}", value(&slot));
}

/// The types' thresholds, pinned to Compose's `VisibilityThresholds.kt` values: 0.4 dp, a
/// pixel, 1 for integers.
#[test]
fn the_types_thresholds_are_composes() {
    assert_eq!(Dp::VISIBILITY_THRESHOLD, Dp(0.4));
    assert_eq!(DpOffset::VISIBILITY_THRESHOLD, DpOffset::new(Dp(0.4), Dp(0.4)));
    assert_eq!(Offset::VISIBILITY_THRESHOLD, Offset::new(1.0, 1.0));
    assert_eq!(Size::VISIBILITY_THRESHOLD, Size::new(1.0, 1.0));
    assert_eq!(Rect::VISIBILITY_THRESHOLD, Rect::new(1.0, 1.0, 1.0, 1.0));
    assert_eq!(IntOffset::VISIBILITY_THRESHOLD, IntOffset::new(1, 1));
    assert_eq!(IntSize::VISIBILITY_THRESHOLD, IntSize::new(1, 1));
    assert_eq!(i32::VISIBILITY_THRESHOLD, 1);
}

/// The default springs (stiffness 1500) from 0 to 100, pinned to Compose's estimated
/// durations: 198 ms within 0.4 dp, 171 ms within a pixel, 303 ms within 0.01.
#[test]
fn the_default_springs_last_composes_durations() {
    let dp = TargetBasedAnimation::new(__dp_default_spring(), Dp(0.0), Dp(100.0));
    assert_eq!(dp.duration_millis(), 198);
    let offset = TargetBasedAnimation::new(
        __offset_default_spring(),
        Offset::ZERO,
        Offset::new(100.0, 100.0),
    );
    assert_eq!(offset.duration_millis(), 171);
    let float = TargetBasedAnimation::new(spring(), 0.0f32, 100.0);
    assert_eq!(float.duration_millis(), 303);
}
