//! Values that interpolate with values of other types, as Compose's `Interpolatable`: what a
//! brush (and a shadow's brush) interpolates through, and [`lerp`], as
//! `Interpolatable.lerp(a, b, t)`, for values of any type (D2402).

use std::any::Any;

/// A value that may interpolate with another, of its type or not, as Compose's
/// `Interpolatable`: [`Brush`](crate::Brush) is one.
pub trait Interpolatable: Any {
    /// This value at `t` on its way to `other` (`None` for nothing: no effect), as
    /// `lerp(other, t)`: this at 0, `other` (or its equivalent) at 1, extrapolated beyond;
    /// `None` if it doesn't know how.
    fn lerp(&self, other: Option<&dyn Any>, t: f32) -> Option<Box<dyn Any>>;

    /// Whether `other` is equal to it, as Kotlin's `equals`: `false` by default (a value equal
    /// to itself only, which [`lerp`] tells by address).
    fn equals(&self, _other: &dyn Any) -> bool {
        false
    }
}

/// A value [`lerp`] takes: one that may interpolate, or any other.
#[derive(Clone, Copy)]
pub enum Interpolant<'a> {
    Interpolatable(&'a dyn Interpolatable),
    Other(&'a dyn Any),
}

impl<'a> Interpolant<'a> {
    fn as_any(self) -> &'a dyn Any {
        match self {
            Interpolant::Interpolatable(value) => value,
            Interpolant::Other(value) => value,
        }
    }
}

/// What [`lerp`] gives: the start or the stop as they were, or a value made between them.
pub enum Lerped {
    Start,
    Stop,
    Value(Box<dyn Any>),
}

impl std::fmt::Debug for Lerped {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lerped::Start => formatter.write_str("Start"),
            Lerped::Stop => formatter.write_str("Stop"),
            Lerped::Value(_) => formatter.write_str("Value(..)"),
        }
    }
}

/// Between `start` and `stop` at `t`, as `Interpolatable.lerp(a, b, t)`: two equal values (by
/// address, or by the start's [`Interpolatable::equals`]) switch halfway; else the start's
/// `lerp` if it has one, then the stop's (backwards, at `1 - t`), then the start before half way
/// and the stop after.
pub fn lerp(start: Option<Interpolant<'_>>, stop: Option<Interpolant<'_>>, t: f32) -> Lerped {
    let halfway = || if t < 0.5 { Lerped::Start } else { Lerped::Stop };
    let equal = match (start, stop) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            std::ptr::addr_eq(a.as_any(), b.as_any())
                || matches!(a, Interpolant::Interpolatable(a) if a.equals(b.as_any()))
        }
        _ => false,
    };
    if equal {
        return halfway();
    }
    if let Some(Interpolant::Interpolatable(a)) = start
        && let Some(value) = a.lerp(stop.map(Interpolant::as_any), t)
    {
        return Lerped::Value(value);
    }
    if let Some(Interpolant::Interpolatable(b)) = stop
        && let Some(value) = b.lerp(start.map(Interpolant::as_any), 1.0 - t)
    {
        return Lerped::Value(value);
    }
    halfway()
}
