//! `Transition`s against Compose's: values per segment on the transition's play time, values
//! added mid-way, `MutableTransitionState`, child transitions on their parent's clock,
//! recompositions, labels.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Color;

/// What a child transition test sees each composition: the parent's state, its play time,
/// whether it and the child run, and their values.
type ChildSeen = Rc<RefCell<Vec<(i32, i64, bool, bool, f32, f32)>>>;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AnimStates {
    From,
    To,
}

use AnimStates::{From, To};

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, transitionTest
#[test]
fn values_follow_their_segments_specs_on_the_transitions_play_time() {
    let target = mutable_state_of(From);
    let handle = target.handle();
    let float_to = TargetBasedAnimation::new(
        spring_with(Spring::DAMPING_RATIO_HIGH_BOUNCY, Spring::STIFFNESS_MEDIUM),
        0.0f32,
        1.0,
    );
    let float_from = TargetBasedAnimation::new(
        spring_with(Spring::DAMPING_RATIO_LOW_BOUNCY, Spring::STIFFNESS_LOW),
        1.0f32,
        0.0,
    );
    let color_to = TargetBasedAnimation::new(tween(1000), Color::RED, Color::GREEN);
    let color_from = TargetBasedAnimation::new(tween(1000), Color::GREEN, Color::RED);
    let keyframes_to = keyframes::<f32>(|frames| {
        frames.duration_millis = 1000;
        frames.at(0, 0.0);
        frames.at(400, 200.0);
        frames.at(1000, 1000.0);
    });
    let keyframes_from = keyframes::<f32>(|frames| {
        frames.duration_millis = 800;
        frames.at(0, 0.0);
        frames.at(400, -500.0);
        frames.at(800, -1000.0);
    });
    let keyframes_to_animation = TargetBasedAnimation::new(keyframes_to.clone(), 0.0f32, 0.0);
    let keyframes_from_animation = TargetBasedAnimation::new(keyframes_from.clone(), 0.0f32, 0.0);
    let checked = Rc::new(Cell::new(0));
    let values = Rc::new(Cell::new((0.0, Color::BLACK)));
    let (count, seen) = (checked.clone(), values.clone());
    let mut ui = compose_test(move || {
        let transition = update_transition(handle.get(), None);
        let float = transition.animate_float(
            |segment| {
                if segment.is_transitioning_to(&From, &To) {
                    spring_with(Spring::DAMPING_RATIO_HIGH_BOUNCY, Spring::STIFFNESS_MEDIUM).into()
                } else {
                    spring_with(Spring::DAMPING_RATIO_LOW_BOUNCY, Spring::STIFFNESS_LOW).into()
                }
            },
            |state| if *state == From { 0.0 } else { 1.0 },
        );
        let color = transition.animate_color(
            |_| tween(1000).into(),
            |state| if *state == From { Color::RED } else { Color::GREEN },
        );
        // The same value for both states: only the keyframes move it.
        let with_keyframes = transition.animate_float(
            |segment| {
                if segment.is_transitioning_to(&From, &To) {
                    keyframes_to.clone().into()
                } else {
                    keyframes_from.clone().into()
                }
            },
            |_| 0.0,
        );
        let (float, color, with_keyframes) = (float.get(), color.get(), with_keyframes.get());
        seen.set((float, color));
        if transition.is_running() {
            let play = transition.play_time_nanos();
            let segment = transition.segment();
            if transition.target_state() == To {
                assert!((float_to.value_from_nanos(play) - float).abs() < 1e-5, "{play}");
                assert_eq!(color_to.value_from_nanos(play), color, "{play}");
                let keyframe = keyframes_to_animation.value_from_nanos(play);
                assert!((keyframe - with_keyframes).abs() < 1e-5, "{play}");
                assert_eq!((segment.initial_state, segment.target_state), (From, To));
            } else {
                assert!((float_from.value_from_nanos(play) - float).abs() < 1e-5, "{play}");
                assert_eq!(color_from.value_from_nanos(play), color, "{play}");
                let keyframe = keyframes_from_animation.value_from_nanos(play);
                assert!((keyframe - with_keyframes).abs() < 1e-5, "{play}");
                assert_eq!((segment.initial_state, segment.target_state), (To, From));
            }
            count.set(count.get() + 1);
        }
    });
    assert_eq!(values.get(), (0.0, Color::RED));
    target.set(To);
    ui.wait_for_idle();
    assert_eq!(values.get(), (1.0, Color::GREEN));
    target.set(From);
    ui.wait_for_idle();
    assert_eq!(values.get(), (0.0, Color::RED));
    assert!(checked.get() > 100, "{} frames checked", checked.get());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, startPulsingNextFrameTest
#[test]
fn a_transition_starts_at_the_frame_after_its_change() {
    let target = mutable_state_of(From);
    let handle = target.handle();
    let samples = Rc::new(RefCell::new(Vec::new()));
    let seen = samples.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), None);
        let actual = transition.animate_float(
            |_| tween(200).into(),
            |state| if *state == From { 0.0 } else { 1000.0 },
        );
        seen.borrow_mut().push((transition.play_time_nanos(), actual.get()));
    });
    target.set(To);
    // The change composes at the next frame.
    ui.advance_time_by_frame();
    let expected = TargetBasedAnimation::new(tween(200), 0.0f32, 1000.0);
    // The first frame after the change is play time 0, then one every 16 ms, up to 208 ms,
    // where it ends (its play time back at 0). The frame at 0 changes nothing, so composes
    // nothing: the change's composition (0, 0) stands for it.
    for frame in 0..=13 {
        ui.advance_time_by_frame();
        let (play, value) = *samples.borrow().last().expect("composed");
        if frame == 13 {
            assert_eq!((play, value), (0, 1000.0));
        } else {
            assert_eq!(play, frame * 16_000_000, "frame {frame}");
            assert_eq!(value, expected.value_from_nanos(play), "frame {frame}");
        }
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, addNewAnimationInFlightTest
#[test]
fn a_value_added_mid_way_animates_from_its_value_for_the_current_state() {
    let target = mutable_state_of(From);
    let handle = target.handle();
    let checked = Rc::new(Cell::new(0i64));
    let latest = checked.clone();
    let mut ui = compose_test(move || {
        let transition = update_transition(handle.get(), None);
        transition.animate_float(
            |_| tween(1000).into(),
            |state| if *state == From { -100.0 } else { 0.0 },
        );
        if transition.play_time_nanos() > 0 {
            let start = remember(|| transition.play_time_nanos());
            let later = transition.animate_float(
                |_| tween(800).into(),
                |state| if *state == From { 0.0 } else { 1000.0 },
            );
            let expected = TargetBasedAnimation::new(tween(800), 0.0f32, 1000.0);
            let play = transition.play_time_nanos() - start;
            assert_eq!(later.get(), expected.value_from_nanos(play), "at {play}");
            latest.set(play);
        }
    });
    target.set(To);
    ui.wait_for_idle();
    assert!(checked.get() > 800_000_000, "{}", checked.get());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, initialStateTest
#[test]
fn a_transition_state_set_before_composing_animates_at_once() {
    let state = MutableTransitionState::new(From);
    state.set_target_state(To);
    let samples = Rc::new(RefCell::new(Vec::new()));
    let seen = samples.clone();
    let given = state.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, None);
        let value = transition.animate_float(
            |_| tween(800).into(),
            |state| if *state == From { 0.0 } else { 1000.0 },
        );
        seen.borrow_mut().push((transition.play_time_nanos(), value.get()));
    });
    let expected = TargetBasedAnimation::new(tween(800), 0.0f32, 1000.0);
    for _ in 0..60 {
        ui.advance_time_by_frame();
        let (play, value) = *samples.borrow().last().expect("composed");
        if play > 0 {
            assert_eq!(value, expected.value_from_nanos(play), "at {play}");
        }
    }
    assert_eq!(samples.borrow().last().unwrap().1, 1000.0);
    assert_eq!(state.current_state(), To);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, recreatingMutableStatesAmidTransition
