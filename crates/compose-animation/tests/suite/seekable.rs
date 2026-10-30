//! `SeekableTransitionState` against Compose's `SeekableTransitionStateTest`: seeking to
//! fractions, animating to the target and on from where a seek left it, snapping, and changes
//! of target interrupting one under way.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Alignment, IntOffset, IntSize, Modifier, TransformOrigin, UnitExt};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
enum AnimStates {
    From,
    To,
    Other,
}

use AnimStates::{From, Other, To};

impl ComposeArg for AnimStates {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// A call on the state, as a test's content starts it.
type Call = fn(SeekableTransitionState<AnimStates>) -> Pinned;

type Pinned = std::pin::Pin<Box<dyn std::future::Future<Output = ()>>>;

/// A task's outcome, as the `Deferred` of Compose's tests' `async`.
struct Outcome {
    done: Rc<Cell<bool>>,
    job: Job,
}

impl Outcome {
    /// `None` while it runs, then whether it got to its end (`Some(false)`: cancelled, as
    /// `isCancelled`, by an interrupted call).
    fn get(&self) -> Option<bool> {
        if self.done.get() {
            Some(true)
        } else if self.job.is_active() {
            None
        } else {
            Some(false)
        }
    }
}

/// Runs `call` as a task, as Compose's tests `launch` or `async` one.
fn launch(call: impl std::future::Future<Output = ()> + 'static) -> Outcome {
    let done = Rc::new(Cell::new(false));
    let finished = done.clone();
    let scope = CoroutineScope::new();
    let job = scope.launch(async move {
        call.await;
        finished.set(true);
    });
    std::mem::forget(scope);
    Outcome { done, job }
}

/// Runs `call` to its end, the clock advancing, as `runBlocking` in Compose's `runOnIdle`.
fn block_on(ui: &mut ComposeTestRule, call: impl std::future::Future<Output = ()> + 'static) {
    let outcome = launch(call);
    for _ in 0..1_000 {
        ui.wait_for_idle();
        if outcome.get().is_some() {
            assert_eq!(outcome.get(), Some(true), "the call was interrupted");
            return;
        }
        ui.advance_time_by_frame();
    }
    panic!("the call never ended");
}

fn value_of(state: AnimStates) -> i32 {
    match state {
        From => 0,
        To => 1000,
        Other => 2000,
    }
}

/// A rule whose content's transition, remembered with `state`, animates an int linearly from
/// 0 (`From`) to 1000 (`To`) or 2000 (`Other`), seeking to `To` first.
fn seekable_int(
    state: &SeekableTransitionState<AnimStates>,
    manual: bool,
) -> (ComposeTestRule, Rc<Cell<i32>>) {
    let value = Rc::new(Cell::new(-1));
    let (seen, given) = (value.clone(), state.clone());
    let content = move || {
        let seeking = given.clone();
        LaunchedEffect(given.clone(), async move {
            seeking.seek_to_with(0.0, To).await;
        });
        let transition = remember_transition(&given, "Test");
        let animated = transition.animate_int_with(
            |_| tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into(),
            "Value",
            |state| value_of(*state),
        );
        seen.set(animated.get());
    };
    let ui =
        if manual { ComposeTestRule::with_manual_clock(content) } else { compose_test(content) };
    (ui, value)
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, seekFraction
#[test]
fn seeking_sets_the_values_at_the_fraction() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, value) = seekable_int(&state, false);
    ui.wait_for_idle();
    assert_eq!(value.get(), 0);
    for (fraction, expected) in [(0.5, 500), (1.0, 1000), (0.5, 500), (0.0, 0)] {
        let seeking = state.clone();
        block_on(&mut ui, async move { seeking.seek_to(fraction).await });
        assert_eq!(state.fraction(), fraction);
        ui.wait_for_idle();
        assert_eq!(value.get(), expected, "at {fraction}");
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToTarget
#[test]
fn animating_goes_on_from_where_a_seek_left_it() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, value) = seekable_int(&state, true);
    // The composition after `seek_to`.
    ui.advance_time_by_frame();
    let animating = state.clone();
    let first = launch(async move { animating.animate_to(To).await });
    // A frame for the start time, then one moves it.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let progress = state.fraction();
    assert!(progress > 0.0, "{progress}");
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(state.fraction() > progress);
    // Interrupted by a seek.
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to(0.5).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    assert_eq!(first.get(), Some(false), "the animation was interrupted");
    assert_eq!(state.fraction(), 0.5);
    assert_eq!(value.get(), 500);
    // On from there.
    let animating = state.clone();
    let second = launch(async move { animating.animate_to(To).await });
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(state.fraction() > 0.5 && state.fraction() < 1.0, "{}", state.fraction());
    ui.advance_time_by(std::time::Duration::from_millis(5000));
    ui.wait_for_idle();
    assert_eq!(second.get(), Some(true));
    assert_eq!(state.fraction(), 0.0);
    assert_eq!(value.get(), 1000);
    assert_eq!(state.current_state(), To);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, snapToStopsAllAnimations
#[test]
fn snapping_stops_the_animations() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, value) = seekable_int(&state, true);
    ui.advance_time_by_frame();
    let animating = state.clone();
    let animation = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(state.fraction() > 0.0);
    let snapping = state.clone();
    let snap = launch(async move { snapping.snap_to(Other).await });
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    ui.wait_for_idle();
    assert_eq!(snap.get(), Some(true));
    assert_eq!(animation.get(), Some(false));
    assert_eq!(state.current_state(), Other);
    assert_eq!(state.target_state(), Other);
    assert_eq!(value.get(), 2000);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, updatedTransition
