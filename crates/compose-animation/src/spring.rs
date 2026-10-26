//! The damped spring's equations and the estimation of how long it takes to settle, as Compose's
//! `SpringSimulation` and `SpringEstimation`.

/// A damped spring pulling a value to `final_position`, as Compose's `SpringSimulation`: its
/// position and velocity a given time after a start, in closed form.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct SpringSimulation {
    pub final_position: f32,
    natural_frequency: f64,
    pub damping_ratio: f32,
}

impl SpringSimulation {
    /// A spring of `stiffness` and `damping_ratio` (≥ 0) to `final_position`.
    pub fn new(final_position: f32, damping_ratio: f32, stiffness: f32) -> SpringSimulation {
        assert!(stiffness > 0.0, "a spring's stiffness must be positive");
        assert!(damping_ratio >= 0.0, "a spring's damping ratio must not be negative");
        SpringSimulation {
            final_position,
            natural_frequency: f64::from(stiffness).sqrt(),
            damping_ratio,
        }
    }

    pub fn stiffness(&self) -> f32 {
        (self.natural_frequency * self.natural_frequency) as f32
    }

    /// The position and velocity `time_elapsed_millis` after leaving `last_displacement` at
    /// `last_velocity`, as `updateValues`: computed in doubles from floats, as Compose's, so the
    /// values are the same to the last bit.
    pub fn update_values(
        &self,
        last_displacement: f32,
        last_velocity: f32,
        time_elapsed_millis: i64,
    ) -> (f32, f32) {
        let adjusted_displacement = f64::from(last_displacement - self.final_position);
        let last_velocity = f64::from(last_velocity);
        let delta_t = time_elapsed_millis as f64 / 1000.0;
        let damping_ratio = f64::from(self.damping_ratio);
        let damping_ratio_squared = damping_ratio * damping_ratio;
        let natural = self.natural_frequency;
        let r = -damping_ratio * natural;
        let (displacement, velocity) = if self.damping_ratio > 1.0 {
            let s = natural * (damping_ratio_squared - 1.0).sqrt();
            let gamma_plus = r + s;
            let gamma_minus = r - s;
            let coeff_b =
                (gamma_minus * adjusted_displacement - last_velocity) / (gamma_minus - gamma_plus);
            let coeff_a = adjusted_displacement - coeff_b;
            (
                coeff_a * (gamma_minus * delta_t).exp() + coeff_b * (gamma_plus * delta_t).exp(),
                coeff_a * gamma_minus * (gamma_minus * delta_t).exp()
                    + coeff_b * gamma_plus * (gamma_plus * delta_t).exp(),
            )
        } else if self.damping_ratio == 1.0 {
            let coeff_a = adjusted_displacement;
            let coeff_b = last_velocity + natural * adjusted_displacement;
            let decay = (-natural * delta_t).exp();
            let displacement = (coeff_a + coeff_b * delta_t) * decay;
            (displacement, displacement * -natural + coeff_b * decay)
        } else {
            let damped = natural * (1.0 - damping_ratio_squared).sqrt();
            let cos_coeff = adjusted_displacement;
            let sin_coeff = (1.0 / damped) * (-r * adjusted_displacement + last_velocity);
            let angle = damped * delta_t;
            let decay = (r * delta_t).exp();
            let displacement = decay * (cos_coeff * angle.cos() + sin_coeff * angle.sin());
            (
                displacement,
                displacement * r
                    + decay
                        * (-damped * cos_coeff * angle.sin() + damped * sin_coeff * angle.cos()),
            )
        };
        ((displacement + f64::from(self.final_position)) as f32, velocity as f32)
    }
}

/// `i64::MAX` nanoseconds in milliseconds: an undamped spring's duration.
const MAX_LONG_MILLIS: i64 = i64::MAX / 1_000_000;

/// How long a spring of `stiffness` and `damping_ratio` takes, from `initial_displacement` at
/// `initial_velocity`, to stay within `delta` of its target, in milliseconds, as Compose's
/// `estimateAnimationDurationMillis`: an undamped spring (`damping_ratio` 0) never settles.
#[doc(hidden)]
pub fn estimate_animation_duration_millis(
    stiffness: f64,
    damping_ratio: f64,
    initial_velocity: f64,
    initial_displacement: f64,
    delta: f64,
) -> i64 {
    if damping_ratio == 0.0 {
        return MAX_LONG_MILLIS;
    }
    let damping_coefficient = 2.0 * damping_ratio * stiffness.sqrt();
    let partial_root = damping_coefficient * damping_coefficient - 4.0 * stiffness;
    let (root_real, root_imaginary) = if partial_root < 0.0 {
        (0.0, partial_root.abs().sqrt())
    } else {
        (partial_root.sqrt(), 0.0)
    };
    estimate_duration(
        (-damping_coefficient + root_real) * 0.5,
        root_imaginary * 0.5,
        (-damping_coefficient - root_real) * 0.5,
        damping_ratio,
        initial_velocity,
        initial_displacement,
        delta,
    )
}

