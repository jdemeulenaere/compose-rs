//! Decays, as Compose's `DecayAnimationSpec`: a value thrown at a velocity, slowing down until
//! its speed is under a threshold (a fling).

use std::rc::Rc;

use crate::animation::Animation;
use crate::spec::MILLIS_TO_NANOS;
use crate::vector::{Animate, AnimationVector};

/// `exp` as Kotlin's on floats: in doubles, then back to a float.
fn exp(x: f32) -> f32 {
    f64::from(x).exp() as f32
}

/// A decay of one float, as Compose's `FloatDecayAnimationSpec`.
pub trait FloatDecayAnimationSpec {
    /// The speed under which it's over, as `absVelocityThreshold`.
    fn abs_velocity_threshold(&self) -> f32;

    /// The value `play_time_nanos` after leaving `initial_value` at `initial_velocity`, as
    /// `getValueFromNanos`.
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        initial_velocity: f32,
    ) -> f32;

    /// How long it takes to slow under the threshold, as `getDurationNanos`.
    fn duration_nanos(&self, initial_value: f32, initial_velocity: f32) -> i64;

    /// The velocity then, as `getVelocityFromNanos`.
    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        initial_velocity: f32,
    ) -> f32;

    /// Where it stops, as `getTargetValue`.
    fn target_value(&self, initial_value: f32, initial_velocity: f32) -> f32;
}

/// Compose's exponential decay's friction.
const EXPONENTIAL_DECAY_FRICTION: f32 = -4.2;

/// A decay slowing exponentially, as Compose's `FloatExponentialDecaySpec`: the more
/// `friction_multiplier`, the sooner it stops.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatExponentialDecaySpec {
    friction: f32,
    abs_velocity_threshold: f32,
}

impl FloatExponentialDecaySpec {
    /// A decay of `friction_multiplier` (1 by default) stopping under `abs_velocity_threshold`
    /// (0.1 by default), as `FloatExponentialDecaySpec(frictionMultiplier, absVelocityThreshold)`.
    pub fn new(friction_multiplier: f32, abs_velocity_threshold: f32) -> FloatExponentialDecaySpec {
        FloatExponentialDecaySpec {
            friction: EXPONENTIAL_DECAY_FRICTION * friction_multiplier.max(0.0001),
            abs_velocity_threshold: abs_velocity_threshold.abs().max(0.000_000_1),
        }
    }
}

impl Default for FloatExponentialDecaySpec {
    fn default() -> FloatExponentialDecaySpec {
        FloatExponentialDecaySpec::new(1.0, 0.1)
    }
}

impl FloatDecayAnimationSpec for FloatExponentialDecaySpec {
    fn abs_velocity_threshold(&self) -> f32 {
        self.abs_velocity_threshold
    }

    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        let millis = (play_time_nanos / MILLIS_TO_NANOS) as f32;
        initial_value - initial_velocity / self.friction
            + initial_velocity / self.friction * exp(self.friction * millis / 1000.0)
    }

    fn duration_nanos(&self, _initial_value: f32, initial_velocity: f32) -> i64 {
        let ratio = f64::from(self.abs_velocity_threshold / initial_velocity.abs());
        let millis = 1000.0 * (ratio.ln() as f32) / self.friction;
        (millis as i64).saturating_mul(MILLIS_TO_NANOS)
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        _initial_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        let millis = (play_time_nanos / MILLIS_TO_NANOS) as f32;
        initial_velocity * exp(millis / 1000.0 * self.friction)
    }

    fn target_value(&self, initial_value: f32, initial_velocity: f32) -> f32 {
        if initial_velocity.abs() <= self.abs_velocity_threshold {
            return initial_value;
        }
        let duration = f64::from(self.abs_velocity_threshold / initial_velocity).abs().ln()
            / f64::from(self.friction)
            * 1000.0;
        let decayed = (f64::from(self.friction) * duration / 1000.0).exp() as f32;
        initial_value - initial_velocity / self.friction
            + initial_velocity / self.friction * decayed
    }
}

/// A decay of vectors, as Compose's `VectorizedDecayAnimationSpec`.
pub trait VectorizedDecayAnimationSpec {
    /// The speed under which it's over, as `absVelocityThreshold`.
    fn abs_velocity_threshold(&self) -> f32;
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector;
    fn duration_nanos(
        &self,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64;
    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector;
    fn target_value(
        &self,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector;
}

/// A decay of any animated type, as Compose's `DecayAnimationSpec`: a float decay applied to
/// each component ([`generate_decay_animation_spec`]).
#[derive(Clone)]
pub struct DecayAnimationSpec {
    float: Rc<dyn FloatDecayAnimationSpec>,
}

impl std::fmt::Debug for DecayAnimationSpec {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "DecayAnimationSpec(threshold {})", self.float.abs_velocity_threshold())
    }
}

/// The same float decay.
impl PartialEq for DecayAnimationSpec {
    fn eq(&self, other: &DecayAnimationSpec) -> bool {
        Rc::ptr_eq(&self.float, &other.float)
    }
}