#[test]
fn a_transition_state_recreated_mid_way_restarts_its_animation() {
    let state = mutable_state_of(MutableTransitionState::new(From));
    state.get().set_target_state(To);
    let handle = state.handle();
    let samples = Rc::new(RefCell::new(Vec::new()));
    let seen = samples.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&handle.get(), None);
        let value = transition.animate_float(
            |_| tween(800).into(),
            |state| if *state == From { 0.0 } else { 1000.0 },
        );
        seen.borrow_mut().push((transition.play_time_nanos(), value.get()));
    });
    ui.advance_time_by(Duration::from_millis(100));
    assert!(samples.borrow().last().unwrap().1 > 0.0);
    // A new state from the start: a new transition, from 0 again.
    let recreated = MutableTransitionState::new(From);
    recreated.set_target_state(To);
    state.set(recreated);
    ui.advance_time_by_frame();
    assert_eq!(samples.borrow().last().unwrap().1, 0.0);
    let expected = TargetBasedAnimation::new(tween(800), 0.0f32, 1000.0);
    for _ in 0..60 {
        ui.advance_time_by_frame();
        let (play, value) = *samples.borrow().last().expect("composed");
        // Once over, it's at the target and its play time back at 0.
        if !(play == 0 && value == 1000.0) {
            assert_eq!(value, expected.value_from_nanos(play), "at {play}");
        }
    }
    assert_eq!(samples.borrow().last().unwrap().1, 1000.0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, testMutableTransitionStateIsIdle
#[test]
fn a_transition_state_is_idle_only_at_rest_at_its_target() {
    let state = MutableTransitionState::new(false);
    let given = state.clone();
    let current = Rc::new(Cell::new(false));
    let seen = current.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, None);
        transition.animate_float(|_| spring().into(), |on| if *on { 1.0 } else { 0.0 });
        seen.set(transition.current_state());
    });
    for target in [true, false] {
        assert!(state.is_idle());
        state.set_target_state(target);
        assert!(!state.is_idle());
        let mut frames = 0;
        while current.get() != target {
            assert!(!state.is_idle(), "idle before the transition got there");
            ui.advance_time_by_frame();
            frames += 1;
            assert!(frames < 200);
        }
        assert!(state.is_idle());
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, testCreateChildTransition
#[test]
fn a_child_transition_runs_only_when_its_state_changes() {
    let int_state = mutable_state_of(1);
    let handle = int_state.handle();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let report = seen.clone();
    let mut ui = compose_test(move || {
        let transition = update_transition(handle.get(), None);
        let parent = transition.animate_float(
            |_| tween(100).into(),
            |state| match state {
                0 => 0.0,
                1 => 1.0,
                _ => 2.0,
            },
        );
        let child_transition = transition.create_child_transition(None, |state| *state == 1);
        let child =
            child_transition.animate_float(|_| tween(500).into(), |on| if *on { 1.0 } else { 0.0 });
        report.borrow_mut().push((
            handle.get(),
            transition.play_time_nanos(),
            transition.is_running(),
            child_transition.is_running(),
            parent.get(),
            child.get(),
        ));
    });
    let last = |seen: &ChildSeen| *seen.borrow().last().unwrap();
    assert_eq!((last(&seen).4, last(&seen).5), (1.0, 1.0));
    int_state.set(0);
    ui.wait_for_idle();
    assert_eq!((last(&seen).4, last(&seen).5), (0.0, 0.0));
    for &(state, play, running, child_running, _, _) in seen.borrow().iter() {
        if state == 0 && play > 0 && play <= 500_000_000 {
            assert!(running && child_running, "1 -> 0 at {play}");
        }
    }
    seen.borrow_mut().clear();
    int_state.set(2);
    ui.wait_for_idle();
    assert_eq!((last(&seen).4, last(&seen).5), (2.0, 0.0));
    for &(_, play, running, child_running, _, _) in seen.borrow().iter() {
        // 0 -> 2: the child's state (false) doesn't change.
        assert!(!child_running, "0 -> 2 at {play}");
        if play > 120_000_000 {
            assert!(!running, "0 -> 2 at {play}");
        }
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, addAnimationToCompletedChildTransition
#[test]
fn a_value_added_to_a_finished_child_plays_on_the_parents_clock() {
    let state = MutableTransitionState::new(false);
    let given = state.clone();
    let values = Rc::new(Cell::new((0.0f32, 0.0f32, 0.0f32)));
    let seen = values.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let parent = remember_transition(&given, None);
        let first = parent.animate_float(
            |_| tween(1600).easing(LINEAR_EASING).into(),
            |on| if *on { 1000.0 } else { 0.0 },
        );
        let child = parent.create_child_transition(None, |on| *on);
        let second = child.animate_float(
            |_| tween(160).easing(LINEAR_EASING).into(),
            |on| if *on { 1000.0 } else { 0.0 },
        );
        let third = if !parent.target_state() {
            child
                .animate_float(
                    |_| tween(160).easing(LINEAR_EASING).into(),
                    |on| if *on { 0.0 } else { 1000.0 },
                )
                .get()
        } else {
            0.0
        };
        seen.set((first.get(), second.get(), third));
    });
    let near = |actual: f32, expected: f32| (actual - expected).abs() <= 0.1;
    state.set_target_state(true);
    ui.advance_time_by_frame(); // The change composes.
    assert_eq!(values.get(), (0.0, 0.0, 0.0));
    ui.advance_time_by_frame(); // Play time 0.
    assert_eq!(values.get(), (0.0, 0.0, 0.0));
    ui.advance_time_by_frame(); // 16 ms.
    let (first, second, third) = values.get();
    assert!(near(first, 10.0) && near(second, 100.0) && third == 0.0, "{:?}", values.get());
    ui.advance_time_by(Duration::from_millis(160));
    let (first, second, third) = values.get();
    assert!(near(first, 110.0) && second == 1000.0 && third == 0.0, "{:?}", values.get());
    state.set_target_state(false);
    ui.advance_time_by_frame(); // The change composes: the first value a frame further.
    let (first, second, third) = values.get();
    assert!(near(first, 120.0) && second == 1000.0 && third == 0.0, "{:?}", values.get());
    ui.advance_time_by_frame();
    let (first2, second2, third2) = values.get();
    assert!(first2 < first && second2 < second && near(third2, 100.0), "{:?}", values.get());
    ui.advance_time_by_frame();
    let (first3, second3, third3) = values.get();
    assert!(first3 < first2 && second3 < second2 && near(third3, 200.0), "{:?}", values.get());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, snapshotTotalDurationNanos
// The total duration once a change gives the values their animations (Compose reads it with a
// `snapshotFlow`, under an `AnimatedContent` of the transition).
#[test]
fn the_total_duration_is_the_longest_values() {
    let target = mutable_state_of(false);
    let handle = target.handle();
    let durations = Rc::new(Cell::new(0i64));
    let seen = durations.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), "");
        transition.animate_float(|_| tween(300).into(), |on| if *on { 1.0 } else { 0.0 });
        transition
            .animate_float(|_| tween(500).delay(100).into(), |on| if *on { 1.0 } else { 0.0 });
        seen.set(transition.total_duration_nanos());
    });
    assert_eq!(durations.get(), 0);
    target.set(true);
    ui.advance_time_by_frame();
    assert_eq!(durations.get(), 600_000_000);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, testRecompositionCountWhenPassingTransition