#[test]
fn the_fraction_is_of_the_total_duration_children_included() {
    use compose_foundation::layout::*;
    use compose_ui::UnitExt;
    let state = SeekableTransitionState::new(From);
    let value = Rc::new(Cell::new(-1));
    let duration = Rc::new(Cell::new(-1i64));
    let (seen, seen_duration, given) = (value.clone(), duration.clone(), state.clone());
    let mut ui = compose_test(move || {
        let seeking = given.clone();
        LaunchedEffect(given.clone(), async move {
            seeking.seek_to_with(0.0, To).await;
        });
        let transition = remember_transition(&given, "Test");
        let animated = transition.animate_int_with(
            |_| tween(200).easing(LINEAR_EASING).into(),
            "Value",
            |state| if *state == From { 0 } else { 1000 },
        );
        seen.set(animated.get());
        transition.AnimatedContent(
            Modifier,
            |_| {
                fade_in_with(tween(1000).easing(LINEAR_EASING), 0.0)
                    .together_with(fade_out_with(tween(1000).easing(LINEAR_EASING), 0.0))
            },
            Alignment::TOP_START,
            ContentKey::state(),
            |_, state: AnimStates| {
                if state == To {
                    Box!(modifier = Modifier.size(100.dp()));
                }
            },
        );
        seen_duration.set(transition.total_duration_nanos());
    });
    ui.wait_for_idle();
    assert_eq!(duration.get(), 1_000_000_000);
    assert_eq!(state.fraction(), 0.0);
    for (fraction, expected) in [(0.5, 1000), (1.0, 1000), (0.1, 500)] {
        let seeking = state.clone();
        block_on(&mut ui, async move { seeking.seek_to(fraction).await });
        ui.wait_for_idle();
        assert_eq!(value.get(), expected, "at {fraction}");
        assert_eq!(state.fraction(), fraction);
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, repeatAnimate
#[test]
fn a_call_interrupts_the_one_running() {
    let state = SeekableTransitionState::new(From);
    let given = state.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        let _ = transition.animate_int_with(
            |_| tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into(),
            "Value",
            |state| if *state == From { 0 } else { 1000 },
        );
    });
    let animating = state.clone();
    let first = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    // The same animation again: it interrupts the first.
    let animating = state.clone();
    let second = launch(async move { animating.animate_to(To).await });
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    assert_eq!(first.get(), Some(false));
    assert_ne!(second.get(), Some(false));
    // A seek interrupts it.
    let seeking = state.clone();
    let third = launch(async move { seeking.seek_to(0.25).await });
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    assert_eq!(second.get(), Some(false));
    assert_eq!(third.get(), Some(true));
    // And it animates again.
    let animating = state.clone();
    let fourth = launch(async move { animating.animate_to(To).await });
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    assert_ne!(fourth.get(), Some(false));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, segmentInitialized
#[test]
fn the_segment_is_the_seeked_change() {
    let state = SeekableTransitionState::new(From);
    let segment = Rc::new(RefCell::new(None));
    let (seen, given) = (segment.clone(), state.clone());
    let mut ui = compose_test(move || {
        let seeking = given.clone();
        LaunchedEffect(given.clone(), async move {
            seeking.seek_to_with(0.0, To).await;
        });
        let transition = remember_transition(&given, "Test");
        let _ = transition.animate_int_with(
            |segment| {
                if segment.initial_state == segment.target_state {
                    snap(0).into()
                } else {
                    tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into()
                }
            },
            "Value",
            |state| if *state == From { 0 } else { 1000 },
        );
        *seen.borrow_mut() = Some(transition.segment());
    });
    ui.wait_for_idle();
    let segment = segment.borrow().clone().unwrap();
    assert_eq!((segment.initial_state, segment.target_state), (From, To));
}

thread_local! {
    static VALUES: RefCell<Vec<State<i32>>> = const { RefCell::new(Vec::new()) };
}

fn values() -> Vec<i32> {
    VALUES.with(|values| values.borrow().iter().map(State::get).collect())
}

fn close(actual: i32, expected: f32, what: &str) {
    assert!((actual as f32 - expected).abs() <= 1.0, "{what}: {actual}, Compose's {expected}");
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, seekThirdState
#[test]
fn seeking_a_third_state_mid_way_goes_on_from_the_values() {
    let state = SeekableTransitionState::new(From);
    let given = state.clone();
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let seeking = given.clone();
        LaunchedEffect(given.clone(), async move {
            seeking.seek_to_with(0.0, To).await;
        });
        let transition = remember_transition(&given, "Test");
        let linear = || tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into();
        let first = transition.animate_int_with(
            move |_| linear(),
            "Value",
            |state| {
                if *state == From { 0 } else { 1000 }
            },
        );
        let second = transition.animate_int_with(
            move |_| linear(),
            "Value",
            |state| {
                if *state == Other { 1000 } else { 0 }
            },
        );
        let third =
            transition.animate_int_with(move |_| linear(), "Value", |state| value_of(*state));
        VALUES.with(|values| *values.borrow_mut() = vec![first, second, third]);
    });
    // The seek runs.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [0, 0, 0]);
    let seeking = state.clone();
    block_on(&mut ui, async move { seeking.seek_to(0.5).await });
    assert_eq!(state.fraction(), 0.5);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [500, 0, 500]);
    // A seek to Other: it won't end until the values got to To.
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, Other).await });
    // The composition of Other; its first frame changes nothing yet.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(state.target_state(), Other);
    assert_eq!(values(), [500, 0, 500]);
    // The values go on to To linearly: 150 ms left.
    ui.advance_time_by(std::time::Duration::from_millis(80));
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 500.0 + 500.0 * 80.0 / 150.0, "first");
    assert_eq!(second, 0);
    close(third, 500.0 + 500.0 * 80.0 / 150.0, "third");
    let seeking = state.clone();
    let half = launch(async move { seeking.seek_to(0.5).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    let expected = 500.0 + 500.0 * 96.0 / 150.0;
    close(first, expected, "first");
    assert_eq!(second, 500);
    close(third, expected + 0.5 * (2000.0 - expected), "third");
    // To the end of the values' animation.
    ui.advance_time_by(std::time::Duration::from_millis(5000));
    ui.wait_for_idle();
    assert_eq!(half.get(), Some(true));
    assert_eq!(seek.get(), Some(false));
    assert_eq!(values(), [1000, 500, 1500]);
    let seeking = state.clone();
    block_on(&mut ui, async move { seeking.seek_to(1.0).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [1000, 1000, 2000]);
}

/// A rule whose content's transition animates three ints linearly over 300 ms: 0 to 1000 for
/// `To`, 1000 for `Other` alone, and 0, 1000 or 2000; `start` runs first.
fn three_values(state: &SeekableTransitionState<AnimStates>, start: Call) -> ComposeTestRule {
    let given = state.clone();
    ComposeTestRule::with_manual_clock(move || {
        let starting = given.clone();
        LaunchedEffect(given.clone(), async move {
            start(starting).await;
        });
        let transition = remember_transition(&given, "Test");
        let linear = || tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into();
        let first = transition.animate_int_with(
            move |_| linear(),
            "Value",
            |state| {
                if *state == From { 0 } else { 1000 }
            },
        );
        let second = transition.animate_int_with(
            move |_| linear(),
            "Value",
            |state| {
                if *state == Other { 1000 } else { 0 }
            },
        );
        let third =
            transition.animate_int_with(move |_| linear(), "Value", |state| value_of(*state));
        VALUES.with(|values| *values.borrow_mut() = vec![first, second, third]);
    })
}

fn animate_to_to(state: SeekableTransitionState<AnimStates>) -> Pinned {
    Box::pin(async move { state.animate_to(To).await })
}

fn millis(millis: u64) -> std::time::Duration {
    std::time::Duration::from_millis(millis)
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, interruptAnimationWithSeekThirdState
#[test]
fn a_seek_to_a_third_state_interrupts_an_animation() {
    let state = SeekableTransitionState::new(From);
    let mut ui = three_values(&state, animate_to_to);
    // The animation's start time.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(state.fraction().abs() <= 0.01);
    assert_eq!(values(), [0, 0, 0]);
    // About half way: 160 / 300.
    ui.advance_time_by(millis(160));
    ui.wait_for_idle();
    assert!((state.fraction() - 0.53).abs() <= 0.01, "{}", state.fraction());
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 533.0, "first");
    assert_eq!(second, 0);
    close(third, 533.0, "third");
    // A seek to Other: it ends when the values got to To.
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, Other).await });
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 533.0, "first");
    assert_eq!(second, 0);
    close(third, 533.0, "third");
    // Two frames on: 192 / 300.
    ui.advance_time_by(millis(32));
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 640.0, "first");
    assert_eq!(second, 0);
    close(third, 640.0, "third");
    let seeking = state.clone();
    let half = launch(async move { seeking.seek_to(0.5).await });
    ui.wait_for_idle();
    assert_eq!(values()[1], 500);
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(half.get(), Some(true));
    assert_eq!(seek.get(), Some(false));
    assert_eq!(values(), [1000, 500, 1500]);
    let seeking = state.clone();
    let end = launch(async move { seeking.seek_to(1.0).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(end.get(), Some(true));
    assert_eq!(state.fraction(), 1.0);
    assert_eq!(values(), [1000, 1000, 2000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, interruptAnimationWithAnimateToThirdState
#[test]
fn an_animation_to_a_third_state_interrupts_an_animation() {
    let state = SeekableTransitionState::new(From);
    let mut ui = three_values(&state, animate_to_to);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [0, 0, 0]);
    ui.advance_time_by(millis(160));
    ui.wait_for_idle();
    assert!((state.fraction() - 0.53).abs() <= 0.01, "{}", state.fraction());
    let animating = state.clone();
    let other = launch(async move { animating.animate_to(Other).await });
    // The composition after `animate_to`: the first values go on, 176 / 300.
    ui.advance_time_by(millis(16));
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 586.7, "first");
    assert_eq!(second, 0);
    close(third, 586.7, "third");
    // The animation to Other starts: 192 / 300 to To, 16 / 300 to Other.
    ui.advance_time_by(millis(16));
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 640.0, "first");
    close(second, 53.3, "second");
    close(third, 640.0 + (2000.0 - 640.0) * 0.053, "third");
    // Two frames on: 224 / 300 and 48 / 300.
    ui.advance_time_by(millis(32));
    ui.wait_for_idle();
    let [first, second, third] = values()[..] else { panic!() };
    close(first, 746.7, "first");
    close(second, 160.0, "second");
    assert!((third as f32 - (746.7 + (2000.0 - 746.7) * 0.16)).abs() <= 2.0, "third {third}");
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(other.get(), Some(true));
    assert_eq!(values(), [1000, 1000, 2000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, cancelAnimationWithAnimateToThirdStateW
#[test]
fn an_animation_cancelled_for_a_third_state_goes_on_from_the_values() {
    let state = SeekableTransitionState::new(From);
    let target = mutable_state_of(To);
    let (given, handle) = (state.clone(), target.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let animating = given.clone();
        let target = handle.get();
        LaunchedEffect((given.clone(), target), async move {
            animating.animate_to(target).await;
        });
        let transition = remember_transition(&given, "Test");
        let linear = || tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into();
        let first = transition.animate_int_with(
            move |_| linear(),
            "Value",
            |state| {
                if *state == From { 0 } else { 1000 }
            },
        );
        let second = transition.animate_int_with(
            move |_| linear(),
            "Value",
            |state| {
                if *state == Other { 1000 } else { 0 }
            },
        );
        let third =
            transition.animate_int_with(move |_| linear(), "Value", |state| value_of(*state));
        VALUES.with(|values| *values.borrow_mut() = vec![first, second, third]);
    });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [0, 0, 0]);
    ui.advance_time_by(millis(160));
    ui.wait_for_idle();
    assert!((state.fraction() - 0.53).abs() <= 0.01);
    target.set(Other);
    ui.advance_time_by(millis(16));
    ui.wait_for_idle();
    assert_eq!(state.target_state(), Other);
    assert_eq!(values(), [586, 0, 586]);
    ui.advance_time_by(millis(16));
    ui.wait_for_idle();
    // The animation to Other hasn't started yet.
    assert_eq!(values(), [640, 0, 640]);
    ui.advance_time_by(millis(16));
    ui.wait_for_idle();
    assert_eq!(values(), [693, 53, 693 + (2000 - 693) * 16 / 300]);
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(values(), [1000, 1000, 2000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, interruptInterruption
#[test]
fn interruptions_of_interruptions_end_at_the_last_target_values() {
    let state = SeekableTransitionState::new(From);
    let mut ui = three_values(&state, |_| Box::pin(async {}));
    ui.wait_for_idle();
    let calls: [(f32, Option<AnimStates>); 5] =
        [(0.0, Some(To)), (0.5, None), (0.0, Some(Other)), (0.5, None), (0.0, Some(From))];
    for (index, (fraction, target)) in calls.into_iter().enumerate() {
        let seeking = state.clone();
        let _ = launch(async move {
            match target {
                Some(target) => seeking.seek_to_with(fraction, target).await,
                None => seeking.seek_to(fraction).await,
            }
        });
        ui.wait_for_idle();
        if index == 2 {
            // The initial-value animation's start time.
            ui.advance_time_by_frame();
        }
    }
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(values(), [1000, 1000, 2000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, delayedTransition
#[test]
fn a_seek_sets_the_play_time_of_a_child_visibility() {
    use compose_foundation::layout::*;
    let state = SeekableTransitionState::new(From);
    let watched = Rc::new(RefCell::new(None));
    let (seen, given) = (watched.clone(), state.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        *seen.borrow_mut() = Some(transition.clone());
        let linear = tween(300).easing(LINEAR_EASING);
        transition.AnimatedVisibility(
            |state| *state != To,
            Modifier,
            fade_in_with(linear.clone(), 0.0),
            fade_out_with(linear, 0.0),
            |_| Box!(modifier = Modifier.fill_max_size()),
        );
    });
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.5, To).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    let transition = watched.borrow().clone().unwrap();
    assert_eq!(transition.play_time_nanos(), 150 * 1_000_000);
}

/// A rule whose content's transition animates one int linearly over `duration` ms, 0 to 1000.
fn one_value(state: &SeekableTransitionState<AnimStates>, duration: u32) -> ComposeTestRule {
    let given = state.clone();
    ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        let value = transition.animate_int_with(
            move |_| tween(duration).easing(LINEAR_EASING).into(),
            "Value",
            |state| if *state == From { 0 } else { 1000 },
        );
        VALUES.with(|values| *values.borrow_mut() = vec![value]);
    })
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, seekAfterAnimating
#[test]
fn a_seek_after_the_animation_ended_changes_nothing() {
    let state = SeekableTransitionState::new(From);
    let mut ui = one_value(&state, DEFAULT_DURATION_MILLIS);
    ui.wait_for_idle();
    let animating = state.clone();
    let animation = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by(millis(10_000));
    ui.wait_for_idle();
    assert_eq!(animation.get(), Some(true));
    assert_eq!(values(), [1000]);
    let seeking = state.clone();
    let _ = launch(async move { seeking.seek_to(0.5).await });
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    assert_eq!(values(), [1000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToWithSpec
#[test]
fn an_animation_follows_its_spec_of_the_fraction() {
    let state = SeekableTransitionState::new(From);
    let mut ui = one_value(&state, 100);
    ui.wait_for_idle();
    let seeking = state.clone();
    let _ = launch(async move { seeking.seek_to_with(0.5, To).await });
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    let animating = state.clone();
    let spec = Some(tween(1000).easing(LINEAR_EASING).into());
    let animation = launch(async move { animating.animate_to_with(To, spec).await });
    // The start time.
    ui.advance_time_by_frame();
    for (advance, expected) in [(64, 532), (192, 628), (256, 756)] {
        ui.advance_time_by(millis(advance));
        ui.wait_for_idle();
        assert_eq!(values(), [expected], "{advance} ms on");
    }
    ui.advance_time_by(millis(512));
    ui.wait_for_idle();
    assert_eq!(animation.get(), Some(true));
    assert_eq!(values(), [1000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, seekToFollowedByAnimation
#[test]
fn a_seek_to_the_end_then_an_animation_back() {
    let state = SeekableTransitionState::new(From);
    let mut ui = one_value(&state, 1000);
    ui.wait_for_idle();
    let running = state.clone();
    let _ = launch(async move {
        running.seek_to_with(1.0, To).await;
        running.animate_to(From).await
    });
    // The composition after the seek: the seek sets the value.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [1000]);
    // The animation's start time.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [1000]);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [984]);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [968]);
    ui.advance_time_by(millis(1000));
    ui.wait_for_idle();
    assert_eq!(values(), [0]);
}

fn to_or_1000(state: AnimStates) -> i32 {
    if state == From { 0 } else { 1000 }
}

/// A rule whose content's transition animates an int per `targets` linearly over `duration`
/// ms, into [`VALUES`].
fn values_rule(
    state: &SeekableTransitionState<AnimStates>,
    duration: u32,
    targets: &'static [fn(AnimStates) -> i32],
) -> ComposeTestRule {
    let given = state.clone();
    ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        let values = targets
            .iter()
            .map(|target| {
                transition.animate_int_with(
                    move |_| tween(duration).easing(LINEAR_EASING).into(),
                    "Value",
                    move |state| target(*state),
                )
            })
            .collect();
        VALUES.with(|slot| *slot.borrow_mut() = values);
    })
}

const BY_STATE: &[fn(AnimStates) -> i32] = &[value_of];
const BY_STATE_AND_TO: &[fn(AnimStates) -> i32] = &[value_of, to_or_1000];
const TO: &[fn(AnimStates) -> i32] = &[to_or_1000];

/// Compose's conflictingSeekTo and conflictingSnapTo: two calls, then animations back to From;
/// the second interrupts the first, if `first_interrupted`.
fn conflicting(first: Call, second: Call, first_interrupted: bool) {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1000, BY_STATE);
    ui.wait_for_idle();
    let first = launch(first(state.clone()));
    let second = launch(second(state.clone()));
    // The composition after the calls.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    if first_interrupted {
        assert_eq!(first.get(), Some(false));
    }
    assert_ne!(second.get(), Some(false));
    assert_eq!(values(), [2000]);
    // The animation's start time.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [2000]);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [1968]);
    ui.advance_time_by(millis(1000));
    ui.wait_for_idle();
    assert_eq!(values(), [0]);
    assert_eq!(second.get(), Some(true));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, conflictingSeekTo
#[test]
fn a_second_seek_interrupts_the_first() {
    conflicting(
        |state| {
            Box::pin(async move {
                state.seek_to_with(1.0, To).await;
                state.animate_to(From).await
            })
        },
        |state| {
            Box::pin(async move {
                state.seek_to_with(1.0, Other).await;
                state.animate_to(From).await
            })
        },
        true,
    );
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, conflictingSnapTo
#[test]
fn a_second_snap_interrupts_the_first() {
    conflicting(
        |state| {
            Box::pin(async move {
                state.snap_to(To).await;
                state.animate_to(From).await
            })
        },
        |state| {
            Box::pin(async move {
                state.snap_to(Other).await;
                state.animate_to(From).await
            })
        },
        true,
    );
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, conflictingSeekTo2
#[test]
fn a_seek_to_the_current_state_then_a_conflicting_one() {
    conflicting(
        |state| {
            Box::pin(async move {
                state.seek_to_with(1.0, From).await;
                state.animate_to(To).await
            })
        },
        |state| {
            Box::pin(async move {
                state.seek_to_with(1.0, Other).await;
                state.animate_to(From).await
            })
        },
        false,
    );
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, conflictingSnapTo2
#[test]
fn a_snap_to_the_current_state_then_a_conflicting_one() {
    conflicting(
        |state| {
            Box::pin(async move {
                state.snap_to(From).await;
                state.animate_to(To).await
            })
        },
        |state| {
            Box::pin(async move {
                state.snap_to(Other).await;
                state.animate_to(From).await
            })
        },
        false,
    );
}

/// A seek half way to `To`, then a snap to `target`: the value ends at `expected`.
fn seek_then_snap(target: AnimStates, expected: i32) {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1000, BY_STATE);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.5, To).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    let snapping = state.clone();
    let snap = launch(async move { snapping.snap_to(target).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(snap.get(), Some(true));
    assert_eq!(values(), [expected]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, snapToSameTargetState
#[test]
fn a_snap_to_the_target_ends_there() {
    seek_then_snap(To, 1000);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, snapToSameCurrentState
#[test]
fn a_snap_to_the_current_state_goes_back() {
    seek_then_snap(From, 0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, snapToExistingState
#[test]
fn snaps_to_the_current_state() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1000, BY_STATE);
    ui.wait_for_idle();
    let snapping = state.clone();
    let snap = launch(async move { snapping.snap_to(From).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(snap.get(), Some(true));
    assert_eq!(values(), [0]);
    let running = state.clone();
    let calls = launch(async move {
        running.seek_to_with(0.5, To).await;
        running.snap_to(From).await;
        running.snap_to(From).await
    });
    // The seek's composition, then the snap's.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(calls.get(), Some(true));
    assert_eq!(values(), [0]);
}

/// A rule animating over `duration` ms, a seek to `To` at 0 composed, then an animation to it
/// started and its start time locked.
fn animating(duration: u32) -> (SeekableTransitionState<AnimStates>, ComposeTestRule, Outcome) {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, duration, BY_STATE);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, To).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    let animating = state.clone();
    let animation = launch(async move { animating.animate_to(animating.target_state()).await });
    ui.advance_time_by_frame();
    (state, ui, animation)
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateAndContinueAnimation
#[test]
fn a_second_animation_goes_on_from_the_first() {
    let (state, mut ui, first) = animating(1000);
    ui.advance_time_by(millis(160));
    ui.wait_for_idle();
    assert_eq!(values(), [160]);
    let animating = state.clone();
    let second = launch(async move { animating.animate_to(animating.target_state()).await });
    ui.wait_for_idle();
    assert_eq!(first.get(), Some(false));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [176]);
    ui.advance_time_by(millis(900));
    ui.wait_for_idle();
    assert_eq!(second.get(), Some(true));
    assert_eq!(values(), [1000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, continueAnimationWithNewSpec
#[test]
fn a_second_animation_with_a_spec_goes_on_from_the_first() {
    let (state, mut ui, first) = animating(1000);
    ui.advance_time_by(millis(160));
    ui.wait_for_idle();
    assert_eq!(values(), [160]);
    let animating = state.clone();
    let spec = Some(tween(200).easing(LINEAR_EASING).into());
    let second =
        launch(async move { animating.animate_to_with(animating.target_state(), spec).await });
    ui.wait_for_idle();
    assert_eq!(first.get(), Some(false));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    // 160 + 840 * 16 / 200.
    close(values()[0], 227.2, "value");
    ui.advance_time_by(millis(200));
    ui.wait_for_idle();
    assert_eq!(second.get(), Some(true));
    assert_eq!(values(), [1000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, continueAnimationUsesInitialVelocity
#[test]
fn a_second_animation_starts_at_the_velocity_of_the_first() {
    let (state, mut ui, first) = animating(1600);
    ui.advance_time_by(millis(800));
    ui.wait_for_idle();
    assert_eq!(values(), [500]);
    let animating = state.clone();
    let spec = spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_VERY_LOW)
        .visibility_threshold(0.01f32);
    let _ = launch(async move {
        animating.animate_to_with(animating.target_state(), Some(spec.into())).await
    });
    ui.wait_for_idle();
    assert_eq!(first.get(), Some(false));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    // 500 / 800 = 0.625 per ms: 10 a frame.
    assert!((values()[0] - 510).abs() <= 2, "{:?}", values());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, continueAnimationNewSpecUsesInitialVelocity
#[test]
fn a_second_animation_starts_at_the_velocity_of_a_first_spring() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1000, BY_STATE);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, To).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    let spring = spring_with(2.0, Spring::STIFFNESS_MEDIUM);
    let animating = state.clone();
    let first = launch(async move {
        animating.animate_to_with(animating.target_state(), Some(spring.into())).await
    });
    ui.advance_time_by_frame();
    let (zero, one) = (AnimationVector::from([0.0]), AnimationVector::from([1.0]));
    let at = |millis: i64| spring.value_from_millis(millis, zero, one, zero)[0];
    // How long it takes to get about half way.
    let mut half = 16;
    while at(half) < 0.5 {
        half += 16;
    }
    ui.advance_time_by(millis(half as u64));
    ui.wait_for_idle();
    let half_value = at(half) * 1000.0;
    close(values()[0], half_value, "half way");
    let velocity = spring.velocity_from_nanos(half * 1_000_000, zero, one, zero)[0];
    let animating = state.clone();
    let bouncy = spring_with(Spring::DAMPING_RATIO_HIGH_BOUNCY, Spring::STIFFNESS_VERY_LOW)
        .visibility_threshold(0.01f32);
    let _ = launch(async move {
        animating.animate_to_with(animating.target_state(), Some(bouncy.into())).await
    });
    ui.wait_for_idle();
    assert_eq!(first.get(), Some(false));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let expected = half_value + velocity * 16.0;
    assert!((values()[0] as f32 - expected).abs() <= 2.0, "{:?}, Compose's {expected}", values());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animationCompletionHasNoInitialValueAnimation
#[test]
fn an_ended_animation_leaves_no_initial_value_animation() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1600, BY_STATE);
    ui.wait_for_idle();
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by(millis(1700));
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(From).await });
    // The start time.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [1000]);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [990]);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [980]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animationDurationWorksOnInitialStateChange
#[test]
fn values_keep_their_durations_when_the_initial_state_changes() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1600, BY_STATE_AND_TO);
    ui.wait_for_idle();
    let animating = state.clone();
    let spec = Some(tween(160).easing(LINEAR_EASING).into());
    let _ = launch(async move { animating.animate_to_with(To, spec).await });
    // The start time.
    ui.advance_time_by_frame();
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(Other).await });
    // A frame on to To, and the composition of Other.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(values(), [100, 100]);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    // 200 + 1800 * 16 / 1600; the second goes on.
    assert_eq!(values(), [218, 200]);
    ui.advance_time_by(millis(128));
    ui.wait_for_idle();
    // 1000 + 1000 * 144 / 1600.
    assert_eq!(values(), [1090, 1000]);
    ui.advance_time_by(millis(1600));
    ui.wait_for_idle();
    assert_eq!(values(), [2000, 1000]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animationAlreadyAtTarget
#[test]
fn an_animation_to_where_a_seek_ended_ends_at_once() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1600, TO);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(1.0, To).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    assert_eq!(values(), [1000]);
    let animating = state.clone();
    let animation = launch(async move { animating.animate_to(To).await });
    // The composition of the current state as the target.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(animation.get(), Some(true));
    assert_eq!(state.current_state(), To);
    assert_eq!(state.target_state(), To);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, seekCurrentEqualsTarget
#[test]
fn a_seek_without_a_change_of_state_changes_nothing() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1600, TO);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to(0.5).await });
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    assert_eq!(values(), [0]);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToAtEndCurrentInitialValueAnimations
#[test]
fn an_animation_from_the_end_goes_on_with_the_initial_value_animations() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1600, BY_STATE_AND_TO);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, To).await });
    ui.advance_time_by_frame();
    assert_eq!(seek.get(), Some(true));
    let seeking = state.clone();
    let other = launch(async move { seeking.seek_to_with(1.0, Other).await });
    ui.advance_time_by_frame();
    // The second value animates from To.
    assert_eq!(other.get(), None);
    let animating = state.clone();
    let animation = launch(async move { animating.animate_to(Other).await });
    ui.wait_for_idle();
    assert_eq!(other.get(), Some(false));
    assert_eq!(animation.get(), None);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(animation.get(), None);
    assert_eq!(values(), [2000, 1000 * 16 / 1600]);
}

/// Compose's changingDuration and changingAnimationWithAnimateToThirdState: a second value
/// over 3200 ms joins once the first is past 500.
fn joining_value(
    state: &SeekableTransitionState<AnimStates>,
    target: fn(AnimStates) -> i32,
) -> (ComposeTestRule, Rc<Cell<i32>>, Rc<Cell<f32>>) {
    let (first, second) = (Rc::new(Cell::new(-1)), Rc::new(Cell::new(-1.0)));
    let (seen_first, seen_second, given) = (first.clone(), second.clone(), state.clone());
    let ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        let value = transition.animate_int_with(
            |_| tween(1600).easing(LINEAR_EASING).into(),
            "Value",
            move |state| target(*state),
        );
        let joining: State<f32> = if value.get() < 500 {
            remember(|| mutable_state_of(0.0f32)).into()
        } else {
            transition.animate_float_with(
                |_| tween(3200).easing(LINEAR_EASING).into(),
                "Value2",
                move |state| target(*state) as f32,
            )
        };
        seen_first.set(value.get());
        seen_second.set(joining.get());
    });
    (ui, first, second)
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, changingDuration
#[test]
fn a_value_joining_lengthens_the_animation() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, first, second) = joining_value(&state, to_or_1000);
    ui.wait_for_idle();
    let animating = state.clone();
    let animation = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(800));
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (500, 0.0));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (510, 5.0));
    ui.advance_time_by(millis(784));
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (1000, 250.0));
    assert_eq!(animation.get(), None);
    ui.advance_time_by(millis(2400));
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (1000, 1000.0));
    ui.advance_time_by_frame();
    assert_eq!(animation.get(), Some(true));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, changingAnimationWithAnimateToThirdState
