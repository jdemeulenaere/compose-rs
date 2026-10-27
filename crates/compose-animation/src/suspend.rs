//! Animations without an `Animatable`, as Compose's `SuspendAnimation.kt`: `animate` and
//! `animate_decay` call a block each frame with the value and velocity, and an
//! `AnimationState` keeps a value and velocity that `animate_to` and `animate_decay` move, a
//! frame at a time, an `AnimationScope` given to their block.

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::{OwnedState, mutable_state_of, with_frame_nanos};

use crate::animation::{Animation, TargetBasedAnimation};
use crate::decay::{DecayAnimation, DecayAnimationSpec, FloatDecayAnimationSpec};
use crate::spec::{AnimationConstants, AnimationSpec};
use crate::vector::{Animate, AnimationVector};

/// A value and its velocity that animations move, with their timing, as Compose's
/// `AnimationState`: a handle, its clones sharing the state ([`AnimationState::copy`] makes
/// another). Its value is a state read.
pub struct AnimationState<T: Animate> {
    inner: Rc<StateInner<T>>,
}

struct StateInner<T: Animate> {
    value: OwnedState<T>,
    velocity_vector: Cell<AnimationVector>,
    last_frame_time_nanos: Cell<i64>,
    finished_time_nanos: Cell<i64>,
    is_running: Cell<bool>,
}

impl<T: Animate> Clone for AnimationState<T> {
    fn clone(&self) -> Self {
        AnimationState { inner: self.inner.clone() }
    }
}

impl<T: Animate> AnimationState<T> {
    /// `initial_value` at rest, as `AnimationState(typeConverter, initialValue)`.
    pub fn new(initial_value: T) -> AnimationState<T> {
        let velocity = initial_value.to_vector().new_instance();
        AnimationState::with(
            initial_value,
            velocity,
            AnimationConstants::UNSPECIFIED_TIME,
            AnimationConstants::UNSPECIFIED_TIME,
            false,
        )
    }

    /// `initial_value` at `initial_velocity`, as `AnimationState(typeConverter, initialValue,
    /// initialVelocity)`.
    pub fn with_velocity(initial_value: T, initial_velocity: T) -> AnimationState<T> {
        AnimationState::with(
            initial_value,
            initial_velocity.to_vector(),
            AnimationConstants::UNSPECIFIED_TIME,
            AnimationConstants::UNSPECIFIED_TIME,
            false,
        )
    }

    /// Every field given, as `AnimationState(typeConverter, initialValue,
    /// initialVelocityVector, lastFrameTimeNanos, finishedTimeNanos, isRunning)`.
    pub fn with(
        initial_value: T,
        initial_velocity_vector: AnimationVector,
        last_frame_time_nanos: i64,
        finished_time_nanos: i64,
        is_running: bool,
    ) -> AnimationState<T> {
        AnimationState {
            inner: Rc::new(StateInner {
                value: mutable_state_of(initial_value),
                velocity_vector: Cell::new(initial_velocity_vector),
                last_frame_time_nanos: Cell::new(last_frame_time_nanos),
                finished_time_nanos: Cell::new(finished_time_nanos),
                is_running: Cell::new(is_running),
            }),
        }
    }

    /// A new state with the same fields, as `copy()`.
    pub fn copy(&self) -> AnimationState<T> {
        AnimationState::with(
            self.value(),
            self.velocity_vector(),
            self.last_frame_time_nanos(),
            self.finished_time_nanos(),
            self.is_running(),
        )
    }

    /// The value, as `value` (a state read).
    pub fn value(&self) -> T {
        self.inner.value.get()
    }

    /// The velocity, as `velocity`.
    pub fn velocity(&self) -> T {
        T::from_vector(self.velocity_vector())
    }

    /// The velocity as a vector, as `velocityVector`.
    pub fn velocity_vector(&self) -> AnimationVector {
        self.inner.velocity_vector.get()
    }

    /// The time of the last frame an animation set it at, as `lastFrameTimeNanos`.
    pub fn last_frame_time_nanos(&self) -> i64 {
        self.inner.last_frame_time_nanos.get()
    }

    /// The time the last animation finished at, as `finishedTimeNanos`.
    pub fn finished_time_nanos(&self) -> i64 {
        self.inner.finished_time_nanos.get()
    }

    /// Whether an animation moves it, as `isRunning`.
    pub fn is_running(&self) -> bool {
        self.inner.is_running.get()
    }

    /// Whether the last animation finished, as `isFinished`.
    pub fn is_finished(&self) -> bool {
        self.finished_time_nanos() != AnimationConstants::UNSPECIFIED_TIME
    }

