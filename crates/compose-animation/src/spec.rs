//! Animation specs, as Compose's `AnimationSpec`s: `tween`, `spring`, `snap`, `keyframes`,
//! `repeatable` and `infinite_repeatable`, each evaluated as Compose's `Vectorized*Spec` at a
//! play time, per component of an animated value's vector ([`VectorizedAnimationSpec`]).

use crate::easing::{Easing, FAST_OUT_SLOW_IN_EASING};
use crate::keyframes::KeyframesSpec;
use crate::spring::{SpringSimulation, estimate_animation_duration_millis};
use crate::vector::{Animate, AnimationVector};

/// Nanoseconds in a millisecond.
pub(crate) const MILLIS_TO_NANOS: i64 = 1_000_000;

/// The default duration of a `tween`, as Compose's `AnimationConstants.DefaultDurationMillis`.
pub const DEFAULT_DURATION_MILLIS: u32 = 300;

/// Compose's `AnimationConstants`.
pub struct AnimationConstants;

impl AnimationConstants {
    /// The default duration of a duration-based animation, as `DefaultDurationMillis`.
    pub const DEFAULT_DURATION_MILLIS: u32 = DEFAULT_DURATION_MILLIS;
    /// No time (an animation not started), as `UnspecifiedTime`.
    pub const UNSPECIFIED_TIME: i64 = i64::MIN;
}