#[test]
fn a_joined_value_goes_on_to_a_third_state() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, first, second) = joining_value(&state, value_of);
    ui.wait_for_idle();
    let animating = state.clone();
    let to = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(800));
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (1000 * 800 / 1600, 0.0));
    // A frame past the composition: the second runs.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (1000 * 816 / 1600, 5.0));
    let animating = state.clone();
    let other = launch(async move { animating.animate_to(Other).await });
    ui.wait_for_idle();
    assert_eq!(to.get(), Some(false));
    // The composition of Other: still on to To.
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get().round()), (1000 * 832 / 1600, 10.0));
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    let (towards_first, towards_second) = (1000 * 848 / 1600, (1000 * 48 / 3200) as f32);
    assert_eq!(first.get(), towards_first + (2000 - towards_first) * 16 / 1600);
    assert!(
        (second.get() - (towards_second + (2000.0 - towards_second) * 16.0 / 3200.0)).abs() < 0.01
    );
    ui.advance_time_by(millis(752));
    ui.wait_for_idle();
    let towards_second = (1000 * 800 / 3200) as f32;
    assert_eq!(first.get(), 1000 + 1000 * 768 / 1600);
    assert!(
        (second.get() - (towards_second + (2000.0 - towards_second) * 768.0 / 3200.0)).abs() < 0.01
    );
    assert_eq!(other.get(), None);
    ui.advance_time_by(millis(832));
    ui.wait_for_idle();
    let towards_second = (1000 * 1632 / 3200) as f32;
    assert_eq!(first.get(), 2000);
    assert!(
        (second.get() - (towards_second + (2000.0 - towards_second) * 1600.0 / 3200.0)).abs()
            < 0.01
    );
    assert_eq!(other.get(), None);
    ui.advance_time_by(millis(1600));
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get()), (2000, 2000.0));
    // Compose's ends a frame later: its coroutine resumes after the frame's composition, so the
    // change of current state composes on the next frame; compose-rs's task resumes before the
    // composition of the same frame (D817). This encodes today's order: a runtime parity
    // item (docs/parity/runtime.md, behaviour 15), to go back to Compose's frame then.
    assert_eq!(other.get(), Some(true));
}

