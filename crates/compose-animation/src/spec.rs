//! Animation specs, as Compose's `AnimationSpec`s: `tween`, `spring`, `snap`, `repeatable` and
//! `infinite_repeatable`, evaluated per component of an animated value's vector.

use crate::easing::{Easing, FAST_OUT_SLOW_IN_EASING};

/// The default duration of a `tween`, as Compose's `AnimationConstants.DefaultDurationMillis`.
pub const DEFAULT_DURATION_MILLIS: u32 = 300;

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

    fn total_nanos(&self) -> u64 {
        (u64::from(self.delay_millis) + u64::from(self.duration_millis)) * 1_000_000
    }

    fn fraction(&self, play_nanos: u64) -> f32 {
        let delay = u64::from(self.delay_millis) * 1_000_000;
        let duration = u64::from(self.duration_millis) * 1_000_000;
        if duration == 0 {
            return if play_nanos >= delay { 1.0 } else { 0.0 };
        }
        let elapsed = play_nanos.saturating_sub(delay).min(duration);
        self.easing.transform(elapsed as f32 / duration as f32)
    }
}

/// A physics-based animation, as Compose's `SpringSpec`: a damped spring pulling the value to the
/// target, keeping the velocity it had when interrupted.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    /// How close to the target (and how slow) counts as there, in the value's units; `None`:
    /// the value type's own.
    pub visibility_threshold: Option<f32>,
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

/// A velocity under the threshold times this counts as at rest, as Compose's spring simulation
/// (`VelocityThresholdMultiplier`, per 16 ms frame).
const VELOCITY_THRESHOLD_MULTIPLIER: f32 = 1000.0 / 16.0;

impl SpringSpec {
    /// The value and velocity `play_nanos` after a start at `start` moving at `velocity`
    /// towards `target`, as Compose's `SpringSimulation.updateValues`.
    fn at(&self, play_nanos: u64, start: f32, target: f32, velocity: f32) -> (f32, f32) {
        let time = play_nanos as f64 / 1e9;
        let displacement = f64::from(start - target);
        let velocity = f64::from(velocity);
        let damping = f64::from(self.damping_ratio);
        let natural = f64::from(self.stiffness).sqrt();
        let r = -damping * natural;
        let (position, speed) = if damping > 1.0 {
            let s = natural * (damping * damping - 1.0).sqrt();
            let (plus, minus) = (r + s, r - s);
            let b = (minus * displacement - velocity) / (minus - plus);
            let a = displacement - b;
            (
                a * (minus * time).exp() + b * (plus * time).exp(),
                a * minus * (minus * time).exp() + b * plus * (plus * time).exp(),
            )
        } else if damping == 1.0 {
            let a = displacement;
            let b = velocity + natural * displacement;
            let decay = (-natural * time).exp();
            ((a + b * time) * decay, (a + b * time) * decay * -natural + b * decay)
        } else {
            let damped = natural * (1.0 - damping * damping).sqrt();
            let cos = displacement;
            let sin = (-r * displacement + velocity) / damped;
            let decay = (r * time).exp();
            let angle = damped * time;
            let position = decay * (cos * angle.cos() + sin * angle.sin());
            (
                position,
                position * r + decay * (-damped * cos * angle.sin() + damped * sin * angle.cos()),
            )
        };
        (position as f32 + target, speed as f32)
    }
}

/// An animation repeated, as Compose's `RepeatableSpec` and `InfiniteRepeatableSpec`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RepeatableSpec {
    /// How many times it plays; `None`: forever.
    pub iterations: Option<u32>,
    pub animation: TweenSpec,
    pub repeat_mode: RepeatMode,
}

/// How a repeated animation goes again, as Compose's `RepeatMode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RepeatMode {
    /// From the start each time.
    Restart,
    /// Back and forth.
    Reverse,
}

/// How a value animates to its target, as Compose's `AnimationSpec`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum AnimationSpec {
    Tween(TweenSpec),
    Spring(SpringSpec),
    /// At once, after a delay, as `SnapSpec`.
    Snap {
        delay_millis: u32,
    },
    Repeatable(RepeatableSpec),
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

/// A tween of `duration_millis` along `FastOutSlowInEasing`, as Compose's `tween(durationMillis)`;
/// `.delay(…)` and `.easing(…)` change the rest.
pub fn tween(duration_millis: u32) -> TweenSpec {
    TweenSpec { duration_millis, delay_millis: 0, easing: FAST_OUT_SLOW_IN_EASING }
}

