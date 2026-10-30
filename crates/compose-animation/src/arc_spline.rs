//! Paths of quarter ellipses between values, as Compose's `ArcSpline`, what `ArcMode` keyframes and
//! `ArcAnimationSpec` play: each pair of dimensions is one arc, by stretch of time. In Kotlin's
//! numeric semantics: its float `sin`, `cos` and `hypot` go through doubles.

use std::f64::consts::PI;

use crate::vector::AnimationVector;

/// How a pair of a value's dimensions goes from one keyframe to the next, as Compose's `ArcMode`,
/// for positions (`Offset`, and the like); a third and a lone dimension go linearly, as does a pair
/// changing in one dimension.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
// Compose's names, which the program keeps.
#[allow(clippy::enum_variant_names)]
pub enum ArcMode {
    /// A quarter of an ellipse above the line between them, as `ArcMode.ArcAbove`.
    ArcAbove,
    /// A quarter of an ellipse below it, as `ArcMode.ArcBelow`.
    ArcBelow,
    /// The line, as `ArcMode.ArcLinear`.
    ArcLinear,
}

const LUT_SIZE: usize = 101;
const PERCENTS: usize = 91;
const EPSILON: f32 = 0.001;
const HALF_PI: f32 = (PI * 0.5) as f32;

fn sin(value: f32) -> f32 {
    f64::from(value).sin() as f32
}

fn cos(value: f32) -> f32 {
    f64::from(value).cos() as f32
}

fn hypot(x: f32, y: f32) -> f32 {
    f64::from(x).hypot(f64::from(y)) as f32
}

/// `java.util.Arrays.binarySearch`: the index, or `-(insertion point) - 1`.
fn binary_search(array: &[f32; PERCENTS], key: f32) -> i32 {
    let (mut low, mut high) = (0i32, PERCENTS as i32 - 1);
    while low <= high {
        let middle = (low + high) >> 1;
        let value = array[middle as usize];
        if value < key {
            low = middle + 1;
        } else if value > key {
            high = middle - 1;
        } else {
            return middle;
        }
    }
    -(low + 1)
}

/// One pair of dimensions over one stretch of time, as `ArcSpline.Arc`.
#[derive(Debug)]
struct Arc {
    time1: f32,
    time2: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    one_over_delta_time: f32,
    arc_velocity: f32,
    vertical: f32,
    is_linear: bool,
    ellipse_a: f32,
    ellipse_b: f32,
    /// The ellipse's center, or, for a line, its slope.
    center_x: f32,
    center_y: f32,
    /// From a fraction of the arc's length to a fraction of its angle: empty for a line.
    lut: Vec<f32>,
}

impl Arc {
    fn new(mode: ArcMode, times: (f32, f32), from: (f32, f32), to: (f32, f32)) -> Arc {
        let (time1, time2) = times;
        let ((x1, y1), (x2, y2)) = (from, to);
        let (dx, dy) = (x2 - x1, y2 - y1);
        let is_vertical = match mode {
            ArcMode::ArcAbove => dy < 0.0,
            ArcMode::ArcBelow => dy > 0.0,
            ArcMode::ArcLinear => false,
        };
        let vertical = if is_vertical { -1.0 } else { 1.0 };
        let one_over_delta_time = 1.0 / (time2 - time1);
        let mut arc = Arc {
            time1,
            time2,
            x1,
            y1,
            x2,
            y2,
            one_over_delta_time,
            arc_velocity: 0.0,
            vertical,
            is_linear: false,
            ellipse_a: f32::NAN,
            ellipse_b: f32::NAN,
            center_x: 0.0,
            center_y: 0.0,
            lut: Vec::new(),
        };
        if mode == ArcMode::ArcLinear || dx.abs() < EPSILON || dy.abs() < EPSILON {
            arc.is_linear = true;
            arc.arc_velocity = hypot(dy, dx) * one_over_delta_time;
            arc.center_x = dx * one_over_delta_time;
            arc.center_y = dy * one_over_delta_time;
        } else {
            arc.ellipse_a = dx * vertical;
            arc.ellipse_b = dy * -vertical;
            arc.center_x = if is_vertical { x2 } else { x1 };
            arc.center_y = if is_vertical { y1 } else { y2 };
            let distance = arc.build_table();
            arc.arc_velocity = distance * one_over_delta_time;
        }
        arc
    }

    /// The table, and the arc's length, as `buildTable`.
    fn build_table(&mut self) -> f32 {
        let (a, b) = (self.x2 - self.x1, self.y1 - self.y2);
        let (mut last_x, mut last_y, mut distance) = (0.0f32, b, 0.0f32);
        let last_index = PERCENTS - 1;
        let last_index_float = last_index as f32;
        let mut percents = [0.0f32; PERCENTS];
        for (i, percent) in percents.iter_mut().enumerate().skip(1) {
            let angle = (90.0 * i as f64 / last_index as f64 * (PI / 180.0)) as f32;
            let (x, y) = (a * sin(angle), b * cos(angle));
            distance += hypot(x - last_x, y - last_y);
            *percent = distance;
            last_x = x;
            last_y = y;
        }
        for percent in percents.iter_mut().skip(1) {
            *percent /= distance;
        }
        let lut_last_index = (LUT_SIZE - 1) as f32;
        self.lut = (0..LUT_SIZE)
            .map(|i| {
                let position = i as f32 / lut_last_index;
                let index = binary_search(&percents, position);
                if index >= 0 {
                    index as f32 / last_index_float
                } else if index == -1 {
                    0.0
                } else {
                    let (p1, p2) = ((-index - 2) as usize, (-index - 1) as usize);
                    (p1 as f32 + (position - percents[p1]) / (percents[p2] - percents[p1]))
                        / last_index_float
                }
            })
            .collect();
        distance
    }