type Watched<T> = Rc<RefCell<Option<T>>>;

fn watched<T>() -> Watched<T> {
    Rc::new(RefCell::new(None))
}

/// A rule whose content, started with `start`, animates one int (0 to 1000, the default
/// tween) of a transition `watched`.
fn running_rule(
    state: &SeekableTransitionState<AnimStates>,
    start: Call,
) -> (ComposeTestRule, Watched<Transition<AnimStates>>, Rc<Cell<i32>>) {
    let (transition, value) = (watched(), Rc::new(Cell::new(-1)));
    let (seen, seen_value, given) = (transition.clone(), value.clone(), state.clone());
    let ui = ComposeTestRule::with_manual_clock(move || {
        let starting = given.clone();
        LaunchedEffect(given.clone(), async move {
            start(starting).await;
        });
        let transition = remember_transition(&given, "Test");
        *seen.borrow_mut() = Some(transition.clone());
        let animated = transition.animate_int_with(
            |_| tween(DEFAULT_DURATION_MILLIS).easing(LINEAR_EASING).into(),
            "Value",
            |state| to_or_1000(*state),
        );
        seen_value.set(animated.get());
    });
    (ui, transition, value)
}

fn running(transition: &Watched<Transition<AnimStates>>) -> bool {
    transition.borrow().as_ref().unwrap().is_running()
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, isRunningDuringAnimateTo
#[test]
fn a_transition_runs_while_it_animates() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, transition, value) = running_rule(&state, animate_to_to);
    ui.wait_for_idle();
    assert_eq!(value.get(), 0);
    assert!(!running(&transition));
    // The composition after `animate_to`, then the start time.
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert!(value.get() > 0);
    assert!(running(&transition));
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(value.get(), 1000);
    assert!(!running(&transition));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, isRunningFalseAfterSnapTo
