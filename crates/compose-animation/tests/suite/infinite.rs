//! Infinite transitions driven by one task each (D420), against the task-per-value
//! implementation they replaced: the same values at every frame, values added and changed
//! mid-way, and nothing running once they leave the composition.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::time::Duration;

use compose_animation::*;
use compose_runtime::platform;
use compose_runtime::*;
use compose_test::*;
use compose_ui::Color;

/// The implementation D420 replaced, word for word: a task per value, each an `Animatable`
/// snapped to `initial_value` and animated to `target_value`, restarted when they or the spec
/// change.
fn reference_animate_value<T: Animate + ComposeArg>(
    initial_value: T,
    target_value: T,
    animation_spec: RepeatableSpec,
) -> State<T> {
    let animatable = remember(|| Animatable::new(initial_value.clone()));
    let state = animatable.as_state();
    let keys = (initial_value.clone(), target_value.clone(), animation_spec);
    LaunchedEffect(keys, async move {
        animatable.snap_to(initial_value);
        animatable.animate_to(target_value, animation_spec).await;
    });
    state
}

thread_local! {
    /// What each comparison saw, per name: the values its compositions read.
    static SEEN: RefCell<Vec<(&'static str, [f32; 4])>> = const { RefCell::new(Vec::new()) };
    static COMPARED: Cell<usize> = const { Cell::new(0) };
}

/// A value of `transition` beside the reference's: whenever either changes, this composition
/// reads both, which must be the same.
fn compare<T: Animate + ComposeArg + std::fmt::Debug>(
    name: &'static str,
    transition: &InfiniteTransition,
    initial_value: T,
    target_value: T,
    animation_spec: RepeatableSpec,
) {
    let value =
        transition.animate_value(initial_value.clone(), target_value.clone(), animation_spec);
    let reference = reference_animate_value(initial_value, target_value, animation_spec);
    let (value, reference) = (value.get(), reference.get());
    assert!(
        value
            .to_vector()
            .iter()
            .zip(reference.to_vector())
            .all(|(a, b)| *a == b || (a.is_nan() && b.is_nan())),
        "{name} at {} ms: {value:?}, the reference {reference:?}",
        platform::now_nanos() / 1_000_000
    );
    COMPARED.set(COMPARED.get() + 1);
    SEEN.with(|seen| seen.borrow_mut().push((name, value.to_vector())));
}

fn seen(name: &str) -> Vec<[f32; 4]> {
    SEEN.with(|seen| {
        seen.borrow().iter().filter(|(seen, _)| *seen == name).map(|(_, value)| *value).collect()
    })
}

/// Frames 16 ms apart, `count` of them.
fn frames(ui: &mut ComposeTestRule, count: usize) {
    for _ in 0..count {
        ui.advance_time_by_frame();
    }
}

/// Each kind of spec, a value changing its target mid-way, and one added, removed and added
/// again, all in one transition.
#[composable]
fn Compared(target: MutableState<f32>, extra: MutableState<bool>) {
    let transition = remember_infinite_transition!();
    compare(
        "linear restart",
        &transition,
        0.0f32,
        360.0,
        infinite_repeatable(tween(1500).easing(LINEAR_EASING), RepeatMode::Restart),
    );
    compare(
        "eased reverse",
        &transition,
        0.6f32,
        1.0,
        infinite_repeatable(tween(700).delay(50), RepeatMode::Reverse),
    );
    compare(
        "keyframes",
        &transition,
        0.0f32,
        100.0,
        infinite_repeatable(
            keyframes(400, |frames| {
                frames.at(100, 80.0f32).with(EASE_IN);
            }),
            RepeatMode::Reverse,
        ),
    );
    compare(
        "three times",
        &transition,
        0.0f32,
        10.0,
        repeatable(3, tween(200), RepeatMode::Reverse),
    );
    compare(
        "color",
        &transition,
        Color::RED,
        Color::BLUE,
        infinite_repeatable(tween(900), RepeatMode::Reverse),
    );
    compare(
        "changing",
        &transition,
        0.0f32,
        target.get(),
        infinite_repeatable(tween(300), RepeatMode::Reverse),
    );
    if extra.get() {
        Extra(transition);
    }
}

#[composable]
fn Extra(transition: InfiniteTransition) {
    compare(
        "extra",
        &transition,
        -5.0f32,
        5.0,
        infinite_repeatable(tween(250).easing(LINEAR_EASING), RepeatMode::Reverse),
    );
}

#[test]
fn an_infinite_transition_sets_the_values_its_values_tasks_did() {
    SEEN.with(|seen| seen.borrow_mut().clear());
    COMPARED.set(0);
    let target = mutable_state_of(50.0f32);
    let extra = mutable_state_of(false);
    let (target_handle, extra_handle) = (target.handle(), extra.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || Compared(target_handle, extra_handle));
    for frame in 0..240 {
        match frame {
            40 => target.set(-20.0),
            60 => extra.set(true),
            100 => extra.set(false),
            130 => extra.set(true),
            150 => target.set(80.0),
            _ => {}
        }
        // Frames 16 ms apart, and some at other intervals.
        if frame % 7 == 3 {
            ui.advance_time_by(Duration::from_millis(5 + frame % 11));
        } else {
            ui.advance_time_by_frame();
        }
    }
    assert!(COMPARED.get() > 6 * 200, "compared {} times", COMPARED.get());
    // They moved.
    for name in ["linear restart", "eased reverse", "keyframes", "color", "changing", "extra"] {
        let values = seen(name);
        let distinct = values.windows(2).filter(|pair| pair[0] != pair[1]).count();
        assert!(distinct > 30, "{name} moved {distinct} times: {values:?}");
    }
    // The finite one ended at its target.
    assert_eq!(seen("three times").last(), Some(&[10.0, 0.0, 0.0, 0.0]));
}

thread_local! {
    static VALUE: Cell<Option<State<f32>>> = const { Cell::new(None) };
}

#[composable]
fn Shown(shown: MutableState<bool>, spec: RepeatableSpec) {
    if shown.get() {
        Pulsing(spec);
    }
}

#[composable]
fn Pulsing(spec: RepeatableSpec) {
    let transition = remember_infinite_transition!();
    VALUE.set(Some(transition.animate_float(0.0, 1.0, spec)));
}

/// A transition leaving the composition stops: its task is cancelled, and no frame is asked for.
#[test]
fn an_infinite_transition_leaving_the_composition_stops() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let spec = infinite_repeatable(tween(100), RepeatMode::Reverse);
    let mut ui = ComposeTestRule::with_manual_clock(move || Shown(handle, spec));
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(platform::has_frame_awaiters(), "it animates");
    shown.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    assert!(!platform::has_frame_awaiters(), "nothing animates");
    assert!(!platform::has_ready_tasks());
    // Shown again: it animates again, from its start (the frame composing it, the first frame
    // of its animation, then 16 ms in).
    shown.set(true);
    frames(&mut ui, 3);
    assert!(platform::has_frame_awaiters());
    let value = VALUE.get().expect("composed").get();
    assert!(value > 0.0 && value < 0.5, "{value}");
}

