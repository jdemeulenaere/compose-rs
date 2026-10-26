//! Compose's spring duration estimation against the spring's own motion: a spring is settled
//! (within the threshold) two frames after its estimated end, and not two frames before.

use compose_animation::*;

const TWO_FRAMES_MILLIS: i64 = 33;

struct Case {
    mass: f64,
    spring_constant: f64,
    damping_coefficient: f64,
    initial_velocity: f64,
    initial_displacement: f64,
}

impl Case {
    fn damping_ratio(&self) -> f64 {
        self.damping_coefficient / (2.0 * (self.spring_constant * self.mass).sqrt())
    }

    fn stiffness(&self) -> f64 {
        self.spring_constant / self.mass
    }
}

/// Whether the estimate for `case` is right: the same from both forms of the estimation (within
/// a millisecond), finite, and the spring settled around it.
fn check(case: &Case) -> Result<(), String> {
    let end = estimate_animation_duration_millis_of_mass(
        case.spring_constant,
        case.damping_coefficient,
        case.mass,
        case.initial_velocity,
        case.initial_displacement,
        1.0,
    );
    let alternate = estimate_animation_duration_millis(
        case.stiffness(),
        case.damping_ratio(),
        case.initial_velocity,
        case.initial_displacement,
        1.0,
    );
    if (end - alternate).abs() > 1 {
        return Err(format!("the two estimations differ: {end} and {alternate}"));
    }
    if end == i64::MAX {
        return Err("never settles".into());
    }
    let spring = SpringSimulation::new(0.0, case.damping_ratio() as f32, case.stiffness() as f32);
    let at = |time: i64| {
        spring
            .update_values(
                case.initial_displacement as f32,
                case.initial_velocity as f32,
                time.max(0),
            )
            .0
            .abs()
    };
    let (before, at_end, after) =
        (at(end - TWO_FRAMES_MILLIS), at(end), at(end + TWO_FRAMES_MILLIS));
    let pass = if case.damping_ratio() >= 1.0 {
        // Out of the threshold before, in after; or, near an inflection, at its peak there.
        (before >= 0.999 && after <= 1.001)
            || (at_end >= 0.999 && before < at_end && after < at_end)
    } else {
        // Oscillating: settled after.
        after < 1.0
    };
    if pass { Ok(()) } else { Err(format!("before {before}, at {at_end}, after {after}")) }
}

// Compose: compose/animation/animation-core/src/androidHostTest/kotlin/androidx/compose/animation/core/SpringEstimationTest.kt, runTestCases
// Compose runs about 70 million cases (50 × 50 masses and spring constants); this takes every
// fifth mass and spring constant of the same ranges (about 2.9 million cases).
#[test]
fn spring_durations_end_where_the_spring_settles() {
    let mut parameters = Vec::new();
    for m in (1..100).step_by(10) {
        for k in (1..1000).step_by(100) {
            parameters.push(((m * m) as f64, (k * k) as f64));
        }
    }
    parameters.push((10_000.0, 1.0));
    let mut failures = Vec::new();
    let mut count = 0;
    for (mass, spring_constant) in parameters {
        let mut cases = Vec::new();
        for c in (1..10_000).step_by(500) {
            for v0 in (-200_000..=200_000).step_by(100_000) {
                for p0 in (-10_000..=10_000).step_by(100) {
                    if v0 != 0 || p0 != 0 {
                        cases.push(Case {
                            mass,
                            spring_constant,
                            damping_coefficient: f64::from(c),
                            initial_velocity: f64::from(v0),
                            initial_displacement: f64::from(p0),
                        });
                    }
                }
            }
        }
        // Critically damped: c² = 4mk.
        let critical = 2.0 * (spring_constant * mass).sqrt();
        for v0 in (-200_000..=200_000).step_by(10_000) {
            for p0 in (-10_000..=10_000).step_by(100) {
                if v0 != 0 || p0 != 0 {
                    cases.push(Case {
                        mass,
                        spring_constant,
                        damping_coefficient: critical,
                        initial_velocity: f64::from(v0),
                        initial_displacement: f64::from(p0),
                    });
                }
            }
        }
        for case in &cases {
            count += 1;
            if let Err(reason) = check(case) {
                failures.push(format!(
                    "m {} k {} c {} v0 {} p0 {}: {reason}",
                    case.mass,
                    case.spring_constant,
                    case.damping_coefficient,
                    case.initial_velocity,
                    case.initial_displacement
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {count} cases failed: {:#?}",
        failures.len(),
        &failures[..failures.len().min(20)]
    );
}
