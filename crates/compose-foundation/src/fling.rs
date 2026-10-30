//! Velocity tracking along an axis (compose-ui's `VelocityTracker1D`, as Compose's drags use
//! it) and flings, as Compose's default fling on Android (`SplineBasedFloatDecayAnimationSpec`:
//! Android's `OverScroller` curve).

use compose_animation::{FloatDecayAnimationSpec, SplineBasedFloatDecayAnimationSpec};
use compose_ui::Density;

/// A pointer's speed along an axis, in pixels per second, as Compose's default
/// (`Lsq2VelocityTracker.addPointerInputChange`): compose-ui's `VelocityTracker1D` fitting the
/// last 20 positions within 100 ms by Lsq2, cut at a pause of more than 40 ms, and none if the
/// release comes more than 40 ms after the last move (D862).
#[derive(Default)]
pub(crate) struct VelocityTracker {
    tracker: compose_ui::VelocityTracker1D,
    /// The last event's time, in milliseconds.
    last_millis: i64,
}

impl VelocityTracker {
    /// A press at `position` (along the axis), `time_nanos`: tracking starts over from it.
    pub(crate) fn press(&mut self, time_nanos: u64, position: f32) {
        self.tracker.reset_tracking();
        self.add(time_nanos, position);
    }

    /// A move to `position`.
    pub(crate) fn add(&mut self, time_nanos: u64, position: f32) {
        let millis = (time_nanos / 1_000_000) as i64;
        self.tracker.add_data_point(millis, position);
        self.last_millis = millis;
    }

    /// The release: no velocity if it comes more than 40 ms after the last move (the pointer
    /// had stopped), as Compose's fix for b/238654963.
    pub(crate) fn release(&mut self, time_nanos: u64) {
        let millis = (time_nanos / 1_000_000) as i64;
        if millis - self.last_millis > 40 {
            self.tracker.reset_tracking();
        }
        self.last_millis = millis;
    }

    pub(crate) fn velocity(&self) -> f32 {
        // Within no maximum: a NaN (samples at one time) is 0, as Compose's drags ask it.
        self.tracker.calculate_velocity_with(f32::MAX)
    }
}

/// A fling's course: how far (pixels, signed) and for how long (nanoseconds), and where it is
/// along it, as Compose's default fling on Android: compose-animation's spline-based decay
/// (`SplineBasedFloatDecayAnimationSpec`, Android's `OverScroller` spline to the bit, D820,
/// D826).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fling {
    pub distance: f32,
    pub duration_nanos: u64,
    velocity: f32,
    spec: SplineBasedFloatDecayAnimationSpec,
}

impl Fling {
    /// A fling at `velocity` pixels per second, at `density` (pixels per dp).
    pub(crate) fn new(velocity: f32, density: f32) -> Fling {
        let spec = SplineBasedFloatDecayAnimationSpec::new(Density::new(density));
        Fling {
            distance: spec.target_value(0.0, velocity),
            duration_nanos: spec.duration_nanos(0.0, velocity).max(0) as u64,
            velocity,
            spec,
        }
    }

    /// The play time `elapsed_nanos` after its start, as Compose's `animateDecay` computes it under
    /// a fling's scale of 1: `(elapsed / durationScale).toLong()`, a `Long` over a `Float`.
    pub(crate) fn play_time_nanos(elapsed_nanos: u64, motion_scale: f32) -> i64 {
        (elapsed_nanos as f32 / motion_scale) as i64
    }

    /// Whether it's over at `play_time_nanos`, as `isFinishedFromNanos`.
    pub(crate) fn is_finished(&self, play_time_nanos: i64) -> bool {
        play_time_nanos >= self.duration_nanos as i64
    }

    /// Its velocity at `play_time_nanos`: the decay's then.
    pub(crate) fn velocity_at(&self, play_time_nanos: i64) -> f32 {
        self.spec.velocity_from_nanos(play_time_nanos.max(0), 0.0, self.velocity)
    }