/// Compose's linear interpolation, `(1 - fraction) * start + fraction * stop`: its rounding,
/// so values match Compose's to the last bit.
pub(crate) fn lerp(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

/// An animation spec evaluated on vectors at a play time, as Compose's
/// `VectorizedAnimationSpec`: every spec is one (compose-rs's specs aren't typed, so they need
/// no `vectorize`). Play times are in nanoseconds since the animation's start; the values are
/// the animated value's components.
pub trait VectorizedAnimationSpec {
    /// Whether it never ends, as `isInfinite`.
    fn is_infinite(&self) -> bool {
        false
    }

    /// The value `play_time_nanos` in, from `initial_value` (at `initial_velocity`, per second)
    /// to `target_value`, as `getValueFromNanos`.
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector;

    /// The velocity then, per second, as `getVelocityFromNanos`.
    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector;

    /// How long it plays, as `getDurationNanos`: `i64::MAX` when it never ends.
    fn duration_nanos(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64;

    /// The velocity at its end, as `getEndVelocity`.
    fn end_velocity(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        let duration = self.duration_nanos(initial_value, target_value, initial_velocity);
        self.velocity_from_nanos(duration, initial_value, target_value, initial_velocity)
    }

    /// The value `play_time_millis` in, as Compose's `getValueFromMillis`.
    fn value_from_millis(
        &self,
        play_time_millis: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.value_from_nanos(
            play_time_millis.saturating_mul(MILLIS_TO_NANOS),
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    /// The duration in milliseconds, as Compose's `getDurationMillis`.
    fn duration_millis(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64 {
        self.duration_nanos(initial_value, target_value, initial_velocity) / MILLIS_TO_NANOS
    }
}

/// An animation spec of one float, as Compose's `FloatAnimationSpec`.
pub trait FloatAnimationSpec {
    /// The value `play_time_nanos` in, as `getValueFromNanos`.
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32;

    /// The velocity then, per second, as `getVelocityFromNanos`.
    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32;

    /// How long it plays, as `getDurationNanos`.
    fn duration_nanos(&self, initial_value: f32, target_value: f32, initial_velocity: f32) -> i64;

    /// The velocity at its end, as `getEndVelocity`.
    fn end_velocity(&self, initial_value: f32, target_value: f32, initial_velocity: f32) -> f32 {
        let duration = self.duration_nanos(initial_value, target_value, initial_velocity);
        self.velocity_from_nanos(duration, initial_value, target_value, initial_velocity)
    }

    /// The value `play_time_millis` in, as `getValueFromMillis`.
    fn value_from_millis(
        &self,
        play_time_millis: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        self.value_from_nanos(
            play_time_millis.saturating_mul(MILLIS_TO_NANOS),
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    /// The velocity `play_time_millis` in, as `getVelocityFromMillis`.
    fn velocity_from_millis(
        &self,
        play_time_millis: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        self.velocity_from_nanos(
            play_time_millis.saturating_mul(MILLIS_TO_NANOS),
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    /// The duration in milliseconds, as `getDurationMillis`.
    fn duration_millis(&self, initial_value: f32, target_value: f32, initial_velocity: f32) -> i64 {
        self.duration_nanos(initial_value, target_value, initial_velocity) / MILLIS_TO_NANOS
    }
}

/// A tween of one float, as Compose's `FloatTweenSpec`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatTweenSpec {
    pub duration: u32,
    pub delay: u32,
    pub easing: Easing,
}

impl FloatTweenSpec {
    pub fn new(duration: u32, delay: u32, easing: Easing) -> FloatTweenSpec {
        FloatTweenSpec { duration, delay, easing }
    }

    fn duration_nanos_value(&self) -> i64 {
        i64::from(self.duration) * MILLIS_TO_NANOS
    }

    fn delay_nanos(&self) -> i64 {
        i64::from(self.delay) * MILLIS_TO_NANOS
    }

    fn clamp_play_time(&self, play_time_nanos: i64) -> i64 {
        play_time_nanos.saturating_sub(self.delay_nanos()).clamp(0, self.duration_nanos_value())
    }

    /// The eased fraction of the change `play_time_nanos` in.
    fn fraction(&self, play_time_nanos: i64) -> f32 {
        let clamped = self.clamp_play_time(play_time_nanos);
        let raw_fraction = if self.duration == 0 {
            1.0
        } else {
            clamped as f32 / self.duration_nanos_value() as f32
        };
        self.easing.transform(raw_fraction)
    }
}

impl Default for FloatTweenSpec {
    fn default() -> FloatTweenSpec {
        FloatTweenSpec::new(DEFAULT_DURATION_MILLIS, 0, FAST_OUT_SLOW_IN_EASING)
    }
}

impl FloatAnimationSpec for FloatTweenSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        target_value: f32,
        _initial_velocity: f32,
    ) -> f32 {
        lerp(initial_value, target_value, self.fraction(play_time_nanos))
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        let clamped = self.clamp_play_time(play_time_nanos);
        if clamped == 0 {
            return initial_velocity;
        }
        // As Compose's: the value a millisecond before, the play times taken as they are (a
        // delay is taken off them again).
        let start = self.value_from_nanos(
            clamped - MILLIS_TO_NANOS,
            initial_value,
            target_value,
            initial_velocity,
        );
        let end = self.value_from_nanos(clamped, initial_value, target_value, initial_velocity);
        (end - start) * 1000.0
    }

    fn duration_nanos(&self, _: f32, _: f32, _: f32) -> i64 {
        self.delay_nanos() + self.duration_nanos_value()
    }
}

/// A spring of one float, as Compose's `FloatSpringSpec`: it ends at the time the spring takes
/// to stay within `visibility_threshold` of its target (`estimateAnimationDurationMillis`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatSpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    pub visibility_threshold: f32,
}

impl FloatSpringSpec {
    pub fn new(damping_ratio: f32, stiffness: f32, visibility_threshold: f32) -> FloatSpringSpec {
        FloatSpringSpec { damping_ratio, stiffness, visibility_threshold }
    }

    fn simulation(&self, target_value: f32) -> SpringSimulation {
        SpringSimulation::new(target_value, self.damping_ratio, self.stiffness)
    }
}

impl Default for FloatSpringSpec {
    fn default() -> FloatSpringSpec {
        FloatSpringSpec::new(
            Spring::DAMPING_RATIO_NO_BOUNCY,
            Spring::STIFFNESS_MEDIUM,
            Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        )
    }
}

impl FloatAnimationSpec for FloatSpringSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        let millis = play_time_nanos / MILLIS_TO_NANOS;
        self.simulation(target_value).update_values(initial_value, initial_velocity, millis).0
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        target_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        let millis = play_time_nanos / MILLIS_TO_NANOS;
        self.simulation(target_value).update_values(initial_value, initial_velocity, millis).1
    }

    fn end_velocity(&self, _: f32, _: f32, _: f32) -> f32 {
        0.0
    }

    fn duration_nanos(&self, initial_value: f32, target_value: f32, initial_velocity: f32) -> i64 {
        if self.damping_ratio == 0.0 {
            return (i64::MAX / MILLIS_TO_NANOS) * MILLIS_TO_NANOS;
        }
        // In floats first, as Compose's.
        let stiffness = self.simulation(target_value).stiffness();
        let displacement = (initial_value - target_value) / self.visibility_threshold;
        let velocity = initial_velocity / self.visibility_threshold;
        estimate_animation_duration_millis(
            f64::from(stiffness),
            f64::from(self.damping_ratio),
            f64::from(velocity),
            f64::from(displacement),
            1.0,
        )
        .saturating_mul(MILLIS_TO_NANOS)
    }
}

/// A duration-based animation, as Compose's `TweenSpec`: from the start to the target over
/// `duration_millis`, after `delay_millis`, along `easing`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TweenSpec {
    pub duration_millis: u32,
    pub delay_millis: u32,
    pub easing: Easing,
}

impl TweenSpec {
    /// The same, starting `delay_millis` later.
    pub fn delay(self, delay_millis: u32) -> TweenSpec {
        TweenSpec { delay_millis, ..self }
    }

    /// The same, along `easing`.
    pub fn easing(self, easing: Easing) -> TweenSpec {
        TweenSpec { easing, ..self }
    }

    fn float(&self) -> FloatTweenSpec {
        FloatTweenSpec::new(self.duration_millis, self.delay_millis, self.easing)
    }
}

impl Default for TweenSpec {
    fn default() -> TweenSpec {
        tween(DEFAULT_DURATION_MILLIS)
    }
}

/// Applies a float spec to each component, as Compose's `VectorizedFloatAnimationSpec`.
fn each_component(
    initial_value: AnimationVector,
    target_value: AnimationVector,
    initial_velocity: AnimationVector,
    f: impl Fn(usize, f32, f32, f32) -> f32,
) -> AnimationVector {
    initial_value.map(|index, initial| {
        f(index, initial, target_value[index], component(initial_velocity, index))
    })
}

/// A vector's component, 0 past its size (a velocity of another size: none).
fn component(vector: AnimationVector, index: usize) -> f32 {
    vector.as_slice().get(index).copied().unwrap_or(0.0)
}

impl VectorizedAnimationSpec for TweenSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        // Each component's value is a `FloatTweenSpec`'s: the same eased fraction, computed once.
        let _ = initial_velocity;
        let fraction = self.float().fraction(play_time_nanos);
        initial_value.map(|index, from| lerp(from, component(target_value, index), fraction))
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        let float = self.float();
        each_component(initial_value, target_value, initial_velocity, |_, from, to, velocity| {
            float.velocity_from_nanos(play_time_nanos, from, to, velocity)
        })
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        (i64::from(self.delay_millis) + i64::from(self.duration_millis)) * MILLIS_TO_NANOS
    }
}

