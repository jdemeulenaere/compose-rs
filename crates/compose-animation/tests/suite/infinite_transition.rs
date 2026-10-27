//! `remember_infinite_transition` against Compose's `InfiniteTransitionTest`: its values, read
//! each frame by a task, are the repeated animations' at the frame's play time.

#![allow(non_snake_case)]

use std::cell::RefCell;

use compose_animation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Color;

const MILLIS: i64 = 1_000_000;

thread_local! {
    /// The frames' play times whose values weren't the expected ones, with what was read.
    static MISMATCHES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CHECKED: RefCell<usize> = const { RefCell::new(0) };
}

fn spec() -> KeyframesSpec {
    keyframes(|frames| {
        frames.duration_millis = 1000;
        frames.at(0, 0.0f32);
        frames.at(400, 200.0);
        frames.at(1000, 1000.0);
    })
}

#[composable]
fn Checked(run_animation: MutableState<bool>) {
    let transition = remember_infinite_transition!();
    if run_animation.get() {
        let float =
            transition.animate_float(0.0, 0.0, infinite_repeatable(spec(), RepeatMode::Reverse));
        let color = transition.animate_color(
            Color::RED,
            Color::GREEN,
            infinite_repeatable(tween(1000), RepeatMode::Restart),
        );
        LaunchedEffect((), async move {
            let color_animation = TargetBasedAnimation::new(tween(1000), Color::RED, Color::GREEN);
            // From 0 to 0 over 1000 ms.
            let keyframes_animation = TargetBasedAnimation::new(spec(), 0.0f32, 0.0);
            let start = with_frame_nanos(|time| time).await as i64;
            let mut play_time = 0;
            // Compose's loop compares the nanoseconds with 2100, checking one frame; this one
            // checks 2.1 s of them.
            while play_time < 2100 * MILLIS {
                play_time = with_frame_nanos(|time| time).await as i64 - start;
                let mut iteration = play_time % (2000 * MILLIS);
                if iteration > 1000 * MILLIS {
                    iteration = 2000 * MILLIS - iteration;
                }
                let expected_float = keyframes_animation.value_from_nanos(iteration);
                let expected_color = color_animation.value_from_nanos(play_time % (1000 * MILLIS));
                let (read_float, read_color) = (float.get(), color.get());
                if (expected_float - read_float).abs() > 0.01 || expected_color != read_color {
                    MISMATCHES.with(|mismatches| {
                        mismatches.borrow_mut().push(format!(
                            "at {play_time} ns: {read_float} and {read_color:?}, Compose's \
                             {expected_float} and {expected_color:?}"
                        ))
                    });
                }
                CHECKED.with(|checked| *checked.borrow_mut() += 1);
            }
            run_animation.set(false);
        });
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/InfiniteTransitionTest.kt, transitionTest
#[test]
fn an_infinite_transitions_values_repeat_their_animations() {
    let run_animation = mutable_state_of(true);
    let handle = run_animation.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Checked(handle));
    let mut frames = 0;
    while run_animation.get() {
        ui.advance_time_by_frame();
        frames += 1;
        assert!(frames < 1000, "the check never ended");
    }
    // The check ran through each frame of its 2.1 s.
    assert!(CHECKED.with(|checked| *checked.borrow()) >= 2100 / 16);
    MISMATCHES.with(|mismatches| {
        let mismatches = mismatches.borrow();
        assert!(mismatches.is_empty(), "{} frames off: {:?}", mismatches.len(), &mismatches[..]);
    });
}

// Review H1: a spec built inline (a custom easing, equal only to itself) is new at every
// composition; as Compose's `animateValue`, only a change of the initial or target value
// updates the animation, so it doesn't start again each frame.
#[test]
fn a_spec_built_inline_does_not_restart_the_animation() {
    let value = std::rc::Rc::new(std::cell::Cell::new(-1.0f32));
    let seen = value.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_infinite_transition!();
        let spec = infinite_repeatable(
            tween(1000).easing(Easing::custom(|fraction| fraction)),
            RepeatMode::Restart,
        );
        let animated = transition.animate_float(0.0, 1000.0, spec);
        seen.set(animated.get());
    });
    for _ in 0..20 {
        ui.advance_time_by_frame();
    }
    assert!(value.get() > 100.0, "{}", value.get());
}