    /// Animates from its value and velocity to `target_value` along `spec`, calling `block`
    /// each frame, as `AnimationState.animateTo(targetValue, animationSpec,
    /// sequentialAnimation, block)`: with `sequential_animation`, from the last frame an
    /// animation set it at (its first frame at play time 0 at once), else from the next frame.
    pub async fn animate_to(
        &self,
        target_value: T,
        spec: impl Into<AnimationSpec>,
        sequential_animation: bool,
        block: impl FnMut(&AnimationScope<T>),
    ) {
        let animation = TargetBasedAnimation::with_velocity_vector(
            spec.into(),
            self.value(),
            target_value,
            self.velocity_vector(),
        );
        let start = self.start_time(sequential_animation);
        self.animate(&animation, start, block).await;
    }

    /// Decays from its value at its velocity along `spec`, calling `block` each frame, as
    /// `AnimationState.animateDecay(animationSpec, sequentialAnimation, block)`.
    pub async fn animate_decay(
        &self,
        spec: &DecayAnimationSpec,
        sequential_animation: bool,
        block: impl FnMut(&AnimationScope<T>),
    ) {
        let animation = DecayAnimation::with_velocity_vector(
            spec.clone(),
            self.value(),
            self.velocity_vector(),
        );
        let start = self.start_time(sequential_animation);
        self.animate(&animation, start, block).await;
    }

    fn start_time(&self, sequential_animation: bool) -> i64 {
        if sequential_animation {
            self.last_frame_time_nanos()
        } else {
            AnimationConstants::UNSPECIFIED_TIME
        }
    }

    /// Plays `animation` on this state, as Compose's internal `AnimationState.animate`: the first
    /// frame at `start_time_nanos` at once if given, else at the next frame; then a frame at a
    /// time until it ends or the block cancels it.
    pub(crate) async fn animate(
        &self,
        animation: &dyn Animation<T>,
        start_time_nanos: i64,
        mut block: impl FnMut(&AnimationScope<T>),
    ) {
        let initial_value = animation.value_from_nanos(0);
        let initial_velocity = animation.velocity_vector_from_nanos(0);
        let infinite = animation.is_infinite();
        let start = if start_time_nanos == AnimationConstants::UNSPECIFIED_TIME {
            frame(infinite).await
        } else {
            start_time_nanos
        };
        let scope = AnimationScope {
            value: mutable_state_of(initial_value),
            velocity_vector: Cell::new(initial_velocity),
            last_frame_time_nanos: Cell::new(start),
            finished_time_nanos: Cell::new(AnimationConstants::UNSPECIFIED_TIME),
            is_running: Cell::new(true),
            target_value: animation.target_value(),
            start_time_nanos: start,
            state: self.clone(),
        };
        // If cancelled (dropped) while running, the state stops, unless another animation
        // moved it since.
        let guard = StopOnDrop { scope: &scope, state: self };
        scope.frame(start, animation, self, &mut block);
        while scope.is_running.get() {
            let time = frame(infinite).await;
            scope.frame(time, animation, self, &mut block);
        }
        std::mem::forget(guard);
    }
}

/// Stops a state an animation was moving when the animation's task is cancelled, as Compose's
/// `catch (e: CancellationException)`.
struct StopOnDrop<'a, T: Animate> {
    scope: &'a AnimationScope<T>,
    state: &'a AnimationState<T>,
}

impl<T: Animate> Drop for StopOnDrop<'_, T> {
    fn drop(&mut self) {
        self.scope.is_running.set(false);
        if self.scope.last_frame_time_nanos.get() == self.state.last_frame_time_nanos() {
            self.state.inner.is_running.set(false);
        }
    }
}

/// The next frame's time: an infinite animation's through the infinite-animation policy, as
/// Compose's `callWithFrameNanos`.
async fn frame(infinite: bool) -> i64 {
    if infinite {
        compose_runtime::with_infinite_animation_frame_nanos(|time| time).await as i64
    } else {
        with_frame_nanos(|time| time).await as i64
    }
}

/// An animation running, as Compose's `AnimationScope`: what its block sees each frame, and
/// `cancel_animation` to stop it.
pub struct AnimationScope<T: Animate> {
    value: OwnedState<T>,
    velocity_vector: Cell<AnimationVector>,
    last_frame_time_nanos: Cell<i64>,
    finished_time_nanos: Cell<i64>,
    is_running: Cell<bool>,
    target_value: T,
    start_time_nanos: i64,
    state: AnimationState<T>,
}

impl<T: Animate> AnimationScope<T> {
    /// The value at this frame, as `value`.
    pub fn value(&self) -> T {
        self.value.get()
    }

    /// The velocity at this frame, as `velocity`.
    pub fn velocity(&self) -> T {
        T::from_vector(self.velocity_vector.get())
    }

