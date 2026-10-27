//! `animate` and `AnimationState`, as animation-core's `SuspendAnimation.kt`: a value animated
//! frame by frame (`with_frame_nanos`) along a `TargetBasedAnimation`, the caller told each
//! frame's value and velocity, which is how scrolls animate (`animateScrollBy`,
//! `animateScrollToItem`).

use compose_runtime::with_frame_nanos;

use crate::animation::{Animation, TargetBasedAnimation};
use crate::spec::AnimationSpec;
use crate::vector::{Animate, AnimationVector};

/// Animates from `initial_value` to `target_value` along `spec`, starting at `initial_velocity`
/// (per second), calling `block` with each frame's value and velocity, as animation-core's
/// `animate(initialValue, targetValue, initialVelocity, animationSpec, block)`. The first frame
/// is at play time 0 (the initial value); the last, the target.
pub async fn animate<T: Animate>(
    initial_value: T,
    target_value: T,
    initial_velocity: T,
    spec: impl Into<AnimationSpec>,
    mut block: impl FnMut(T, T),
) {
    let mut state = AnimationState::new(initial_value, initial_velocity);
    state
        .animate_to(target_value, spec, false, |scope| block(scope.value(), scope.velocity()))
        .await;
}

/// A value's animation state, as animation-core's `AnimationState<T, V>`: its value and
/// velocity, as the last animation left them, which the next one starts from.
#[derive(Clone, Debug)]
pub struct AnimationState<T: Animate> {
    value: T,
    velocity_vector: AnimationVector,
    is_running: bool,
    /// The last frame's time (nanoseconds; 0: none yet), where a sequential animation starts.
    last_frame_time_nanos: u64,
    /// When the last animation finished (nanoseconds; 0: none has).
    finished_time_nanos: u64,
}

impl<T: Animate> AnimationState<T> {
    /// At `initial_value`, moving at `initial_velocity` (per second).
    pub fn new(initial_value: T, initial_velocity: T) -> AnimationState<T> {
        AnimationState {
            velocity_vector: initial_velocity.to_vector(),
            value: initial_value,
            is_running: false,
            last_frame_time_nanos: 0,
            finished_time_nanos: 0,
        }
    }

    pub fn value(&self) -> T {
        self.value.clone()
    }

    /// Per second.
    pub fn velocity(&self) -> T {
        T::from_vector(self.velocity_vector)
    }

    pub fn velocity_vector(&self) -> AnimationVector {
        self.velocity_vector
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// The last frame's time, in nanoseconds (0 before any).
    pub fn last_frame_time_nanos(&self) -> u64 {
        self.last_frame_time_nanos
    }

    /// When the last animation finished, in nanoseconds (0 if none has).
    pub fn finished_time_nanos(&self) -> u64 {
        self.finished_time_nanos
    }

    /// The same state at `value`, as `copy(value = …)`: its velocity and last frame kept.
    pub fn with_value(self, value: T) -> AnimationState<T> {
        AnimationState { value, ..self }
    }

    /// Animates from the current value at the current velocity to `target` along `spec`, a step
    /// each frame, as `AnimationState.animateTo`: a `TargetBasedAnimation` played from the first
    /// frame (play time 0), or, `sequential_animation`, from the last frame's time, as one
    /// continuing the one before. `block` sees each frame's values and can stop the animation
    /// (`AnimationScope::cancel_animation`). Returns when it finished or was stopped; the state
    /// then holds the last frame's value and velocity (the animation's end velocity once
    /// finished, as Compose's `doAnimationFrame`).
    pub async fn animate_to(
        &mut self,
        target: T,
        spec: impl Into<AnimationSpec>,
        sequential_animation: bool,
        mut block: impl FnMut(&mut AnimationScope<T>),
    ) {
        let animation = TargetBasedAnimation::with_velocity_vector(
            spec,
            self.value.clone(),
            target.clone(),
            self.velocity_vector,
        );
        self.is_running = true;
        let mut start = (sequential_animation && self.last_frame_time_nanos != 0)
            .then_some(self.last_frame_time_nanos);
        loop {
            let time = with_frame_nanos(|time| time).await;
            let begin = *start.get_or_insert(time);
            let play_time = time.saturating_sub(begin).min(i64::MAX as u64) as i64;
            self.value = animation.value_from_nanos(play_time);
            self.velocity_vector = animation.velocity_vector_from_nanos(play_time);
            self.last_frame_time_nanos = time;
            let finished = animation.is_finished_from_nanos(play_time);
            if finished {
                self.finished_time_nanos = time;
            }
            let mut scope = AnimationScope {
                value: self.value.clone(),
                velocity_vector: self.velocity_vector,
                target_value: target.clone(),
                cancelled: false,
            };
            block(&mut scope);
            if scope.cancelled || finished {
                self.is_running = false;
                return;
            }
        }
    }
}

/// An animation frame, as animation-core's `AnimationScope`: its value and velocity, and a way
/// to stop the animation.
#[derive(Debug)]
pub struct AnimationScope<T: Animate> {
    value: T,
    velocity_vector: AnimationVector,
    target_value: T,
    cancelled: bool,
}

impl<T: Animate> AnimationScope<T> {
    pub fn value(&self) -> T {
        self.value.clone()
    }

    /// Per second.
    pub fn velocity(&self) -> T {
        T::from_vector(self.velocity_vector)
    }

    pub fn velocity_vector(&self) -> AnimationVector {
        self.velocity_vector
    }

    pub fn target_value(&self) -> T {
        self.target_value.clone()
    }

    /// Stops the animation after this frame, as `cancelAnimation`.
    pub fn cancel_animation(&mut self) {
        self.cancelled = true;
    }
}