#[test]
fn passing_a_transition_composes_once_for_its_old_value_and_twice_for_a_change() {
    let show = mutable_state_of(false);
    let state = mutable_state_of(MutableTransitionState::new(false));
    let (show_handle, state_handle) = (show.handle(), state.handle());
    let counts = Rc::new(Cell::new((0, 0)));
    let count = counts.clone();
    let mut ui = compose_test(move || {
        let transition_state = state_handle.get();
        let transition = remember_transition(&transition_state, None);
        let shown = remember(move || {
            derived_state_of(move || show_handle.get() || state_handle.get().current_state())
        })
        .get();
        let (hidden_count, shown_count) = count.get();
        count.set(if shown {
            (hidden_count, shown_count + 1)
        } else {
            (hidden_count + 1, shown_count)
        });
        if shown {
            let alpha = transition
                .animate_float(|_| tween(2000).into(), |shown| if *shown { 1.0 } else { 0.0 });
            Box!(modifier = Modifier.graphics_layer(move |layer| layer.alpha = alpha.get()));
        }
    });
    show.set(true);
    state.get().set_target_state(true);
    ui.wait_for_idle();
    // Twice with it shown: the transition's target, written in the scope that reads it, is heard at
    // the apply, as Compose's (D793).
    assert_eq!(counts.get(), (1, 2));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, animateFloatCallerRecompositionCount
#[test]
fn a_transitions_caller_composes_once() {
    let state = MutableTransitionState::new(false);
    let given = state.clone();
    let count = Rc::new(Cell::new(0));
    let counter = count.clone();
    #[composable]
    fn Animated(state: MutableTransitionState<bool>, count: Rc<Cell<i32>>) {
        count.set(count.get() + 1);
        let transition = remember_transition(&state, None);
        transition.animate_float(|_| spring().into(), |on| if *on { 1.0 } else { 0.0 });
    }
    let mut ui = compose_test(move || {
        if given.target_state() {
            Animated(given.clone(), counter.clone());
        }
    });
    state.set_target_state(true);
    ui.wait_for_idle();
    assert_eq!(count.get(), 1);
    assert!(state.current_state());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, childTransitionStartsUninterrupted_usingTransitionState
#[test]
fn a_child_transition_starts_uninterrupted_after_changes_that_didnt_move_it() {
    let state = MutableTransitionState::new(0);
    let given = state.clone();
    let yellow = Color::from_argb(0xFFFFFF00);
    let color = Rc::new(Cell::new(Color::BLACK));
    let seen = color.clone();
    let mut ui = compose_test(move || {
        let transition = remember_transition(&given, None);
        let child = transition.create_child_transition(None, |state| *state > 1);
        // A keyframe at the start makes the spec's start (yellow) stand apart from an
        // interruption's spring.
        let animated = child.animate_color(
            move |_| {
                keyframes::<Color>(|frames| {
                    frames.at_fraction(0.0, yellow);
                })
                .into()
            },
            |on| if *on { Color::RED } else { Color::BLUE },
        );
        seen.set(animated.get());
    });
    assert_eq!((color.get(), state.current_state()), (Color::BLUE, 0));
    ui.set_auto_advance(false);
    // A change the child doesn't see: no animation.
    state.set_target_state(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((color.get(), state.current_state()), (Color::BLUE, 1));
    // The child's first change: its spec from the start (yellow), not an interruption.
    state.set_target_state(2);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((color.get(), state.current_state()), (yellow, 1));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!((color.get(), state.current_state()), (Color::RED, 2));
    ui.set_auto_advance(false);
    state.set_target_state(3);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((color.get(), state.current_state()), (Color::RED, 3));
    state.set_target_state(1);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((color.get(), state.current_state()), (yellow, 3));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, recreatedTransition_animatesAsExpected
#[test]
fn a_recreated_transition_state_starts_a_fresh_segment() {
    let duration = 10 * 16;
    let base = Rc::new(Cell::new(0.0f32));
    let state = mutable_state_of(MutableTransitionState::new(From));
    let handle = state.handle();
    let text = Rc::new(Cell::new(f32::NAN));
    let (seen, base_read) = (text.clone(), base.clone());
    let mut ui = compose_test(move || {
        let transition = remember_transition(&handle.get(), None);
        let base = base_read.get();
        let value = transition.animate_float(
            move |segment| {
                if segment.is_transitioning_to(&From, &To) {
                    tween(duration).easing(LINEAR_EASING).into()
                } else {
                    tween(duration * 2).easing(LINEAR_EASING).into()
                }
            },
            move |state| if *state == From { base * 2.0 } else { base * 2.0 + 1.0 },
        );
        seen.set(value.get());
    });
    assert_eq!(text.get(), 0.0);
    state.set(MutableTransitionState::new(From));
    ui.set_auto_advance(false);
    state.get().set_target_state(To);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!(text.get(), 0.0);
    ui.advance_time_by(Duration::from_millis(u64::from(duration) / 2));
    assert_eq!(text.get(), 0.5);
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!((state.get().current_state(), state.get().target_state()), (To, To));
    ui.set_auto_advance(false);
    base.set(1.0);
    state.set(MutableTransitionState::new(From));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((state.get().current_state(), state.get().target_state()), (From, From));
    assert_eq!(text.get(), 2.0);
    state.get().set_target_state(To);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(u64::from(duration) / 2));
    assert_eq!((state.get().current_state(), state.get().target_state()), (From, To));
    assert_eq!(text.get(), 2.5);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, testLazyChildTransitionLabel
#[test]
fn child_transitions_labels_follow_their_parents() {
    let labels = Rc::new(RefCell::new(Vec::new()));
    let seen = labels.clone();
    let _ui = compose_test(move || {
        let unlabeled = update_transition(1, None);
        let default_child = unlabeled.create_child_transition(None, |state| *state == 1);
        let custom_child = unlabeled.create_child_transition("CustomChild", |state| *state == 1);
        let labeled = update_transition(1, "Parent");
        let labeled_default = labeled.create_child_transition(None, |state| *state == 1);
        let labeled_custom = labeled.create_child_transition("CustomChild", |state| *state == 1);
        let grandchild = labeled_custom.create_child_transition("Grandchild", |on| *on);
        *seen.borrow_mut() = vec![
            unlabeled.label(),
            default_child.label(),
            custom_child.label(),
            labeled.label(),
            labeled_default.label(),
            labeled_custom.label(),
            grandchild.label(),
        ];
    });
    let expected = [
        None,
        Some("ChildTransition"),
        Some("CustomChild"),
        Some("Parent"),
        Some("Parent > ChildTransition"),
        Some("Parent > CustomChild"),
        Some("Parent > CustomChild > Grandchild"),
    ];
    let labels = labels.borrow();
    for (label, expected) in labels.iter().zip(expected) {
        assert_eq!(label.as_deref(), expected);
    }
}

/// A composition writing a state read only while measuring (a transition's target, D802): its
/// frame lays out with the write and asks for no other (D806), and the transition it changes
/// starts at play time 0 at the next frame.
#[test]
fn a_composition_writing_a_measured_state_asks_for_no_other_frame() {
    let trigger = mutable_state_of(0u32);
    let handle = trigger.handle();
    let measured_values = Rc::new(RefCell::new(Vec::new()));
    let seen = measured_values.clone();
    let play_times = Rc::new(RefCell::new(Vec::new()));
    let plays = play_times.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let measured = remember(|| mutable_state_of(0u32));
        // Written as it composes, read only while measuring.
        measured.set(handle.get());
        let seen = seen.clone();
        let modifier = remember(move || {
            Modifier.layout(move |scope, content, constraints| {
                seen.borrow_mut().push(measured.get());
                let placeable = scope.measure(content, constraints);
                let size = placeable.size();
                scope.place(&placeable, 0, 0);
                scope.layout(size.width, size.height)
            })
        });
        Box!(modifier = modifier);
        if handle.get() >= 2 {
            let transition = update_transition(handle.get(), None);
            let _ = transition.animate_float(|_| tween(100).into(), |state| *state as f32);
            plays.borrow_mut().push(transition.play_time_nanos());
        }
    });
    trigger.set(1);
    ui.advance_time_by_frame();
    assert_eq!(measured_values.borrow().last(), Some(&1), "measured with the write");
    assert!(!ui.host().has_pending_work(), "the write asks for no other frame");
    // A transition's target is read in the scope that writes it: its apply invalidates it, as
    // Compose's (D793), the transition's first frame then at play time 0, the next 16 ms on.
    trigger.set(2);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let composed = play_times.borrow().len();
    trigger.set(3);
    ui.advance_time_by_frame();
    assert_eq!(measured_values.borrow().last(), Some(&3));
    assert!(ui.host().has_pending_work(), "the scope read what it wrote");
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let plays = play_times.borrow();
    assert_eq!(&plays[composed..], &[0, 0, 16_000_000][..], "{plays:?}");
}

thread_local! {
    static LISTED: RefCell<Vec<(String, Option<f32>)>> = const { RefCell::new(Vec::new()) };
}

/// Tools see a transition's values and an infinite transition's with their labels: the ones
/// given, or Compose's defaults.
#[test]
fn tools_list_the_values_with_their_labels() {
    let target = mutable_state_of(From);
    let handle = target.handle();
    let _ui = ComposeTestRule::with_manual_clock(move || {
        let transition = update_transition(handle.get(), None);
        let _ = transition.animate_float_with(|_| tween(100).into(), "alpha", |_| 0.5);
        let _ = transition.animate_dp(|_| tween(100).into(), |_| compose_ui::Dp(1.0));
        let infinite = remember_infinite_transition!("pulse");
        let _ =
            infinite.animate_float(0.0, 1.0, infinite_repeatable(tween(100), RepeatMode::Reverse));
        let _ = infinite.animate_color_with(
            Color::RED,
            Color::BLUE,
            infinite_repeatable(tween(100), RepeatMode::Reverse),
            "glow",
        );
        let mut listed: Vec<(String, Option<f32>)> = transition
            .animations()
            .iter()
            .map(|animation| (animation.label(), animation.value::<f32>()))
            .collect();
        listed.push((infinite.label().to_string(), None));
        listed.extend(infinite.animations().iter().map(|animation| (animation.label(), None)));
        LISTED.with(|seen| *seen.borrow_mut() = listed);
    });
    // The lists fill as the effects run: a later pass sees them.
    let listed = LISTED.with(|seen| seen.borrow().clone());
    let labels: Vec<&str> = listed.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels, ["alpha", "DpAnimation", "pulse", "FloatAnimation", "glow"]);
    assert_eq!(listed[0].1, Some(0.5));
}