    /// The velocity as a vector, as `velocityVector`.
    pub fn velocity_vector(&self) -> AnimationVector {
        self.velocity_vector.get()
    }

    /// Where it goes, as `targetValue`.
    pub fn target_value(&self) -> T {
        self.target_value.clone()
    }

    /// The frame time it started at, as `startTimeNanos`.
    pub fn start_time_nanos(&self) -> i64 {
        self.start_time_nanos
    }

    /// This frame's time, as `lastFrameTimeNanos`.
    pub fn last_frame_time_nanos(&self) -> i64 {
        self.last_frame_time_nanos.get()
    }

    /// The time it finished at, as `finishedTimeNanos`.
    pub fn finished_time_nanos(&self) -> i64 {
        self.finished_time_nanos.get()
    }

    /// Whether it runs, as `isRunning`.
    pub fn is_running(&self) -> bool {
        self.is_running.get()
    }

    /// Stops it after this frame, as `cancelAnimation()`: its state stops too.
    pub fn cancel_animation(&self) {
        self.is_running.set(false);
        self.state.inner.is_running.set(false);
    }

    /// Its fields as a new state, as `toAnimationState()`.
    pub fn to_animation_state(&self) -> AnimationState<T> {
        AnimationState::with(
            self.value(),
            self.velocity_vector(),
            self.last_frame_time_nanos(),
            self.finished_time_nanos(),
            self.is_running(),
        )
    }

    /// A frame at `frame_time_nanos`, as `doAnimationFrame`: the value and velocity, whether it
    /// ended, copied to the state, then the block.
    fn frame(
        &self,
        frame_time_nanos: i64,
        animation: &dyn Animation<T>,
        state: &AnimationState<T>,
        block: &mut impl FnMut(&AnimationScope<T>),
    ) {
        let play_time = frame_time_nanos - self.start_time_nanos;
        self.last_frame_time_nanos.set(frame_time_nanos);
        self.value.set(animation.value_from_nanos(play_time));
        self.velocity_vector.set(animation.velocity_vector_from_nanos(play_time));
        if animation.is_finished_from_nanos(play_time) {
            self.finished_time_nanos.set(frame_time_nanos);
            self.is_running.set(false);
        }
        let inner = &state.inner;
        inner.value.set(self.value());
        inner.velocity_vector.set(self.velocity_vector());
        inner.finished_time_nanos.set(self.finished_time_nanos());
        inner.last_frame_time_nanos.set(frame_time_nanos);
        inner.is_running.set(self.is_running());
        block(self);
    }
}

/// Animates `initial_value` to `target_value` at `initial_velocity` along `spec`, calling
/// `block(value, velocity)` each frame from the next, as Compose's top-level `animate(…)`.
pub async fn animate(
    initial_value: f32,
    target_value: f32,
    initial_velocity: f32,
    spec: impl Into<AnimationSpec>,
    block: impl FnMut(f32, f32),
) {
    animate_value(initial_value, target_value, Some(initial_velocity), spec, block).await;
}

/// The same for any animated type, the velocity at rest when `None`, as Compose's
/// `animate(typeConverter, initialValue, targetValue, initialVelocity, animationSpec, block)`.
pub async fn animate_value<T: Animate>(
    initial_value: T,
    target_value: T,
    initial_velocity: Option<T>,
    spec: impl Into<AnimationSpec>,
    mut block: impl FnMut(T, T),
) {
    let velocity = initial_velocity
        .map(|velocity| velocity.to_vector())
        .unwrap_or_else(|| initial_value.to_vector().new_instance());
    let animation = TargetBasedAnimation::with_velocity_vector(
        spec.into(),
        initial_value.clone(),
        target_value,
        velocity,
    );
    let state = AnimationState::with(
        initial_value,
        velocity,
        AnimationConstants::UNSPECIFIED_TIME,
        AnimationConstants::UNSPECIFIED_TIME,
        false,
    );
    state
        .animate(&animation, AnimationConstants::UNSPECIFIED_TIME, move |scope| {
            block(scope.value(), scope.velocity())
        })
        .await;
}

/// Decays `initial_value` at `initial_velocity` along `spec`, calling `block(value, velocity)`
/// each frame from the next, as Compose's top-level `animateDecay(…)`.
pub async fn animate_decay(
    initial_value: f32,
    initial_velocity: f32,
    spec: impl FloatDecayAnimationSpec + 'static,
    mut block: impl FnMut(f32, f32),
) {
    let animation = crate::decay::float_decay_animation(spec, initial_value, initial_velocity);
    let state = AnimationState::with_velocity(initial_value, initial_velocity);
    state
        .animate(&animation, AnimationConstants::UNSPECIFIED_TIME, move |scope| {
            block(scope.value(), scope.velocity())
        })
        .await;
}