/// A physics-based animation, as Compose's `SpringSpec`: a damped spring pulling the value to the
/// target, keeping the velocity it had when interrupted. It ends when it stays within the
/// visibility threshold of the target: at the time Compose's spring estimation gives, each
/// component on its own threshold (0.01 without one, `Spring::DEFAULT_DISPLACEMENT_THRESHOLD`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    /// How close to the target counts as there, per component, in the value's units, as
    /// `visibilityThreshold`; one component applies to all; `None`: 0.01.
    pub visibility_threshold: Option<AnimationVector>,
}

/// Compose's `Spring` constants.
pub struct Spring;

impl Spring {
    pub const STIFFNESS_HIGH: f32 = 10_000.0;
    pub const STIFFNESS_MEDIUM: f32 = 1_500.0;
    pub const STIFFNESS_MEDIUM_LOW: f32 = 400.0;
    pub const STIFFNESS_LOW: f32 = 200.0;
    pub const STIFFNESS_VERY_LOW: f32 = 50.0;
    pub const DAMPING_RATIO_HIGH_BOUNCY: f32 = 0.2;
    pub const DAMPING_RATIO_MEDIUM_BOUNCY: f32 = 0.5;
    pub const DAMPING_RATIO_LOW_BOUNCY: f32 = 0.75;
    pub const DAMPING_RATIO_NO_BOUNCY: f32 = 1.0;
    /// The default displacement threshold, as `Spring.DefaultDisplacementThreshold`.
    pub const DEFAULT_DISPLACEMENT_THRESHOLD: f32 = 0.01;
}