/// A transition whose values all ended asks for no frame: idle, until a value starts again.
#[test]
fn an_infinite_transition_whose_values_ended_is_idle() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let spec = repeatable(2, tween(100), RepeatMode::Reverse);
    let mut ui = ComposeTestRule::with_manual_clock(move || Shown(handle, spec));
    frames(&mut ui, 20);
    assert!(!platform::has_frame_awaiters(), "the values ended");
    assert_eq!(VALUE.get().expect("composed").get(), 1.0);
}

thread_local! {
    static CHANGED: Cell<Option<State<f32>>> = const { Cell::new(None) };
}

#[composable]
fn Restarted(target: MutableState<f32>) {
    let transition = remember_infinite_transition!();
    CHANGED.set(Some(transition.animate_float(
        0.0,
        target.get(),
        repeatable(1, tween(100).easing(LINEAR_EASING), RepeatMode::Restart),
    )));
}

/// A value that ended starts again when its target changes, waking its idle transition.
#[test]
fn an_ended_value_changed_animates_again() {
    let target = mutable_state_of(10.0f32);
    let handle = target.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || Restarted(handle));
    frames(&mut ui, 10);
    let value = || CHANGED.get().expect("composed").get();
    assert_eq!(value(), 10.0);
    assert!(!platform::has_frame_awaiters());
    target.set(20.0);
    frames(&mut ui, 3);
    assert!(value() > 0.0 && value() < 20.0, "{}", value());
    frames(&mut ui, 10);
    assert_eq!(value(), 20.0);
    assert!(!platform::has_frame_awaiters());
}

// ---- From the adversarial review --------------------------------------------------------------

thread_local! {
    static MOVED: Cell<Option<State<f32>>> = const { Cell::new(None) };
    static MOVED_REF: Cell<Option<State<f32>>> = const { Cell::new(None) };
}

/// A value whose call moves from transition A (which then leaves the composition) to B.
#[composable]
fn MovingBetween(use_b: MutableState<bool>) {
    let b = remember_infinite_transition!();
    let a = if !use_b.get() { Some(remember_infinite_transition!()) } else { None };
    let transition = a.unwrap_or(b);
    let spec = infinite_repeatable(tween(500).easing(LINEAR_EASING), RepeatMode::Restart);
    MOVED.set(Some(transition.animate_float(0.0, 1.0, spec)));
    MOVED_REF.set(Some(reference_animate_value(0.0f32, 1.0, spec)));
}

