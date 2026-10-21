//! `update_transition`, as Compose's `updateTransition`: values animating together when one
//! state changes, each from its value for the state before to its value for the new one.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{OwnedState, SideEffect, State, remember, remember_coroutine_scope};
use compose_ui::{Color, Dp, Offset};

use crate::animatable::{Animatable, AnimationEndReason};
use crate::spec::{AnimationSpec, spring};
use crate::vector::Animate;

/// A state's change animating values together, as Compose's `Transition`.
pub struct Transition<S: 'static> {
    inner: Rc<TransitionInner<S>>,
}

impl<S: 'static> Clone for Transition<S> {
    fn clone(&self) -> Self {
        Transition { inner: self.inner.clone() }
    }
}

struct TransitionInner<S: 'static> {
    /// The state the values are at (once their animations end), as `currentState`.
    current: OwnedState<S>,
    /// The state they're going to, as `targetState`.
    target: RefCell<S>,
    /// Animations running toward the target (state: `is_running` readers follow it).
    running: OwnedState<usize>,
}

/// A transition following `target_state`, remembered at its call site, as
/// `updateTransition(targetState, label)`: its values animate whenever it changes.
pub fn update_transition<S: Clone + PartialEq + 'static>(
    target_state: S,
    label: &'static str,
) -> Transition<S> {
    let _ = label;
    let transition = remember(|| Transition {
        inner: Rc::new(TransitionInner {
            current: compose_runtime::mutable_state_of(target_state.clone()),
            target: RefCell::new(target_state.clone()),
            running: compose_runtime::mutable_state_of(0),
        }),
    });
    let inner = transition.inner.clone();
    let scope = remember_coroutine_scope();
    // The target as of this pass (a plain cell: what the animations below go to).
    *inner.target.borrow_mut() = target_state.clone();
    SideEffect(move || {
        // Nothing to animate (or all done already): at the target at once, once the
        // animations of this pass have launched.
        let inner = inner.clone();
        scope.launch(async move {
            if inner.running.get() == 0 {
                let target = inner.target.borrow().clone();
                if inner.current.get() != target {
                    inner.current.set(target);
                }
            }
        });
    });
    transition
}

impl<S: Clone + PartialEq + 'static> Transition<S> {
    /// The state its values are at, as `currentState` (a state read: it changes when the
    /// animations end).
    pub fn current_state(&self) -> S {
        self.inner.current.get()
    }

    /// The state it goes to, as `targetState`.
    pub fn target_state(&self) -> S {
        self.inner.target.borrow().clone()
    }

    /// Whether its values are animating, as `isRunning` (a state read).
    pub fn is_running(&self) -> bool {
        self.inner.running.get() > 0
    }

    /// A value animating to `target_value_by_state(target)` whenever the target changes, along
    /// `transition_spec(initial, target)` (a default spring when it interrupts a running
    /// animation of another kind), as `Transition.animateValue`. Remembered at its call
    /// site: call it on every pass, as a composable.
    pub fn animate_value<T: Animate>(
        &self,
        transition_spec: impl Fn(&S, &S) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> T,
    ) -> State<T> {
        let target_state = self.target_state();
        let target_value = target_value_by_state(&target_state);
        let animatable = remember(|| Animatable::new(target_value.clone()));
        let launched = remember(|| Rc::new(RefCell::new(target_value.clone())));
        let scope = remember_coroutine_scope();
        let state = animatable.as_state();
        // The spec of a change from the state the values are at.
        let changed = !crate::vector::same_value(&*launched.borrow(), &target_value);
        let spec = changed.then(|| transition_spec(&self.inner.current.get(), &target_state));
        let running = animatable.clone();
        let inner = self.inner.clone();
        SideEffect(move || {
            let Some(spec) = spec else { return };
            if crate::vector::same_value(&*launched.borrow(), &target_value) {
                return;
            }
            *launched.borrow_mut() = target_value.clone();
            // Interrupted mid-way: on from where it is, on a spring (unless it's one already),
            // as Compose's `interruptionSpec`.
            let spec = match spec {
                AnimationSpec::Spring(_) => spec,
                _ if running.is_running() => AnimationSpec::Spring(spring()),
                _ => spec,
            };
            inner.running.set(inner.running.get() + 1);
            scope.launch(async move {
                let result = animatable.animate_to(target_value, spec).await;
                inner.running.set(inner.running.get().saturating_sub(1));
                // The last of the transition's animations to end puts it at its target.
                if result.end_reason == AnimationEndReason::Finished && inner.running.get() == 0 {
                    let target = inner.target.borrow().clone();
                    inner.current.set(target);
                }
            });
        });
        state
    }

    /// As `Transition.animateFloat`.
    pub fn animate_float(
        &self,
        transition_spec: impl Fn(&S, &S) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> f32,
    ) -> State<f32> {
        self.animate_value(transition_spec, target_value_by_state)
    }

    /// As `Transition.animateDp`.
    pub fn animate_dp(
        &self,
        transition_spec: impl Fn(&S, &S) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Dp,
    ) -> State<Dp> {
        self.animate_value(transition_spec, target_value_by_state)
    }

    /// As `Transition.animateColor`.
    pub fn animate_color(
        &self,
        transition_spec: impl Fn(&S, &S) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Color,
    ) -> State<Color> {
        self.animate_value(transition_spec, target_value_by_state)
    }

    /// As `Transition.animateOffset`.
    pub fn animate_offset(
        &self,
        transition_spec: impl Fn(&S, &S) -> AnimationSpec,
        target_value_by_state: impl Fn(&S) -> Offset,
    ) -> State<Offset> {
        self.animate_value(transition_spec, target_value_by_state)
    }
}