    fn lookup(&self, value: f32) -> f32 {
        if value <= 0.0 {
            return 0.0;
        }
        if value >= 1.0 {
            return 1.0;
        }
        let position = value * (LUT_SIZE - 1) as f32;
        let index = position as usize;
        let offset = position - index as f32;
        self.lut[index] + offset * (self.lut[index + 1] - self.lut[index])
    }

    /// The sine and cosine of `time`'s angle, as `setPoint`.
    fn point(&self, time: f32) -> (f32, f32) {
        let elapsed = if self.vertical == -1.0 { self.time2 - time } else { time - self.time1 };
        let angle = HALF_PI * self.lookup(elapsed * self.one_over_delta_time);
        (sin(angle), cos(angle))
    }

    fn position(&self, time: f32) -> (f32, f32) {
        if self.is_linear {
            let fraction = (time - self.time1) * self.one_over_delta_time;
            (self.x1 + fraction * (self.x2 - self.x1), self.y1 + fraction * (self.y2 - self.y1))
        } else {
            let (sine, cosine) = self.point(time);
            (self.center_x + self.ellipse_a * sine, self.center_y + self.ellipse_b * cosine)
        }
    }

    fn slope(&self, time: f32) -> (f32, f32) {
        if self.is_linear {
            (self.center_x, self.center_y)
        } else {
            let (sine, cosine) = self.point(time);
            let (vx, vy) = (self.ellipse_a * cosine, -self.ellipse_b * sine);
            let norm = self.arc_velocity / hypot(vx, vy);
            (vx * self.vertical * norm, vy * self.vertical * norm)
        }
    }
}

/// A value's component as a float array's, 0 past its size.
fn component(vector: &AnimationVector, index: usize) -> f32 {
    vector.to_array()[index]
}

/// Arcs stitched together, as Compose's `ArcSpline`: a mode, of which the last has no use, for each
/// of `times`, with a value at each; a value's dimensions pair off, a lone last with a zero.
#[derive(Debug)]
pub(crate) struct ArcSpline {
    /// By stretch, then by pair of dimensions.
    arcs: Vec<Vec<Arc>>,
}

impl ArcSpline {
    pub(crate) fn new(modes: &[ArcMode], times: &[f32], values: &[AnimationVector]) -> ArcSpline {
        let pairs = values[0].size().div_ceil(2);
        let arcs = (0..times.len() - 1)
            .map(|i| {
                (0..pairs)
                    .map(|pair| {
                        let k = pair * 2;
                        Arc::new(
                            modes[i],
                            (times[i], times[i + 1]),
                            (component(&values[i], k), component(&values[i], k + 1)),
                            (component(&values[i + 1], k), component(&values[i + 1], k + 1)),
                        )
                    })
                    .collect()
            })
            .collect();
        ArcSpline { arcs }
    }

    /// The values at `time`, by dimension: past the ends, on the slope there, as `getPos`.
    pub(crate) fn position(&self, time: f32) -> [f32; 4] {
        let start = self.arcs[0][0].time1;
        let last = self.arcs.len() - 1;
        let end = self.arcs[last][0].time2;
        let mut values = [0.0; 4];
        if time < start || time > end {
            let (stretch, from) = if time > end { (last, end) } else { (0, start) };
            let elapsed = time - from;
            for (pair, arc) in self.arcs[stretch].iter().enumerate() {
                let (x, y) = arc.position(from);
                let (dx, dy) = arc.slope(from);
                values[pair * 2] = x + elapsed * dx;
                values[pair * 2 + 1] = y + elapsed * dy;
            }
            return values;
        }
        if let Some(stretch) = self.arcs.iter().find(|stretch| time <= stretch[0].time2) {
            for (pair, arc) in stretch.iter().enumerate() {
                (values[pair * 2], values[pair * 2 + 1]) = arc.position(time);
            }
        }
        values
    }