impl SpringSpec {
    /// The same, ending within `threshold` of the target (per component), as `spring(…,
    /// visibilityThreshold)`: `spring().visibility_threshold(Dp(0.4))`.
    pub fn visibility_threshold<T: Animate>(self, threshold: T) -> SpringSpec {
        SpringSpec { visibility_threshold: Some(threshold.to_vector()), ..self }
    }

    /// Component `index`'s spring.
    fn float(&self, index: usize) -> FloatSpringSpec {
        let threshold = match self.visibility_threshold {
            Some(threshold) => component_or_first(threshold, index),
            None => Spring::DEFAULT_DISPLACEMENT_THRESHOLD,
        };
        FloatSpringSpec::new(self.damping_ratio, self.stiffness, threshold)
    }
}

/// A threshold's component `index`, or its first when it has fewer.
fn component_or_first(vector: AnimationVector, index: usize) -> f32 {
    vector.as_slice().get(index).copied().unwrap_or(vector[0])
}

impl Default for SpringSpec {
    fn default() -> SpringSpec {
        spring()
    }
}

impl VectorizedAnimationSpec for SpringSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        each_component(initial_value, target_value, initial_velocity, |index, from, to, speed| {
            self.float(index).value_from_nanos(play_time_nanos, from, to, speed)
        })
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        each_component(initial_value, target_value, initial_velocity, |index, from, to, speed| {
            self.float(index).velocity_from_nanos(play_time_nanos, from, to, speed)
        })
    }

    fn end_velocity(
        &self,
        initial_value: AnimationVector,
        _: AnimationVector,
        _: AnimationVector,
    ) -> AnimationVector {
        initial_value.new_instance()
    }

    fn duration_nanos(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64 {
        (0..initial_value.size())
            .map(|index| {
                self.float(index).duration_nanos(
                    initial_value[index],
                    target_value[index],
                    component(initial_velocity, index),
                )
            })
            .max()
            .unwrap_or(0)
    }
}

/// The target at once, after a delay, as Compose's `SnapSpec`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct SnapSpec {
    pub delay_millis: u32,
}

impl VectorizedAnimationSpec for SnapSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        _: AnimationVector,
    ) -> AnimationVector {
        if play_time_nanos < i64::from(self.delay_millis) * MILLIS_TO_NANOS {
            initial_value
        } else {
            target_value
        }
    }

    fn velocity_from_nanos(
        &self,
        _: i64,
        _: AnimationVector,
        _: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        initial_velocity
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        i64::from(self.delay_millis) * MILLIS_TO_NANOS
    }
}

/// An animation of a fixed duration, which a repeatable plays again, as Compose's
/// `DurationBasedAnimationSpec`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DurationBasedSpec {
    Tween(TweenSpec),
    Keyframes(KeyframesSpec),
    Snap(SnapSpec),
}

impl From<TweenSpec> for DurationBasedSpec {
    fn from(spec: TweenSpec) -> DurationBasedSpec {
        DurationBasedSpec::Tween(spec)
    }
}

impl From<KeyframesSpec> for DurationBasedSpec {
    fn from(spec: KeyframesSpec) -> DurationBasedSpec {
        DurationBasedSpec::Keyframes(spec)
    }
}

impl From<SnapSpec> for DurationBasedSpec {
    fn from(spec: SnapSpec) -> DurationBasedSpec {
        DurationBasedSpec::Snap(spec)
    }
}

impl DurationBasedSpec {
    /// How long it plays, without its delay, as `durationMillis`.
    pub fn duration_millis(&self) -> u32 {
        match self {
            DurationBasedSpec::Tween(tween) => tween.duration_millis,
            DurationBasedSpec::Keyframes(keyframes) => keyframes.duration_millis(),
            DurationBasedSpec::Snap(_) => 0,
        }
    }

    /// How long it waits before it starts, as `delayMillis`.
    pub fn delay_millis(&self) -> u32 {
        match self {
            DurationBasedSpec::Tween(tween) => tween.delay_millis,
            DurationBasedSpec::Keyframes(keyframes) => keyframes.delay_millis(),
            DurationBasedSpec::Snap(snap) => snap.delay_millis,
        }
    }

