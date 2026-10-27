//! The spline-based decay against Compose's: the spline clamped out of range
//! (`AndroidFlingSplineTest`), and flings' durations, targets, values and velocities against
//! values computed with Kotlin's numeric semantics by a script mirroring Compose's code.

use compose_animation::*;
use compose_ui::Density;

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/AndroidFlingSplineTest.kt, testClampedOutOfRangePosition
#[test]
fn the_spline_clamps_times_out_of_range() {
    let (start, end) = (fling_position(0.0), fling_position(1.0));
    for time in [-10.0, -1.0, -0.06] {
        assert_eq!(fling_position(time), start);
    }
    for time in [1.5, 10.0] {
        assert_eq!(fling_position(time), end);
    }
}

/// (density, velocity, duration in ms, target bits, [(ms, value bits, velocity bits)]).
type Fling = (f32, f32, i64, u32, [(i64, u32, u32); 4]);

#[test]
fn a_fling_decays_as_composes() {
    let flings: [Fling; 2] = [
        (
            1.0,
            1000.0,
            555,
            0x4342504a,
            [
                (0, 0x3b91bc38, 0x4479ca50),
                (16, 0x417f2889, 0x44780825),
                (100, 0x42b18455, 0x442efaa4),
                (250, 0x431f7b0b, 0x438ea096),
            ],
        ),
        (
            2.75,
            -4000.0,
            731,
            0xc4800598,
            [
                (0, 0xbcc00864, 0xc579e5c4),
                (16, 0xc27fa3ea, 0xc5782369),
                (100, 0xc3b9cbc8, 0xc54a88fe),
                (250, 0xc4366211, 0xc4cf1510),
            ],
        ),
    ];
    for (density, velocity, duration, target, samples) in flings {
        let spec = SplineBasedFloatDecayAnimationSpec::new(Density { density, font_scale: 1.0 });
        assert_eq!(spec.duration_nanos(0.0, velocity), duration * 1_000_000);
        assert_eq!(spec.target_value(0.0, velocity).to_bits(), target, "target at {velocity}");
        for (millis, value, speed) in samples {
            let nanos = millis * 1_000_000;
            assert_eq!(spec.value_from_nanos(nanos, 0.0, velocity).to_bits(), value, "{millis} ms");
            assert_eq!(
                spec.velocity_from_nanos(nanos, 0.0, velocity).to_bits(),
                speed,
                "{millis} ms"
            );
        }
        // Through `spline_based_decay`, a decay of any type.
        let decay = DecayAnimation::new(
            spline_based_decay(Density { density, font_scale: 1.0 }),
            0.0f32,
            velocity,
        );
        assert_eq!(decay.target_value().to_bits(), target);
    }
}