impl compose_runtime::ComposeArg for DecayAnimationSpec {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `spec` applied to each component, as Compose's `FloatDecayAnimationSpec.generateDecayAnimationSpec()`.
pub fn generate_decay_animation_spec(
    spec: impl FloatDecayAnimationSpec + 'static,
) -> DecayAnimationSpec {
    DecayAnimationSpec { float: Rc::new(spec) }
}

/// An exponential decay, as Compose's `exponentialDecay(frictionMultiplier,
/// absVelocityThreshold)` (1 and 0.1 by default).
pub fn exponential_decay(
    friction_multiplier: f32,
    abs_velocity_threshold: f32,
) -> DecayAnimationSpec {
    generate_decay_animation_spec(FloatExponentialDecaySpec::new(
        friction_multiplier,
        abs_velocity_threshold,
    ))
}

/// A vector's component, 0 past its size.
fn component(vector: AnimationVector, index: usize) -> f32 {
    vector.as_slice().get(index).copied().unwrap_or(0.0)
}

impl VectorizedDecayAnimationSpec for DecayAnimationSpec {
    fn abs_velocity_threshold(&self) -> f32 {
        self.float.abs_velocity_threshold()
    }

    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        initial_value.map(|index, value| {
            self.float.value_from_nanos(play_time_nanos, value, component(initial_velocity, index))
        })
    }

    fn duration_nanos(
        &self,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64 {
        (0..initial_value.size())
            .map(|index| {
                self.float.duration_nanos(initial_value[index], component(initial_velocity, index))
            })
            .fold(0, i64::max)
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        initial_value.map(|index, value| {
            let velocity = component(initial_velocity, index);
            self.float.velocity_from_nanos(play_time_nanos, value, velocity)
        })
    }

    fn target_value(
        &self,
        initial_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        initial_value
            .map(|index, value| self.float.target_value(value, component(initial_velocity, index)))
    }
}

impl DecayAnimationSpec {
    /// Where a value thrown from `initial_value` at `initial_velocity` stops, as
    /// `calculateTargetValue`.
    pub fn calculate_target_value<T: Animate>(&self, initial_value: T, initial_velocity: T) -> T {
        T::from_vector(self.target_value(initial_value.to_vector(), initial_velocity.to_vector()))
    }
}

/// A value decaying from its start and velocity, as Compose's `DecayAnimation`: its target is
/// where it stops, its end velocity the threshold's.
#[derive(Clone, Debug)]
pub struct DecayAnimation<T: Animate> {
    spec: DecayAnimationSpec,
    initial_value: T,
    initial_value_vector: AnimationVector,
    initial_velocity_vector: AnimationVector,
    target_value: T,
    duration_nanos: i64,
    end_velocity: AnimationVector,
}

impl<T: Animate> DecayAnimation<T> {
    /// A decay of `spec` from `initial_value` at `initial_velocity`, as `DecayAnimation(spec,
    /// typeConverter, initialValue, initialVelocity)`.
    pub fn new(spec: DecayAnimationSpec, initial_value: T, initial_velocity: T) -> Self {
        DecayAnimation::with_velocity_vector(spec, initial_value, initial_velocity.to_vector())
    }

    /// The same from a velocity vector, as `DecayAnimation(…, initialVelocityVector)`.
    pub fn with_velocity_vector(
        spec: DecayAnimationSpec,
        initial_value: T,
        initial_velocity_vector: AnimationVector,
    ) -> Self {
        let initial_value_vector = initial_value.to_vector();
        let target_value =
            T::from_vector(spec.target_value(initial_value_vector, initial_velocity_vector));
        let duration_nanos = spec.duration_nanos(initial_value_vector, initial_velocity_vector);
        let threshold = spec.abs_velocity_threshold();
        let end_velocity = spec
            .velocity_from_nanos(duration_nanos, initial_value_vector, initial_velocity_vector)
            .map(|_, velocity| velocity.clamp(-threshold, threshold));
        DecayAnimation {
            spec,
            initial_value,
            initial_value_vector,
            initial_velocity_vector,
            target_value,
            duration_nanos,
            end_velocity,
        }
    }

    /// Where it starts, as `initialValue`.
    pub fn initial_value(&self) -> T {
        self.initial_value.clone()
    }

    /// The velocity it starts with, as `initialVelocityVector`.
    pub fn initial_velocity_vector(&self) -> AnimationVector {
        self.initial_velocity_vector
    }
}

/// A decay of floats, as Compose's `DecayAnimation(FloatDecayAnimationSpec, initialValue,
/// initialVelocity)`.
pub fn float_decay_animation(
    spec: impl FloatDecayAnimationSpec + 'static,
    initial_value: f32,
    initial_velocity: f32,
) -> DecayAnimation<f32> {
    DecayAnimation::new(generate_decay_animation_spec(spec), initial_value, initial_velocity)
}

impl<T: Animate> Animation<T> for DecayAnimation<T> {
    fn duration_nanos(&self) -> i64 {
        self.duration_nanos
    }

    fn target_value(&self) -> T {
        self.target_value.clone()
    }

    fn is_infinite(&self) -> bool {
        false
    }

    fn value_from_nanos(&self, play_time_nanos: i64) -> T {
        if self.is_finished_from_nanos(play_time_nanos) {
            return self.target_value.clone();
        }
        T::from_vector(self.spec.value_from_nanos(
            play_time_nanos,
            self.initial_value_vector,
            self.initial_velocity_vector,
        ))
    }

    fn velocity_vector_from_nanos(&self, play_time_nanos: i64) -> AnimationVector {
        if self.is_finished_from_nanos(play_time_nanos) {
            return self.end_velocity;
        }
        self.spec.velocity_from_nanos(
            play_time_nanos,
            self.initial_value_vector,
            self.initial_velocity_vector,
        )
    }
}
