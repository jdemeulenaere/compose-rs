//! Velocity tracking, as Compose's `VelocityTracker` and `VelocityTracker1D`
//! (`input/pointer/util/VelocityTracker.kt`, its `Lsq2VelocityTracker` being Android's
//! default): the last 20 samples within 100 ms, cut at a gap of more than 40 ms (the pointer
//! stopped), fitted by a second-degree least-squares polynomial (Lsq2), or by the impulse
//! strategy for differential data.

use crate::unit::Offset;

/// How long a pause means the pointer stopped, as Compose's `AssumePointerMoveStoppedMilliseconds`.
const ASSUME_POINTER_MOVE_STOPPED_MILLISECONDS: i64 = 40;
/// The samples kept, as Compose's `HistorySize`.
const HISTORY_SIZE: usize = 20;
/// The fewest samples a velocity is worked out from, as Compose's `MinSampleSize`.
const MIN_SAMPLE_SIZE: usize = 2;
/// How far back samples count, as Compose's `HorizonMilliseconds`.
const HORIZON_MILLISECONDS: i64 = 100;

/// A two-dimensional velocity, in pixels per second, as Compose's `Velocity`.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

impl Velocity {
    pub const ZERO: Velocity = Velocity { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Velocity {
        Velocity { x, y }
    }
}

impl std::ops::Add for Velocity {
    type Output = Velocity;
    fn add(self, other: Velocity) -> Velocity {
        Velocity::new(self.x + other.x, self.y + other.y)
    }
}

impl std::ops::Sub for Velocity {
    type Output = Velocity;
    fn sub(self, other: Velocity) -> Velocity {
        Velocity::new(self.x - other.x, self.y - other.y)
    }
}

impl std::ops::Neg for Velocity {
    type Output = Velocity;
    fn neg(self) -> Velocity {
        Velocity::new(-self.x, -self.y)
    }
}

/// How a one-dimensional tracker fits its samples, as Compose's internal
/// `VelocityTracker1D.Strategy`.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VelocityTrackingStrategy {
    /// A second-degree least-squares polynomial: positions only.
    Lsq2,
    /// The impulse strategy (kinetic energy), for positions or differential data.
    Impulse,
}

/// A pointer's velocity along one axis, as Compose's `VelocityTracker1D`: data points (positions,
/// or changes when `is_data_differential`) with their times in milliseconds.
#[derive(Clone, Debug)]
pub struct VelocityTracker1D {
    is_data_differential: bool,
    strategy: VelocityTrackingStrategy,
    /// A ring of samples, the newest at `index`.
    samples: [Option<(i64, f32)>; HISTORY_SIZE],
    index: usize,
}

impl VelocityTracker1D {
    /// A tracker of positions (`is_data_differential` false) or of changes (true), with the
    /// impulse strategy, as Compose's public `VelocityTracker1D(isDataDifferential)`.
    pub fn new(is_data_differential: bool) -> VelocityTracker1D {
        VelocityTracker1D::with_strategy(is_data_differential, VelocityTrackingStrategy::Impulse)
    }

    /// A tracker with a strategy, as Compose's internal constructor. Panics for differential
    /// data with Lsq2, as Compose throws.
    #[doc(hidden)]
    pub fn with_strategy(
        is_data_differential: bool,
        strategy: VelocityTrackingStrategy,
    ) -> VelocityTracker1D {
        assert!(
            !(is_data_differential && strategy == VelocityTrackingStrategy::Lsq2),
            "compose-ui: Lsq2 not (yet) supported for differential axes"
        );
        VelocityTracker1D {
            is_data_differential,
            strategy,
            samples: [None; HISTORY_SIZE],
            index: 0,
        }
    }

    /// Whether the data points are changes since the previous one, not positions.
    pub fn is_data_differential(&self) -> bool {
        self.is_data_differential
    }

    /// Adds a data point at `time_millis`, as `addDataPoint`.
    pub fn add_data_point(&mut self, time_millis: i64, data_point: f32) {
        self.index = (self.index + 1) % HISTORY_SIZE;
        self.samples[self.index] = Some((time_millis, data_point));
    }

