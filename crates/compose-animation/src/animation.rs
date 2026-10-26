//! Animations as values, as Compose's `Animation`: a spec bound to its start, target and initial
//! velocity, evaluated at any play time (`TargetBasedAnimation`), or a decay from a start and a
//! velocity (`DecayAnimation`).

use crate::spec::{AnimationSpec, MILLIS_TO_NANOS, VectorizedAnimationSpec};
use crate::vector::{Animate, AnimationVector};

/// An animation of a `T` from its start, evaluated at play times, as Compose's `Animation`.
pub trait Animation<T: Animate> {
    /// How long it plays, as `durationNanos`: `i64::MAX` when it never ends.
    fn duration_nanos(&self) -> i64;

    /// Where it ends, as `targetValue`.
    fn target_value(&self) -> T;

    /// Whether it never ends, as `isInfinite`.
    fn is_infinite(&self) -> bool;

    /// The value `play_time_nanos` after its start, as `getValueFromNanos`.
    fn value_from_nanos(&self, play_time_nanos: i64) -> T;

    /// The velocity then, per second, as a vector, as `getVelocityVectorFromNanos`.
    fn velocity_vector_from_nanos(&self, play_time_nanos: i64) -> AnimationVector;

    /// Whether it has ended `play_time_nanos` in, as `isFinishedFromNanos`.
    fn is_finished_from_nanos(&self, play_time_nanos: i64) -> bool {
        play_time_nanos >= self.duration_nanos()
    }

    /// The velocity then, as `getVelocityFromNanos`.
    fn velocity_from_nanos(&self, play_time_nanos: i64) -> T {
        T::from_vector(self.velocity_vector_from_nanos(play_time_nanos))
    }

    /// How long it plays in milliseconds, as Compose's `durationMillis`.
    fn duration_millis(&self) -> i64 {
        self.duration_nanos() / MILLIS_TO_NANOS
    }

    /// The value `play_time_millis` in, as Compose's `getValueFromMillis`.
    fn value_from_millis(&self, play_time_millis: i64) -> T {
        self.value_from_nanos(play_time_millis.saturating_mul(MILLIS_TO_NANOS))
    }

    /// The velocity `play_time_millis` in, as Compose's `getVelocityFromMillis`.
    fn velocity_from_millis(&self, play_time_millis: i64) -> T {
        self.velocity_from_nanos(play_time_millis.saturating_mul(MILLIS_TO_NANOS))
    }

    /// The velocity vector `play_time_millis` in, as Compose's
    /// `getVelocityVectorFromMillis`.
    fn velocity_vector_from_millis(&self, play_time_millis: i64) -> AnimationVector {
        self.velocity_vector_from_nanos(play_time_millis.saturating_mul(MILLIS_TO_NANOS))
    }

    /// Whether it has ended `play_time_millis` in, as Compose's `isFinishedFromMillis`.
    fn is_finished_from_millis(&self, play_time_millis: i64) -> bool {
        self.is_finished_from_nanos(play_time_millis.saturating_mul(MILLIS_TO_NANOS))
    }
}

/// A spec from an initial value and velocity to a target, as Compose's `TargetBasedAnimation`:
/// at its duration and after, it is at its target with the spec's end velocity.
#[derive(Clone, Debug)]
pub struct TargetBasedAnimation<T: Animate> {
    spec: AnimationSpec,
    initial_value: T,
    target_value: T,
    initial_value_vector: AnimationVector,
    target_value_vector: AnimationVector,
    initial_velocity_vector: AnimationVector,
    /// Play time before the spec starts (Compose's internal `delayed`, which a transition's value
    /// added mid-way plays): the initial value and velocity until then.
    start_delay_nanos: i64,
    duration_nanos: std::cell::Cell<Option<i64>>,
    end_velocity: std::cell::Cell<Option<AnimationVector>>,
}

impl<T: Animate> TargetBasedAnimation<T> {
    /// From `initial_value` at rest to `target_value` along `spec`, as
    /// `TargetBasedAnimation(animationSpec, typeConverter, initialValue, targetValue)`.
    pub fn new(spec: impl Into<AnimationSpec>, initial_value: T, target_value: T) -> Self {
        let velocity = initial_value.to_vector().new_instance();
        TargetBasedAnimation::with_velocity_vector(spec, initial_value, target_value, velocity)
    }

