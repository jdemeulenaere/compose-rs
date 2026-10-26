//! `InterruptionAwareAnimationSpec` against Compose's: its two specs, and `Animatable`,
//! `animate*AsState` and transitions taking the interruption spec only when they interrupt.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;

fn linear(duration: u32) -> AnimationSpec {
    tween(duration).easing(LINEAR_EASING).into()
}

fn spec() -> InterruptionAwareAnimationSpec {
    InterruptionAwareAnimationSpec::new(linear(200), linear(100))
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testInterruptionAwareAnimationSpec
#[test]
fn an_interruption_aware_spec_holds_its_two_specs() {
    let forward = AnimationSpec::from(tween(300));
    let interruption = AnimationSpec::from(spring_with(1.0, Spring::STIFFNESS_LOW));
    let combined = InterruptionAwareAnimationSpec::new(forward, interruption);
    assert_eq!(combined.animation_spec(), forward);
    assert_eq!(combined.interruption_spec(), interruption);
    assert_eq!(InterruptionAwareAnimationSpec::new(forward, interruption), combined);
    let alone = InterruptionAwareAnimationSpec::of(forward);
    assert_eq!((alone.animation_spec(), alone.interruption_spec()), (forward, forward));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/AnimatableTest.kt, animateTo_interruptionAwareAnimationSpec
#[test]
fn an_animatable_takes_the_interruption_spec_when_interrupting() {
    let animatable = Animatable::new(0.0f32);
    let mut ui = ComposeTestRule::with_manual_clock(|| {});
    let scope = CoroutineScope::new();
    let run = |target: f32| {
        let animatable = animatable.clone();
        scope.launch(async move {
            animatable.animate_to(target, spec()).await;
        });
    };
    // From rest: the 200 ms tween; frames at 0, 16, 32 and 48 ms.
    run(100.0);
    for _ in 0..4 {
        ui.advance_time_by_frame();
    }
    assert!((animatable.value() - 24.0).abs() < 0.1, "{}", animatable.value());
    assert!(animatable.is_running());
    // Interrupting: the 100 ms tween from 24, 16 ms in at the next frame.
    run(0.0);
    ui.advance_time_by_frame();
    assert!((animatable.value() - 24.0 * 0.84).abs() < 0.1, "{}", animatable.value());
    ui.advance_time_by(Duration::from_millis(96));
    assert_eq!(animatable.value(), 0.0);
    assert!(!animatable.is_running());
    // From rest again: the 200 ms tween.
    run(100.0);
    for _ in 0..7 {
        ui.advance_time_by_frame();
    }
    assert!((animatable.value() - 48.0).abs() < 0.1, "{}", animatable.value());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SingleValueAnimationTest.kt, interruptionAwareAnimationSpec_usesInterruptionSpecOnInterruption
#[test]
fn a_value_as_state_takes_the_interruption_spec_when_interrupting() {
    let target = mutable_state_of(false);
    let handle = target.handle();
    let current = Rc::new(Cell::new(0.0f32));
    let seen = current.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let value = animate_float_as_state!(
            if handle.get() { 100.0 } else { 0.0 },
            animation_spec = spec(),
            label = "InterruptibleAsState"
        );
        seen.set(value.get());
    });
    check_interruptions(&mut ui, &target, &current);
}

/// The frames of Compose's interruption-aware tests: from rest along the 200 ms tween, the
/// interruption along the 100 ms one, and from rest along the 200 ms one again.
fn check_interruptions(
    ui: &mut ComposeTestRule,
    target: &OwnedState<bool>,
    current: &Rc<Cell<f32>>,
) {
    let near = |expected: f32| (current.get() - expected).abs() < 0.1;
    target.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // 64 ms into the 200 ms tween: 32 (the 100 ms one would be at 64).
    ui.advance_time_by(Duration::from_millis(64));
    assert!(near(32.0), "{}", current.get());
    target.set(false);
    ui.advance_time_by_frame();
    let at_interruption = current.get();
    assert!(near(40.0), "{at_interruption}");
    // 64 ms into the 100 ms tween: 36 % of the way left.
    ui.advance_time_by(Duration::from_millis(64));
    assert!(near(at_interruption * 0.36), "{}", current.get());
    // Past 100 ms: over (the 200 ms tween would still run).
    ui.advance_time_by(Duration::from_millis(48));
    assert_eq!(current.get(), 0.0);
    target.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(64));
    assert!(near(32.0), "{}", current.get());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, interruptionAwareAnimationSpec_usesInterruptionSpecOnInterruption
#[test]
fn a_transitions_value_takes_the_interruption_spec_when_interrupted() {
    let target = mutable_state_of(false);
    let handle = target.handle();
    let current = Rc::new(Cell::new(0.0f32));
    let seen = current.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "FactoryInterruptibleTest");
        let value = transition.animate_float(|_| spec().into(), |on| if *on { 100.0 } else { 0.0 });
        seen.set(value.get());
    });
    check_interruptions(&mut ui, &target, &current);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, interruptionAwareAnimationSpec_preservedOnInterruption
// Compose's spec is one of the test's own (a spring recording what it's given); here keyframes,
// which a transition would otherwise replace by its interruption spring when interrupted.
#[test]
fn an_interruption_aware_spec_is_kept_when_a_transition_is_interrupted() {
    let target = mutable_state_of(false);
    let handle = target.handle();
    let current = Rc::new(Cell::new(0.0f32));
    let seen = current.clone();
    let keyframes = keyframes::<f32>(|frames| {
        frames.duration_millis = 200;
        frames.at(100, 50.0);
    });
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "InterruptibleTest");
        let value = transition.animate_float(
            move |_| InterruptionAwareAnimationSpec::of(keyframes).into(),
            |on| if *on { 100.0 } else { 0.0 },
        );
        seen.set(value.get());
    });
    target.set(true);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(64));
    target.set(false);
    ui.advance_time_by_frame();
    let at_interruption = current.get();
    assert!(at_interruption > 0.0 && at_interruption < 100.0, "{at_interruption}");
    // The keyframes from where it was to 0: 50 at 100 ms, whatever the start.
    let expected = TargetBasedAnimation::new(keyframes, at_interruption, 0.0);
    for frame in 1..=6 {
        ui.advance_time_by_frame();
        let play = frame * 16;
        assert_eq!(current.get(), expected.value_from_millis(play), "at {play}");
    }
}
