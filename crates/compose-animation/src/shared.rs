//! What shared transitions are configured with, as the parts of Compose's
//! `SharedTransitionScope.kt` that need no scope: the transition itself needs ui lookahead and
//! an overlay drawing recorded layers (D829). What takes a `SharedContentState` (`OverlayClip`,
//! `SharedContentConfig`'s `isEnabled` and `alternativeTargetBoundsInTransitionScopeAfterRemoval`)
//! comes with it.

use std::rc::Rc;

use compose_runtime::ComposeArg;
use compose_ui::{Alignment, ContentScale, IntSize, Rect};

use crate::spec::{AnimationSpec, Spring, spring_with};
use crate::vector::VisibilityThreshold;

/// The animation of bounds from the initial ones to the target's, as Compose's `BoundsTransform`.
#[derive(Clone)]
pub struct BoundsTransform {
    create: Rc<dyn Fn(Rect, Rect) -> AnimationSpec>,
}

impl BoundsTransform {
    /// As `BoundsTransform { initialBounds, targetBounds -> … }`.
    pub fn new(create: impl Fn(Rect, Rect) -> AnimationSpec + 'static) -> BoundsTransform {
        BoundsTransform { create: Rc::new(create) }
    }

    /// The spec animating `initial_bounds` to `target_bounds`, as `createAnimationSpec`.
    pub fn create_animation_spec(
        &self,
        initial_bounds: Rect,
        target_bounds: Rect,
    ) -> AnimationSpec {
        (self.create)(initial_bounds, target_bounds)
    }
}

/// The same function, as Compose's identity.
impl PartialEq for BoundsTransform {
    fn eq(&self, other: &BoundsTransform) -> bool {
        Rc::ptr_eq(&self.create, &other.create)
    }
}

impl ComposeArg for BoundsTransform {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for BoundsTransform {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BoundsTransform")
    }
}

/// The size a shared content's parent sees during a transition, as Compose's
/// `SharedTransitionScope.PlaceholderSize`.
#[derive(Clone)]
pub struct PlaceholderSize {
    calculate: Rc<dyn Fn(IntSize, IntSize) -> IntSize>,
}

impl PlaceholderSize {
    /// As `PlaceholderSize { contentSize, animatedSize -> … }`.
    pub fn new(calculate: impl Fn(IntSize, IntSize) -> IntSize + 'static) -> PlaceholderSize {
        PlaceholderSize { calculate: Rc::new(calculate) }
    }

    /// The size animating, as `AnimatedSize`: the parent lays out again each frame. One shared
    /// value.
    pub fn animated_size() -> PlaceholderSize {
        thread_local! {
            static SIZE: PlaceholderSize = PlaceholderSize::new(|_, animated| animated);
        }
        SIZE.with(PlaceholderSize::clone)
    }

    /// The content's own size, as `ContentSize`. One shared value.
    pub fn content_size() -> PlaceholderSize {
        thread_local! {
            static SIZE: PlaceholderSize = PlaceholderSize::new(|content, _| content);
        }
        SIZE.with(PlaceholderSize::clone)
    }

    /// As `calculateSize(contentSize, animatedSize)`.
    pub fn calculate_size(&self, content_size: IntSize, animated_size: IntSize) -> IntSize {
        (self.calculate)(content_size, animated_size)
    }
}

impl PartialEq for PlaceholderSize {
    fn eq(&self, other: &PlaceholderSize) -> bool {
        Rc::ptr_eq(&self.calculate, &other.calculate)
    }
}

impl ComposeArg for PlaceholderSize {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl std::fmt::Debug for PlaceholderSize {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("PlaceholderSize")
    }
}

/// How shared bounds' content fits the animated bounds, as Compose's
/// `SharedTransitionScope.ResizeMode`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ResizeMode {
    /// The content is measured again at the bounds each frame, as `RemeasureToBounds`.
    RemeasureToBounds,
    /// The content keeps its lookahead size, scaled to the bounds, as `scaleToBounds`.
    ScaleToBounds { content_scale: ContentScale, alignment: Alignment },
}

impl ResizeMode {
    /// As `scaleToBounds(contentScale, alignment)`.
    pub fn scale_to_bounds(content_scale: ContentScale, alignment: Alignment) -> ResizeMode {
        ResizeMode::ScaleToBounds { content_scale, alignment }
    }
}

/// The default, as `scaleToBounds()`'s and `SharedTransitionDefaults.ResizeMode`: `FillWidth`,
/// centred.
impl Default for ResizeMode {
    fn default() -> ResizeMode {
        ResizeMode::scale_to_bounds(ContentScale::FillWidth, Alignment::CENTER)
    }
}

impl ComposeArg for ResizeMode {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// How a shared content takes part in transitions, as Compose's
/// `SharedTransitionScope.SharedContentConfig`.
pub trait SharedContentConfig {
    /// Whether an animation running goes on once the content is disabled, as
    /// `shouldKeepEnabledForOngoingAnimation`.
    fn should_keep_enabled_for_ongoing_animation(&self) -> bool {
        true
    }

    /// Whether a deferred transition's manual transforms apply to it, as
    /// `permitTransformDuringDeferredTransition`.
    fn permit_transform_during_deferred_transition(&self) -> bool {
        true
    }
}

/// Compose's defaults, as `SharedTransitionDefaults.SharedContentConfig`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DefaultSharedContentConfig;

impl SharedContentConfig for DefaultSharedContentConfig {}

/// What shared transitions take by default, as Compose's `SharedTransitionDefaults`.
pub struct SharedTransitionDefaults;

impl SharedTransitionDefaults {
    /// A spring of stiffness 400 and a pixel's threshold for any bounds, as `BoundsTransform`. One
    /// shared value.
    pub fn bounds_transform() -> BoundsTransform {
        thread_local! {
            static TRANSFORM: BoundsTransform = BoundsTransform::new(|_, _| {
                AnimationSpec::Spring(
                    spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
                        .visibility_threshold(Rect::VISIBILITY_THRESHOLD),
                )
            });
        }
        TRANSFORM.with(BoundsTransform::clone)
    }

    /// As `SharedContentConfig`.
    pub fn shared_content_config() -> DefaultSharedContentConfig {
        DefaultSharedContentConfig
    }
}