    /// The same, starting at `initial_velocity`, as `TargetBasedAnimation(…, initialVelocity)`.
    pub fn with_velocity(
        spec: impl Into<AnimationSpec>,
        initial_value: T,
        target_value: T,
        initial_velocity: T,
    ) -> Self {
        let velocity = initial_velocity.to_vector();
        TargetBasedAnimation::with_velocity_vector(spec, initial_value, target_value, velocity)
    }

    /// The same, starting at `initial_velocity_vector`, as `TargetBasedAnimation(…,
    /// initialVelocityVector)`.
    pub fn with_velocity_vector(
        spec: impl Into<AnimationSpec>,
        initial_value: T,
        target_value: T,
        initial_velocity_vector: AnimationVector,
    ) -> Self {
        TargetBasedAnimation {
            spec: spec.into(),
            initial_value_vector: initial_value.to_vector(),
            target_value_vector: target_value.to_vector(),
            initial_value,
            target_value,
            initial_velocity_vector,
            start_delay_nanos: 0,
            duration_nanos: std::cell::Cell::new(None),
            end_velocity: std::cell::Cell::new(None),
        }
    }

    /// Where it starts, as `initialValue`.
    pub fn initial_value(&self) -> T {
        self.initial_value.clone()
    }

    /// The spec it plays.
    pub fn animation_spec(&self) -> AnimationSpec {
        self.spec
    }

    /// The velocity it starts with, as a vector.
    pub fn initial_velocity_vector(&self) -> AnimationVector {
        self.initial_velocity_vector
    }

    fn spec_value(&self, play_time_nanos: i64) -> AnimationVector {
        if play_time_nanos < self.start_delay_nanos {
            return self.initial_value_vector;
        }
        self.spec.value_from_nanos(
            play_time_nanos - self.start_delay_nanos,
            self.initial_value_vector,
            self.target_value_vector,
            self.initial_velocity_vector,
        )
    }

    fn spec_velocity(&self, play_time_nanos: i64) -> AnimationVector {
        if play_time_nanos < self.start_delay_nanos {
            return self.initial_velocity_vector;
        }
        self.spec.velocity_from_nanos(
            play_time_nanos - self.start_delay_nanos,
            self.initial_value_vector,
            self.target_value_vector,
            self.initial_velocity_vector,
        )
    }

    fn end_velocity(&self) -> AnimationVector {
        if let Some(velocity) = self.end_velocity.get() {
            return velocity;
        }
        let (initial, target, speed) =
            (self.initial_value_vector, self.target_value_vector, self.initial_velocity_vector);
        // Delayed, Compose's wrapper reads the velocity at the spec's end (a spring's isn't
        // its end velocity, 0).
        let velocity = if self.start_delay_nanos > 0 {
            let duration = self.spec.duration_nanos(initial, target, speed);
            self.spec.velocity_from_nanos(duration, initial, target, speed)
        } else {
            self.spec.end_velocity(initial, target, speed)
        };
        self.end_velocity.set(Some(velocity));
        velocity
    }
}

impl<T: Animate> Animation<T> for TargetBasedAnimation<T> {
    fn duration_nanos(&self) -> i64 {
        if let Some(duration) = self.duration_nanos.get() {
            return duration;
        }
        let duration = self
            .spec
            .duration_nanos(
                self.initial_value_vector,
                self.target_value_vector,
                self.initial_velocity_vector,
            )
            .saturating_add(self.start_delay_nanos);
        self.duration_nanos.set(Some(duration));
        duration
    }

    fn target_value(&self) -> T {
        self.target_value.clone()
    }

    fn is_infinite(&self) -> bool {
        self.spec.is_infinite()
    }

    fn value_from_nanos(&self, play_time_nanos: i64) -> T {
        if self.is_finished_from_nanos(play_time_nanos) {
            return self.target_value.clone();
        }
        T::from_vector(self.spec_value(play_time_nanos))
    }

    fn velocity_vector_from_nanos(&self, play_time_nanos: i64) -> AnimationVector {
        if self.is_finished_from_nanos(play_time_nanos) {
            return self.end_velocity();
        }
        self.spec_velocity(play_time_nanos)
    }
}
