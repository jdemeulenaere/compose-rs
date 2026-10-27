//! The spline-based decay of flings, as Compose's `SplineBasedFloatDecayAnimationSpec`: Android's
//! `OverScroller` curve (`AndroidFlingSpline`, `FlingCalculator`), in Kotlin's numeric semantics
//! (floats where Compose computes in `Float`, doubles where it widens).

use compose_runtime::remember_keyed;
use compose_ui::{Density, LOCAL_DENSITY};

use crate::decay::{DecayAnimationSpec, FloatDecayAnimationSpec, generate_decay_animation_spec};

const INFLECTION: f32 = 0.35;
const START_TENSION: f32 = 0.5;
const END_TENSION: f32 = 1.0;
const P1: f32 = START_TENSION * INFLECTION;
const P2: f32 = 1.0 - END_TENSION * (1.0 - INFLECTION);
const SAMPLES: usize = 100;

/// Android's scroll friction, as `ViewConfiguration.getScrollFriction()`
/// (`platformFlingScrollFriction`).
const SCROLL_FRICTION: f32 = 0.015;
const GRAVITY_EARTH: f32 = 9.80665;
const INCHES_PER_METER: f32 = 39.37;

/// Kotlin's `sign`: 0 for 0 (and NaN for NaN).
fn sign(x: f32) -> f32 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        x
    }
}

/// The spline's samples, as Compose's `computeSplineInfo`: positions and times.
fn spline_info() -> ([f32; SAMPLES + 1], [f32; SAMPLES + 1]) {
    let mut positions = [0.0f32; SAMPLES + 1];
    let mut times = [0.0f32; SAMPLES + 1];
    let (mut x_min, mut y_min) = (0.0f32, 0.0f32);
    for i in 0..SAMPLES {
        let alpha = i as f32 / SAMPLES as f32;
        let mut x_max = 1.0f32;
        let (mut x, mut coef);
        loop {
            x = x_min + (x_max - x_min) / 2.0;
            coef = 3.0 * x * (1.0 - x);
            let tx = coef * ((1.0 - x) * P1 + x * P2) + x * x * x;
            // Kotlin compares the float with the double `1E-5`.
            if f64::from((tx - alpha).abs()) < 1e-5 {
                break;
            }
            if tx > alpha {
                x_max = x;
            } else {
                x_min = x;
            }
        }
        positions[i] = coef * ((1.0 - x) * START_TENSION + x) + x * x * x;
        let mut y_max = 1.0f32;
        let mut y;
        loop {
            y = y_min + (y_max - y_min) / 2.0;
            coef = 3.0 * y * (1.0 - y);
            let dy = coef * ((1.0 - y) * START_TENSION + y) + y * y * y;
            if f64::from((dy - alpha).abs()) < 1e-5 {
                break;
            }
            if dy > alpha {
                y_max = y;
            } else {
                y_min = y;
            }
        }
        times[i] = coef * ((1.0 - y) * P1 + y * P2) + y * y * y;
    }
    times[SAMPLES] = 1.0;
    positions[SAMPLES] = times[SAMPLES];
    (positions, times)
}

thread_local! {
    static SPLINE_POSITIONS: [f32; SAMPLES + 1] = spline_info().0;
}

/// The fraction of a fling's distance and its velocity coefficient at a fraction of its
/// duration, as Compose's `AndroidFlingSpline.FlingResult`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FlingResult {
    pub distance_coefficient: f32,
    pub velocity_coefficient: f32,
}

/// The spline at `time` (a fraction of the duration, clamped to 0..1), as Compose's
/// `AndroidFlingSpline.flingPosition`.
pub fn fling_position(time: f32) -> FlingResult {
    let clamped = time.clamp(0.0, 1.0);
    let index = (SAMPLES as f32 * clamped) as usize;
    let (mut distance, mut velocity) = (1.0f32, 0.0f32);
    if index < SAMPLES {
        SPLINE_POSITIONS.with(|positions| {
            let t_inf = index as f32 / SAMPLES as f32;
            let t_sup = (index + 1) as f32 / SAMPLES as f32;
            let (d_inf, d_sup) = (positions[index], positions[index + 1]);
            velocity = (d_sup - d_inf) / (t_sup - t_inf);
            distance = d_inf + (clamped - t_inf) * velocity;
        });
    }
    FlingResult { distance_coefficient: distance, velocity_coefficient: velocity }
}

/// `ln(INFLECTION × |velocity| / friction)`, as `AndroidFlingSpline.deceleration`.
fn deceleration(velocity: f32, friction: f32) -> f64 {
    (f64::from(INFLECTION * velocity.abs()) / f64::from(friction)).ln()
}

/// `(ln(0.78) / ln(0.9)).toFloat()`, as Compose's `DecelerationRate`.
fn deceleration_rate() -> f32 {
    (0.78f64.ln() / 0.9f64.ln()) as f32
}