    fn total_nanos(&self) -> i64 {
        (i64::from(self.delay_millis()) + i64::from(self.duration_millis())) * MILLIS_TO_NANOS
    }

    fn spec(&self) -> &dyn VectorizedAnimationSpec {
        match self {
            DurationBasedSpec::Tween(tween) => tween,
            DurationBasedSpec::Keyframes(keyframes) => keyframes,
            DurationBasedSpec::Snap(snap) => snap,
        }
    }
}

impl VectorizedAnimationSpec for DurationBasedSpec {
    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.spec().value_from_nanos(play_time_nanos, initial_value, target_value, initial_velocity)
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.spec().velocity_from_nanos(
            play_time_nanos,
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        self.total_nanos()
    }
}

impl From<DurationBasedSpec> for AnimationSpec {
    fn from(spec: DurationBasedSpec) -> AnimationSpec {
        match spec {
            DurationBasedSpec::Tween(tween) => AnimationSpec::Tween(tween),
            DurationBasedSpec::Keyframes(keyframes) => AnimationSpec::Keyframes(keyframes),
            DurationBasedSpec::Snap(snap) => AnimationSpec::Snap(snap),
        }
    }
}

/// Whether a repetition's start offset delays it or fast-forwards it, as Compose's
/// `StartOffsetType`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StartOffsetType {
    /// The animation starts after the offset, as `Delay`.
    Delay,
    /// The animation starts as if it had played for the offset already, as `FastForward`.
    FastForward,
}

/// How far a repeatable animation's start is moved, as Compose's `StartOffset`: delayed or
/// fast-forwarded by `offset_millis`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StartOffset {
    /// Negative: a delay; positive: a fast-forward.
    value: i64,
}

impl StartOffset {
    /// A delay of `offset_millis`, as `StartOffset(offsetMillis)`.
    pub fn new(offset_millis: u32) -> StartOffset {
        StartOffset::with_type(offset_millis, StartOffsetType::Delay)
    }

    /// An offset of `offset_millis` of `offset_type`, as `StartOffset(offsetMillis, offsetType)`.
    pub fn with_type(offset_millis: u32, offset_type: StartOffsetType) -> StartOffset {
        let sign = match offset_type {
            StartOffsetType::Delay => -1,
            StartOffsetType::FastForward => 1,
        };
        StartOffset { value: i64::from(offset_millis) * sign }
    }

    /// The offset's length, as `offsetMillis`.
    pub fn offset_millis(&self) -> u32 {
        self.value.unsigned_abs() as u32
    }

    /// Whether it delays or fast-forwards, as `offsetType`.
    pub fn offset_type(&self) -> StartOffsetType {
        if self.value > 0 { StartOffsetType::FastForward } else { StartOffsetType::Delay }
    }

    fn nanos(&self) -> i64 {
        self.value * MILLIS_TO_NANOS
    }
}

/// An animation repeated, as Compose's `RepeatableSpec` and `InfiniteRepeatableSpec`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RepeatableSpec {
    /// How many times it plays; `None`: forever.
    pub iterations: Option<u32>,
    pub animation: DurationBasedSpec,
    pub repeat_mode: RepeatMode,
    /// Where the first repetition starts, as `initialStartOffset`.
    pub initial_start_offset: StartOffset,
}

/// How a repeated animation goes again, as Compose's `RepeatMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepeatMode {
    /// From the start each time.
    Restart,
    /// Back and forth.
    Reverse,
}

impl RepeatableSpec {
    fn repetition_nanos(&self) -> i64 {
        self.animation.total_nanos()
    }

    /// The play time within the repetition `play_time_nanos` is in, as Compose's
    /// `repetitionPlayTimeNanos`: a reversed repetition plays the animation backwards in time.
    fn repetition_play_time_nanos(&self, play_time_nanos: i64) -> i64 {
        let offset = self.initial_start_offset.nanos();
        let post_offset = play_time_nanos.saturating_add(offset);
        if post_offset <= 0 {
            return 0;
        }
        let duration = self.repetition_nanos().max(1);
        let mut repeats = post_offset / duration;
        if let Some(iterations) = self.iterations {
            repeats = repeats.min(i64::from(iterations) - 1);
        }
        if self.repeat_mode == RepeatMode::Restart || repeats % 2 == 0 {
            post_offset - repeats * duration
        } else {
            (repeats + 1) * duration - post_offset
        }
    }

