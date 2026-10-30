//! Animations under a `MotionDurationScale`, against Compose's `DurationScaleTest` and
//! `DurationScaleTransitionTest`: a scale stretches play times (5: five times as long), 0 ends
//! them at once; a task's own scale (`with_motion_duration_scale`, Compose's `withContext`) or
//! the thread's (the rule's, Compose's test `effectContext`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    motion_duration_scale, set_motion_duration_scale, set_motion_duration_scale_source,
    with_motion_duration_scale,
};

/// Runs `task` in a rule's clock, a frame at a time, until it ends.
fn run(ui: &mut ComposeTestRule, task: impl std::future::Future<Output = ()> + 'static) {
    let done = Rc::new(Cell::new(false));
    let finished = done.clone();
    let scope = CoroutineScope::new();
    scope.launch(async move {
        task.await;
        finished.set(true);
    });
    let mut frames = 0;
    while !done.get() {
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 1_000, "the animation never ended");
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DurationScaleTest.kt, testAnimatable
#[test]
fn a_scale_stretches_an_animation_and_0_ends_it_at_once() {
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    // Five times as long: 500 over 500 ms, one a millisecond.
    let values = Rc::new(RefCell::new(Vec::new()));
    let seen = values.clone();
    run(
        &mut ui,
        with_motion_duration_scale(5.0, async move {
            animate(0.0, 500.0, 0.0, tween(100).easing(LINEAR_EASING), move |value, _| {
                seen.borrow_mut().push(value);
            })
            .await;
        }),
    );
    let values = values.borrow();
    assert_eq!(values.len(), 500 / 16 + 2, "{values:?}");
    for (frame, value) in values.iter().enumerate() {
        assert_eq!(*value, (16.0 * frame as f32).min(500.0), "frame {frame}");
    }
    // At 0: at the target on the first frame, an infinite animation too.
    let ends = Rc::new(RefCell::new(Vec::new()));
    let (seen, seen_infinite) = (ends.clone(), ends.clone());
    run(
        &mut ui,
        with_motion_duration_scale(0.0, async move {
            animate(0.0, 500.0, 0.0, tween(100).easing(LINEAR_EASING), move |value, _| {
                seen.borrow_mut().push(value);
            })
            .await;
            let infinite = infinite_repeatable(tween(100), RepeatMode::Restart);
            animate(0.0, 100.0, 0.0, infinite, move |value, _| {
                seen_infinite.borrow_mut().push(value);
            })
            .await;
        }),
    );
    assert_eq!(*ends.borrow(), [500.0, 100.0]);
}

/// Compose's three tests' content: a value over 160 ms, and a child transition's once
/// `with_child`, under the rule's scale of 4.
fn child_rule(
    state: impl TransitionState<i32> + 'static,
    with_child: MutableState<bool>,
) -> (ComposeTestRule, Rc<Cell<f32>>, Rc<Cell<f32>>) {
    let (first, second) = (Rc::new(Cell::new(-1.0f32)), Rc::new(Cell::new(-1.0f32)));
    let (seen_first, seen_second) = (first.clone(), second.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&state, None);
        let linear = || tween(160).easing(LINEAR_EASING).into();
        let value =
            transition.animate_float(move |_| linear(), |state| f32::from(*state != 0) * 1000.0);
        seen_first.set(value.get());
        if with_child.get() {
            let child = transition.create_child_transition(None, |state| *state);
            let value =
                child.animate_float(move |_| linear(), |state| f32::from(*state != 0) * 1000.0);
            seen_second.set(value.get());
        }
    });
    ui.set_motion_duration_scale(4.0);
    (ui, first, second)
}

fn close(actual: f32, expected: f32, what: &str) {
    assert!((actual - expected).abs() <= 0.1, "{what}: {actual}, Compose's {expected}");
}