    /// The velocity, in units per second, as `calculateVelocity()`: 0 with fewer than two
    /// samples of continuous motion.
    pub fn calculate_velocity(&self) -> f32 {
        let mut data_points = [0.0f32; HISTORY_SIZE];
        let mut times = [0.0f32; HISTORY_SIZE];
        let mut count = 0;
        let mut index = self.index;
        let Some(newest) = self.samples[index] else { return 0.0 };
        let mut previous = newest;
        // From the newest sample back, while the samples are continuous motion.
        while let Some(sample) = self.samples[index] {
            let age = newest.0 - sample.0;
            let delta = (sample.0 - previous.0).abs();
            previous =
                if self.strategy == VelocityTrackingStrategy::Lsq2 || self.is_data_differential {
                    sample
                } else {
                    newest
                };
            if age > HORIZON_MILLISECONDS || delta > ASSUME_POINTER_MOVE_STOPPED_MILLISECONDS {
                break;
            }
            data_points[count] = sample.1;
            times[count] = -(age as f32);
            index = if index == 0 { HISTORY_SIZE } else { index } - 1;
            count += 1;
            if count >= HISTORY_SIZE {
                break;
            }
        }
        if count < MIN_SAMPLE_SIZE {
            return 0.0;
        }
        let per_millisecond = match self.strategy {
            VelocityTrackingStrategy::Impulse => {
                impulse_velocity(&data_points, &times, count, self.is_data_differential)
            }
            VelocityTrackingStrategy::Lsq2 => {
                // The quadratic's derivative at the newest sample's time (0): its second
                // coefficient.
                poly_fit_least_squares(&times[..count], &data_points[..count], 2)
                    .map_or(0.0, |coefficients| coefficients[1])
            }
        };
        per_millisecond * 1000.0
    }

    /// The velocity, within `[-maximum_velocity, maximum_velocity]` (NaN: 0), as
    /// `calculateVelocity(maximumVelocity)`. Panics if `maximum_velocity` isn't positive, as
    /// Compose throws.
    pub fn calculate_velocity_with(&self, maximum_velocity: f32) -> f32 {
        assert!(
            maximum_velocity > 0.0,
            "compose-ui: maximum_velocity should be a positive value; it is {maximum_velocity}"
        );
        let velocity = self.calculate_velocity();
        if velocity == 0.0 || velocity.is_nan() {
            0.0
        } else {
            velocity.clamp(-maximum_velocity, maximum_velocity)
        }
    }

    /// Forgets the samples, as `resetTracking`.
    pub fn reset_tracking(&mut self) {
        self.samples = [None; HISTORY_SIZE];
        self.index = 0;
    }
}

impl Default for VelocityTracker1D {
    /// A tracker of positions fitted by Lsq2, as Compose's `VelocityTracker1D()` (internal).
    fn default() -> VelocityTracker1D {
        VelocityTracker1D::with_strategy(false, VelocityTrackingStrategy::Lsq2)
    }
}

/// A pointer's velocity, as Compose's `VelocityTracker` (Android's default,
/// `Lsq2VelocityTracker`): a 1D Lsq2 tracker per axis.
#[derive(Clone, Debug, Default)]
pub struct VelocityTracker {
    x: VelocityTracker1D,
    y: VelocityTracker1D,
    /// The time of the last event added by `add_pointer_input_change`.
    last_move_event_time_millis: i64,
}

impl VelocityTracker {
    pub fn new() -> VelocityTracker {
        VelocityTracker::default()
    }

    /// Adds a position at `time_millis`, as `addPosition`.
    pub fn add_position(&mut self, time_millis: i64, position: Offset) {
        self.x.add_data_point(time_millis, position.x);
        self.y.add_data_point(time_millis, position.y);
    }

    /// Adds a pointer's event, as `addPointerInputChange`: a press starts over, a move or a
    /// press adds its position (`offset` added), and a release adds nothing but starts over if
    /// it came more than 40 ms after the last event (the pointer had stopped).
    pub fn add_pointer_input_change(
        &mut self,
        time_millis: i64,
        position: Offset,
        changed_to_down: bool,
        changed_to_up: bool,
    ) {
        if changed_to_down {
            self.reset_tracking();
        }
        if !changed_to_up {
            self.add_position(time_millis, position);
        }
        if changed_to_up
            && time_millis - self.last_move_event_time_millis
                > ASSUME_POINTER_MOVE_STOPPED_MILLISECONDS
        {
            self.reset_tracking();
        }
        self.last_move_event_time_millis = time_millis;
    }

