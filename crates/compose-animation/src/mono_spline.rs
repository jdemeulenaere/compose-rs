//! A monotonic cubic Hermite spline through values at times, in up to four dimensions, as Compose's
//! `MonoSpline`, what `keyframes_with_spline` plays.

use crate::vector::AnimationVector;

/// A value's component, 0 past its size.
fn component(vector: &AnimationVector, index: usize) -> f32 {
    vector.as_slice().get(index).copied().unwrap_or(0.0)
}

/// Compose's `MonoSpline`: its values and tangents by time, read in each dimension.
#[derive(Debug)]
pub(crate) struct MonoSpline {
    times: Vec<f32>,
    values: Vec<AnimationVector>,
    tangents: Vec<[f32; 4]>,
}

impl MonoSpline {
    /// A spline through `values` at `times`, as `MonoSpline(time, y, periodicBias)`: `periodic_bias`
    /// NaN for none. At least two points, in ascending order.
    pub(crate) fn new(
        times: Vec<f32>,
        values: Vec<AnimationVector>,
        periodic_bias: f32,
    ) -> MonoSpline {
        let n = times.len();
        let dimensions = values[0].size();
        let mut slopes = vec![[0.0f32; 4]; n - 1];
        let mut tangents = vec![[0.0f32; 4]; n];
        for j in 0..dimensions {
            for i in 0..n - 1 {
                let dt = times[i + 1] - times[i];
                slopes[i][j] = (component(&values[i + 1], j) - component(&values[i], j)) / dt;
                tangents[i][j] =
                    if i == 0 { slopes[i][j] } else { (slopes[i - 1][j] + slopes[i][j]) * 0.5 };
            }
            tangents[n - 1][j] = slopes[n - 2][j];
        }
        if !periodic_bias.is_nan() {
            for j in 0..dimensions {
                // What the bias says, where 0 is the last slope and 1 the first.
                let adjusted =
                    slopes[n - 2][j] * (1.0 - periodic_bias) + slopes[0][j] * periodic_bias;
                slopes[0][j] = adjusted;
                slopes[n - 2][j] = adjusted;
                tangents[n - 1][j] = adjusted;
                tangents[0][j] = adjusted;
            }
        }
        for i in 0..n - 1 {
            for j in 0..dimensions {
                if slopes[i][j] == 0.0 {
                    tangents[i][j] = 0.0;
                    tangents[i + 1][j] = 0.0;
                } else {
                    let a = tangents[i][j] / slopes[i][j];
                    let b = tangents[i + 1][j] / slopes[i][j];
                    // Kotlin's `hypot` of floats goes through doubles.
                    let h = f64::from(a).hypot(f64::from(b)) as f32;
                    if h > 9.0 {
                        let t = 3.0 / h;
                        tangents[i][j] = t * a * slopes[i][j];
                        tangents[i + 1][j] = t * b * slopes[i][j];
                    }
                }
            }
        }
        MonoSpline { times, values, tangents }
    }

    /// The point `time` is outside the times or at their ends: the first or the last.
    fn end_at(&self, time: f32) -> Option<usize> {
        let last = self.times.len() - 1;
        if time <= self.times[0] {
            Some(0)
        } else if time >= self.times[last] {
            Some(last)
        } else {
            None
        }
    }

    /// The value at `time`, searching the stretches from `index`, as `getPos(time, v, index)`:
    /// beyond the ends, extrapolated along their slopes. `out` is what comes back if no stretch
    /// holds it, with the size of what comes back.
    pub(crate) fn position(
        &self,
        time: f32,
        index: usize,
        out: AnimationVector,
    ) -> AnimationVector {
        if let Some(end) = self.end_at(time) {
            let slope = self.slope_clamped(self.times[end], out);
            let from = self.times[end];
            return out.map(|j, _| {
                component(&self.values[end], j) + (time - from) * component(&slope, j)
            });
        }
        for i in index..self.times.len() - 1 {
            if time == self.times[i] {
                return out.map(|j, _| component(&self.values[i], j));
            }
            if time < self.times[i + 1] {
                let h = self.times[i + 1] - self.times[i];
                let x = (time - self.times[i]) / h;
                return out.map(|j, _| {
                    hermite_interpolate(
                        h,
                        x,
                        component(&self.values[i], j),
                        component(&self.values[i + 1], j),
                        self.tangents[i][j],
                        self.tangents[i + 1][j],
                    )
                });
            }
        }
        out
    }

    /// The slope within the stretches, `time` coerced into them, as the private `getSlope`.
    fn slope_clamped(&self, time: f32, out: AnimationVector) -> AnimationVector {
        let last = self.times.len() - 1;
        let time = time.max(self.times[0]).min(self.times[last]);
        self.slope_in(time, 0, out)
    }

    fn slope_in(&self, time: f32, index: usize, out: AnimationVector) -> AnimationVector {
        for i in index..self.times.len() - 1 {
            if time <= self.times[i + 1] {
                let h = self.times[i + 1] - self.times[i];
                let x = (time - self.times[i]) / h;
                return out.map(|j, _| {
                    hermite_differential(
                        h,
                        x,
                        component(&self.values[i], j),
                        component(&self.values[i + 1], j),
                        self.tangents[i][j],
                        self.tangents[i + 1][j],
                    ) / h
                });
            }
        }
        out
    }

