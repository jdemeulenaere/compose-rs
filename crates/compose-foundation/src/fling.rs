//! Velocity tracking along an axis (compose-ui's `VelocityTracker1D`, as Compose's drags use
//! it) and flings, as Compose's default fling on Android (`SplineBasedFloatDecayAnimationSpec`:
//! Android's `OverScroller` curve).

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
/// along it, as Android's spline (`OverScroller`, Compose's `FlingCalculator`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fling {
    pub distance: f32,
    pub duration_nanos: u64,
}

/// `ViewConfiguration.getScrollFriction()`.
const FRICTION: f64 = 0.015;
const INFLEXION: f64 = 0.35;
const START_TENSION: f64 = 0.5;
const END_TENSION: f64 = 1.0;
const SAMPLES: usize = 100;

fn deceleration_rate() -> f64 {
    0.78f64.ln() / 0.9f64.ln()
}

impl Fling {
    /// A fling at `velocity` pixels per second, at `density` (pixels per dp).
    pub(crate) fn new(velocity: f32, density: f32) -> Fling {
        // Gravity × inches per meter × pixels per inch × a tuning factor, as Android's.
        let physical = 9.80665 * 39.37 * (f64::from(density) * 160.0) * 0.84;
        let speed = f64::from(velocity.abs());
        if speed == 0.0 {
            return Fling { distance: 0.0, duration_nanos: 0 };
        }
        let rate = deceleration_rate();
        let deceleration = (INFLEXION * speed / (FRICTION * physical)).ln();
        let duration = (deceleration / (rate - 1.0)).exp();
        let distance = FRICTION * physical * (rate / (rate - 1.0) * deceleration).exp();
        Fling {
            distance: (distance as f32).copysign(velocity),
            duration_nanos: (duration * 1e9) as u64,
        }
    }

    /// The distance covered at `fraction` (0 to 1) of the duration.
    pub(crate) fn distance_at(&self, fraction: f32) -> f32 {
        self.distance * spline_position(fraction.clamp(0.0, 1.0))
    }
}

/// Android's fling spline: the fraction of the distance at a fraction of the duration.
fn spline_position(fraction: f32) -> f32 {
    thread_local! {
        static TABLE: [f32; SAMPLES + 1] = spline_table();
    }
    TABLE.with(|table| {
        let t = f64::from(fraction);
        let index = ((SAMPLES as f64 * t) as usize).min(SAMPLES - 1);
        let (t_inf, t_sup) = (index as f64 / SAMPLES as f64, (index + 1) as f64 / SAMPLES as f64);
        let (d_inf, d_sup) = (f64::from(table[index]), f64::from(table[index + 1]));
        let slope = (d_sup - d_inf) / (t_sup - t_inf);
        (d_inf + (t - t_inf) * slope) as f32
    })
}

fn spline_table() -> [f32; SAMPLES + 1] {
    let p1 = START_TENSION * INFLEXION;
    let p2 = 1.0 - END_TENSION * (1.0 - INFLEXION);
    let mut table = [0.0f32; SAMPLES + 1];
    let mut x_min = 0.0f64;
    for (index, entry) in table.iter_mut().enumerate().take(SAMPLES) {
        let alpha = index as f64 / SAMPLES as f64;
        let mut x_max = 1.0f64;
        let (mut x, mut coefficient);
        loop {
            x = x_min + (x_max - x_min) / 2.0;
            coefficient = 3.0 * x * (1.0 - x);
            let tx = coefficient * ((1.0 - x) * p1 + x * p2) + x * x * x;
            if (tx - alpha).abs() < 1e-5 {
                break;
            }
            if tx > alpha {
                x_max = x;
            } else {
                x_min = x;
            }
        }
        *entry = (coefficient * ((1.0 - x) * START_TENSION + x) + x * x * x) as f32;
    }
    table[SAMPLES] = 1.0;
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spline_goes_from_nothing_to_all_the_distance() {
        assert!(spline_position(0.0).abs() < 1e-4);
        assert!((spline_position(1.0) - 1.0).abs() < 1e-4);
        assert!(spline_position(0.25) > 0.25, "fast at first");
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