/// A fling's duration and distance, as Compose's `FlingCalculator`.
#[derive(Clone, Copy, PartialEq, Debug)]
struct FlingCalculator {
    friction: f32,
    magic_physical_coefficient: f32,
}

/// A fling from a velocity, as `FlingCalculator.FlingInfo`.
struct FlingInfo {
    initial_velocity: f32,
    distance: f32,
    duration: i64,
}

impl FlingCalculator {
    fn new(friction: f32, density: f32) -> FlingCalculator {
        let magic = GRAVITY_EARTH * INCHES_PER_METER * density * 160.0 * 0.84;
        FlingCalculator { friction, magic_physical_coefficient: magic }
    }

    fn spline_deceleration(&self, velocity: f32) -> f64 {
        deceleration(velocity, self.friction * self.magic_physical_coefficient)
    }

    fn duration(&self, velocity: f32) -> i64 {
        let l = self.spline_deceleration(velocity);
        let decel_minus_one = f64::from(deceleration_rate()) - 1.0;
        (1000.0 * (l / decel_minus_one).exp()) as i64
    }

    fn distance(&self, velocity: f32) -> f32 {
        let l = self.spline_deceleration(velocity);
        let rate = f64::from(deceleration_rate());
        let decel_minus_one = rate - 1.0;
        (f64::from(self.friction * self.magic_physical_coefficient)
            * (rate / decel_minus_one * l).exp()) as f32
    }

    fn info(&self, velocity: f32) -> FlingInfo {
        FlingInfo {
            initial_velocity: velocity,
            distance: self.distance(velocity),
            duration: self.duration(velocity),
        }
    }
}

impl FlingInfo {
    fn spline_position(&self, time: i64) -> f32 {
        if self.duration > 0 { time as f32 / self.duration as f32 } else { 1.0 }
    }

    fn position(&self, time: i64) -> f32 {
        let spline = fling_position(self.spline_position(time));
        self.distance * sign(self.initial_velocity) * spline.distance_coefficient
    }

    fn velocity(&self, time: i64) -> f32 {
        let spline = fling_position(self.spline_position(time));
        spline.velocity_coefficient * sign(self.initial_velocity) * self.distance
            / self.duration as f32
            * 1000.0
    }
}

/// The decay of a fling on Android, as Compose's `SplineBasedFloatDecayAnimationSpec(density)`:
/// the `OverScroller` spline, at the platform's scroll friction.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct SplineBasedFloatDecayAnimationSpec {
    calculator: FlingCalculator,
}

impl SplineBasedFloatDecayAnimationSpec {
    /// At `density`, as `SplineBasedFloatDecayAnimationSpec(density)`.
    pub fn new(density: Density) -> SplineBasedFloatDecayAnimationSpec {
        SplineBasedFloatDecayAnimationSpec {
            calculator: FlingCalculator::new(SCROLL_FRICTION, density.density),
        }
    }

    fn fling_distance(&self, velocity: f32) -> f32 {
        self.calculator.distance(velocity) * sign(velocity)
    }
}

impl FloatDecayAnimationSpec for SplineBasedFloatDecayAnimationSpec {
    fn abs_velocity_threshold(&self) -> f32 {
        0.0
    }

    fn value_from_nanos(
        &self,
        play_time_nanos: i64,
        initial_value: f32,
        initial_velocity: f32,
    ) -> f32 {
        let millis = play_time_nanos / 1_000_000;
        initial_value + self.calculator.info(initial_velocity).position(millis)
    }

    fn duration_nanos(&self, _: f32, initial_velocity: f32) -> i64 {
        self.calculator.duration(initial_velocity).saturating_mul(1_000_000)
    }

    fn velocity_from_nanos(&self, play_time_nanos: i64, _: f32, initial_velocity: f32) -> f32 {
        let millis = play_time_nanos / 1_000_000;
        self.calculator.info(initial_velocity).velocity(millis)
    }

    fn target_value(&self, initial_value: f32, initial_velocity: f32) -> f32 {
        initial_value + self.fling_distance(initial_velocity)
    }
}

/// The spline-based decay of any type at `density`, as Compose's `splineBasedDecay(density)`.
pub fn spline_based_decay(density: Density) -> DecayAnimationSpec {
    generate_decay_animation_spec(SplineBasedFloatDecayAnimationSpec::new(density))
}

/// The same at the composition's density, remembered while it stays, as Compose's
/// `rememberSplineBasedDecay()`.
pub fn remember_spline_based_decay() -> DecayAnimationSpec {
    let density = LOCAL_DENSITY.current();
    remember_keyed(DensityKey(density.density), move || spline_based_decay(density))
}

#[derive(Clone, Copy, PartialEq)]
struct DensityKey(f32);

impl compose_runtime::ComposeArg for DensityKey {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}