    /// The velocity, as `calculateVelocity()`.
    pub fn calculate_velocity(&self) -> Velocity {
        self.calculate_velocity_with(Velocity::new(f32::MAX, f32::MAX))
    }

    /// The velocity within `maximum_velocity` each way, as `calculateVelocity(maximumVelocity)`.
    /// Panics if a component isn't positive, as Compose throws.
    pub fn calculate_velocity_with(&self, maximum_velocity: Velocity) -> Velocity {
        assert!(
            maximum_velocity.x > 0.0 && maximum_velocity.y > 0.0,
            "compose-ui: maximum_velocity should be positive; it is {maximum_velocity:?}"
        );
        Velocity::new(
            self.x.calculate_velocity_with(maximum_velocity.x),
            self.y.calculate_velocity_with(maximum_velocity.y),
        )
    }

    /// Forgets the samples, as `resetTracking`.
    pub fn reset_tracking(&mut self) {
        self.x.reset_tracking();
        self.y.reset_tracking();
        self.last_move_event_time_millis = 0;
    }
}

/// The coefficients of the polynomial of `degree` (truncated to the samples less one) fitting
/// `y` at `x` by least squares, as Compose's `polyFitLeastSquares` (a QR decomposition by
/// Gram-Schmidt); `None` for a degree below 1 or no samples, where Compose throws.
#[doc(hidden)]
pub fn poly_fit_least_squares(x: &[f32], y: &[f32], degree: usize) -> Option<Vec<f32>> {
    let m = x.len().min(y.len());
    if degree < 1 || m == 0 {
        return None;
    }
    let n = degree.min(m - 1) + 1;
    // The X vector expanded to a matrix A (n rows of m).
    let mut a = vec![vec![0.0f32; m]; n];
    for h in 0..m {
        a[0][h] = 1.0;
        for i in 1..n {
            a[i][h] = a[i - 1][h] * x[h];
        }
    }
    let dot = |a: &[f32], b: &[f32]| a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    // Gram-Schmidt: an orthonormal basis Q (column-major) and an upper triangular R.
    let mut q = vec![vec![0.0f32; m]; n];
    let mut r = vec![vec![0.0f32; n]; n];
    for j in 0..n {
        let mut w = a[j].clone();
        for z in q.iter().take(j) {
            let d = dot(&w, z);
            for h in 0..m {
                w[h] -= d * z[h];
            }
        }
        let inverse_norm = 1.0 / dot(&w, &w).sqrt().max(1e-6);
        for value in &mut w {
            *value *= inverse_norm;
        }
        for i in 0..n {
            r[j][i] = if i < j { 0.0 } else { dot(&w, &a[i]) };
        }
        q[j] = w;
    }
    // R B = Qt Y, solved from the bottom right.
    let mut coefficients = vec![0.0f32; degree + 1];
    for i in (0..n).rev() {
        let mut c = dot(&q[i], &y[..m]);
        for j in (i + 1..n).rev() {
            c -= r[i][j] * coefficients[j];
        }
        coefficients[i] = c / r[i][i];
    }
    Some(coefficients)
}

/// The impulse strategy's velocity (per millisecond), as Compose's `calculateImpulseVelocity`.
fn impulse_velocity(data_points: &[f32], time: &[f32], count: usize, differential: bool) -> f32 {
    let mut work = 0.0f32;
    let start = count - 1;
    let mut next_time = time[start];
    for i in (1..=start).rev() {
        let current_time = next_time;
        next_time = time[i - 1];
        if current_time == next_time {
            continue;
        }
        let delta =
            if differential { -data_points[i - 1] } else { data_points[i] - data_points[i - 1] };
        let current = delta / (current_time - next_time);
        let previous = kinetic_energy_to_velocity(work);
        work += (current - previous) * current.abs();
        if i == start {
            work *= 0.5;
        }
    }
    kinetic_energy_to_velocity(work)
}

fn kinetic_energy_to_velocity(kinetic_energy: f32) -> f32 {
    kinetic_energy.signum() * (2.0 * kinetic_energy.abs()).sqrt()
}