/// The default spring: no bounce, medium stiffness, as Compose's `spring()`.
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
pub fn snap(delay_millis: u32) -> AnimationSpec {
    AnimationSpec::Snap { delay_millis }
}

/// `animation` played `iterations` times, as Compose's `repeatable`.
pub fn repeatable(
    iterations: u32,
    animation: TweenSpec,
    repeat_mode: RepeatMode,
) -> RepeatableSpec {
    RepeatableSpec { iterations: Some(iterations.max(1)), animation, repeat_mode }
}

/// `animation` played forever, as Compose's `infiniteRepeatable`.
pub fn infinite_repeatable(animation: TweenSpec, repeat_mode: RepeatMode) -> RepeatableSpec {
    RepeatableSpec { iterations: None, animation, repeat_mode }
}

/// One frame of an animation of a vector: its values and velocities, and whether it is over.
pub(crate) struct Frame<const N: usize> {
    pub values: [f32; N],
    pub velocities: [f32; N],
    pub finished: bool,
}

impl AnimationSpec {
    /// The animation `play_nanos` after it started at `start` with `velocity`, towards `target`;
    /// `threshold` is the value type's (per component) for springs without their own.
    pub(crate) fn frame<const N: usize>(
        &self,
        play_nanos: u64,
        start: [f32; N],
        target: [f32; N],
        velocity: [f32; N],
        threshold: f32,
    ) -> Frame<N> {
        match self {
            AnimationSpec::Tween(tween) => {
                let fraction = tween.fraction(play_nanos);
                let earlier = tween.fraction(play_nanos.saturating_sub(1_000_000));
                let values = lerp(start, target, fraction);
                let previous = lerp(start, target, earlier);
                let mut velocities = [0.0; N];
                for index in 0..N {
                    velocities[index] = (values[index] - previous[index]) * 1000.0;
                }
                Frame { values, velocities, finished: play_nanos >= tween.total_nanos() }
            }
            AnimationSpec::Snap { delay_millis } => {
                let finished = play_nanos >= u64::from(*delay_millis) * 1_000_000;
                Frame {
                    values: if finished { target } else { start },
                    velocities: [0.0; N],
                    finished,
                }
            }
            AnimationSpec::Spring(spring) => {
                let threshold = spring.visibility_threshold.unwrap_or(threshold);
                let mut values = [0.0; N];
                let mut velocities = [0.0; N];
                let mut finished = true;
                for index in 0..N {
                    let (value, speed) =
                        spring.at(play_nanos, start[index], target[index], velocity[index]);
                    values[index] = value;
                    velocities[index] = speed;
                    finished &= (value - target[index]).abs() < threshold
                        && speed.abs() < threshold * VELOCITY_THRESHOLD_MULTIPLIER;
                }
                if finished {
                    return Frame { values: target, velocities: [0.0; N], finished };
                }
                Frame { values, velocities, finished }
            }
            AnimationSpec::Repeatable(repeat) => {
                // As Compose's `VectorizedRepeatableSpec`: a reversed repetition plays the
                // animation backwards in time (`(count + 1) × duration − play time`).
                let duration = repeat.animation.total_nanos().max(1);
                let total = repeat.iterations.map(|count| u64::from(count) * duration);
                let finished = total.is_some_and(|total| play_nanos >= total);
                let play = total.map_or(play_nanos, |total| play_nanos.min(total));
                let count = play / duration;
                let within = if repeat.repeat_mode == RepeatMode::Restart || count % 2 == 0 {
                    play - count * duration
                } else {
                    (count + 1) * duration - play
                };
                let frame = AnimationSpec::Tween(repeat.animation)
                    .frame(within, start, target, velocity, threshold);
                Frame { finished, ..frame }
            }
        }
    }
}

fn lerp<const N: usize>(start: [f32; N], end: [f32; N], fraction: f32) -> [f32; N] {
    let mut out = [0.0; N];
    for index in 0..N {
        out[index] = start[index] + (end[index] - start[index]) * fraction;
    }
    out
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

by_value!(AnimationSpec, TweenSpec, SpringSpec, RepeatableSpec, crate::easing::Easing);
