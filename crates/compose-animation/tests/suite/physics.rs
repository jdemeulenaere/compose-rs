//! Springs as Compose evaluates them: values and velocities from the spring's equations, the
//! duration its estimation gives (the time the spring stays within its threshold), each
//! component on its own threshold, and the target and no velocity at the end.

use compose_animation::*;

fn spring_to(target: f32, spec: &FloatSpringSpec) -> SpringSimulation {
    SpringSimulation::new(target, spec.damping_ratio, spec.stiffness)
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, velocityCalculation
#[test]
fn a_springs_velocity_is_its_equations() {
    let spec = FloatSpringSpec::default();
    let velocity = spec.velocity_from_millis(150, 200.0, 500.0, 0.0);
    assert_eq!(velocity, spring_to(500.0, &spec).update_values(200.0, 0.0, 150).1);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, velocityCalculationForInts
#[test]
fn an_int_springs_velocity_is_truncated_from_its_equations() {
    let animation = TargetBasedAnimation::with_velocity(spring(), 200, 500, 0);
    let expected = spring_to(500.0, &FloatSpringSpec::default()).update_values(200.0, 0.0, 150).1;
    assert_eq!(animation.velocity_from_millis(150), expected as i32);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, animationWithoutRangePreservesVelocity
#[test]
fn a_spring_to_where_it_is_keeps_the_velocity_it_starts_with() {
    let first = TargetBasedAnimation::with_velocity(spring(), 200.0f32, 500.0, 0.0);
    let (value, velocity) = (first.value_from_millis(150), first.velocity_from_millis(150));
    // From there to there, with that velocity: it overshoots and comes back.
    let second = TargetBasedAnimation::with_velocity(spring(), value, value, velocity);
    let expected = spring_to(value, &FloatSpringSpec::default()).update_values(value, velocity, 15);
    assert_eq!(second.value_from_millis(15), expected.0);
    assert_eq!(second.velocity_from_millis(15), expected.1);
}

/// The duration of a spring from 100 to 0 at 3000 per second, of stiffness 100, threshold 1.
fn duration_of(damping_ratio: f32) -> (i64, i64) {
    let spec = FloatSpringSpec::new(damping_ratio, 100.0, 1.0);
    let animation = TargetBasedAnimation::with_velocity(
        AnimationSpec::Spring(spring_with(damping_ratio, 100.0).visibility_threshold(1.0f32)),
        100.0f32,
        0.0,
        3000.0,
    );
    let estimated =
        estimate_animation_duration_millis(100.0, f64::from(damping_ratio), 3000.0, 100.0, 1.0);
    assert_eq!(spec.duration_millis(100.0, 0.0, 3000.0), estimated);
    (estimated, animation.duration_millis())
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testCriticallydampedDuration
#[test]
fn a_critically_damped_springs_duration_is_its_estimate() {
    let (estimated, duration) = duration_of(1.0);
    assert_eq!(duration, estimated);
    assert_eq!(duration, 811);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testOverdampedDuration
#[test]
fn an_overdamped_springs_duration_is_its_estimate() {
    let (estimated, duration) = duration_of(5.0);
    assert_eq!(duration, estimated);
    assert_eq!(duration, 4830);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testUnderdampedDuration
#[test]
fn an_underdamped_springs_duration_is_its_estimate() {
    let (estimated, duration) = duration_of(0.5);
    assert_eq!(duration, estimated);
    assert_eq!(duration, 1206);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testEndSnapping
#[test]
fn a_spring_ends_at_its_target_without_velocity() {
    let animation = TargetBasedAnimation::with_velocity(spring(), 0.0f32, 100.0, 0.0);
    let duration = animation.duration_millis();
    assert_eq!(animation.velocity_vector_from_millis(duration)[0], 0.0);
    assert_eq!(animation.value_from_millis(duration), 100.0);
    // A frame earlier it's still on its way.
    assert_ne!(animation.value_from_millis(duration - 16), 100.0);
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testSpringVectorAnimationDuration
#[test]
fn a_vector_springs_duration_is_its_longest_components() {
    let spec = spring().visibility_threshold(AnimationVector::from([1.0, 2.0, 3.0]));
    let duration = spec.duration_millis(
        AnimationVector::from([100.0, 100.0, 100.0]),
        AnimationVector::from([0.0, 0.0, 0.0]),
        AnimationVector::from([0.0, 0.0, 0.0]),
    );
    // The component of the lowest threshold takes longest.
    let float =
        FloatSpringSpec::new(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM, 1.0);
    assert_eq!(duration, float.duration_millis(100.0, 0.0, 0.0));
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testSpringVectorAnimationValues
#[test]
fn a_vector_springs_components_move_as_springs_of_their_own() {
    let start = AnimationVector::from([100.0, 100.0, 100.0]);
    let zero = AnimationVector::from([0.0, 0.0, 0.0]);
    let spec = spring().visibility_threshold(AnimationVector::from([1.0, 2.0, 3.0]));
    let duration = spec.duration_millis(start, zero, zero);
    let alone = |threshold: f32| spring().visibility_threshold(AnimationVector::from([threshold]));
    for time in 0..duration {
        let vector = spec.value_from_millis(time, start, zero, zero);
        for (index, threshold) in [1.0, 2.0, 3.0].into_iter().enumerate() {
            let one = AnimationVector::from(100.0);
            let float = alone(threshold).value_from_millis(time, one, 0.0.into(), 0.0.into());
            assert!((float[0] - vector[index]).abs() < 1e-4, "at {time} ms: {vector:?}");
        }
    }
}

// Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/PhysicsAnimationTest.kt, testDurationWithZeroDampingRatio
#[test]
fn an_undamped_spring_lasts_as_long_as_can_be() {
    let (initial, target, velocity) = (100.0.into(), 0.0.into(), 0.0.into());
    let undamped = spring_with(0.0, Spring::STIFFNESS_MEDIUM).visibility_threshold(1.0f32);
    let duration = undamped.duration_nanos(initial, target, velocity);
    // As many whole milliseconds as fit, in nanoseconds: no overflow.
    assert!(duration < i64::MAX && duration > i64::MAX - 1_000_000, "{duration}");
    let barely_damped = spring_with(0.001, Spring::STIFFNESS_MEDIUM).visibility_threshold(1.0f32);
    assert!(duration > barely_damped.duration_nanos(initial, target, velocity));
}
