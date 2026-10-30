//! Easing curves, as Compose's `Easing`: a fraction of a duration in, a fraction of the change
//! out. Cubic Bézier curves are solved as Compose's `CubicBezierEasing` (its cubic root and
//! clamping), so eased values match Compose's.

use std::cell::OnceCell;
use std::f64::consts::PI;
use std::rc::Rc;

use compose_ui::{Offset, Path, PathCommand};

/// An easing curve, as Compose's `Easing`: one of Compose's curves, or any function of the
/// fraction (a capturing closure too, as Compose's `fun interface`, [`Easing::custom`]).
#[derive(Clone)]
pub enum Easing {
    /// The fraction as it is, as `LinearEasing`.
    Linear,
    /// A cubic Bézier curve from (0, 0) to (1, 1), as `CubicBezierEasing(a, b, c, d)`: made by
    /// [`Easing::cubic_bezier`].
    CubicBezier(CubicBezierEasing),
    /// A function of the fraction.
    Function(fn(f32) -> f32),
    /// Any curve: a closure, which may hold data (a path, parameters), as an `Easing` object
    /// of an app's. Equal only to itself (its clones), as Compose compares easings it can't
    /// see into by identity.
    Custom(Rc<dyn Fn(f32) -> f32>),
}

impl std::fmt::Debug for Easing {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Easing::Linear => formatter.write_str("Linear"),
            Easing::CubicBezier(curve) => write!(formatter, "{curve:?}"),
            Easing::Function(_) => formatter.write_str("Function"),
            Easing::Custom(_) => formatter.write_str("Custom"),
        }
    }
}

