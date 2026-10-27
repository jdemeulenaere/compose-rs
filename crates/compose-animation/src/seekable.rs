//! `SeekableTransitionState`, as Compose's: a transition's state an app seeks to a fraction of
//! the change (a gesture), animates to its end or snaps.
//!
//! As Compose's: a transition remembered with it ([`remember_transition`](crate::remember_transition))
//! has no frame loop of its own; `seek_to`, `animate_to` and `snap_to` set its target and its
//! values' play time (`seekAnimations`), the fraction of its total duration. A change of target
//! interrupting one under way makes the values go on from where they are to their new targets
//! along an initial-value animation, while the new change starts at fraction 0. The functions
//! are suspending, one at a time (Compose's `MutatorMutex`): a call interrupts the one running,
//! which returns `false`; they wait for the composition after a change of target, as Compose's.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};

use compose_runtime::platform::ReadObserver;
use compose_runtime::{ComposeArg, OwnedState, SideEffect, mutable_state_of, with_frame_nanos};

use crate::spec::{AnimationConstants, AnimationSpec, VectorizedAnimationSpec};
use crate::transition::{
    RESET_ANIMATION_SNAP, RESET_ANIMATION_SNAP_CURRENT, RESET_ANIMATION_SNAP_TARGET,
    SeekingAnimationState, Transition, TransitionInner,
};
use crate::vector::AnimationVector;

/// A transition's state an app seeks, as Compose's `SeekableTransitionState`: a handle, its
/// clones sharing the state.
pub struct SeekableTransitionState<S: 'static> {
    inner: Rc<SeekableInner<S>>,
}

struct SeekableInner<S: 'static> {
    current: OwnedState<S>,
    current_untracked: RefCell<S>,
    target: OwnedState<S>,
    target_untracked: RefCell<S>,
    fraction: OwnedState<f32>,
    fraction_untracked: Cell<f32>,
    is_running: OwnedState<bool>,
    transition: RefCell<Option<Weak<TransitionInner<S>>>>,
    /// The target the last composition saw, as `composedTargetState`.
    composed_target: RefCell<S>,
    /// The current and target states the last composition saw.
    composed: RefCell<Option<(S, S)>>,
    /// Bumped when a composition sees a new current or target state.
    compositions: Cell<u64>,
    /// What waits for such a composition.
    composition_waiters: RefCell<Vec<Waker>>,
    total_duration_nanos: Cell<i64>,
    /// Hears a change of the total duration outside a composition (a deferred value added at
    /// layout), as Compose's `snapshotStateObserver` of `totalDurationNanos`.
    duration_observer: RefCell<Option<ReadObserver>>,
    /// The call running, as the `MutatorMutex`: a new one bumps it, and the one running stops.
    mutation: Cell<u64>,
    /// The task of the call running: a new call wakes it, and it stops at once, as a
    /// cancelled coroutine.
    holder: RefCell<Option<Waker>>,
    last_frame_time_nanos: Cell<i64>,
    initial_value_animations: RefCell<Vec<Rc<SeekingAnimationState>>>,
    current_animation: RefCell<Option<Rc<SeekingAnimationState>>>,
}

impl<S: 'static> Clone for SeekableTransitionState<S> {
    fn clone(&self) -> Self {
        SeekableTransitionState { inner: self.inner.clone() }
    }
}

impl<S: 'static> ComposeArg for SeekableTransitionState<S> {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<S: 'static> PartialEq for SeekableTransitionState<S> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

impl<S: 'static> std::fmt::Debug for SeekableTransitionState<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SeekableTransitionState")
    }
}

/// The call was interrupted by another of the state's: Compose's `CancellationException`.
struct Interrupted;

type Step<T = ()> = Result<T, Interrupted>;

