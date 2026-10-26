//! `animate` and `AnimationState`, as animation-core's for a `Float`: a value animated frame by
//! frame (`with_frame_nanos`), the caller told each frame's value and velocity, which is how
//! scrolls animate (`animateScrollBy`, `animateScrollToItem`).

use compose_runtime::with_frame_nanos;

use crate::spec::{AnimationSpec, Spring};

/// Animates from `initial_value` to `target_value` along `spec`, starting at `initial_velocity`
/// (per second), calling `block` with each frame's value and velocity, as animation-core's
/// `animate(initialValue, targetValue, initialVelocity, animationSpec, block)`. The first frame
/// gives the initial value; the last, the target.
pub async fn animate(
    initial_value: f32,
    target_value: f32,
    initial_velocity: f32,
    spec: impl Into<AnimationSpec>,
    mut block: impl FnMut(f32, f32),
) {
    let mut state = AnimationState::new(initial_value, initial_velocity);
    state
        .animate_to(target_value, spec, false, |scope| block(scope.value(), scope.velocity()))
        .await;
}

/// A float's animation state, as animation-core's `AnimationState<Float, AnimationVector1D>`:
/// its value and velocity, as the last animation left them, which the next one starts from.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct AnimationState {
    value: f32,
    velocity: f32,
    is_running: bool,
    /// The last frame's time (nanoseconds; 0: none yet), where a sequential animation starts.
    last_frame_time_nanos: u64,
}

impl AnimationState {
    /// At `initial_value`, moving at `initial_velocity` (per second).
    pub fn new(initial_value: f32, initial_velocity: f32) -> AnimationState {
        AnimationState {
            value: initial_value,
            velocity: initial_velocity,
            is_running: false,
            last_frame_time_nanos: 0,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Per second.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// The last frame's time, in nanoseconds (0 before any).
    pub fn last_frame_time_nanos(&self) -> u64 {
        self.last_frame_time_nanos
    }

    /// The same state at `value`, as `copy(value = …)`: its velocity and last frame kept.
    pub fn with_value(self, value: f32) -> AnimationState {
        AnimationState { value, ..self }
    }

    /// Animates from the current value at the current velocity to `target` along `spec`, a step
    /// each frame, as `AnimationState.animateTo`: `block` sees each frame's values and can stop
    /// the animation (`AnimationScope::cancel_animation`). A `sequential_animation` starts at the
    /// last frame's time, as one continuing the one before. Returns when it finished or was
    /// stopped; the state then holds the last frame's value and velocity.
    pub async fn animate_to(
        &mut self,
        target: f32,
        spec: impl Into<AnimationSpec>,
        sequential_animation: bool,
        mut block: impl FnMut(&mut AnimationScope),
    ) {
        let spec = spec.into();
        let (start, velocity) = (self.value, self.velocity);
        self.is_running = true;
        let mut started = (sequential_animation && self.last_frame_time_nanos != 0)
            .then_some(self.last_frame_time_nanos);
        loop {
            let time = with_frame_nanos(|time| time).await;
            let begin = *started.get_or_insert(time);
            let frame = spec.frame(
                time.saturating_sub(begin),
                [start],
                [target],
                [velocity],
                Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
            );
            self.value = frame.values[0];
            self.velocity = frame.velocities[0];
            self.last_frame_time_nanos = time;
            if frame.finished {
                self.velocity = 0.0;
            }
            let mut scope = AnimationScope {
                value: self.value,
                velocity: self.velocity,
                target_value: target,
                cancelled: false,
            };
            block(&mut scope);
            if scope.cancelled || frame.finished {
                self.is_running = false;
                return;
            }
        }
    }
}

/// An animation frame, as animation-core's `AnimationScope`: its value and velocity, and a way
/// to stop the animation.
#[derive(Debug)]
pub struct AnimationScope {
    value: f32,
    velocity: f32,
    target_value: f32,
    cancelled: bool,
}

impl AnimationScope {
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Per second.
    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn target_value(&self) -> f32 {
        self.target_value
    }

    /// Stops the animation after this frame, as `cancelAnimation`.
    pub fn cancel_animation(&mut self) {
        self.cancelled = true;
    }
}