fn millis(millis: u64) -> std::time::Duration {
    std::time::Duration::from_millis(millis)
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DurationScaleTransitionTest.kt, childTransitionWithDurationScale
#[test]
fn a_child_joining_a_scaled_transition_takes_its_scaled_time() {
    let state = MutableTransitionState::new(0);
    let with_child = mutable_state_of(false);
    let (mut ui, first, second) = child_rule(state.clone(), with_child.handle());
    ui.advance_time_by_frame();
    state.set_target_state(1);
    // The composition, then the start time.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((first.get(), second.get()), (0.0, -1.0));
    // Half way: 320 of 640 ms.
    ui.advance_time_by(millis(320));
    close(first.get(), 500.0, "first");
    assert_eq!(second.get(), -1.0);
    with_child.set(true);
    ui.advance_time_by_frame();
    assert_eq!(second.get(), 0.0);
    ui.advance_time_by(millis(320));
    assert_eq!(first.get(), 1000.0);
    close(second.get(), 500.0, "second");
    ui.advance_time_by(millis(320));
    assert_eq!(second.get(), 1000.0);
}

/// Runs `call` as a task; whether it ended.
fn launch(call: impl std::future::Future<Output = ()> + 'static) -> Rc<Cell<bool>> {
    let done = Rc::new(Cell::new(false));
    let finished = done.clone();
    let scope = CoroutineScope::new();
    scope.launch(async move {
        call.await;
        finished.set(true);
    });
    std::mem::forget(scope);
    done
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DurationScaleTransitionTest.kt, childTransitionWithDurationScaleSeekableTransition
#[test]
fn a_child_joining_a_scaled_seekable_animation_takes_its_scaled_time() {
    let state = SeekableTransitionState::new(0);
    let with_child = mutable_state_of(false);
    let (mut ui, first, second) = child_rule(state.clone(), with_child.handle());
    ui.advance_time_by_frame();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, 1).await });
    ui.advance_time_by_frame();
    assert!(seek.get());
    assert_eq!((first.get(), second.get()), (0.0, -1.0));
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(1).await });
    // The start time, then half way.
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(320));
    close(first.get(), 500.0, "first");
    assert_eq!(second.get(), -1.0);
    with_child.set(true);
    ui.advance_time_by_frame();
    assert_eq!(second.get(), 0.0);
    ui.advance_time_by(millis(320));
    assert_eq!(first.get(), 1000.0);
    close(second.get(), 500.0, "second");
    ui.advance_time_by(millis(320));
    assert_eq!(second.get(), 1000.0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DurationScaleTransitionTest.kt, childTransitionWithDurationScaleSeekTransition
#[test]
fn a_child_joining_a_seek_counts_in_the_fraction() {
    let state = SeekableTransitionState::new(0);
    let with_child = mutable_state_of(false);
    let (mut ui, first, second) = child_rule(state.clone(), with_child.handle());
    ui.advance_time_by_frame();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.5, 1).await });
    ui.advance_time_by_frame();
    assert!(seek.get());
    assert_eq!((first.get(), second.get()), (500.0, -1.0));
    with_child.set(true);
    // The composition with the child, then the seek again for the new total duration.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // Half its total, unscaled (a seek's play time): the child, added 80 ms in, ends at 240 ms;
    // 120 ms in, the first is at 3/4 and the child at 1/4.
    close(first.get(), 750.0, "first");
    close(second.get(), 250.0, "second");
    let seeking = state.clone();
    let _ = launch(async move { seeking.seek_to(0.75).await });
    ui.advance_time_by_frame();
    assert_eq!(first.get(), 1000.0);
    close(second.get(), 625.0, "second");
    let seeking = state.clone();
    let _ = launch(async move { seeking.seek_to(1.0).await });
    ui.advance_time_by_frame();
    assert_eq!((first.get(), second.get()), (1000.0, 1000.0));
}

// Compose: compose/animation/animation-core/src/commonMain/kotlin/androidx/compose/animation/core/InfiniteTransition.kt, run (no test of Compose's: its frame loop's reading of the scale)
#[test]
fn an_infinite_transition_stretches_ends_at_0_and_restarts_when_the_scale_changes() {
    let value = Rc::new(Cell::new(-1.0f32));
    let seen = value.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_infinite_transition!();
        let animated = transition.animate_float(
            0.0,
            1000.0,
            infinite_repeatable(tween(1000).easing(LINEAR_EASING), RepeatMode::Restart),
        );
        seen.set(animated.get());
    });
    ui.set_motion_duration_scale(2.0);
    // The scale changed: the first frame restarts it, 0 ms in.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 8.0);
    // 1024 ms in (a time rounds up to whole frames), at half speed.
    ui.advance_time_by(millis(1000));
    assert_eq!(value.get(), 512.0);
    // At 0: at the target on the next frame, and no more frames.
    ui.set_motion_duration_scale(0.0);
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 1000.0);
    ui.advance_time_by(millis(100));
    assert_eq!(value.get(), 1000.0);
    // It waits for the scale, asking for no frames.
    assert!(!platform::has_frame_awaiters());
    // Above 0 again: from the start, at the first frame after.
    ui.set_motion_duration_scale(1.0);
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 0.0);
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 16.0);
}

