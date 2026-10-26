//! Easing curves, as Compose's `Easing`: a fraction of a duration in, a fraction of the change
//! out. Cubic Bézier curves are solved as Compose's `CubicBezierEasing` (its cubic root and
//! clamping), so eased values match Compose's.

use std::f64::consts::PI;

/// An easing curve, as Compose's `Easing`.
#[derive(Clone, Copy, Debug)]
pub enum Easing {
    /// The fraction as it is, as `LinearEasing`.
    Linear,
    /// A cubic Bézier curve from (0, 0) to (1, 1) with control points (a, b) and (c, d), as
    /// `CubicBezierEasing(a, b, c, d)`.
    CubicBezier(f32, f32, f32, f32),
    /// A function of the fraction.
    Function(fn(f32) -> f32),
}

impl PartialEq for Easing {
    fn eq(&self, other: &Easing) -> bool {
        match (self, other) {
            (Easing::Linear, Easing::Linear) => true,
            (Easing::CubicBezier(a, b, c, d), Easing::CubicBezier(e, f, g, h)) => {
                (a, b, c, d) == (e, f, g, h)
            }
            (Easing::Function(a), Easing::Function(b)) => std::ptr::fn_addr_eq(*a, *b),
            _ => false,
        }
    }
}

/// Material's standard curve: quick to start, slow to settle, as `FastOutSlowInEasing`.
pub const FAST_OUT_SLOW_IN_EASING: Easing = Easing::CubicBezier(0.4, 0.0, 0.2, 1.0);
/// Entering elements: at full speed, slowing down, as `LinearOutSlowInEasing`.
pub const LINEAR_OUT_SLOW_IN_EASING: Easing = Easing::CubicBezier(0.0, 0.0, 0.2, 1.0);
/// Exiting elements: speeding up, as `FastOutLinearInEasing`.
pub const FAST_OUT_LINEAR_IN_EASING: Easing = Easing::CubicBezier(0.4, 0.0, 1.0, 1.0);
/// As `LinearEasing`.
pub const LINEAR_EASING: Easing = Easing::Linear;

/// Compose's `EasingFunctions`: the CSS and easings.net curves.
pub const EASE: Easing = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
pub const EASE_OUT: Easing = Easing::CubicBezier(0.0, 0.0, 0.58, 1.0);
pub const EASE_IN: Easing = Easing::CubicBezier(0.42, 0.0, 1.0, 1.0);
pub const EASE_IN_OUT: Easing = Easing::CubicBezier(0.42, 0.0, 0.58, 1.0);
pub const EASE_IN_SINE: Easing = Easing::CubicBezier(0.12, 0.0, 0.39, 0.0);
pub const EASE_OUT_SINE: Easing = Easing::CubicBezier(0.61, 1.0, 0.88, 1.0);
pub const EASE_IN_OUT_SINE: Easing = Easing::CubicBezier(0.37, 0.0, 0.63, 1.0);
pub const EASE_IN_CUBIC: Easing = Easing::CubicBezier(0.32, 0.0, 0.67, 0.0);
pub const EASE_OUT_CUBIC: Easing = Easing::CubicBezier(0.33, 1.0, 0.68, 1.0);
pub const EASE_IN_OUT_CUBIC: Easing = Easing::CubicBezier(0.65, 0.0, 0.35, 1.0);
pub const EASE_IN_QUINT: Easing = Easing::CubicBezier(0.64, 0.0, 0.78, 0.0);
pub const EASE_OUT_QUINT: Easing = Easing::CubicBezier(0.22, 1.0, 0.36, 1.0);
pub const EASE_IN_OUT_QUINT: Easing = Easing::CubicBezier(0.83, 0.0, 0.17, 1.0);
pub const EASE_IN_CIRC: Easing = Easing::CubicBezier(0.55, 0.0, 1.0, 0.45);
pub const EASE_OUT_CIRC: Easing = Easing::CubicBezier(0.0, 0.55, 0.45, 1.0);
pub const EASE_IN_OUT_CIRC: Easing = Easing::CubicBezier(0.85, 0.0, 0.15, 1.0);
pub const EASE_IN_QUAD: Easing = Easing::CubicBezier(0.11, 0.0, 0.5, 0.0);
pub const EASE_OUT_QUAD: Easing = Easing::CubicBezier(0.5, 1.0, 0.89, 1.0);
pub const EASE_IN_OUT_QUAD: Easing = Easing::CubicBezier(0.45, 0.0, 0.55, 1.0);
pub const EASE_IN_QUART: Easing = Easing::CubicBezier(0.5, 0.0, 0.75, 0.0);
pub const EASE_OUT_QUART: Easing = Easing::CubicBezier(0.25, 1.0, 0.5, 1.0);
pub const EASE_IN_OUT_QUART: Easing = Easing::CubicBezier(0.76, 0.0, 0.24, 1.0);
pub const EASE_IN_EXPO: Easing = Easing::CubicBezier(0.7, 0.0, 0.84, 0.0);
pub const EASE_OUT_EXPO: Easing = Easing::CubicBezier(0.16, 1.0, 0.3, 1.0);
pub const EASE_IN_OUT_EXPO: Easing = Easing::CubicBezier(0.87, 0.0, 0.13, 1.0);
pub const EASE_IN_BACK: Easing = Easing::CubicBezier(0.36, 0.0, 0.66, -0.56);
pub const EASE_OUT_BACK: Easing = Easing::CubicBezier(0.34, 1.56, 0.64, 1.0);
pub const EASE_IN_OUT_BACK: Easing = Easing::CubicBezier(0.68, -0.6, 0.32, 1.6);
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
    /// The eased fraction of `fraction`.
    pub fn transform(&self, fraction: f32) -> f32 {
        match *self {
            Easing::Linear => fraction,
            Easing::Function(function) => function(fraction),
            Easing::CubicBezier(a, b, c, d) => cubic_bezier(a, b, c, d, fraction),
        }
    }
}