#[test]
fn a_value_moving_to_another_transition_keeps_animating() {
    let use_b = mutable_state_of(false);
    let handle = use_b.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || MovingBetween(handle));
    frames(&mut ui, 5);
    use_b.set(true);
    frames(&mut ui, 2);
    let before = MOVED.get().unwrap().get();
    let reference_before = MOVED_REF.get().unwrap().get();
    frames(&mut ui, 5);
    let after = MOVED.get().unwrap().get();
    let reference_after = MOVED_REF.get().unwrap().get();
    assert_ne!(reference_before, reference_after, "the reference moves");
    assert_ne!(before, after, "the value froze once its first transition left");
    // It went on where it was: the same values as before.
    assert_eq!((before, after), (reference_before, reference_after));
}

/// Changes of every kind, at odd times: initial value, spec, a finite value finishing then
/// changed, a NaN and an infinite target, keyed children reordered.
#[composable]
fn ComparedChanges(step: MutableState<u32>, order: MutableState<bool>) {
    let transition = remember_infinite_transition!();
    let step = step.get();
    let initial = if step % 3 == 1 { 5.0f32 } else { 0.0 };
    let duration = if step % 4 == 2 { 300 } else { 220 };
    compare(
        "initial and spec",
        &transition,
        initial,
        40.0,
        infinite_repeatable(tween(duration), RepeatMode::Reverse),
    );
    compare(
        "finite changed",
        &transition,
        0.0f32,
        if step >= 3 { 7.0 } else { 3.0 },
        repeatable(2, tween(100).easing(LINEAR_EASING), RepeatMode::Restart),
    );
    compare(
        "nan",
        &transition,
        0.0f32,
        if step % 2 == 1 { f32::NAN } else { 9.0 },
        infinite_repeatable(tween(200), RepeatMode::Reverse),
    );
    compare(
        "infinite",
        &transition,
        0.0f32,
        if step == 4 { f32::INFINITY } else { 2.0 },
        infinite_repeatable(tween(200), RepeatMode::Restart),
    );
    let names: [(u64, &'static str); 2] = [(1, "keyed one"), (2, "keyed two")];
    let ordered: Vec<(u64, &'static str)> =
        if order.get() { names.iter().rev().copied().collect() } else { names.to_vec() };
    for (id, name) in ordered {
        key(id, || {
            compare(
                name,
                &transition,
                id as f32,
                10.0 * id as f32,
                infinite_repeatable(tween(150 * id as u32), RepeatMode::Reverse),
            );
        });
    }
}

#[test]
fn values_changed_every_way_match_the_reference() {
    COMPARED.set(0);
    let step = mutable_state_of(0u32);
    let order = mutable_state_of(false);
    let (step_handle, order_handle) = (step.handle(), order.handle());
    let mut ui =
        ComposeTestRule::with_manual_clock(move || ComparedChanges(step_handle, order_handle));
    for frame in 0..300u64 {
        if frame % 37 == 11 {
            step.set(step.get() + 1);
        }
        if frame % 53 == 20 {
            order.set(!order.get());
        }
        if frame % 5 == 2 {
            ui.advance_time_by(Duration::from_millis(3 + frame % 13));
        } else {
            ui.advance_time_by_frame();
        }
    }
    assert!(COMPARED.get() > 1000, "compared {}", COMPARED.get());
}

#[composable]
fn ChildToggled(shown: MutableState<bool>) {
    let transition = remember_infinite_transition!();
    if shown.get() {
        ChildOf(transition);
    }
}

#[composable]
fn ChildOf(transition: InfiniteTransition) {
    let _ = transition
        .animate_float(0.0, 1.0, infinite_repeatable(tween(100), RepeatMode::Reverse))
        .get();
}

#[test]
fn a_transition_whose_only_value_left_goes_idle_and_restarts() {
    let shown = mutable_state_of(true);
    let handle = shown.handle();
    let mut ui = ComposeTestRule::with_manual_clock(move || ChildToggled(handle));
    frames(&mut ui, 3);
    assert!(platform::has_frame_awaiters());
    shown.set(false);
    frames(&mut ui, 3);
    assert!(!platform::has_frame_awaiters(), "the transition went idle");
    shown.set(true);
    frames(&mut ui, 2);
    assert!(platform::has_frame_awaiters(), "animating again");
    shown.set(false);
    frames(&mut ui, 3);
    assert!(!platform::has_frame_awaiters(), "idle again");
}