    /// The slopes at `time`, within the stretches, by dimension, as `getSlope`.
    pub(crate) fn slope(&self, time: f32) -> [f32; 4] {
        let start = self.arcs[0][0].time1;
        let end = self.arcs[self.arcs.len() - 1][0].time2;
        let time = time.max(start).min(end);
        let mut slopes = [0.0; 4];
        if let Some(stretch) = self.arcs.iter().find(|stretch| time <= stretch[0].time2) {
            for (pair, arc) in stretch.iter().enumerate() {
                (slopes[pair * 2], slopes[pair * 2 + 1]) = arc.slope(time);
            }
        }
        slopes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_graph::draw_text_graph;

    /// An arc below from (0, 0) to (200, 400) over 5 s, between `start` and `end` as parts of it,
    /// in one dimension.
    fn plot(dimension: usize, start: f32, end: f32) -> String {
        let values = [
            AnimationVector::zero(2),
            AnimationVector::zero(2).map(|index, _| [200.0, 400.0][index]),
        ];
        let spline = ArcSpline::new(&[ArcMode::ArcBelow], &[0.0, 5.0], &values);
        let (start, end) = (5.0 * start, 5.0 * end);
        let times: Vec<f32> = (0..60).map(|i| start + (end - start) * i as f32 / 59.0).collect();
        let at: Vec<f32> = times.iter().map(|time| spline.position(*time)[dimension]).collect();
        draw_text_graph(60, 15, &times, &at)
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, testArcSplineGraph_overallCurve
    #[test]
    fn the_whole_of_an_arc_has_its_curve() {
        assert_eq!(
            plot(0, 0.0, 1.0),
            "\
|*******************                                         | 0.0
|                   ******                                   |
|                          *****                             |
|                               *****                        |
|                                    ***                     |
|                                       ***                  | 71.429
|                                          ***               |
|                                             ***            |
|                                                **          |
|                                                 * *        |
|                                                    **      | 142.857
|                                                      **    |
|                                                        **  |
|                                                          * |
|                                                           *| 200.0
0.0                                                        5.0"
        );
        assert_eq!(
            plot(1, 0.0, 1.0),
            "\
|****                                                        | 0.0
|    ***                                                     |
|       ****                                                 |
|           ***                                              |
|              ****                                          |
|                  ****                                      | 142.857
|                      ***                                   |
|                        * ***                               |
|                             ****                           |
|                                 ***                        |
|                                    ****                    | 285.714
|                                        *****               |
|                                             *****          |
|                                                 * ******** |
|                                                           *| 400.0
0.0                                                        5.0"
        );
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, testArcSplineGraph_startOfCurve
    #[test]
    fn the_start_of_an_arc_has_its_curve() {
        assert_eq!(
            plot(0, 0.0, 1.0 / 5.0),
            "\
|****************                                            | 0.0
|                *******                                     |
|                       *****                                |
|                            ***                             |
|                                ****                        |
|                                    ***                     | 2.116
|                                       ***                  |
|                                          ***               |
|                                             ***            |
|                                                **          |
|                                                  ***       | 4.232
|                                                     **     |
|                                                       **   |
|                                                         ** |
|                                                           *| 5.925
0.0                                                        1.0"
        );
        assert_eq!(
            plot(1, 0.0, 1.0 / 5.0),
            "\
|*****                                                       | 0.0
|     ****                                                   |
|         ****                                               |
|             ****                                           |
|                 *****                                      |
|                      ****                                  | 34.515
|                          ****                              |
|                              * **                          |
|                                  ****                      |
|                                      *****                 |
|                                           ****             | 69.029
|                                               ****         |
|                                                   ****     |
|                                                       **** |
|                                                           *| 96.641
0.0                                                        1.0"
        );
    }

    // Compose: compose/animation/animation-core/src/commonTest/kotlin/androidx/compose/animation/core/ArcAnimationTest.kt, testArcSplineGraph_endOfCurve
    #[test]
    fn the_end_of_an_arc_has_its_curve() {
        assert_eq!(
            plot(0, 4.0 / 5.0, 1.0),
            "\
|******                                                      | 113.9
|      ****                                                  |
|          ****                                              |
|              **** *                                        |
|                    ****                                    |
|                        ****                                | 144.65
|                            ***                             |
|                               *****                        |
|                                    * **                    |
|                                        ****                |
|                                            ****            | 175.4
|                                                * *         |
|                                                   ****     |
|                                                       ** **|
|                                                            | 200.0
4.0                                                        5.0"
        );
        assert_eq!(
            plot(1, 4.0 / 5.0, 1.0),
            "\
|***                                                         | 361.036
|   ***                                                      |
|      **                                                    |
|        ***                                                 |
|          ***                                               |
|             ***                                            | 374.952
|                ** *                                        |
|                    ***                                     |
|                       ***                                  |
|                          ****                              |
|                             ****                           | 388.868
|                                 **** *                     |
|                                       ******               |
|                                             **** ******* * |
|                                                           *| 400.0
4.0                                                        5.0"
        );
    }

    /// Past the ends, on the slope there, as `isExtrapolate`.
    #[test]
    fn beyond_its_ends_an_arc_goes_on_along_its_slope() {
        let values =
            [AnimationVector::zero(2), AnimationVector::zero(2).map(|index, _| [2.0, 2.0][index])];
        let spline = ArcSpline::new(&[ArcMode::ArcLinear], &[0.0, 1.0], &values);
        assert_eq!(spline.position(2.0), [4.0, 4.0, 0.0, 0.0]);
        assert_eq!(spline.position(-1.0), [-2.0, -2.0, 0.0, 0.0]);
    }
}