    /// The slope at `time`, searching the stretches from `index`, as `getSlope(time, v, index)`: at
    /// the ends and beyond, the tangent there. `out` is what comes back if no stretch holds it.
    pub(crate) fn slope(&self, time: f32, index: usize, out: AnimationVector) -> AnimationVector {
        if let Some(end) = self.end_at(time) {
            return out.map(|j, _| self.tangents[end][j]);
        }
        self.slope_in(time, index, out)
    }
}

/// A cubic Hermite spline, as `hermiteInterpolate`.
fn hermite_interpolate(h: f32, x: f32, y1: f32, y2: f32, t1: f32, t2: f32) -> f32 {
    let x2 = x * x;
    let x3 = x2 * x;
    h * t1 * (x - 2.0 * x2 + x3) + h * t2 * (x3 - x2) + y1 - (3.0 * x2 - 2.0 * x3) * (y1 - y2)
}

/// Its slope, as `hermiteDifferential`.
fn hermite_differential(h: f32, x: f32, y1: f32, y2: f32, t1: f32, t2: f32) -> f32 {
    let x2 = x * x;
    h * (t1 - 2.0 * x * (2.0 * t1 + t2) + 3.0 * (t1 + t2) * x2) - 6.0 * (x - x2) * (y1 - y2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(values: &[f32]) -> AnimationVector {
        AnimationVector::zero(values.len()).map(|index, _| values[index])
    }

    fn spline(times: &[f32], points: &[[f32; 2]]) -> MonoSpline {
        let values = points.iter().map(|point| vector(point)).collect();
        MonoSpline::new(times.to_vec(), values, f32::NAN)
    }

    fn at(spline: &MonoSpline, time: f32, dimension: usize) -> f32 {
        spline.position(time, 0, AnimationVector::zero(2))[dimension]
    }

    fn close(actual: f32, expected: f32) {
        assert!((actual - expected).abs() <= 0.001, "{actual} isn't {expected}");
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/MonoSplineTest.kt, testCurveFit01
    #[test]
    fn a_curve_goes_through_its_points_and_between() {
        let spline = spline(&[0.0, 5.0, 10.0], &[[0.0, 0.0], [1.0, 1.0], [2.0, 0.0]]);
        close(at(&spline, 5.0, 0), 1.0);
        close(at(&spline, 7.0, 0), 1.4);
        close(at(&spline, 7.0, 1), 0.744);
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/MonoSplineTest.kt, testMonoSpline
    #[test]
    fn a_spline_stays_flat_where_its_values_do() {
        let spline = spline(
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            &[[0.0, 0.0], [1.0, 1.0], [1.0, 0.0], [2.0, 0.0], [2.0, 0.0], [3.0, 0.0]],
        );
        for (time, expected) in
            [(1.0, 1.0), (1.1, 1.0), (1.3, 1.0), (1.6, 1.0), (1.9, 1.0), (3.5, 2.0)]
        {
            close(at(&spline, time, 0), expected);
        }
        let times: Vec<f32> = (0..60).map(|i| 5.0 * i as f32 / 59.0).collect();
        let values: Vec<f32> = times.iter().map(|time| at(&spline, *time, 0)).collect();
        let expected = "\
|***                                                         | 0.0
|   **                                                       |
|     **                                                     |
|       **                                                   |
|         ****************                                   |
|                          **                                | 1.071
|                            **                              |
|                              **                            |
|                                **                          |
|                                  ****************          |
|                                                   **       | 2.143
|                                                     **     |
|                                                       **   |
|                                                         ** |
|                                                           *| 3.0
0.0                                                        5.0";
        assert_eq!(crate::text_graph::draw_text_graph(60, 15, &times, &values), expected);
    }

    /// Its tangents are limited where a slope is much steeper than the next: no overshoot, which a
    /// plain Hermite spline would have before the step.
    #[test]
    fn a_steep_step_does_not_overshoot() {
        let spline = spline(&[0.0, 1.0, 2.0], &[[0.0, 0.0], [0.01, 0.0], [10.0, 0.0]]);
        for i in 0..=100 {
            let at_first = at(&spline, i as f32 / 100.0, 0);
            assert!((0.0..=0.01001).contains(&at_first), "{i}: {at_first}");
            let at_second = at(&spline, 1.0 + i as f32 / 100.0, 0);
            assert!((0.00999..=10.0).contains(&at_second), "{i}: {at_second}");
        }
    }

    /// Past the ends: on the slope there, as `MonoSplineIsExtrapolate`.
    #[test]
    fn beyond_its_ends_a_spline_extrapolates() {
        let spline = spline(&[0.0, 1.0], &[[0.0, 0.0], [1.0, 2.0]]);
        close(at(&spline, 2.0, 0), 2.0);
        close(at(&spline, -1.0, 1), -2.0);
    }
}