impl PartialEq for Easing {
    fn eq(&self, other: &Easing) -> bool {
        match (self, other) {
            (Easing::Linear, Easing::Linear) => true,
            (Easing::CubicBezier(a), Easing::CubicBezier(b)) => a == b,
            (Easing::Function(a), Easing::Function(b)) => std::ptr::fn_addr_eq(*a, *b),
            (Easing::Custom(a), Easing::Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl Easing {
    /// The curve of `path`, as `PathEasing(path)`: its x is the fraction and its y what comes out,
    /// so it goes from (0, 0) to (1, 1), covering all the x between, with no `close`. An invalid
    /// path panics at the first fraction that isn't an end, as there. A path of
    /// [`Path::op`](Path::op)'s result, on Android and headless where it has no segments to read
    /// until a platform makes them, is refused here, where it's written (D1415).
    pub fn path(path: &Path) -> Easing {
        assert_has_segments(path);
        let path = path.clone();
        let segments = OnceCell::new();
        Easing::Custom(Rc::new(move |fraction| {
            if fraction <= 0.0 {
                return 0.0;
            }
            if fraction >= 1.0 {
                return 1.0;
            }
            let segments = segments.get_or_init(|| EasingSegment::of_path(&path));
            let segment = segments
                .iter()
                .find(|segment: &&EasingSegment| {
                    segment.min_x <= fraction && fraction <= segment.max_x
                })
                .expect("The easing path is invalid. Make sure it is continuous on the x axis.");
            let t = segment.first_root(fraction);
            assert!(
                !t.is_nan(),
                "The easing path is invalid. Make sure it does not contain NaN/Infinity values."
            );
            segment.y(t)
        }))
    }
}

/// Refuses a path of steps a platform would compute.
fn assert_has_segments(path: &Path) {
    assert!(
        path.data().steps().is_empty(),
        "The easing path can't be the result of a path operation."
    );
}

/// A line, a quadratic or a cubic of a path easing, its points as they are in a `PathSegment`, with
/// its range along x. Compose keeps them in an interval tree: for the few of an easing curve, a list
/// finds the same overlap.
struct EasingSegment {
    points: Vec<Offset>,
    min_x: f32,
    max_x: f32,
}

impl EasingSegment {
    fn new(points: Vec<Offset>) -> EasingSegment {
        let mut segment = EasingSegment { points, min_x: 0.0, max_x: 0.0 };
        let (start, end) = (segment.points[0].x, segment.points[segment.points.len() - 1].x);
        segment.min_x = start.min(end);
        segment.max_x = start.max(end);
        for t in segment.derivative_roots() {
            let x = segment.x(t);
            segment.min_x = segment.min_x.min(x);
            segment.max_x = segment.max_x.max(x);
        }
        segment
    }

    /// What `path` has, as `initializeEasing`.
    fn of_path(path: &Path) -> Vec<EasingSegment> {
        assert_has_segments(path);
        let data = path.data();
        let mut segments = Vec::new();
        let mut current = Offset::new(0.0, 0.0);
        for command in data.commands() {
            let points = match *command {
                PathCommand::MoveTo(to) => {
                    current = to;
                    continue;
                }
                PathCommand::LineTo(end) => vec![current, end],
                PathCommand::QuadTo(control, end) => vec![current, control, end],
                PathCommand::CubicTo(first, second, end) => vec![current, first, second, end],
                PathCommand::Close => panic!("The path cannot contain a close() command."),
            };
            current = points[points.len() - 1];
            segments.push(EasingSegment::new(points));
        }
        let covers =
            |x: f32| segments.iter().any(|segment| segment.min_x <= x && x <= segment.max_x);
        assert!(covers(0.0) && covers(1.0), "The easing path must start at 0.0f and end at 1.0f.");
        segments
    }

    fn x(&self, t: f32) -> f32 {
        self.evaluate(t, |point| point.x)
    }

    fn y(&self, t: f32) -> f32 {
        self.evaluate(t, |point| point.y)
    }

    /// As `evaluateX` and `evaluateY`.
    fn evaluate(&self, t: f32, of: impl Fn(&Offset) -> f32) -> f32 {
        match self.points.as_slice() {
            [p0, p1] => (of(p1) - of(p0)) * t + of(p0),
            [p0, p1, p2] => {
                let by = 2.0 * (of(p1) - of(p0));
                let ay = of(p2) - 2.0 * of(p1) + of(p0);
                (ay * t + by) * t + of(p0)
            }
            [p0, p1, p2, p3] => evaluate_cubic_4(of(p0), of(p1), of(p2), of(p3), t),
            _ => f32::NAN,
        }
    }

    /// As `findFirstRoot`.
    fn first_root(&self, fraction: f32) -> f32 {
        let x = |point: &Offset| point.x - fraction;
        match self.points.as_slice() {
            [p0, p1] => clamp_valid_root_in_unit_range(-x(p0) / (x(p1) - x(p0))),
            [p0, p1, p2] => find_first_quadratic_root(x(p0), x(p1), x(p2)),
            [p0, p1, p2, p3] => find_first_cubic_root(x(p0), x(p1), x(p2), x(p3)),
            _ => f32::NAN,
        }
    }

    /// As `findDerivativeRoots`, along x.
    fn derivative_roots(&self) -> Vec<f32> {
        let line_root = |p0: f32, p1: f32| clamp_valid_root_in_unit_range(-p0 / (p1 - p0));
        let mut found = Vec::new();
        match self.points.as_slice() {
            [p0, p1, p2] => {
                found.push(line_root(2.0 * (p1.x - p0.x), 2.0 * (p2.x - p1.x)));
            }
            [p0, p1, p2, p3] => {
                let (d0, d1, d2) = (3.0 * (p1.x - p0.x), 3.0 * (p2.x - p1.x), 3.0 * (p3.x - p2.x));
                let mut roots = [0.0f32; 3];
                let count = find_quadratic_roots(d0, d1, d2, &mut roots);
                found.extend_from_slice(&roots[..count]);
                found.push(line_root(2.0 * (d1 - d0), 2.0 * (d2 - d1)));
            }
            _ => {}
        }
        found.retain(|root| !root.is_nan());
        found
    }
}

/// The first root in 0..1 of the quadratic of control values `p0` to `p2`, as Compose's
/// `findFirstQuadraticRoot`.
fn find_first_quadratic_root(p0: f32, p1: f32, p2: f32) -> f32 {
    let (a, b, c) = (f64::from(p0), f64::from(p1), f64::from(p2));
    let d = a - 2.0 * b + c;
    if d != 0.0 {
        let v1 = -(b * b - a * c).sqrt();
        let v2 = -a + b;
        let root = clamp_valid_root_in_unit_range((-(v1 + v2) / d) as f32);
        if !root.is_nan() {
            return root;
        }
        return clamp_valid_root_in_unit_range(((v1 - v2) / d) as f32);
    }
    if b != c {
        return clamp_valid_root_in_unit_range(((2.0 * b - c) / (2.0 * b - 2.0 * c)) as f32);
    }
    f32::NAN
}

/// Material's standard curve: quick to start, slow to settle, as `FastOutSlowInEasing`.
pub const FAST_OUT_SLOW_IN_EASING: Easing = Easing::cubic_bezier(0.4, 0.0, 0.2, 1.0);
/// Entering elements: at full speed, slowing down, as `LinearOutSlowInEasing`.
pub const LINEAR_OUT_SLOW_IN_EASING: Easing = Easing::cubic_bezier(0.0, 0.0, 0.2, 1.0);
/// Exiting elements: speeding up, as `FastOutLinearInEasing`.
pub const FAST_OUT_LINEAR_IN_EASING: Easing = Easing::cubic_bezier(0.4, 0.0, 1.0, 1.0);
/// As `LinearEasing`.
pub const LINEAR_EASING: Easing = Easing::Linear;

/// Compose's `EasingFunctions`: the CSS and easings.net curves.
pub const EASE: Easing = Easing::cubic_bezier(0.25, 0.1, 0.25, 1.0);
pub const EASE_OUT: Easing = Easing::cubic_bezier(0.0, 0.0, 0.58, 1.0);
pub const EASE_IN: Easing = Easing::cubic_bezier(0.42, 0.0, 1.0, 1.0);
pub const EASE_IN_OUT: Easing = Easing::cubic_bezier(0.42, 0.0, 0.58, 1.0);
pub const EASE_IN_SINE: Easing = Easing::cubic_bezier(0.12, 0.0, 0.39, 0.0);
pub const EASE_OUT_SINE: Easing = Easing::cubic_bezier(0.61, 1.0, 0.88, 1.0);
pub const EASE_IN_OUT_SINE: Easing = Easing::cubic_bezier(0.37, 0.0, 0.63, 1.0);
pub const EASE_IN_CUBIC: Easing = Easing::cubic_bezier(0.32, 0.0, 0.67, 0.0);
pub const EASE_OUT_CUBIC: Easing = Easing::cubic_bezier(0.33, 1.0, 0.68, 1.0);
pub const EASE_IN_OUT_CUBIC: Easing = Easing::cubic_bezier(0.65, 0.0, 0.35, 1.0);
pub const EASE_IN_QUINT: Easing = Easing::cubic_bezier(0.64, 0.0, 0.78, 0.0);
pub const EASE_OUT_QUINT: Easing = Easing::cubic_bezier(0.22, 1.0, 0.36, 1.0);
pub const EASE_IN_OUT_QUINT: Easing = Easing::cubic_bezier(0.83, 0.0, 0.17, 1.0);
pub const EASE_IN_CIRC: Easing = Easing::cubic_bezier(0.55, 0.0, 1.0, 0.45);
pub const EASE_OUT_CIRC: Easing = Easing::cubic_bezier(0.0, 0.55, 0.45, 1.0);
pub const EASE_IN_OUT_CIRC: Easing = Easing::cubic_bezier(0.85, 0.0, 0.15, 1.0);
pub const EASE_IN_QUAD: Easing = Easing::cubic_bezier(0.11, 0.0, 0.5, 0.0);
pub const EASE_OUT_QUAD: Easing = Easing::cubic_bezier(0.5, 1.0, 0.89, 1.0);
pub const EASE_IN_OUT_QUAD: Easing = Easing::cubic_bezier(0.45, 0.0, 0.55, 1.0);
pub const EASE_IN_QUART: Easing = Easing::cubic_bezier(0.5, 0.0, 0.75, 0.0);
pub const EASE_OUT_QUART: Easing = Easing::cubic_bezier(0.25, 1.0, 0.5, 1.0);
pub const EASE_IN_OUT_QUART: Easing = Easing::cubic_bezier(0.76, 0.0, 0.24, 1.0);
pub const EASE_IN_EXPO: Easing = Easing::cubic_bezier(0.7, 0.0, 0.84, 0.0);
pub const EASE_OUT_EXPO: Easing = Easing::cubic_bezier(0.16, 1.0, 0.3, 1.0);
pub const EASE_IN_OUT_EXPO: Easing = Easing::cubic_bezier(0.87, 0.0, 0.13, 1.0);
pub const EASE_IN_BACK: Easing = Easing::cubic_bezier(0.36, 0.0, 0.66, -0.56);
pub const EASE_OUT_BACK: Easing = Easing::cubic_bezier(0.34, 1.56, 0.64, 1.0);
pub const EASE_IN_OUT_BACK: Easing = Easing::cubic_bezier(0.68, -0.6, 0.32, 1.6);
pub const EASE_IN_ELASTIC: Easing = Easing::Function(ease_in_elastic);
pub const EASE_OUT_ELASTIC: Easing = Easing::Function(ease_out_elastic);
pub const EASE_IN_OUT_ELASTIC: Easing = Easing::Function(ease_in_out_elastic);
pub const EASE_OUT_BOUNCE: Easing = Easing::Function(ease_out_bounce);
pub const EASE_IN_BOUNCE: Easing = Easing::Function(ease_in_bounce);
pub const EASE_IN_OUT_BOUNCE: Easing = Easing::Function(ease_in_out_bounce);

/// Kotlin's `Float.pow`: in doubles, back to a float.
fn pow(base: f32, exponent: f32) -> f32 {
    f64::from(base).powf(f64::from(exponent)) as f32
}

fn ease_in_elastic(fraction: f32) -> f32 {
    let c4 = (2.0 * PI) / 3.0;
    if fraction == 0.0 || fraction == 1.0 {
        return fraction;
    }
    let sine = (f64::from(fraction * 10.0 - 10.75) * c4).sin();
    (-f64::from(pow(2.0, 10.0 * fraction - 10.0)) * sine) as f32
}

fn ease_out_elastic(fraction: f32) -> f32 {
    let c4 = (2.0 * PI) / 3.0;
    if fraction == 0.0 || fraction == 1.0 {
        return fraction;
    }
    let sine = (f64::from(fraction * 10.0 - 0.75) * c4).sin();
    (f64::from(pow(2.0, -10.0 * fraction)) * sine + 1.0) as f32
}

fn ease_in_out_elastic(fraction: f32) -> f32 {
    let c5 = (2.0 * PI) / 4.5;
    if fraction == 0.0 || fraction == 1.0 {
        return fraction;
    }
    if (0.0..=0.5).contains(&fraction) {
        let sine = (f64::from(20.0 * fraction - 11.125) * c5).sin();
        (-(f64::from(pow(2.0, 20.0 * fraction - 10.0)) * sine) / 2.0) as f32
    } else {
        let sine = (f64::from(fraction * 20.0 - 11.125) * c5).sin();
        ((f64::from(pow(2.0, -20.0 * fraction + 10.0)) * sine) / 2.0) as f32 + 1.0
    }
}

fn ease_out_bounce(fraction: f32) -> f32 {
    let (n1, d1) = (7.5625f32, 2.75f32);
    let mut x = fraction;
    if x < 1.0 / d1 {
        n1 * x * x
    } else if x < 2.0 / d1 {
        x -= 1.5 / d1;
        n1 * x * x + 0.75
    } else if x < 2.5 / d1 {
        x -= 2.25 / d1;
        n1 * x * x + 0.9375
    } else {
        x -= 2.625 / d1;
        n1 * x * x + 0.984375
    }
}

fn ease_in_bounce(fraction: f32) -> f32 {
    1.0 - ease_out_bounce(1.0 - fraction)
}

fn ease_in_out_bounce(fraction: f32) -> f32 {
    if f64::from(fraction) < 0.5 {
        (1.0 - ease_out_bounce(1.0 - 2.0 * fraction)) / 2.0
    } else {
        (1.0 + ease_out_bounce(2.0 * fraction - 1.0)) / 2.0
    }
}

impl Easing {
    /// The cubic Bézier curve from (0, 0) to (1, 1) with control points (a, b) and (c, d), as
    /// `CubicBezierEasing(a, b, c, d)`: its vertical range is computed here, once.
    pub const fn cubic_bezier(a: f32, b: f32, c: f32, d: f32) -> Easing {
        Easing::CubicBezier(CubicBezierEasing::new(a, b, c, d))
    }

    /// An easing of `transform`, as an `Easing { fraction -> … }` of Compose's: it may capture
    /// what it needs.
    pub fn custom(transform: impl Fn(f32) -> f32 + 'static) -> Easing {
        Easing::Custom(Rc::new(transform))
    }

    /// The eased fraction of `fraction`.
    pub fn transform(&self, fraction: f32) -> f32 {
        match self {
            Easing::Linear => fraction,
            Easing::Function(function) => function(fraction),
            Easing::CubicBezier(curve) => curve.transform(fraction),
            Easing::Custom(transform) => transform(fraction),
        }
    }
}

/// A cubic Bézier curve from (0, 0) to (1, 1) with control points (a, b) and (c, d), as
/// Compose's `CubicBezierEasing`: solved as Compose's, its vertical range computed when it's
/// made (as Compose's `init`), the eased value clamped to it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CubicBezierEasing {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    min: f32,
    max: f32,
}

impl CubicBezierEasing {
    /// The curve of control points (a, b) and (c, d). Panics if one is NaN, as Compose's
    /// `requirePrecondition`.
    pub const fn new(a: f32, b: f32, c: f32, d: f32) -> CubicBezierEasing {
        assert!(
            !a.is_nan() && !b.is_nan() && !c.is_nan() && !d.is_nan(),
            "Parameters to CubicBezierEasing cannot be NaN"
        );
        let (min, max) = vertical_bounds(b, d);
        CubicBezierEasing { a, b, c, d, min, max }
    }

    /// The control points, `(a, b, c, d)`.
    pub const fn control_points(&self) -> (f32, f32, f32, f32) {
        (self.a, self.b, self.c, self.d)
    }

    /// The eased fraction: the curve's y where its x is `fraction`, as `transform`.
    pub fn transform(&self, fraction: f32) -> f32 {
        let CubicBezierEasing { a, b, c, d, min, max } = *self;
        if !(fraction > 0.0 && fraction < 1.0) {
            return fraction;
        }
        let f = fraction.max(ONE_ULP_AT_1);
        let t = find_first_cubic_root(0.0 - f, a - f, c - f, 1.0 - f);
        assert!(
            !t.is_nan(),
            "the cubic curve with parameters ({a}, {b}, {c}, {d}) has no solution at {fraction}"
        );
        // Compose's `fastCoerceIn`: the min, then the max.
        let y = evaluate_cubic(b, d, t);
        let y = if y < min { min } else { y };
        if y > max { max } else { y }
    }
}

/// One ulp at 1.
const ONE_ULP_AT_1: f32 = 1.192_092_9e-7;
const EPSILON: f64 = 1e-7;
const FLOAT_EPSILON: f32 = 1.05e-6;
const TAU: f64 = PI * 2.0;

/// The curve's vertical range, as Compose's `computeCubicVerticalBounds(0, b, d, 1)`: the
/// ends, and the values at the derivative's roots and the second derivative's.
const fn vertical_bounds(b: f32, d: f32) -> (f32, f32) {
    let (p0, p1, p2, p3) = (0.0f32, b, d, 1.0f32);
    let d0 = 3.0 * (p1 - p0);
    let d1 = 3.0 * (p2 - p1);
    let d2 = 3.0 * (p3 - p2);
    let mut roots = [0.0f32; 3];
    let mut count = find_quadratic_roots(d0, d1, d2, &mut roots);
    let dd0 = 2.0 * (d1 - d0);
    let dd1 = 2.0 * (d2 - d1);
    let line_root = clamp_valid_root_in_unit_range(-dd0 / (dd1 - dd0));
    if !line_root.is_nan() {
        roots[count] = line_root;
        count += 1;
    }
    let (mut min, mut max) = if p0 < p3 { (p0, p3) } else { (p3, p0) };
    let mut index = 0;
    while index < count {
        let y = evaluate_cubic_4(p0, p1, p2, p3, roots[index]);
        // Kotlin's `min` and `max`: NaN wins.
        min = if y.is_nan() || y < min { y } else { min };
        max = if y.is_nan() || y > max { y } else { max };
        index += 1;
    }
    (min, max)
}

const fn evaluate_cubic_4(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let a = p3 + 3.0 * (p1 - p2) - p0;
    let b = 3.0 * (p2 - 2.0 * p1 + p0);
    let c = 3.0 * (p1 - p0);
    ((a * t + b) * t + c) * t + p0
}

/// y(t) of a curve from 0 to 1 with control values `p1` and `p2`, as Compose's
/// `evaluateCubic(p1, p2, t)`.
fn evaluate_cubic(p1: f32, p2: f32, t: f32) -> f32 {
    let a = 1.0 / 3.0 + (p1 - p2);
    let b = p2 - 2.0 * p1;
    let c = p1;
    3.0 * ((a * t + b) * t + c) * t
}

/// As Compose's: `fastCoerceIn(0, 1)` (a NaN stays NaN), NaN if that moved it.
const fn clamp_valid_root_in_unit_range(root: f32) -> f32 {
    let clamped = if root < 0.0 { 0.0 } else { root };
    let clamped = if clamped > 1.0 { 1.0 } else { clamped };
    if (clamped - root).abs() > FLOAT_EPSILON { f32::NAN } else { clamped }
}

/// The roots in 0..1 of the quadratic of control values `p0` to `p2`, as Compose's
/// `findQuadraticRoots`: written into `roots`, sorted; how many.
const fn find_quadratic_roots(p0: f32, p1: f32, p2: f32, roots: &mut [f32; 3]) -> usize {
    let (a, b, c) = (p0 as f64, p1 as f64, p2 as f64);
    let d = a - 2.0 * b + c;
    let mut count = 0;
    if d != 0.0 {
        let v1 = -const_sqrt(b * b - a * c);
        let v2 = -a + b;
        let first = clamp_valid_root_in_unit_range((-(v1 + v2) / d) as f32);
        if !first.is_nan() {
            roots[count] = first;
            count += 1;
        }
        let second = clamp_valid_root_in_unit_range(((v1 - v2) / d) as f32);
        if !second.is_nan() {
            roots[count] = second;
            count += 1;
        }
        if count > 1 {
            let (s, t) = (roots[0], roots[1]);
            if s > t {
                roots[0] = t;
                roots[1] = s;
            } else if s == t {
                count -= 1;
            }
        }
    } else if b != c {
        let root = clamp_valid_root_in_unit_range(((2.0 * b - c) / (2.0 * b - 2.0 * c)) as f32);
        if !root.is_nan() {
            roots[count] = root;
            count += 1;
        }
    }
    count
}

/// `x.sqrt()`, correctly rounded as IEEE's (and the JVM's) square root, in a `const fn`: the
/// integer square root of the significand widened to 112 bits, its remainder kept as a sticky
/// bit for the final rounding.
const fn const_sqrt(x: f64) -> f64 {
    if x.is_nan() || x < 0.0 {
        return f64::NAN;
    }
    if x == 0.0 || x.is_infinite() {
        return x;
    }
    let bits = x.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i64;
    let fraction = bits & ((1 << 52) - 1);
    // x = significand × 2^exponent, the significand an integer.
    let (mut significand, mut exponent) = if raw_exponent == 0 {
        (fraction as u128, -1074i64)
    } else {
        ((fraction | (1 << 52)) as u128, raw_exponent - 1075)
    };
    if exponent % 2 != 0 {
        significand <<= 1;
        exponent -= 1;
    }
    // Widened so the root has at least 55 bits: 2 below the rounding position, then sticky.
    let shift = 112 - (128 - significand.leading_zeros() as i64);
    let shift = shift + (shift & 1);
    let widened = significand << shift;
    let root = widened.isqrt();
    let sticky = if root * root != widened { 1 } else { 0 };
    // The cast rounds to nearest, ties to even, the sticky bit settling the ties.
    let value = (root | sticky) as f64;
    let scale = (exponent - shift) / 2;
    scale_by_power_of_two(value, scale)
}

/// `value × 2^scale`, exactly while the result is normal.
const fn scale_by_power_of_two(mut value: f64, mut scale: i64) -> f64 {
    while scale > 1000 {
        value *= f64::from_bits(((1023 + 1000) as u64) << 52);
        scale -= 1000;
    }
    while scale < -1000 {
        value *= f64::from_bits(((1023 - 1000) as u64) << 52);
        scale += 1000;
    }
    value * f64::from_bits(((1023 + scale) as u64) << 52)
}

fn close_to_zero(x: f64) -> bool {
    x.abs() < EPSILON
}

/// A fast cube root, as Compose's `fastCbrt`: an estimate from the float's bits, refined twice.
fn fast_cbrt(x: f32) -> f32 {
    let bits = i64::from(x.to_bits() as i32) & 0x1_ffff_ffff;
    let mut estimate = f32::from_bits(0x2a51_0554i32.wrapping_add((bits / 3) as i32) as u32);
    estimate -= (estimate - x / (estimate * estimate)) * (1.0 / 3.0);
    estimate -= (estimate - x / (estimate * estimate)) * (1.0 / 3.0);
    estimate
}

/// The first root in 0..1 of the cubic of control values `p0` to `p3`, as Compose's
/// `findFirstCubicRoot`.
fn find_first_cubic_root(p0: f32, p1: f32, p2: f32, p3: f32) -> f32 {
    // Kotlin's promotions: `2.0 * p1` makes the first sum doubles, while `p1 - p0`, `p1 - p2`
    // and `-p0` are floats, widened after.
    let (p0d, p1d, p2d) = (f64::from(p0), f64::from(p1), f64::from(p2));
    let mut a = 3.0 * (p0d - 2.0 * p1d + p2d);
    let mut b = 3.0 * f64::from(p1 - p0);
    let mut c = p0d;
    let d = f64::from(-p0) + 3.0 * f64::from(p1 - p2) + f64::from(p3);
    if close_to_zero(d) {
        if close_to_zero(a) {
            if close_to_zero(b) {
                return f32::NAN;
            }
            return clamp_valid_root_in_unit_range((-c / b) as f32);
        }
        let q = (b * b - 4.0 * a * c).sqrt();
        let a2 = 2.0 * a;
        let root = clamp_valid_root_in_unit_range(((q - b) / a2) as f32);
        if !root.is_nan() {
            return root;
        }
        return clamp_valid_root_in_unit_range(((-b - q) / a2) as f32);
    }
    a /= d;
    b /= d;
    c /= d;
    let o3 = (3.0 * b - a * a) / 9.0;
    let q2 = (2.0 * a * a * a - 9.0 * a * b + 27.0 * c) / 54.0;
    let discriminant = q2 * q2 + o3 * o3 * o3;
    let a3 = a / 3.0;
    if discriminant < 0.0 {
        let mp33 = -(o3 * o3 * o3);
        let r = mp33.sqrt();
        let t = -q2 / r;
        let cos_phi = t.clamp(-1.0, 1.0);
        let phi = cos_phi.acos();
        let t1 = f64::from(2.0 * fast_cbrt(r as f32));
        let root = clamp_valid_root_in_unit_range((t1 * (phi / 3.0).cos() - a3) as f32);
        if !root.is_nan() {
            return root;
        }
        let root = clamp_valid_root_in_unit_range((t1 * ((phi + TAU) / 3.0).cos() - a3) as f32);
        if !root.is_nan() {
            return root;
        }
        return clamp_valid_root_in_unit_range((t1 * ((phi + 2.0 * TAU) / 3.0).cos() - a3) as f32);
    } else if discriminant == 0.0 {
        let u1 = -fast_cbrt(q2 as f32);
        let root = clamp_valid_root_in_unit_range(2.0 * u1 - a3 as f32);
        if !root.is_nan() {
            return root;
        }
        return clamp_valid_root_in_unit_range(-u1 - a3 as f32);
    }
    let sd = discriminant.sqrt();
    let u1 = fast_cbrt((-q2 + sd) as f32);
    let v1 = fast_cbrt((q2 + sd) as f32);
    clamp_valid_root_in_unit_range((f64::from(u1 - v1) - a3) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `const` square root is the hardware's, bit for bit.
    #[test]
    fn the_const_square_root_is_ieees() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..200_000 {
            let x = f64::from_bits(next() & 0x7fef_ffff_ffff_ffff);
            assert_eq!(const_sqrt(x).to_bits(), x.sqrt().to_bits(), "sqrt({x:e})");
            let small = (next() >> 11) as f64 / (1u64 << 53) as f64 * 4.0;
            assert_eq!(const_sqrt(small).to_bits(), small.sqrt().to_bits(), "sqrt({small:e})");
        }
        for x in [0.0, 1.0, 2.0, 0.25, 1e-310, f64::MIN_POSITIVE, f64::MAX] {
            assert_eq!(const_sqrt(x).to_bits(), x.sqrt().to_bits(), "sqrt({x:e})");
        }
        assert!(const_sqrt(-1.0).is_nan());
    }
}
