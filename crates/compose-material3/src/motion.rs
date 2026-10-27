//! The standard motion scheme's springs, as Compose's `MotionScheme.standard()`
//! (`StandardMotionTokens`), for the components that animate with them. The theme doesn't carry
//! a motion scheme yet (feature:motion-scheme): components take the standard one.

use compose_animation::SpringSpec;

const fn spring(damping_ratio: f32, stiffness: f32) -> SpringSpec {
    SpringSpec { damping_ratio, stiffness, visibility_threshold: None }
}

/// `defaultSpatialSpec`: moves and size changes.
pub(crate) const DEFAULT_SPATIAL: SpringSpec = spring(0.9, 700.0);
/// `fastSpatialSpec`: small, quick moves (a switch's thumb).
pub(crate) const FAST_SPATIAL: SpringSpec = spring(0.9, 1400.0);
/// `defaultEffectsSpec`: color and opacity.
pub(crate) const DEFAULT_EFFECTS: SpringSpec = spring(1.0, 1600.0);
/// `fastEffectsSpec`: quick color and opacity changes.
pub(crate) const FAST_EFFECTS: SpringSpec = spring(1.0, 3800.0);