#[test]
fn a_transition_snapped_does_not_run() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, transition, value) = running_rule(&state, |state| {
        Box::pin(async move {
            with_frame_nanos(|_| ()).await;
            state.snap_to(To).await
        })
    });
    ui.wait_for_idle();
    assert_eq!(value.get(), 0);
    assert!(!running(&transition));
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    ui.wait_for_idle();
    assert_eq!(value.get(), 1000);
    assert!(!running(&transition));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, isRunningFalseAfterChildAnimatedVisibilityTransition
#[test]
fn a_transition_and_its_visibility_child_stop_running() {
    use compose_foundation::layout::*;
    use compose_ui::UnitExt;
    let state = SeekableTransitionState::new(From);
    let (transition, child) = (watched(), watched());
    let (seen, seen_child, given) = (transition.clone(), child.clone(), state.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        *seen.borrow_mut() = Some(transition.clone());
        let seen_child = seen_child.clone();
        transition.AnimatedVisibility(
            |state| *state == To,
            Modifier,
            fade_in() + expand_in(),
            fade_out() + shrink_out(),
            move |scope| {
                *seen_child.borrow_mut() = Some(scope.transition().clone());
                Box!(modifier = Modifier.size(100.dp()));
            },
        );
    });
    ui.wait_for_idle();
    assert!(!running(&transition));
    assert!(child.borrow().is_none());
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by(millis(50));
    ui.wait_for_idle();
    assert!(running(&transition));
    assert!(child.borrow().as_ref().unwrap().is_running());
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert!(!running(&transition));
    assert!(!child.borrow().as_ref().unwrap().is_running());
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, isRunningFalseAfterRemovingAnimationWhileAnimatingToPreviousState
#[test]
fn a_transition_stops_running_when_its_long_value_leaves() {
    let state = SeekableTransitionState::new(From);
    let transition = watched();
    let (short, long) = (Rc::new(Cell::new(-1.0)), watched::<State<f32>>());
    let with_long = mutable_state_of(true);
    let (seen, seen_short, seen_long, given, adding) =
        (transition.clone(), short.clone(), long.clone(), state.clone(), with_long.handle());
    let mut ui = compose_test(move || {
        let transition = remember_transition(&given, "");
        *seen.borrow_mut() = Some(transition.clone());
        let value = transition.animate_float(
            |_| tween(500).into(),
            |state| if *state == From { 0.0 } else { 1000.0 },
        );
        seen_short.set(value.get());
        *seen_long.borrow_mut() = if adding.get() {
            // Longer than the first: it keeps the transition running.
            Some(transition.animate_float(
                |_| tween(10_000_000).into(),
                |state| if *state == From { 0.0 } else { 1000.0 },
            ))
        } else {
            None
        };
    });
    ui.wait_for_idle();
    assert_eq!(short.get(), 0.0);
    assert_eq!(long.borrow().as_ref().unwrap().get(), 0.0);
    ui.set_auto_advance(false);
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(To).await });
    ui.advance_time_by_frame();
    // The first ends, not the second.
    ui.advance_time_by(millis(750));
    assert_eq!(short.get(), 1000.0);
    assert!(long.borrow().as_ref().unwrap().get() < 1000.0);
    assert!(running(&transition));
    // Back.
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(From).await });
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(500));
    assert_eq!(short.get(), 0.0);
    assert!(long.borrow().as_ref().unwrap().get() > 0.0);
    assert!(running(&transition));
    // The second leaves.
    with_long.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(long.borrow().is_none());
    assert!(!running(&transition));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, exitingContentKillTransition