/// One ulp at 1.
const ONE_ULP_AT_1: f32 = 1.192_092_9e-7;
const EPSILON: f64 = 1e-7;
const FLOAT_EPSILON: f32 = 1.05e-6;
const TAU: f64 = PI * 2.0;

/// The curve's y where its x is `fraction`, as Compose's `CubicBezierEasing.transform`: the
/// first root of x(t) − fraction in 0..1, then y(t), clamped to the curve's vertical range.
fn cubic_bezier(a: f32, b: f32, c: f32, d: f32, fraction: f32) -> f32 {
    if !(fraction > 0.0 && fraction < 1.0) {
        return fraction;
    }
    let f = fraction.max(ONE_ULP_AT_1);
    let t = find_first_cubic_root(0.0 - f, a - f, c - f, 1.0 - f);
    assert!(
        !t.is_nan(),
        "the cubic curve with parameters ({a}, {b}, {c}, {d}) has no solution at {fraction}"
    );
    let (min, max) = vertical_bounds(b, d);
    evaluate_cubic(b, d, t).clamp(min, max)
}

/// The curve's vertical range, as Compose's `computeCubicVerticalBounds(0, b, d, 1)`: 0 and 1
/// when its control points lie between them (the curve can't leave its hull).
fn vertical_bounds(b: f32, d: f32) -> (f32, f32) {
    if (0.0..=1.0).contains(&b) && (0.0..=1.0).contains(&d) {
        return (0.0, 1.0);
    }
    let (p0, p1, p2, p3) = (0.0f32, b, d, 1.0f32);
    let d0 = 3.0 * (p1 - p0);
    let d1 = 3.0 * (p2 - p1);
    let d2 = 3.0 * (p3 - p2);
    let mut roots = Vec::new();
    find_quadratic_roots(d0, d1, d2, &mut roots);
    let dd0 = 2.0 * (d1 - d0);
    let dd1 = 2.0 * (d2 - d1);
    let line_root = clamp_valid_root_in_unit_range(-dd0 / (dd1 - dd0));
    if !line_root.is_nan() {
        roots.push(line_root);
    }
    let (mut min, mut max) = (p0.min(p3), p0.max(p3));
    for t in roots {
        let y = evaluate_cubic_4(p0, p1, p2, p3, t);
        min = min.min(y);
        max = max.max(y);
    }
    (min, max)
}

fn evaluate_cubic_4(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
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

fn clamp_valid_root_in_unit_range(root: f32) -> f32 {
    let clamped = root.clamp(0.0, 1.0);
    if (clamped - root).abs() > FLOAT_EPSILON { f32::NAN } else { clamped }
}

fn find_quadratic_roots(p0: f32, p1: f32, p2: f32, roots: &mut Vec<f32>) {
    let (a, b, c) = (f64::from(p0), f64::from(p1), f64::from(p2));
    let d = a - 2.0 * b + c;
    let start = roots.len();
    let mut push = |root: f32| {
        let root = clamp_valid_root_in_unit_range(root);
        if !root.is_nan() {
            roots.push(root);
        }
    };
    if d != 0.0 {
        let v1 = -(b * b - a * c).sqrt();
        let v2 = -a + b;
        push((-(v1 + v2) / d) as f32);
        push(((v1 - v2) / d) as f32);
        if roots.len() - start > 1 {
            let (s, t) = (roots[start], roots[start + 1]);
            if s > t {
                roots.swap(start, start + 1);
            } else if s == t {
                roots.pop();
            }
        }
    } else if b != c {
        push(((2.0 * b - c) / (2.0 * b - 2.0 * c)) as f32);
    }
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
    let (p0d, p1d, p2d) = (f64::from(p0), f64::from(p1), f64::from(p2));
    let mut a = 3.0 * (p0d - 2.0 * p1d + p2d);
    let mut b = 3.0 * (p1d - p0d);
    let mut c = p0d;
    let d = -p0d + 3.0 * (p1d - p2d) + f64::from(p3);
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