impl<S: Clone + PartialEq + 'static> SeekableTransitionState<S> {
    /// A state at `initial_state`, as `SeekableTransitionState(initialState)`.
    pub fn new(initial_state: S) -> SeekableTransitionState<S> {
        SeekableTransitionState {
            inner: Rc::new(SeekableInner {
                current: mutable_state_of(initial_state.clone()),
                current_untracked: RefCell::new(initial_state.clone()),
                target: mutable_state_of(initial_state.clone()),
                target_untracked: RefCell::new(initial_state.clone()),
                fraction: mutable_state_of(0.0),
                fraction_untracked: Cell::new(0.0),
                is_running: mutable_state_of(false),
                transition: RefCell::new(None),
                composed_target: RefCell::new(initial_state),
                composed: RefCell::new(None),
                compositions: Cell::new(0),
                composition_waiters: RefCell::new(Vec::new()),
                total_duration_nanos: Cell::new(0),
                duration_observer: RefCell::new(None),
                mutation: Cell::new(0),
                holder: RefCell::new(None),
                last_frame_time_nanos: Cell::new(AnimationConstants::UNSPECIFIED_TIME),
                initial_value_animations: RefCell::new(Vec::new()),
                current_animation: RefCell::new(None),
            }),
        }
    }

    /// Where the transition is, as `currentState` (a state read).
    pub fn current_state(&self) -> S {
        self.inner.current.get()
    }

    /// Where it goes, as `targetState` (a state read).
    pub fn target_state(&self) -> S {
        self.inner.target.get()
    }

    /// How far along the change from the current state to the target it is, 0 to 1, as
    /// `fraction` (a state read).
    pub fn fraction(&self) -> f32 {
        self.inner.fraction.get()
    }

    /// Whether its transition's values animate or are seeked away from rest, as `isRunning`.
    pub fn is_running(&self) -> bool {
        self.inner.is_running.get()
    }

    pub(crate) fn current_untracked(&self) -> S {
        self.inner.current_untracked.borrow().clone()
    }

    fn target_untracked(&self) -> S {
        self.inner.target_untracked.borrow().clone()
    }

    pub(crate) fn set_current(&self, current: S) {
        *self.inner.current_untracked.borrow_mut() = current.clone();
        self.inner.current.set(current);
    }

    fn set_target(&self, target: S) {
        *self.inner.target_untracked.borrow_mut() = target.clone();
        self.inner.target.set(target);
    }

    fn set_fraction(&self, fraction: f32) {
        self.inner.fraction_untracked.set(fraction);
        self.inner.fraction.set(fraction);
    }

    pub(crate) fn set_running(&self, running: bool) {
        self.inner.is_running.set(running);
    }

    fn transition(&self) -> Option<Rc<TransitionInner<S>>> {
        self.inner.transition.borrow().as_ref().and_then(Weak::upgrade)
    }

    /// The transition remembered with it, as `transitionConfigured`.
    pub(crate) fn transition_configured(&self, transition: Weak<TransitionInner<S>>) {
        *self.inner.transition.borrow_mut() = Some(transition);
    }

    /// As `transitionRemoved`.
    pub(crate) fn transition_removed(&self) {
        *self.inner.transition.borrow_mut() = None;
        // Stops observing, as Compose's `snapshotStateObserver.clear()`.
        self.inner.duration_observer.borrow_mut().take();
    }

    /// The composition of the transition's caller: it reads the current and target states (so
    /// a change composes again), and once applied, a new pair resumes what waits for it (as
    /// Compose's `LaunchedEffect(currentState, targetState)`) and a new total duration is
    /// taken (its observer of `totalDurationNanos`).
    pub(crate) fn composed(&self, transition: &Transition<S>) {
        let pair = (self.current_state(), self.target_state());
        let state = self.clone();
        let _ = transition;
        SideEffect(move || {
            let changed = state.inner.composed.borrow().as_ref() != Some(&pair);
            state.on_total_duration_changed();
            if changed {
                *state.inner.composed_target.borrow_mut() = pair.1.clone();
                *state.inner.composed.borrow_mut() = Some(pair);
                state.inner.compositions.set(state.inner.compositions.get() + 1);
                let waiters = std::mem::take(&mut *state.inner.composition_waiters.borrow_mut());
                for waiter in waiters {
                    waiter.wake();
                }
            }
        });
    }

    /// The transition's total duration changed: the animation running follows it, or the
    /// values the fraction, as `onTotalDurationChanged`.
    fn on_total_duration_changed(&self) {
        let Some(transition) = self.transition() else { return };
        let previous = self.inner.total_duration_nanos.get();
        let total = self.observe_total_duration(&transition);
        self.inner.total_duration_nanos.set(total);
        if previous == total {
            return;
        }
        let animation = self.inner.current_animation.borrow().clone();
        match animation {
            Some(animation) => {
                if animation.progress_nanos.get() > total {
                    self.end_all_animations();
                } else {
                    animation.duration_nanos.set(total);
                    if animation.animation_spec.borrow().is_none() {
                        let rest = (1.0 - f64::from(animation.start.get())) * total as f64;
                        animation.animation_spec_duration.set(round(rest));
                    }
                }
            }
            None if total != 0 => self.seek_to_fraction(),
            None => {}
        }
    }

    /// The transition's total duration, its reads observed: a change calls
    /// `on_total_duration_changed` on the next frame.
    fn observe_total_duration(&self, transition: &TransitionInner<S>) -> i64 {
        let mut observer = self.inner.duration_observer.borrow_mut();
        let observer = observer.get_or_insert_with(|| {
            let inner = Rc::downgrade(&self.inner);
            ReadObserver::new(move || {
                if let Some(inner) = inner.upgrade() {
                    SeekableTransitionState { inner }.on_total_duration_changed();
                }
            })
        });
        let total = observer.observe(0, || transition.total_duration());
        observer.drain_invalidated(|_| {});
        total
    }

    /// The values at the fraction, as `seekToFraction`.
    fn seek_to_fraction(&self) {
        let Some(transition) = self.transition() else { return };
        let total = transition.total_duration();
        let play_time = round(f64::from(self.inner.fraction_untracked.get()) * total as f64);
        transition.seek_animations(play_time);
    }

    fn end_all_animations(&self) {
        if let Some(transition) = self.transition() {
            transition.clear_initial_animations();
        }
        self.inner.initial_value_animations.borrow_mut().clear();
        let current = self.inner.current_animation.borrow_mut().take();
        if current.is_some() {
            self.set_fraction(1.0);
            self.seek_to_fraction();
        }
    }

    /// Starts a call: the one running stops, woken.
    fn mutate(&self) -> u64 {
        let mutation = self.inner.mutation.get() + 1;
        self.inner.mutation.set(mutation);
        if let Some(holder) = self.inner.holder.take() {
            holder.wake();
        }
        mutation
    }

    /// Runs `call`, a public function's body, as Compose's `mutatorMutex.mutate`: once it
    /// started a call (`mutate`), a later call ends it at once, `false`, as Compose cancels
    /// the coroutine; it doesn't wait for its next frame.
    async fn exclusive(&self, call: impl Future<Output = Step>) -> bool {
        let inner = self.inner.clone();
        let mut call = Box::pin(call);
        let mut mine: Option<u64> = None;
        std::future::poll_fn(move |context| {
            if mine.is_some_and(|mine| inner.mutation.get() != mine) {
                return Poll::Ready(false);
            }
            let before = inner.mutation.get();
            let poll = call.as_mut().poll(context);
            if mine.is_none() && inner.mutation.get() != before {
                // It started a call in this poll: no other task runs meanwhile.
                mine = Some(inner.mutation.get());
            }
            let current = mine.is_some_and(|mine| inner.mutation.get() == mine);
            match poll {
                Poll::Ready(step) => {
                    if current {
                        inner.holder.take();
                    }
                    Poll::Ready(step.is_ok())
                }
                Poll::Pending => {
                    if current {
                        *inner.holder.borrow_mut() = Some(context.waker().clone());
                    }
                    Poll::Pending
                }
            }
        })
        .await
    }

    fn check(&self, mutation: u64) -> Step {
        if self.inner.mutation.get() == mutation { Ok(()) } else { Err(Interrupted) }
    }

    /// Jumps to `target_state`, as `snapTo(targetState)`: the values at its values, every
    /// animation stopped. Returns whether it got there (not interrupted by another call).
    pub async fn snap_to(&self, target_state: S) -> bool {
        self.exclusive(self.snap(target_state)).await
    }

    async fn snap(&self, target_state: S) -> Step {
        let Some(transition) = self.transition() else { return Ok(()) };
        if self.current_untracked() == target_state && self.target_untracked() == target_state {
            return Ok(());
        }
        let mutation = self.mutate();
        self.end_all_animations();
        self.inner.last_frame_time_nanos.set(AnimationConstants::UNSPECIFIED_TIME);
        self.set_fraction(0.0);
        let fraction = if target_state == self.current_untracked() {
            RESET_ANIMATION_SNAP_CURRENT
        } else if target_state == self.target_untracked() {
            RESET_ANIMATION_SNAP_TARGET
        } else {
            RESET_ANIMATION_SNAP
        };
        transition.update_target(target_state.clone());
        transition.set_play_time_nanos(0);
        self.set_target(target_state.clone());
        self.set_fraction(0.0);
        self.set_current(target_state);
        transition.reset_animation_fraction(fraction);
        if fraction == RESET_ANIMATION_SNAP {
            // The values change entirely: a composition gives them.
            self.wait_for_composition_after_target_state_change(mutation).await?;
        }
        transition.end();
        Ok(())
    }

    /// Seeks to `fraction` (0 to 1) of the change to its target, as `seekTo(fraction)`.
    /// Returns whether it got there (not interrupted by another call).
    pub async fn seek_to(&self, fraction: f32) -> bool {
        let target = self.target_untracked();
        self.seek_to_with(fraction, target).await
    }

    /// Seeks to `fraction` of the change to `target_state`, as `seekTo(fraction,
    /// targetState)`: a new target starts its change at `fraction`, the values of the change
    /// interrupted going on from where they are. Panics if `fraction` isn't within 0..1.
    pub async fn seek_to_with(&self, fraction: f32, target_state: S) -> bool {
        assert!(
            (0.0..=1.0).contains(&fraction),
            "Expecting fraction between 0 and 1. Got {fraction}"
        );
        self.exclusive(self.seek(fraction, target_state)).await
    }

    async fn seek(&self, fraction: f32, target_state: S) -> Step {
        let Some(transition) = self.transition() else { return Ok(()) };
        let old_target = self.target_untracked();
        let mutation = self.mutate();
        if target_state != old_target {
            self.move_animation_to_initial_state();
        } else {
            *self.inner.current_animation.borrow_mut() = None;
            if self.current_untracked() == target_state {
                // Nothing to seek: at the target.
                return Ok(());
            }
        }
        if target_state != old_target {
            transition.update_target(target_state.clone());
            transition.set_play_time_nanos(0);
            self.set_target(target_state);
            transition.reset_animation_fraction(fraction);
        }
        self.set_fraction(fraction);
        let animating = !self.inner.initial_value_animations.borrow().is_empty();
        if !animating {
            self.inner.last_frame_time_nanos.set(AnimationConstants::UNSPECIFIED_TIME);
        }
        let this = self.clone();
        let wait = async move {
            this.wait_for_composition_after_target_state_change(mutation).await?;
            this.seek_to_fraction();
            Ok(())
        };
        if animating {
            // The initial-value animations run on as it waits, as Compose's `launch`; the call
            // returns once both are done.
            let this = self.clone();
            let run = async move { this.run_animations(mutation).await };
            let (waited, ran) = join(wait, run).await;
            waited?;
            ran
        } else {
            wait.await
        }
    }

    /// Animates to `target_state` from the fraction it's at, as `animateTo(targetState)`:
    /// linearly over the rest of the total duration. Returns whether it got there.
    pub async fn animate_to(&self, target_state: S) -> bool {
        self.animate_to_with(target_state, None).await
    }

    /// The same along `animation_spec` (a spec of the fraction, 0 to 1), as
    /// `animateTo(targetState, animationSpec)`.
    pub async fn animate_to_with(
        &self,
        target_state: S,
        animation_spec: Option<AnimationSpec>,
    ) -> bool {
        self.exclusive(self.animate(target_state, animation_spec)).await
    }

    async fn animate(&self, target_state: S, animation_spec: Option<AnimationSpec>) -> Step {
        let Some(transition) = self.transition() else { return Ok(()) };
        let mutation = self.mutate();
        let old_target = self.target_untracked();
        if target_state != old_target {
            self.move_animation_to_initial_state();
            self.set_fraction(0.0);
            transition.update_target(target_state.clone());
            transition.set_play_time_nanos(0);
            self.set_current(old_target);
            self.set_target(target_state.clone());
        }
        if target_state != *self.inner.composed_target.borrow() {
            // A frame for the composition, then the composition.
            self.do_one_frame(mutation).await?;
            self.wait_for_composition_after_target_state_change(mutation).await?;
        }
        if self.current_untracked() == target_state {
            return Ok(());
        }
        let fraction = self.inner.fraction_untracked.get();
        if fraction < 1.0 {
            self.start_animation(fraction, animation_spec);
        }
        self.run_animations(mutation).await?;
        self.set_current(target_state);
        self.set_fraction(0.0);
        transition.end();
        self.wait_for_composition(mutation).await
    }

    /// The animation of the fraction to 1, continuing the one running (its velocity) if any,
    /// as `animateTo`'s.
    fn start_animation(&self, fraction: f32, spec: Option<AnimationSpec>) {
        let running = self.inner.current_animation.borrow().clone();
        let same_spec =
            running.as_ref().is_some_and(|running| *running.animation_spec.borrow() == spec);
        if running.is_some() && same_spec {
            return;
        }
        let total = self.inner.total_duration_nanos.get();
        let old_velocity = match &running {
            Some(running) => match &*running.animation_spec.borrow() {
                Some(old_spec) => {
                    let velocity =
                        AnimationVector::from(running.initial_velocity.get().unwrap_or(0.0));
                    old_spec.velocity_from_nanos(
                        running.progress_nanos.get(),
                        AnimationVector::from(running.start.get()),
                        AnimationVector::from(1.0),
                        velocity,
                    )[0]
                }
                None if running.progress_nanos.get() == 0 => 0.0,
                None => {
                    let duration = running.duration_nanos.get();
                    let duration = if duration == AnimationConstants::UNSPECIFIED_TIME {
                        total
                    } else {
                        duration
                    };
                    // Kotlin's `Long / Float`: in floats.
                    let seconds = duration as f32 / (1000.0 * 1_000_000.0);
                    if seconds <= 0.0 { 0.0 } else { 1.0 / seconds }
                }
            },
            None => 0.0,
        };
        let animation = running.unwrap_or_default();
        *animation.animation_spec.borrow_mut() = spec.clone();
        animation.is_complete.set(false);
        animation.value.set(fraction);
        animation.start.set(fraction);
        animation.duration_nanos.set(total);
        animation.progress_nanos.set(0);
        animation.initial_velocity.set(Some(old_velocity));
        let spec_duration = match &spec {
            Some(spec) => spec.duration_nanos(
                AnimationVector::from(fraction),
                AnimationVector::from(1.0),
                AnimationVector::from(old_velocity),
            ),
            None => round(total as f64 * (1.0 - f64::from(fraction))),
        };
        animation.animation_spec_duration.set(spec_duration);
        *self.inner.current_animation.borrow_mut() = Some(animation);
    }

    /// The change interrupted goes on from where its values are, as
    /// `moveAnimationToInitialState`.
    fn move_animation_to_initial_state(&self) {
        let Some(transition) = self.transition() else { return };
        let total = self.inner.total_duration_nanos.get();
        let fraction = self.inner.fraction_untracked.get();
        let animation = self.inner.current_animation.borrow_mut().take().or_else(|| {
            if total <= 0 || fraction == 1.0 || self.current_untracked() == self.target_untracked()
            {
                None
            } else {
                let animation = SeekingAnimationState::default();
                animation.value.set(fraction);
                animation.duration_nanos.set(total);
                animation
                    .animation_spec_duration
                    .set(round(total as f64 * (1.0 - f64::from(fraction))));
                animation.start.set(fraction);
                Some(Rc::new(animation))
            }
        });
        if let Some(animation) = animation {
            animation.duration_nanos.set(total);
            self.inner.initial_value_animations.borrow_mut().push(animation.clone());
            transition.set_initial_animations(&animation);
        }
    }

    /// Runs the initial-value animations and the animation of the fraction to their ends, as
    /// `runAnimations`.
    async fn run_animations(&self, mutation: u64) -> Step {
        let idle = self.inner.initial_value_animations.borrow().is_empty()
            && self.inner.current_animation.borrow().is_none();
        if idle {
            return Ok(());
        }
        if self.inner.last_frame_time_nanos.get() == AnimationConstants::UNSPECIFIED_TIME {
            // A frame for the start time.
            self.first_frame(mutation).await?;
        }
        loop {
            let idle = self.inner.initial_value_animations.borrow().is_empty()
                && self.inner.current_animation.borrow().is_none();
            if idle {
                break;
            }
            self.animate_one_frame(mutation).await?;
        }
        self.inner.last_frame_time_nanos.set(AnimationConstants::UNSPECIFIED_TIME);
        Ok(())
    }

    async fn first_frame(&self, mutation: u64) -> Step {
        let time = with_frame_nanos(|time| time).await as i64;
        self.check(mutation)?;
        self.inner.last_frame_time_nanos.set(time);
        Ok(())
    }

    async fn do_one_frame(&self, mutation: u64) -> Step {
        if self.inner.last_frame_time_nanos.get() == AnimationConstants::UNSPECIFIED_TIME {
            self.first_frame(mutation).await
        } else {
            self.animate_one_frame(mutation).await
        }
    }

    async fn animate_one_frame(&self, mutation: u64) -> Step {
        if self.inner.last_frame_time_nanos.get() == AnimationConstants::UNSPECIFIED_TIME {
            return self.first_frame(mutation).await;
        }
        let time = with_frame_nanos(|time| time).await as i64;
        self.check(mutation)?;
        self.on_animation_frame(time);
        Ok(())
    }

    /// A frame of the animations, as `animateOneFrameLambda`.
    fn on_animation_frame(&self, frame_time_nanos: i64) {
        let last = self.inner.last_frame_time_nanos.get();
        let delta =
            if last == AnimationConstants::UNSPECIFIED_TIME { 0 } else { frame_time_nanos - last };
        self.inner.last_frame_time_nanos.set(frame_time_nanos);
        let initial = self.inner.initial_value_animations.borrow().clone();
        if !initial.is_empty() {
            for animation in &initial {
                // `update_initial_values` sets it back if it isn't over.
                recalculate_animation_value(animation, delta);
                animation.is_complete.set(true);
            }
            if let Some(transition) = self.transition() {
                transition.update_initial_values();
            }
            self.inner
                .initial_value_animations
                .borrow_mut()
                .retain(|animation| !animation.is_complete.get());
        }
        let current = self.inner.current_animation.borrow().clone();
        if let Some(current) = current {
            current.duration_nanos.set(self.inner.total_duration_nanos.get());
            recalculate_animation_value(&current, delta);
            self.set_fraction(current.value.get());
            if current.value.get() == 1.0 {
                *self.inner.current_animation.borrow_mut() = None;
            }
            self.seek_to_fraction();
        }
    }

    /// Waits for a composition of the target, unless it was composed, as
    /// `waitForCompositionAfterTargetStateChange`.
    async fn wait_for_composition_after_target_state_change(&self, mutation: u64) -> Step {
        let expected = self.target_untracked();
        if expected == *self.inner.composed_target.borrow() {
            return Ok(());
        }
        self.composition().await;
        self.check(mutation)?;
        if *self.inner.composed_target.borrow() != expected {
            self.inner.last_frame_time_nanos.set(AnimationConstants::UNSPECIFIED_TIME);
            return Err(Interrupted);
        }
        Ok(())
    }

    /// Waits for the next composition of a new current or target state, as
    /// `waitForComposition`.
    async fn wait_for_composition(&self, mutation: u64) -> Step {
        let expected = self.target_untracked();
        self.composition().await;
        self.check(mutation)?;
        if *self.inner.composed_target.borrow() != expected {
            self.inner.last_frame_time_nanos.set(AnimationConstants::UNSPECIFIED_TIME);
            return Err(Interrupted);
        }
        Ok(())
    }

    /// The next composition seeing a new current or target state.
    fn composition(&self) -> impl Future<Output = ()> + use<S> {
        let inner = self.inner.clone();
        let start = inner.compositions.get();
        std::future::poll_fn(move |context| {
            if inner.compositions.get() != start {
                Poll::Ready(())
            } else {
                inner.composition_waiters.borrow_mut().push(context.waker().clone());
                Poll::Pending
            }
        })
    }
}

