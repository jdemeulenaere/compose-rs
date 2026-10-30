//! The theme's motion scheme, as Compose's `MotionScheme`: the springs components animate with,
//! read as `MaterialTheme::motion_scheme()` (D1031).

use std::rc::Rc;

use compose_animation::{AnimationSpec, SpringSpec};
use compose_runtime::{ComposeArg, CompositionLocal};

const fn spring(damping_ratio: f32, stiffness: f32) -> AnimationSpec {
    AnimationSpec::Spring(SpringSpec { damping_ratio, stiffness, visibility_threshold: None })
}

/// All the animations of a theme, as Compose's `MotionScheme`: built in, [`standard`] and
/// [`expressive`], or an app's own. Spatial specs are for what changes a component's shape or
/// bounds, effects specs for what doesn't, such as a color. Compose's is an interface generic in
/// the value animated; an [`AnimationSpec`] here is for any value, so a scheme is six of them,
/// compared by value.
///
/// [`standard`]: MotionScheme::standard
/// [`expressive`]: MotionScheme::expressive
#[derive(Clone, PartialEq, Debug)]
pub struct MotionScheme {
    pub default_spatial: AnimationSpec,
    pub fast_spatial: AnimationSpec,
    pub slow_spatial: AnimationSpec,
    pub default_effects: AnimationSpec,
    pub fast_effects: AnimationSpec,
    pub slow_effects: AnimationSpec,
}

/// `StandardMotionTokens`.
const STANDARD: MotionScheme = MotionScheme {
    default_spatial: spring(0.9, 700.0),
    fast_spatial: spring(0.9, 1400.0),
    slow_spatial: spring(0.9, 300.0),
    default_effects: spring(1.0, 1600.0),
    fast_effects: spring(1.0, 3800.0),
    slow_effects: spring(1.0, 800.0),
};

/// `ExpressiveMotionTokens`.
const EXPRESSIVE: MotionScheme = MotionScheme {
    default_spatial: spring(0.8, 380.0),
    fast_spatial: spring(0.6, 800.0),
    slow_spatial: spring(0.8, 200.0),
    default_effects: spring(1.0, 1600.0),
    fast_effects: spring(1.0, 3800.0),
    slow_effects: spring(1.0, 800.0),
};

impl MotionScheme {
    /// Material's basic scheme, for utilitarian elements and recurring interactions, as
    /// `MotionScheme.standard()`.
    pub const fn standard() -> MotionScheme {
        STANDARD
    }

    /// Material's scheme for prominent elements and hero interactions, as
    /// `MotionScheme.expressive()`.
    pub const fn expressive() -> MotionScheme {
        EXPRESSIVE
    }

    /// As `defaultSpatialSpec()`.
    pub fn default_spatial_spec(&self) -> AnimationSpec {
        self.default_spatial.clone()
    }

    /// As `fastSpatialSpec()`.
    pub fn fast_spatial_spec(&self) -> AnimationSpec {
        self.fast_spatial.clone()
    }

    /// As `slowSpatialSpec()`.
    pub fn slow_spatial_spec(&self) -> AnimationSpec {
        self.slow_spatial.clone()
    }

    /// As `defaultEffectsSpec()`.
    pub fn default_effects_spec(&self) -> AnimationSpec {
        self.default_effects.clone()
    }

    /// As `fastEffectsSpec()`.
    pub fn fast_effects_spec(&self) -> AnimationSpec {
        self.fast_effects.clone()
    }

    /// As `slowEffectsSpec()`.
    pub fn slow_effects_spec(&self) -> AnimationSpec {
        self.slow_effects.clone()
    }
}

impl ComposeArg for MotionScheme {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// The theme's motion scheme, shared and compared by value, as `LOCAL_COLOR_SCHEME` (D1021):
/// Compose's `MaterialTheme.LocalMaterialTheme` holds it; compose-rs keeps a local each (D1008).
pub static LOCAL_MOTION_SCHEME: CompositionLocal<Rc<MotionScheme>> = CompositionLocal::with_policy(
    "LocalMotionScheme",
    compose_runtime::SharedValueEqualityPolicy,
    default_motion_scheme,
);

/// The standard scheme, made once per thread.
pub(crate) fn default_motion_scheme() -> Rc<MotionScheme> {
    thread_local! {
        static DEFAULT: Rc<MotionScheme> = Rc::new(MotionScheme::standard());
    }
    DEFAULT.with(Rc::clone)
}

/// The expressive scheme, made once per thread.
pub(crate) fn expressive_motion_scheme() -> Rc<MotionScheme> {
    thread_local! {
        static EXPRESSIVE_SCHEME: Rc<MotionScheme> = Rc::new(MotionScheme::expressive());
    }
    EXPRESSIVE_SCHEME.with(Rc::clone)
}
