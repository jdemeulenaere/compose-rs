//! `DeferredTransitionState` and `remember_deferred_transition` against Compose's
//! `DeferredTransitionTest`: a target deferred is pending, its transition held; `animate_to` ends
//! the wait, and a running animation goes on meanwhile.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_animation::*;
use compose_test::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum TestState {
    A,
    B,
    C,
}
use TestState::*;

type Held<T> = Rc<RefCell<Option<T>>>;

/// A rule with a deferred transition at `A`, handing it out.
fn at_a(
    extra: impl Fn(&DeferredTransition<TestState>) + 'static,
) -> (ComposeTestRule, DeferredTransitionState<TestState>, Held<DeferredTransition<TestState>>) {
    let state = DeferredTransitionState::new(A);
    let held: Held<DeferredTransition<TestState>> = Rc::default();
    let (remembered, shared) = (state.clone(), held.clone());
    let ui = compose_test(move || {
        let transition = remember_deferred_transition(&remembered, None);
        extra(&transition);
        *shared.borrow_mut() = Some(transition);
    });
    (ui, state, held)
}

fn millis(millis: u64) -> std::time::Duration {
    std::time::Duration::from_millis(millis)
}

fn close(actual: f32, expected: f32, tolerance: f32) {
    assert!((actual - expected).abs() <= tolerance, "{actual}, Compose's {expected}");
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_initialStateSetImmediately_updatesOnlyWhenReady
#[test]
fn a_deferred_target_is_pending_until_animated_to() {
    let (mut ui, state, held) = at_a(|_| {});
    let read = |ui: &mut ComposeTestRule| {
        ui.wait_for_idle();
        let held = held.borrow();
        let transition = held.as_ref().unwrap();
        (transition.current_state(), transition.target_state(), transition.pending_target_state())
    };
    assert_eq!(read(&mut ui), (A, A, None));
    state.defer(B);
    assert_eq!(read(&mut ui), (A, A, Some(B)));
    state.animate_to(B);
    let (_, target, pending) = read(&mut ui);
    assert_eq!((target, pending), (B, None));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_pendingState_isOverriddenByNewUpdates
#[test]
fn a_new_deferred_target_replaces_the_pending_one() {
    let (mut ui, state, held) = at_a(|_| {});
    state.defer(B);
    ui.wait_for_idle();
    state.defer(C);
    ui.wait_for_idle();
    let held = held.borrow();
    let transition = held.as_ref().unwrap();
    assert_eq!(transition.pending_target_state(), Some(C));
    assert_eq!((transition.current_state(), transition.target_state()), (A, A));
}

/// An int going to 1000 over a second, linearly, in every state but `A`.
fn linear_value(seen: Rc<Cell<i32>>) -> impl Fn(&DeferredTransition<TestState>) + 'static {
    move |transition| {
        let value = transition.animate_int(
            |_| tween(1000).easing(LINEAR_EASING).into(),
            |state| if *state == A { 0 } else { 1000 },
        );
        seen.set(value.get());
    }
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_interruption_continuesCurrentAnimationUntilReady
#[test]
fn a_deferred_interruption_lets_the_running_animation_go_on_until_animated_to() {
    let value = Rc::new(Cell::new(-1));
    let (mut ui, state, held) = at_a(linear_value(value.clone()));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    state.animate_to(B);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(500));
    close(value.get() as f32, 500.0, 20.0);
    state.defer(A);
    ui.advance_time_by_frame();
    {
        let held = held.borrow();
        let transition = held.as_ref().unwrap();
        assert_eq!(transition.pending_target_state(), Some(A));
        assert_eq!(transition.target_state(), B);
    }
    ui.advance_time_by(millis(200));
    close(value.get() as f32, 700.0, 20.0);
    state.animate_to(A);
    ui.advance_time_by_frame();
    assert_eq!(held.borrow().as_ref().unwrap().target_state(), A);
    ui.advance_time_by(millis(2000));
    assert_eq!(value.get(), 0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_childTransition_defersSimultaneouslyWithParent
// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_childTransition_inheritsPendingState
#[test]
fn a_child_transition_has_the_pending_target_of_its_parent_mapped() {
    let child: Held<Transition<bool>> = Rc::default();
    let shared = child.clone();
    let (mut ui, state, _held) = at_a(move |transition| {
        let made = transition.create_child_transition(None, |state| *state == B);
        *shared.borrow_mut() = Some(made);
    });
    let read = |ui: &mut ComposeTestRule| {
        ui.wait_for_idle();
        let child = child.borrow();
        let child = child.as_ref().unwrap();
        (child.target_state(), child.pending_target_state())
    };
    assert_eq!(read(&mut ui), (false, None));
    state.defer(B);
    assert_eq!(read(&mut ui), (false, Some(true)));
    state.animate_to(B);
    assert_eq!(read(&mut ui), (true, None));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_animateFloat_respectsTransitionSpecAfterRelease
#[test]
fn the_transition_spec_applies_once_animated_to() {
    let value = Rc::new(Cell::new(0.0f32));
    let seen = value.clone();
    let (mut ui, state, _held) = at_a(move |transition| {
        let animated = transition.animate_float(
            |segment| {
                if segment.is_transitioning_to(&A, &B) {
                    tween(100).easing(LINEAR_EASING).into()
                } else {
                    snap(0).into()
                }
            },
            |state| if *state == B { 1.0 } else { 0.0 },
        );
        seen.set(animated.get());
    });
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    state.defer(B);
    ui.advance_time_by_frame();
    assert_eq!(value.get(), 0.0);
    state.animate_to(B);
    ui.advance_time_by_frame();
    ui.advance_time_by(millis(50));
    assert!(value.get() > 0.0 && value.get() < 1.0, "{}", value.get());
    ui.advance_time_by(millis(100));
    assert_eq!(value.get(), 1.0);
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_animateToThirdState_overridesPendingState
#[test]
fn animating_to_another_state_overrides_the_pending_target() {
    let (mut ui, state, held) = at_a(|_| {});
    state.defer(B);
    ui.wait_for_idle();
    state.animate_to(C);
    ui.wait_for_idle();
    let held = held.borrow();
    let transition = held.as_ref().unwrap();
    assert_eq!((transition.target_state(), transition.pending_target_state()), (C, None));
}

// Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/DeferredTransitionTest.kt, deferredTransition_deferNewStateWhileAnimating
#[test]
fn deferring_a_new_state_while_animating_leaves_the_animation_running() {
    let value = Rc::new(Cell::new(-1));
    let (mut ui, state, held) = at_a(linear_value(value.clone()));
    ui.set_auto_advance(false);
    ui.wait_for_idle();
    state.animate_to(B);
    ui.advance_time_by(millis(500));
    close(value.get() as f32, 500.0, 20.0);
    state.defer(C);
    ui.advance_time_by_frame();
    {
        let held = held.borrow();
        let transition = held.as_ref().unwrap();
        assert_eq!((transition.target_state(), transition.pending_target_state()), (B, Some(C)));
    }
    ui.advance_time_by(millis(100));
    close(value.get() as f32, 600.0, 20.0);
}