#[test]
fn content_leaving_with_the_scope_of_the_call_leaves_the_play_time_at_0() {
    use compose_foundation::layout::*;
    let state = SeekableTransitionState::new(From);
    let (transition, scope) = (watched(), watched::<CoroutineScope>());
    let (seen, seen_scope, given) = (transition.clone(), scope.clone(), state.clone());
    let mut ui = compose_test(move || {
        let transition = remember_transition(&given, "Test");
        *seen.borrow_mut() = Some(transition.clone());
        let seen_scope = seen_scope.clone();
        transition.AnimatedVisibility(
            |state| *state == From,
            Modifier,
            fade_in() + expand_in(),
            fade_out() + shrink_out(),
            move |_| {
                *seen_scope.borrow_mut() = Some(remember_coroutine_scope());
                Box!(modifier = Modifier.fill_max_size());
            },
        );
    });
    ui.wait_for_idle();
    let animating = state.clone();
    scope.borrow().as_ref().unwrap().launch(async move {
        animating.animate_to(To).await;
    });
    ui.wait_for_idle();
    assert_eq!(transition.borrow().as_ref().unwrap().play_time_nanos(), 0);
}

/// A spec of NaNs over 100 ms, as Compose's test's object.
struct NanSpec;

impl VectorizedAnimationSpec for NanSpec {
    fn value_from_nanos(
        &self,
        _: i64,
        _: AnimationVector,
        _: AnimationVector,
        _: AnimationVector,
    ) -> AnimationVector {
        AnimationVector::from([f32::NAN])
    }