// Compose: compose/animation/animation-core/src/commonMain/kotlin/androidx/compose/animation/core/Transition.kt, animateTo (no test of Compose's: the frame loop reads the scale as it starts)
#[test]
fn a_transition_keeps_the_scale_it_started_at_and_the_next_takes_the_new_one() {
    let state = MutableTransitionState::new(0);
    let with_child = mutable_state_of(false);
    let (mut ui, first, _) = child_rule(state.clone(), with_child.handle());
    ui.advance_time_by_frame();
    state.set_target_state(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(320));
    close(first.get(), 500.0, "at a quarter speed");
    // Its loop read 4 when it started.
    ui.set_motion_duration_scale(1.0);
    ui.advance_time_by(millis(160));
    close(first.get(), 750.0, "still at a quarter speed");
    ui.advance_time_by(millis(320));
    assert_eq!(first.get(), 1000.0);
    // The next one reads 1.
    state.set_target_state(0);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(80));
    close(first.get(), 500.0, "at full speed");
    ui.advance_time_by(millis(80));
    assert_eq!(first.get(), 0.0);
    // At 0, it ends on its first frame.
    ui.set_motion_duration_scale(0.0);
    state.set_target_state(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(first.get(), 1000.0);
    assert!(state.is_idle());
}

// Compose: compose/animation/animation-core/src/commonMain/kotlin/androidx/compose/animation/core/Animatable.kt, animateTo (through SuspendAnimation.kt's durationScale; no test of Compose's for Animatable)
#[test]
fn an_animatable_and_a_value_as_state_follow_the_scale_of_the_thread() {
    let target = mutable_state_of(0.0f32);
    let value = Rc::new(Cell::new(-1.0f32));
    let (seen, read) = (value.clone(), target.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let spec = tween(160).easing(LINEAR_EASING);
        let animated = animate_float_as_state!(read.get(), animation_spec = spec);
        seen.set(animated.get());
    });
    ui.set_motion_duration_scale(2.0);
    ui.advance_time_by_frame();
    target.set(1000.0);
    // The recomposition, the start time, then half way through 320 ms.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(160));
    close(value.get(), 500.0, "the value as state");
    ui.advance_time_by(millis(160));
    assert_eq!(value.get(), 1000.0);
    // At 0: at the target on its first frame.
    ui.set_motion_duration_scale(0.0);
    target.set(0.0);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 0.0);
    // A task's `Animatable` under a scale of its own.
    let animatable = Animatable::new(0.0f32);
    let moved = animatable.clone();
    ui.set_motion_duration_scale(1.0);
    let done = launch(with_motion_duration_scale(4.0, async move {
        moved.animate_to(1000.0, tween(40).easing(LINEAR_EASING)).await;
    }));
    // The first frame is its start: 64 ms in, 16 of its 40 at a quarter speed.
    ui.advance_time_by(millis(80));
    assert_eq!(animatable.value(), 400.0);
    assert!(!done.get());
    ui.advance_time_by(millis(90));
    assert_eq!(animatable.value(), 1000.0);
    assert!(done.get());
}

// Compose: compose/animation/animation-core/src/commonMain/kotlin/androidx/compose/animation/core/Transition.kt, runAnimations (no test of Compose's: a scale of 0 ends a seekable's animation)
#[test]
fn a_seekable_animation_at_a_scale_of_0_ends_at_once() {
    let state = SeekableTransitionState::new(0);
    let with_child = mutable_state_of(false);
    let (mut ui, first, _) = child_rule(state.clone(), with_child.handle());
    ui.advance_time_by_frame();
    ui.set_motion_duration_scale(0.0);
    let animating = state.clone();
    let done = launch(async move { animating.animate_to(1).await });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(done.get());
    assert_eq!(first.get(), 1000.0);
    assert_eq!(state.current_state(), 1);
}