    /// The velocity a repetition starts with, as Compose's `repetitionStartVelocity`: after the
    /// first, the velocity at the end of a repetition. The finite form reads it with the
    /// arguments in Compose's order (the target in the velocity's place, the velocity in the
    /// target's), so its values are Compose's.
    fn repetition_start_velocity(
        &self,
        play_time_nanos: i64,
        start: AnimationVector,
        start_velocity: AnimationVector,
        end: AnimationVector,
    ) -> AnimationVector {
        let offset = self.initial_start_offset.nanos();
        let duration = self.repetition_nanos();
        if play_time_nanos.saturating_add(offset) <= duration {
            return start_velocity;
        }
        match self.iterations {
            None => {
                self.animation.velocity_from_nanos(duration - offset, start, end, start_velocity)
            }
            Some(_) => self.velocity_from_nanos(duration - offset, start, start_velocity, end),
        }
    }
}

impl VectorizedAnimationSpec for RepeatableSpec {
    fn is_infinite(&self) -> bool {
        self.iterations.is_none()
    }

    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        // Compose passes the repetition's start velocity, but no duration-based spec's value
        // depends on its initial velocity (a tween's, keyframes' or snap's): it isn't computed
        // (an infinite transition's values are read every frame).
        self.animation.value_from_nanos(
            self.repetition_play_time_nanos(play_time_nanos),
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.animation.velocity_from_nanos(
            self.repetition_play_time_nanos(play_time_nanos),
            initial_value,
            target_value,
            self.repetition_start_velocity(
                play_time_nanos,
                initial_value,
                initial_velocity,
                target_value,
            ),
        )
    }

    fn duration_nanos(&self, _: AnimationVector, _: AnimationVector, _: AnimationVector) -> i64 {
        match self.iterations {
            None => i64::MAX,
            Some(iterations) => i64::from(iterations)
                .saturating_mul(self.repetition_nanos())
                .saturating_sub(self.initial_start_offset.nanos()),
        }
    }
}

/// How a value animates to its target, as Compose's `AnimationSpec`.
#[derive(Clone, Copy, PartialEq, Debug)]
#[non_exhaustive]
pub enum AnimationSpec {
    Tween(TweenSpec),
    Spring(SpringSpec),
    /// At once, after a delay, as `SnapSpec`.
    Snap(SnapSpec),
    Repeatable(RepeatableSpec),
    /// Values at given times, as `KeyframesSpec`.
    Keyframes(KeyframesSpec),
}

impl From<KeyframesSpec> for AnimationSpec {
    fn from(spec: KeyframesSpec) -> AnimationSpec {
        AnimationSpec::Keyframes(spec)
    }
}

impl From<TweenSpec> for AnimationSpec {
    fn from(spec: TweenSpec) -> AnimationSpec {
        AnimationSpec::Tween(spec)
    }
}

impl From<SpringSpec> for AnimationSpec {
    fn from(spec: SpringSpec) -> AnimationSpec {
        AnimationSpec::Spring(spec)
    }
}

impl From<SnapSpec> for AnimationSpec {
    fn from(spec: SnapSpec) -> AnimationSpec {
        AnimationSpec::Snap(spec)
    }
}

impl From<RepeatableSpec> for AnimationSpec {
    fn from(spec: RepeatableSpec) -> AnimationSpec {
        AnimationSpec::Repeatable(spec)
    }
}

impl Default for AnimationSpec {
    fn default() -> AnimationSpec {
        AnimationSpec::Spring(spring())
    }
}

impl AnimationSpec {
    fn spec(&self) -> &dyn VectorizedAnimationSpec {
        match self {
            AnimationSpec::Tween(tween) => tween,
            AnimationSpec::Spring(spring) => spring,
            AnimationSpec::Snap(snap) => snap,
            AnimationSpec::Repeatable(repeatable) => repeatable,
            AnimationSpec::Keyframes(keyframes) => keyframes,
        }
    }
}