    /// The distance covered at `play_time_nanos`: the decay's value then.
    pub(crate) fn distance_at(&self, play_time_nanos: i64) -> f32 {
        if self.is_finished(play_time_nanos) {
            return self.distance;
        }
        self.spec.value_from_nanos(play_time_nanos.max(0), 0.0, self.velocity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fling_goes_from_nothing_to_all_the_distance() {
        let fling = Fling::new(2000.0, 2.625);
        // Compose's spline starts a hundredth of a pixel in (its table's interpolation).
        assert!(fling.distance_at(0).abs() < 0.01, "{}", fling.distance_at(0));
        let duration = fling.duration_nanos as i64;
        assert_eq!(fling.distance_at(duration), fling.distance);
        assert!(fling.distance_at(duration / 4) > 0.25 * fling.distance, "fast at first");
    }

    #[test]
    fn a_fling_is_the_spline_based_decay() {
        // D826: compose-animation's decay, Compose's to the bit, at its play times.
        let spec = SplineBasedFloatDecayAnimationSpec::new(Density::new(2.625));
        for velocity in [-5000.0f32, -800.0, 300.0, 1000.0, 4000.0, 12_000.0] {
            let fling = Fling::new(velocity, 2.625);
            assert_eq!(fling.distance, spec.target_value(0.0, velocity));
            assert_eq!(fling.duration_nanos as i64, spec.duration_nanos(0.0, velocity));
            for step in 0..10 {
                let play_time = fling.duration_nanos as i64 * step / 10;
                assert_eq!(
                    fling.distance_at(play_time),
                    spec.value_from_nanos(play_time, 0.0, velocity),
                    "{velocity} at {play_time}"
                );
            }
        }
    }

    /// A finished fling leaves no velocity: the frame loop's 0 at its end is the decay's end
    /// velocity, what Compose's `velocityLeft` is then.
    #[test]
    fn a_fling_ends_with_no_velocity() {
        for velocity in [-5000.0f32, 800.0, 12_000.0] {
            let fling = Fling::new(velocity, 2.625);
            assert_eq!(fling.velocity_at(fling.duration_nanos as i64), 0.0, "{velocity}");
        }
    }

    // Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AbstractScrollableNode.kt, DefaultFlingBehavior.performFling (the catch: `velocityLeft = animationState.velocity`; no test of Compose's at this value)
    #[test]
    fn a_fling_whose_node_left_returns_the_decay_s_velocity_there() {
        use crate::gestures::{DefaultFlingBehavior, FlingBehavior, ScrollScope};
        use compose_runtime::platform;
        use std::cell::Cell;
        use std::rc::Rc;
        use std::time::Duration;

        platform::set_manual_clock(1_000_000_000);
        let velocity = 8000.0f32;
        let (cancelled, calls) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(0u32)));
        let (leaving, counted) = (cancelled.clone(), calls.clone());
        // The node leaves during the third frame's scroll.
        let scope = ScrollScope::of_fling(
            move |delta| {
                counted.set(counted.get() + 1);
                if counted.get() == 3 {
                    leaving.set(true);
                }
                delta
            },
            cancelled,
        );
        let left = Rc::new(Cell::new(None));
        let result = left.clone();
        let coroutines = compose_runtime::CoroutineScope::new();
        coroutines.launch(async move {
            let behavior = DefaultFlingBehavior::new(2.625);
            result.set(Some(behavior.perform_fling(scope, velocity).await));
        });
        // The first frame is its start, then three.
        for _ in 0..4 {
            platform::run_ready();
            platform::advance_clock(Duration::from_millis(16));
            platform::dispatch_frame();
        }
        platform::run_ready();
        let expected = Fling::new(velocity, 2.625).velocity_at(48_000_000);
        assert_eq!(left.get(), Some(expected));
        assert!(expected > 0.0 && expected < velocity, "{expected}");
    }

    #[test]
    fn a_fling_goes_further_and_longer_the_faster_it_starts() {
        let slow = Fling::new(1000.0, 2.625);
        let fast = Fling::new(4000.0, 2.625);
        assert!(fast.distance > slow.distance && fast.duration_nanos > slow.duration_nanos);
        assert!(Fling::new(-1000.0, 2.625).distance < 0.0);
        // As Android's OverScroller: 1,000 px/s at 420 dpi glides about 95 px.
        assert!((slow.distance - 95.5).abs() < 1.0, "{}", slow.distance);
    }
}