/// The same for a spring of `spring_constant`, `damping_coefficient` and `mass`, as Compose's
/// other `estimateAnimationDurationMillis`.
#[doc(hidden)]
pub fn estimate_animation_duration_millis_of_mass(
    spring_constant: f64,
    damping_coefficient: f64,
    mass: f64,
    initial_velocity: f64,
    initial_displacement: f64,
    delta: f64,
) -> i64 {
    let critical_damping = 2.0 * (spring_constant * mass).sqrt();
    let damping_ratio = damping_coefficient / critical_damping;
    let partial_root = damping_coefficient * damping_coefficient - 4.0 * mass * spring_constant;
    let divisor = 1.0 / (2.0 * mass);
    let (root_real, root_imaginary) = if partial_root < 0.0 {
        (0.0, partial_root.abs().sqrt())
    } else {
        (partial_root.sqrt(), 0.0)
    };
    estimate_duration(
        (-damping_coefficient + root_real) * divisor,
        root_imaginary * divisor,
        (-damping_coefficient - root_real) * divisor,
        damping_ratio,
        initial_velocity,
        initial_displacement,
        delta,
    )
}

fn estimate_duration(
    first_root_real: f64,
    first_root_imaginary: f64,
    second_root_real: f64,
    damping_ratio: f64,
    initial_velocity: f64,
    initial_position: f64,
    delta: f64,
) -> i64 {
    if initial_position == 0.0 && initial_velocity == 0.0 {
        return 0;
    }
    // As if the spring started on the positive side.
    let v0 = if initial_position < 0.0 { -initial_velocity } else { initial_velocity };
    let p0 = initial_position.abs();
    let seconds = if damping_ratio > 1.0 {
        estimate_over_damped(first_root_real, second_root_real, p0, v0, delta)
    } else if damping_ratio < 1.0 {
        estimate_under_damped(first_root_real, first_root_imaginary, p0, v0, delta)
    } else {
        estimate_critically_damped(first_root_real, p0, v0, delta)
    };
    // As Kotlin's `Double.toLong`: NaN is 0, out of range saturates.
    (seconds * 1000.0) as i64
}

/// The envelope of the oscillation reaching `delta`.
fn estimate_under_damped(r: f64, imaginary: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c1 = p0;
    let c2 = (v0 - r * c1) / imaginary;
    let c = (c1 * c1 + c2 * c2).sqrt();
    (delta / c).ln() / r
}

fn estimate_critically_damped(r: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c1 = p0;
    let c2 = v0 - r * c1;
    let t1 = (delta / c1).abs().ln() / r;
    let t2 = {
        let guess = (delta / c2).abs().ln();
        let mut t = guess;
        for _ in 0..=5 {
            t = guess - (t / r).abs().ln();
        }
        t
    } / r;
    let mut t_current = if !t1.is_finite() {
        t2
    } else if !t2.is_finite() {
        t1
    } else {
        t1.max(t2)
    };
    let t_inflection = -(r * c1 + c2) / (r * c2);
    let x_inflection = c1 * (r * t_inflection).exp() + c2 * t_inflection * (r * t_inflection).exp();
    let signed_delta = if t_inflection.is_nan() || t_inflection <= 0.0 {
        -delta
    } else if t_inflection > 0.0 && -x_inflection < delta {
        if c2 < 0.0 && c1 > 0.0 {
            t_current = 0.0;
        }
        -delta
    } else {
        t_current = -(2.0 / r) - (c1 / c2);
        delta
    };
    let mut t_delta = f64::MAX;
    let mut iterations = 0;
    while t_delta > 0.001 && iterations < 100 {
        iterations += 1;
        let t_last = t_current;
        t_current = newton(
            t_current,
            |t| (c1 + c2 * t) * (r * t).exp() + signed_delta,
            |t| (c2 * (r * t + 1.0) + c1 * r) * (r * t).exp(),
        );
        t_delta = (t_last - t_current).abs();
    }
    t_current
}

fn estimate_over_damped(r1: f64, r2: f64, p0: f64, v0: f64, delta: f64) -> f64 {
    let c2 = (r1 * p0 - v0) / (r1 - r2);
    let c1 = p0 - c2;
    let t1 = (delta / c1).abs().ln() / r1;
    let t2 = (delta / c2).abs().ln() / r2;
    let mut t_current = if !t1.is_finite() {
        t2
    } else if !t2.is_finite() {
        t1
    } else {
        t1.max(t2)
    };
    let t_inflection = ((c1 * r1) / (-c2 * r2)).ln() / (r2 - r1);
    let x_inflection = || c1 * (r1 * t_inflection).exp() + c2 * (r2 * t_inflection).exp();
    let signed_delta = if t_inflection.is_nan() || t_inflection <= 0.0 {
        -delta
    } else if t_inflection > 0.0 && -x_inflection() < delta {
        if c2 > 0.0 && c1 < 0.0 {
            t_current = 0.0;
        }
        -delta
    } else {
        t_current = (-(c2 * r2 * r2) / (c1 * r1 * r1)).ln() / (r1 - r2);
        delta
    };
    if (c1 * r1 * (r1 * t_current).exp() + c2 * r2 * (r2 * t_current).exp()).abs() < 0.0001 {
        return t_current;
    }
    let mut t_delta = f64::MAX;
    let mut iterations = 0;
    while t_delta > 0.001 && iterations < 100 {
        iterations += 1;
        let t_last = t_current;
        t_current = newton(
            t_current,
            |t| c1 * (r1 * t).exp() + c2 * (r2 * t).exp() + signed_delta,
            |t| c1 * r1 * (r1 * t).exp() + c2 * r2 * (r2 * t).exp(),
        );
        t_delta = (t_last - t_current).abs();
    }
    t_current
}

fn newton(x: f64, f: impl Fn(f64) -> f64, derivative: impl Fn(f64) -> f64) -> f64 {
    x - f(x) / derivative(x)
}
