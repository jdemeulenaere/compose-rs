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

    /// The same, its spec starting `start_delay_nanos` in (Compose's internal `delayed`, which a
    /// transition's value added or changed mid-way plays): at its initial value and velocity
    /// until then.
    pub(crate) fn delayed(mut self, start_delay_nanos: i64) -> Self {
        self.start_delay_nanos = start_delay_nanos.max(0);
        self.duration_nanos.set(None);
        self.end_velocity.set(None);
        self
    }

    /// Starts from `initial_value` instead, as Compose's internal `mutableInitialValue`: its
    /// duration and end velocity computed again.
    pub(crate) fn set_initial_value(&mut self, initial_value: T) {
        self.initial_value_vector = initial_value.to_vector();
        self.initial_value = initial_value;
        self.duration_nanos.set(None);
        self.end_velocity.set(None);
    }

    /// Goes to `target_value` instead, as Compose's internal `mutableTargetValue`.
    pub(crate) fn set_target_value(&mut self, target_value: T) {
        self.target_value_vector = target_value.to_vector();
        self.target_value = target_value;
        self.duration_nanos.set(None);
        self.end_velocity.set(None);
    }

    /// Where it starts, as `initialValue`.
    pub fn initial_value(&self) -> T {
        self.initial_value.clone()
    }

    /// The spec it plays.
    pub fn animation_spec(&self) -> AnimationSpec {
        self.spec.clone()
    }

    /// The velocity it starts with, as a vector.
    pub fn initial_velocity_vector(&self) -> AnimationVector {
        self.initial_velocity_vector
    }

    fn spec_value(&self, play_time_nanos: i64) -> AnimationVector {
        // Only a delayed spec holds its start before its delay (Compose wraps only those):
        // a play time before 0 is the spec's own (a fast-forwarded repeatable's).
        if self.start_delay_nanos > 0 && play_time_nanos < self.start_delay_nanos {
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
        // Only a delayed spec holds its start before its delay (Compose wraps only those):
        // a play time before 0 is the spec's own (a fast-forwarded repeatable's).
        if self.start_delay_nanos > 0 && play_time_nanos < self.start_delay_nanos {
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

/// Compose's `DelayedAnimationTest`, of its internal `delayed(spec, nanos)` wrapper: compose-rs
/// delays the animation (`TargetBasedAnimation::delayed`), not the spec, so the specs'
/// comparison (`comparison`) has nothing to compare.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::LINEAR_EASING;
    use crate::spec::{VectorizedAnimationSpec, spring, tween};

    const DELAY: i64 = 500 * 1_000_000;

    fn delayed(spec: impl Into<AnimationSpec>, delay: i64) -> TargetBasedAnimation<f32> {
        TargetBasedAnimation::with_velocity(spec, 0.0f32, 1000.0, 30.0).delayed(delay)
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DelayedAnimationTest.kt, duration
    #[test]
    fn the_delay_adds_to_the_duration() {
        let animation = delayed(tween(1000).easing(LINEAR_EASING), DELAY);
        assert_eq!(animation.duration_nanos(), 1500 * 1_000_000);
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DelayedAnimationTest.kt, values
    #[test]
    fn the_value_holds_through_the_delay() {
        let animation = delayed(tween(1000).easing(LINEAR_EASING), DELAY);
        for (millis, expected) in [(0, 0.0), (500, 0.0), (1000, 500.0), (1500, 1000.0)] {
            assert_eq!(animation.value_from_millis(millis), expected, "at {millis}");
        }
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DelayedAnimationTest.kt, velocity
    #[test]
    fn the_velocity_holds_through_the_delay_then_is_the_specs() {
        let animation = delayed(spring(), DELAY);
        assert_eq!(animation.velocity_from_nanos(0), 30.0);
        assert_eq!(animation.velocity_from_nanos(DELAY), 30.0);
        let (start, end, velocity) = (
            AnimationVector::from([0.0]),
            AnimationVector::from([1000.0]),
            AnimationVector::from([30.0]),
        );
        let spec: AnimationSpec = spring().into();
        let duration = spec.duration_nanos(start, end, velocity);
        for play_time in [duration / 5, duration / 2, duration] {
            assert_eq!(
                animation.velocity_from_nanos(DELAY + play_time),
                spec.velocity_from_nanos(play_time, start, end, velocity)[0],
                "{play_time} in"
            );
        }
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/DelayedAnimationTest.kt, zeroAnimation
    #[test]
    fn no_delay_and_no_change_take_no_time() {
        let animation = TargetBasedAnimation::with_velocity(spring(), 1.0f32, 1.0, 0.0).delayed(0);
        assert_eq!(animation.duration_nanos(), 0);
    }
}