impl VectorizedAnimationSpec for AnimationSpec {
    fn is_infinite(&self) -> bool {
        self.spec().is_infinite()
    }

    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.spec().value_from_nanos(play_time_nanos, initial_value, target_value, initial_velocity)
    }

    fn velocity_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.spec().velocity_from_nanos(
            play_time_nanos,
            initial_value,
            target_value,
            initial_velocity,
        )
    }

    fn duration_nanos(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> i64 {
        self.spec().duration_nanos(initial_value, target_value, initial_velocity)
    }

    fn end_velocity(
        &self,
        initial_value: AnimationVector,
        target_value: AnimationVector,
        initial_velocity: AnimationVector,
    ) -> AnimationVector {
        self.spec().end_velocity(initial_value, target_value, initial_velocity)
    }
}

/// A tween of `duration_millis` along `FastOutSlowInEasing`, as Compose's `tween(durationMillis)`;
/// `.delay(…)` and `.easing(…)` change the rest.
pub fn tween(duration_millis: u32) -> TweenSpec {
    TweenSpec { duration_millis, delay_millis: 0, easing: FAST_OUT_SLOW_IN_EASING }
}

/// The default spring: no bounce, medium stiffness, as Compose's `spring()`;
/// `.visibility_threshold(…)` sets its threshold.
pub fn spring() -> SpringSpec {
    SpringSpec {
        damping_ratio: Spring::DAMPING_RATIO_NO_BOUNCY,
        stiffness: Spring::STIFFNESS_MEDIUM,
        visibility_threshold: None,
    }
}

/// A spring of the given damping ratio and stiffness, as `spring(dampingRatio, stiffness)`.
pub fn spring_with(damping_ratio: f32, stiffness: f32) -> SpringSpec {
    SpringSpec { damping_ratio, stiffness, visibility_threshold: None }
}

/// The target at once, after `delay_millis`, as Compose's `snap(delayMillis)`.
pub fn snap(delay_millis: u32) -> SnapSpec {
    SnapSpec { delay_millis }
}

/// `animation` played `iterations` times (at least 1), as Compose's `repeatable`.
pub fn repeatable(
    iterations: u32,
    animation: impl Into<DurationBasedSpec>,
    repeat_mode: RepeatMode,
) -> RepeatableSpec {
    repeatable_with(iterations, animation, repeat_mode, StartOffset::default())
}

/// The same, its first repetition moved by `initial_start_offset`, as
/// `repeatable(…, initialStartOffset)`.
pub fn repeatable_with(
    iterations: u32,
    animation: impl Into<DurationBasedSpec>,
    repeat_mode: RepeatMode,
    initial_start_offset: StartOffset,
) -> RepeatableSpec {
    assert!(iterations >= 1, "a repeatable's iterations can't be less than 1");
    RepeatableSpec {
        iterations: Some(iterations),
        animation: animation.into(),
        repeat_mode,
        initial_start_offset,
    }
}

/// `animation` played forever, as Compose's `infiniteRepeatable`. It can't take no time.
pub fn infinite_repeatable(
    animation: impl Into<DurationBasedSpec>,
    repeat_mode: RepeatMode,
) -> RepeatableSpec {
    infinite_repeatable_with(animation, repeat_mode, StartOffset::default())
}

/// The same, its first repetition moved by `initial_start_offset`, as
/// `infiniteRepeatable(…, initialStartOffset)`.
pub fn infinite_repeatable_with(
    animation: impl Into<DurationBasedSpec>,
    repeat_mode: RepeatMode,
    initial_start_offset: StartOffset,
) -> RepeatableSpec {
    let animation = animation.into();
    assert!(
        animation.total_nanos() > 0,
        "an animation to be infinitely repeated can't have a 0 duration"
    );
    RepeatableSpec { iterations: None, animation, repeat_mode, initial_start_offset }
}

macro_rules! by_value {
    ($($type:ty),*) => {
        $(impl compose_runtime::ComposeArg for $type {
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

by_value!(
    AnimationSpec,
    TweenSpec,
    SpringSpec,
    SnapSpec,
    RepeatableSpec,
    DurationBasedSpec,
    crate::easing::Easing
);