    fn velocity_from_nanos(
        &self,
        _: i64,
        _: AnimationVector,
        _: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        initial_velocity
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        100_000_000
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToWithZeroDurationOrNaNDoesNotCrash
#[test]
fn an_animation_of_no_duration_along_nans_ends() {
    let state = SeekableTransitionState::new(From);
    let given = state.clone();
    let mut ui = compose_test(move || {
        let transition = remember_transition(&given, "Test");
        // No duration.
        let _ = transition
            .animate_float(|_| tween(0).into(), |state| if *state == From { 0.0 } else { 1.0 });
    });
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    let animating = state.clone();
    let _ = launch(async move {
        animating.animate_to_with(To, Some(AnimationSpec::custom(NanSpec))).await
    });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!(state.current_state(), To);
    assert_eq!(state.fraction(), 0.0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/TransitionTest.kt, childTransitionStartsUninterrupted_usingSeekableTransition
#[test]
fn a_child_of_a_seekable_transition_starts_uninterrupted() {
    use compose_ui::Color;
    let state = SeekableTransitionState::new(0);
    let given = state.clone();
    let yellow = Color::from_argb(0xFFFFFF00);
    let color = Rc::new(Cell::new(Color::BLACK));
    let seen = color.clone();
    let mut ui = compose_test(move || {
        let transition = remember_transition(&given, "");
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
    ui.wait_for_idle();
    assert_eq!((color.get(), state.current_state()), (Color::BLUE, 0));
    ui.set_auto_advance(false);
    // A change the child doesn't see: no animation.
    let animating = state.clone();
    let _ = launch(async move { animating.animate_to(1).await });
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    assert_eq!((color.get(), state.current_state()), (Color::BLUE, 1));
    // The child's first change: its spec from the start (yellow), not an interruption.
    let animating = state.clone();
    let _ = launch(async move {
        animating.seek_to_with(0.0, 2).await;
        animating.animate_to(2).await
    });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert_eq!((color.get(), state.current_state()), (yellow, 1));
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    assert_eq!((color.get(), state.current_state()), (Color::RED, 2));
}

/// Compose's test's `ScreenState`: equal only to itself.
#[derive(Clone)]
struct Screen(Rc<(String, OwnedState<bool>)>);

impl Screen {
    fn new(label: &str) -> Screen {
        Screen(Rc::new((label.to_string(), mutable_state_of(false))))
    }

    fn removing(&self) -> bool {
        self.0.1.get()
    }
}

impl PartialEq for Screen {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::hash::Hash for Screen {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        Rc::as_ptr(&self.0).hash(state);
    }
}

impl ComposeArg for Screen {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

#[derive(Clone, PartialEq)]
struct Screens(Vec<Screen>);

impl ComposeArg for Screens {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, quickAddAndRemove
#[test]
fn screens_added_and_removed_quickly_leave_the_first() {
    use compose_foundation::layout::*;
    use compose_ui::UnitExt;
    let screens = mutable_state_of(vec![Screen::new("1")]);
    let state = SeekableTransitionState::new(screens.get());
    let (given, list) = (state.clone(), screens.handle());
    let mut ui = compose_test(move || {
        let transition = remember_transition(&given, "");
        let visible: Vec<Screen> = list.get().into_iter().filter(|it| !it.removing()).collect();
        // Compose's `snapshotFlow { … }.collectLatest`: a new list starts again.
        let (animating, removing) = (given.clone(), list);
        LaunchedEffect(Screens(visible.clone()), async move {
            animating.animate_to(visible).await;
            // Not interrupted: done animating.
            removing
                .update(|screens| screens.iter().filter(|it| !it.removing()).cloned().collect());
        });
        for screen in list.get() {
            let transition = transition.clone();
            key(screen.clone(), move || {
                let label = screen.0.0.clone();
                let child = transition.create_child_transition(None, move |screens| {
                    screens.last() == Some(&screen) && !screen.removing()
                });
                child.AnimatedVisibility(
                    |visible| *visible,
                    Modifier,
                    fade_in() + expand_in(),
                    fade_out() + shrink_out(),
                    move |_| Box!(modifier = Modifier.size(10.dp()).test_tag(label.clone())),
                );
            });
        }
    });
    let added = std::cell::Cell::new(1);
    let add = || {
        added.set(added.get() + 1);
        let label = added.get().to_string();
        screens.update(|screens| {
            let mut screens = screens.clone();
            screens.push(Screen::new(&label));
            screens
        });
    };
    let remove = || {
        let screens = screens.get();
        screens.iter().rev().find(|it| !it.removing()).unwrap().0.1.set(true);
    };
    ui.wait_for_idle();
    ui.set_auto_advance(false);
    add();
    ui.advance_time_by(millis(50));
    remove();
    ui.advance_time_by(millis(50));
    add();
    ui.advance_time_by(millis(50));
    remove();
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("1").assert_is_displayed();
    ui.on_node_with_tag("2").assert_is_not_displayed();
    ui.on_node_with_tag("3").assert_is_not_displayed();
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateAfterSeekToZero
#[test]
fn an_animation_after_a_seek_to_0_shows_the_content() {
    use compose_foundation::layout::*;
    use compose_ui::UnitExt;
    let state = SeekableTransitionState::new(From);
    let (value, child) = (Rc::new(Cell::new(-1)), watched::<Transition<EnterExitState>>());
    let (seen, seen_child, given) = (value.clone(), child.clone(), state.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        let animated = transition.animate_int_with(
            |_| tween(1600).easing(LINEAR_EASING).into(),
            "Value",
            |state| to_or_1000(*state),
        );
        seen.set(animated.get());
        let seen_child = seen_child.clone();
        Box!(modifier = Modifier.required_size(100.dp()), {
            let seen_child = seen_child.clone();
            transition.AnimatedVisibility(
                |state| *state == To,
                Modifier,
                fade_in() + expand_in(),
                fade_out() + shrink_out(),
                move |scope| {
                    *seen_child.borrow_mut() = Some(scope.transition().clone());
                    Box!(modifier = Modifier.fill_max_size().test_tag("content"));
                },
            );
        });
    });
    ui.wait_for_idle();
    let running = state.clone();
    let calls = launch(async move {
        running.animate_to(To).await;
        running.seek_to_with(0.5, From).await;
        running.seek_to_with(0.0, From).await
    });
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(calls.get(), Some(true));
    assert_eq!(value.get(), 1000);
    // Compose's test reads the content's pixels: the content is all there.
    let visible = |child: &Watched<Transition<EnterExitState>>| {
        let child = child.borrow();
        let child = child.as_ref().unwrap();
        (child.current_state(), child.target_state())
    };
    // Seeking to the exit, at its start.
    assert_eq!(visible(&child), (EnterExitState::Visible, EnterExitState::PostExit));
    ui.on_node_with_tag("content").assert_is_displayed();
    let animating = state.clone();
    let again = launch(async move { animating.animate_to(To).await });
    ui.wait_for_idle();
    // The initial-value animation ends first: the initial state changed from seeking to
    // animating.
    ui.advance_time_by(millis(5_000));
    ui.wait_for_idle();
    assert_eq!(again.get(), Some(true));
    assert_eq!(value.get(), 1000);
    assert_eq!(visible(&child), (EnterExitState::Visible, EnterExitState::Visible));
    ui.on_node_with_tag("content").assert_is_displayed();
}

// Review M1: Compose's vectorized specs have no `equals`, so `animateTo` with a spec always
// starts a new animation of the fraction, from where the one running is.
#[test]
fn an_animation_called_again_with_an_equal_spec_starts_again() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1000, BY_STATE);
    ui.wait_for_idle();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.0, To).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    let linear = || Some(AnimationSpec::from(tween(1000).easing(LINEAR_EASING)));
    let animating = state.clone();
    let _first = launch(async move { animating.animate_to_with(To, linear()).await });
    // The start time, then 480 ms.
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(480));
    ui.wait_for_idle();
    assert_eq!(values(), [480]);
    // Called again at 496 ms with an equal spec: a new 1000 ms tween from 0.48.
    let animating = state.clone();
    let second = launch(async move { animating.animate_to_with(To, linear()).await });
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    close(values()[0], 1000.0 * (0.48 + 0.52 * 16.0 / 1000.0), "a frame on");
    // 30 frames on: 496 ms into the new tween.
    ui.advance_time_by(millis(480));
    ui.wait_for_idle();
    close(values()[0], 1000.0 * (0.48 + 0.52 * 0.496), "half way");
    ui.advance_time_by(millis(500));
    ui.wait_for_idle();
    assert_eq!(values(), [1000]);
    assert_eq!(second.get(), Some(true));
}

// Review M3: an interrupted call ends its task, as Compose's `CancellationException` ends the
// coroutine: nothing after it runs, and a loop calling again can't spin.
#[test]
fn an_interrupted_call_ends_its_task() {
    let state = SeekableTransitionState::new(From);
    let mut ui = values_rule(&state, 1000, BY_STATE);
    ui.wait_for_idle();
    let after = Rc::new(Cell::new(0));
    let (animating, ran) = (state.clone(), after.clone());
    let first = launch(async move {
        loop {
            animating.animate_to(To).await;
            ran.set(ran.get() + 1);
            animating.animate_to(From).await;
        }
    });
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to_with(0.5, Other).await });
    ui.wait_for_idle();
    // Cancelled as the seek starts, as `MutatorMutex.mutate` cancels the mutator it replaces.
    assert_eq!(first.get(), Some(false));
    // The seek ends once the values interrupted got to To.
    ui.advance_time_by(millis(1_500));
    ui.wait_for_idle();
    assert_eq!(after.get(), 0);
    assert_eq!(seek.get(), Some(true));
}

// Review L4: the play time of a seek is Kotlin's `roundToLong` of the fraction (an `f32`,
// widened) times the total duration: 0.009 of 300 ms is 2 699 999.88 ns, so 2 700 000.
#[test]
fn a_seek_rounds_its_play_time_to_the_nearest_nanosecond() {
    let state = SeekableTransitionState::new(From);
    let (mut ui, transition, _) = running_rule(&state, |_| Box::pin(async {}));
    ui.wait_for_idle();
    let seeking = state.clone();
    let _ = launch(async move { seeking.seek_to_with(0.0, To).await });
    ui.advance_time_by_frame();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to(0.009).await });
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    assert_eq!(transition.borrow().as_ref().unwrap().play_time_nanos(), 2_700_000);
}