/// Kotlin's `roundToLong`: half up.
fn round(value: f64) -> i64 {
    (value + 0.5).floor() as i64
}

/// The animation's fraction `delta_play_time_nanos` further, as `recalculateAnimationValue`.
fn recalculate_animation_value(animation: &SeekingAnimationState, delta_play_time_nanos: i64) {
    let play_time = animation.progress_nanos.get() + delta_play_time_nanos;
    animation.progress_nanos.set(play_time);
    let duration = animation.animation_spec_duration.get();
    if duration <= 0 || play_time >= duration {
        animation.value.set(1.0);
        return;
    }
    let value = match &*animation.animation_spec.borrow() {
        Some(spec) => spec.value_from_nanos(
            play_time,
            AnimationVector::from(animation.start.get()),
            AnimationVector::from(1.0),
            AnimationVector::from(animation.initial_velocity.get().unwrap_or(0.0)),
        )[0],
        None => {
            let start = animation.start.get();
            crate::spec::lerp(start, 1.0, play_time as f32 / duration as f32)
        }
    };
    animation.value.set(if value.is_nan() { 1.0 } else { value.clamp(0.0, 1.0) });
}

/// Both futures, polled together until both are done.
async fn join<A: Future, B: Future>(a: A, b: B) -> (A::Output, B::Output) {
    let (mut a, mut b) = (Box::pin(a), Box::pin(b));
    let (mut done_a, mut done_b) = (None, None);
    std::future::poll_fn(move |context: &mut Context<'_>| {
        if done_a.is_none()
            && let Poll::Ready(value) = Pin::new(&mut a).poll(context)
        {
            done_a = Some(value);
        }
        if done_b.is_none()
            && let Poll::Ready(value) = Pin::new(&mut b).poll(context)
        {
            done_b = Some(value);
        }
        if done_a.is_some() && done_b.is_some() {
            Poll::Ready((done_a.take().unwrap(), done_b.take().unwrap()))
        } else {
            Poll::Pending
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compose's animateToWithZeroDurationOrNaNDoesNotCrash,
    // animateToWithZeroTotalDurationLinearLerpDoesNotCrash and
    // animateToWithUnspecifiedLastFrameTimeDoesNotCrash give a frame clock of their own to the
    // call (compose-rs has no per-task clock): their frames, against the fraction's animation.
    fn linear(duration: i64) -> SeekingAnimationState {
        let animation = SeekingAnimationState::default();
        animation.duration_nanos.set(duration);
        animation.animation_spec_duration.set(duration);
        animation
    }

    // Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToWithZeroTotalDurationLinearLerpDoesNotCrash
    #[test]
    fn a_frame_back_in_time_of_no_duration_ends_the_fraction() {
        let animation = linear(0);
        recalculate_animation_value(&animation, -50_000_000);
        assert_eq!(animation.value.get(), 1.0);
    }

    // Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToWithZeroTotalDurationLinearLerpDoesNotCrash
    #[test]
    fn a_frame_back_in_time_keeps_the_fraction_within_bounds() {
        let animation = linear(100_000_000);
        recalculate_animation_value(&animation, -50_000_000);
        assert_eq!(animation.value.get(), 0.0);
    }

    // Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, animateToWithUnspecifiedLastFrameTimeDoesNotCrash
    #[test]
    fn a_frame_after_an_unspecified_one_moves_nothing() {
        let state = SeekableTransitionState::new(0);
        let animation = Rc::new(linear(100_000_000));
        *state.inner.current_animation.borrow_mut() = Some(animation.clone());
        state.inner.last_frame_time_nanos.set(AnimationConstants::UNSPECIFIED_TIME);
        state.on_animation_frame(100_000_000);
        assert_eq!(animation.progress_nanos.get(), 0);
        assert_eq!(state.fraction(), 0.0);
    }

    // Compose: compose/animation/animation-core/src/androidDeviceTest/kotlin/androidx/compose/animation/core/SeekableTransitionStateTest.kt, testCleanupAfterDispose
    #[test]
    fn a_transition_leaving_stops_the_observation_of_its_duration() {
        use compose_runtime::mutable_state_of;
        let state = SeekableTransitionState::new(true);
        let disposed = mutable_state_of(false);
        let (given, gone) = (state.clone(), disposed.handle());
        let mut ui = compose_test::compose_test(move || {
            if !gone.get() {
                let _ = crate::remember_transition(&given, None);
            }
        });
        ui.wait_for_idle();
        assert!(state.inner.duration_observer.borrow().is_some());
        disposed.set(true);
        ui.wait_for_idle();
        assert!(state.inner.duration_observer.borrow().is_none());
    }
}