#[test]
fn a_task_of_its_own_scale_does_not_change_what_runs_around_it() {
    use compose_ui::{await_positive_motion_duration_scale, motion_duration_scale};
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    assert_eq!(motion_duration_scale(), 1.0);
    let seen = Rc::new(Cell::new(-1.0f32));
    let inner = seen.clone();
    run(
        &mut ui,
        with_motion_duration_scale(3.0, async move {
            with_frame_nanos(|_| ()).await;
            inner.set(motion_duration_scale());
        }),
    );
    assert_eq!(seen.get(), 3.0);
    assert_eq!(motion_duration_scale(), 1.0);
    // Waiting for a scale above 0.
    ui.set_motion_duration_scale(0.0);
    let waited = launch(await_positive_motion_duration_scale());
    ui.advance_time_by_frame();
    assert!(!waited.get());
    ui.set_motion_duration_scale(0.5);
    ui.advance_time_by_frame();
    assert!(waited.get());
    // The rule's scale goes with it.
    drop(ui);
    assert_eq!(motion_duration_scale(), 1.0);
}

#[test]
#[should_panic(expected = "negative scale factor")]
fn a_negative_scale_panics_as_compose_s() {
    compose_ui::set_motion_duration_scale(-1.0);
}

// Compose: compose/animation/animation-core/src/commonMain/kotlin/androidx/compose/animation/core/InfiniteTransition.kt, run (no test of Compose's: "Restart every time duration scale changes")
#[test]
fn an_infinite_transition_restarts_when_the_scale_changes_between_two_non_zero_values() {
    let value = Rc::new(Cell::new(-1.0f32));
    let seen = value.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_infinite_transition!();
        let animated = transition.animate_float(
            0.0,
            1000.0,
            infinite_repeatable(tween(1000).easing(LINEAR_EASING), RepeatMode::Restart),
        );
        seen.set(animated.get());
    });
    ui.set_motion_duration_scale(2.0);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(320));
    assert_eq!(value.get(), 160.0);
    ui.set_motion_duration_scale(4.0);
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 0.0, "from the start at the first frame after");
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 4.0, "at a quarter speed");
}

thread_local! {
    static SOURCE_READS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn source() -> f32 {
    SOURCE_READS.with(|reads| reads.set(reads.get() + 1));
    0.5
}

/// A host's platform setting is read the first time a scale is, once (D839), as Compose's
/// `MotionDurationScaleImpl` starts observing at the first read of `scaleFactor`.
#[test]
fn a_source_of_the_scale_is_read_at_its_first_use() {
    SOURCE_READS.with(|reads| reads.set(0));
    set_motion_duration_scale_source(source);
    assert_eq!(SOURCE_READS.with(std::cell::Cell::get), 0);
    assert_eq!(motion_duration_scale(), 0.5);
    assert_eq!(motion_duration_scale(), 0.5);
    assert_eq!(SOURCE_READS.with(std::cell::Cell::get), 1);
    set_motion_duration_scale(1.0);
}

/// A set before the first read has the last word.
#[test]
fn a_set_takes_the_place_of_a_source_not_read_yet() {
    SOURCE_READS.with(|reads| reads.set(0));
    set_motion_duration_scale_source(source);
    set_motion_duration_scale(3.0);
    assert_eq!(motion_duration_scale(), 3.0);
    assert_eq!(SOURCE_READS.with(std::cell::Cell::get), 0);
    set_motion_duration_scale(1.0);
}

/// A task's own scale doesn't touch the thread's, so it doesn't read the source: Compose's
/// `withContext(MotionDurationScale)` replaces the element whose getter would.
#[test]
fn a_task_s_own_scale_leaves_the_source_unread() {
    SOURCE_READS.with(|reads| reads.set(0));
    set_motion_duration_scale_source(source);
    let seen = std::rc::Rc::new(std::cell::Cell::new(0.0));
    let told = seen.clone();
    compose_runtime::platform::launch(with_motion_duration_scale(2.0, async move {
        told.set(motion_duration_scale());
    }));
    compose_runtime::platform::run_ready();
    assert_eq!(seen.get(), 2.0);
    assert_eq!(SOURCE_READS.with(std::cell::Cell::get), 0);
    set_motion_duration_scale(1.0);
}
