//! Easing curves, as Compose's `Easing`: a fraction of a duration in, a fraction of the change
//! out.

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
/// Material 3's emphasized easings, as `EaseInOut` and friends' common ones.
pub const EASE: Easing = Easing::CubicBezier(0.25, 0.1, 0.25, 1.0);
pub const EASE_IN: Easing = Easing::CubicBezier(0.42, 0.0, 1.0, 1.0);
pub const EASE_OUT: Easing = Easing::CubicBezier(0.0, 0.0, 0.58, 1.0);
pub const EASE_IN_OUT: Easing = Easing::CubicBezier(0.42, 0.0, 0.58, 1.0);

impl Easing {
    /// The eased fraction of `fraction` (in 0..=1).
    pub fn transform(&self, fraction: f32) -> f32 {
        match *self {
            Easing::Linear => fraction,
            Easing::Function(function) => function(fraction),
            Easing::CubicBezier(a, b, c, d) => cubic_bezier(a, b, c, d, fraction),
        }
    }
}

/// The curve's y where its x is `fraction`: x(t) solved by Newton's method, bisection as the
/// fallback, as Compose's `CubicBezierEasing`.
fn cubic_bezier(a: f32, b: f32, c: f32, d: f32, fraction: f32) -> f32 {
    if fraction <= 0.0 {
        return 0.0;
    }
    if fraction >= 1.0 {
        return 1.0;
    }
    let curve = |p1: f32, p2: f32, t: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * t * p1 + 3.0 * inverse * t * t * p2 + t * t * t
    };
    let slope = |p1: f32, p2: f32, t: f32| {
        let inverse = 1.0 - t;
        3.0 * inverse * inverse * p1 + 6.0 * inverse * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
    };
    let mut t = fraction;
    for _ in 0..8 {
        let error = curve(a, c, t) - fraction;
        if error.abs() < 1e-6 {
            return curve(b, d, t);
        }
        let derivative = slope(a, c, t);
        if derivative.abs() < 1e-6 {
            break;
        }
        t -= error / derivative;
    }
    let (mut low, mut high) = (0.0f32, 1.0f32);
    t = fraction;
    for _ in 0..32 {
        let x = curve(a, c, t);
        if (x - fraction).abs() < 1e-6 {
            break;
        }
        if x < fraction {
            low = t;
        } else {
            high = t;
        }
        t = (low + high) / 2.0;
    }
    curve(b, d, t)
}