// Review L4: a seek takes the transition's total duration with a value added while measuring
// (a visibility's deferred fade). The duration the state keeps is the same by then: its
// observer hears the write at once (frame requests are delivered as the write happens).
#[test]
fn a_seek_takes_the_total_duration_as_it_is() {
    use compose_foundation::layout::*;
    let state = SeekableTransitionState::new(From);
    let watched = watched::<Transition<AnimStates>>();
    let (seen, given) = (watched.clone(), state.clone());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        let transition = remember_transition(&given, "Test");
        *seen.borrow_mut() = Some(transition.clone());
        let linear = tween(300).easing(LINEAR_EASING);
        transition.AnimatedVisibility(
            |state| *state == To,
            Modifier,
            fade_in_with(linear.clone(), 0.0),
            fade_out_with(linear, 0.0),
            |_| Box!(modifier = Modifier.fill_max_size()),
        );
    });
    ui.wait_for_idle();
    let seeking = state.clone();
    let _ = launch(async move { seeking.seek_to_with(0.0, To).await });
    // The composition of To; the fade joins while measuring.
    ui.advance_time_by_frame();
    let seeking = state.clone();
    let seek = launch(async move { seeking.seek_to(0.5).await });
    ui.wait_for_idle();
    assert_eq!(seek.get(), Some(true));
    assert_eq!(watched.borrow().as_ref().unwrap().play_time_nanos(), 150_000_000);
}

// Compose: compose/animation/animation/src/androidDeviceTest/kotlin/androidx/compose/animation/AnimatedVisibilityTest.kt, testSeekingAnimatedVisibility
#[test]
fn a_visibility_seeks_along_its_enter_and_its_exit() {
    fn spec() -> AnimationSpec {
        tween(200).easing(LINEAR_EASING).into()
    }
    let seeking = SeekableTransitionState::new(false);
    let shown = seeking.clone();
    let root: Rc<RefCell<Option<Transition<bool>>>> = Rc::default();
    let kept = root.clone();
    let mut ui = compose_test(move || {
        let transition = remember_transition(&shown, "test");
        *kept.borrow_mut() = Some(transition.clone());
        transition.AnimatedVisibility(
            |visible: &bool| *visible,
            Modifier,
            fade_in_with(spec(), 0.0) + slide_in_vertically_with(spec(), |_| 200),
            scale_out_with(spec(), 0.0, TransformOrigin::CENTER)
                + shrink_horizontally_with(spec(), Alignment::END, true, |_| 0),
            |_| Box!(modifier = Modifier.size(200.dp())),
        );
    });
    ui.wait_for_idle();
    fn value<T: Animate>(transition: &Transition<bool>, label: &str) -> T {
        transition.transitions()[0]
            .animations()
            .into_iter()
            .find(|animation| animation.label().contains(label))
            .and_then(|animation| animation.value::<T>())
            .unwrap_or_else(|| panic!("a {label}"))
    }
    let transition = root.borrow().clone().unwrap();
    // Seeking the enter.
    for time in (0..=200).step_by(20) {
        let seek = seeking.clone();
        let fraction = time as f32 / 200.0;
        block_on(&mut ui, async move { seek.seek_to_with(fraction, true).await });
        ui.wait_for_idle();
        assert_eq!(transition.total_duration_nanos(), 200_000_000);
        let alpha: f32 = value(&transition, "alpha");
        assert!((alpha - fraction).abs() <= 0.01, "{alpha} at {time}");
        assert_eq!(value::<IntOffset>(&transition, "slide"), IntOffset::new(0, 200 - time));
    }
    assert_eq!(value::<f32>(&transition, "alpha"), 1.0);
    let settle = seeking.clone();
    block_on(&mut ui, async move { settle.snap_to(true).await });
    ui.wait_for_idle();
    // Seeking the exit.
    for time in (0..=200).step_by(20) {
        let seek = seeking.clone();
        let fraction = time as f32 / 200.0;
        block_on(&mut ui, async move { seek.seek_to_with(fraction, false).await });
        ui.wait_for_idle();
        assert_eq!(transition.total_duration_nanos(), 200_000_000);
        let scale: f32 = value(&transition, "scale");
        assert!((scale - (1.0 - fraction)).abs() <= 0.01, "{scale} at {time}");
        assert_eq!(value::<IntSize>(&transition, "shrink"), IntSize::new(200 - time, 200));
    }
    assert_eq!(value::<f32>(&transition, "scale"), 0.0);
    assert_eq!(value::<IntSize>(&transition, "shrink"), IntSize::new(0, 200));
}
