//! `AnimatedVisibility` and the enter and exit transitions, as Compose's: content that animates
//! in when it becomes visible and out before it leaves.
//!
//! As Compose's (D804): the visibility drives a transition, and the content a child transition
//! of `EnterExitState`s (`PreEnter`, `Visible`, `PostExit`); its parts animate their real values
//! on that transition, as deferred animations whose targets are known while measuring and
//! drawing: the alpha and scale (floats, read in a graphics layer), the size (an `IntSize`, the
//! content measured at its full size and clipped), a slide's offset (an `IntOffset`), each along
//! its own spec, defaulting to Compose's springs.

use std::cell::{Cell, RefCell};
use std::ops::Add;
use std::rc::Rc;

use compose_runtime::{
    ComposeArg, LaunchedEffect, State, composable, mutable_state_of, remember, remember_keyed,
};
use compose_ui::{
    Alignment, Color, ColumnScope, Constraints, DrawModifierNode, DrawScope, HorizontalAlignment,
    IntOffset, IntSize, LayoutDirection, LayoutModifierNode, Measurable, MeasurePolicy,
    MeasureResult, MeasureScope, Modifier, ModifierNode, ModifierNodeElement, Paint,
    RECTANGLE_SHAPE, ReleaseContext, RowScope, TransformOrigin, VerticalAlignment,
};

use crate::deferred::{MutableTransform, SharedMutableTransformState};
use crate::spec::{AnimationSpec, Spring, spring, spring_with};
use crate::transition::{
    DeferredAnimation, DeferredTransition, MutableTransitionState, Segment, Transition,
    remember_transition, update_transition,
};
use crate::vector::{Animate, AnimationVector, VisibilityThreshold};

// ---- The transitions ------------------------------------------------------------------------

/// A fade's alpha at the hidden end and its spec, as Compose's `FadeConfig`.
#[derive(Clone, PartialEq, Debug)]
pub struct FadeConfig {
    pub alpha: f32,
    pub animation_spec: AnimationSpec,
}

/// A slide's offset at the hidden end, a function of the content's full size, and its spec, as
/// Compose's `SlideConfig`.
#[derive(Clone)]
pub struct SlideConfig {
    pub slide_offset: Rc<dyn Fn(IntSize) -> IntOffset>,
    pub animation_spec: AnimationSpec,
}

/// An expansion's or shrink's size at the hidden end, a function of the full size; the side
/// that stays; whether the content is clipped to the animated size; and its spec, as Compose's
/// `ChangeSizeConfig`.
#[derive(Clone)]
pub struct ChangeSizeConfig {
    pub alignment: Alignment,
    pub size: Rc<dyn Fn(IntSize) -> IntSize>,
    pub animation_spec: AnimationSpec,
    pub clip: bool,
}

/// A scale at the hidden end, around `transform_origin`, and its spec, as Compose's
/// `ScaleConfig`.
#[derive(Clone, PartialEq, Debug)]
pub struct ScaleConfig {
    pub scale: f32,
    pub transform_origin: TransformOrigin,
    pub animation_spec: AnimationSpec,
}

/// A scrim's color over the content at the hidden end (`initial_color` entering,
/// `target_color` exiting), the other end its transparent copy, along its spec, as Compose's
/// `VeilConfig`. Over the parent's bounds when `match_parent_size`, as Compose's; compose-rs
/// draws it over the content's bounds then (a draw node can't see its parent's), affected by
/// the other parts' transforms either way.
#[derive(Clone, PartialEq, Debug)]
pub struct VeilConfig {
    pub initial_color: Color,
    pub target_color: Color,
    pub animation_spec: AnimationSpec,
    pub match_parent_size: bool,
}

/// The same function (Compose compares lambdas by identity).
impl PartialEq for SlideConfig {
    fn eq(&self, other: &SlideConfig) -> bool {
        Rc::ptr_eq(&self.slide_offset, &other.slide_offset)
            && self.animation_spec == other.animation_spec
    }
}

impl PartialEq for ChangeSizeConfig {
    fn eq(&self, other: &ChangeSizeConfig) -> bool {
        self.alignment == other.alignment
            && Rc::ptr_eq(&self.size, &other.size)
            && self.animation_spec == other.animation_spec
            && self.clip == other.clip
    }
}

impl std::fmt::Debug for SlideConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "SlideConfig({:?})", self.animation_spec)
    }
}

impl std::fmt::Debug for ChangeSizeConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "ChangeSizeConfig({:?}, clip {}, {:?})",
            self.alignment, self.clip, self.animation_spec
        )
    }
}

/// The parts of an enter or exit transition, as Compose's `EnterExitTransitionConfig`: each
/// animates on its own spec.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct EnterExitTransitionConfig {
    pub fade: Option<FadeConfig>,
    pub slide: Option<SlideConfig>,
    pub change_size: Option<ChangeSizeConfig>,
    pub scale: Option<ScaleConfig>,
    pub veil: Option<VeilConfig>,
    /// The content stays until every transition of its `AnimatedContent` ends, as `hold`
    /// (`ExitTransition.KeepUntilTransitionsFinished`).
    pub hold: bool,
}

impl EnterExitTransitionConfig {
    const NONE: EnterExitTransitionConfig = EnterExitTransitionConfig {
        fade: None,
        slide: None,
        change_size: None,
        scale: None,
        veil: None,
        hold: false,
    };

    /// `other`'s parts over this one's, as `plus`.
    fn plus(&self, other: &EnterExitTransitionConfig) -> EnterExitTransitionConfig {
        EnterExitTransitionConfig {
            fade: other.fade.clone().or_else(|| self.fade.clone()),
            slide: other.slide.clone().or_else(|| self.slide.clone()),
            change_size: other.change_size.clone().or_else(|| self.change_size.clone()),
            scale: other.scale.clone().or_else(|| self.scale.clone()),
            veil: other.veil.clone().or_else(|| self.veil.clone()),
            hold: self.hold || other.hold,
        }
    }
}

/// How content appears, as Compose's `EnterTransition`: `fade_in() + expand_vertically()`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct EnterTransition {
    config: EnterExitTransitionConfig,
}

/// How content leaves, as Compose's `ExitTransition`.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct ExitTransition {
    config: EnterExitTransitionConfig,
}

impl EnterTransition {
    /// No animation, as `EnterTransition.None`.
    pub const NONE: EnterTransition = EnterTransition { config: EnterExitTransitionConfig::NONE };

    /// Its parts, as `config`.
    pub fn config(&self) -> &EnterExitTransitionConfig {
        &self.config
    }
}

impl ExitTransition {
    /// No animation, as `ExitTransition.None`.
    pub const NONE: ExitTransition = ExitTransition { config: EnterExitTransitionConfig::NONE };

    /// Keeps the content shown, unchanged, until every transition of its `AnimatedContent` ends,
    /// as `ExitTransition.KeepUntilTransitionsFinished`: `+` another exit to animate it too.
    pub const KEEP_UNTIL_TRANSITIONS_FINISHED: ExitTransition = ExitTransition {
        config: EnterExitTransitionConfig { hold: true, ..EnterExitTransitionConfig::NONE },
    };

    /// Its parts, as `config`.
    pub fn config(&self) -> &EnterExitTransitionConfig {
        &self.config
    }
}

/// Both transitions' parts, the right one's over the left one's where both have one, as
/// Compose's `plus`.
impl Add for EnterTransition {
    type Output = EnterTransition;
    fn add(self, other: EnterTransition) -> EnterTransition {
        if self == EnterTransition::NONE {
            return other;
        }
        if other == EnterTransition::NONE {
            return self;
        }
        EnterTransition { config: self.config.plus(&other.config) }
    }
}

impl Add for ExitTransition {
    type Output = ExitTransition;
    fn add(self, other: ExitTransition) -> ExitTransition {
        if self == ExitTransition::NONE {
            return other;
        }
        if other == ExitTransition::NONE {
            return self;
        }
        ExitTransition { config: self.config.plus(&other.config) }
    }
}

impl ComposeArg for EnterTransition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ComposeArg for ExitTransition {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

fn enter(config: EnterExitTransitionConfig) -> EnterTransition {
    EnterTransition { config }
}

fn exit(config: EnterExitTransitionConfig) -> ExitTransition {
    ExitTransition { config }
}

/// Compose's default spring of the alpha and of an enter's or exit's parts: stiffness 400.
fn default_spring() -> crate::spec::SpringSpec {
    spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
}

/// The default spec of a fade, as `fadeIn()`'s.
pub(crate) fn default_alpha_spec() -> AnimationSpec {
    AnimationSpec::Spring(default_spring())
}

/// The default spec of a scale, as Compose's `DefaultScaleSpring` (threshold 0.002).
#[doc(hidden)]
pub fn default_scale_spec() -> AnimationSpec {
    AnimationSpec::Spring(default_spring().visibility_threshold(0.002f32))
}

/// The default spec of a slide, a pixel's threshold.
pub(crate) fn default_offset_spec() -> AnimationSpec {
    AnimationSpec::Spring(default_spring().visibility_threshold(IntOffset::VISIBILITY_THRESHOLD))
}

/// The default spec of an expansion or shrink, a pixel's threshold.
fn default_size_spec() -> AnimationSpec {
    AnimationSpec::Spring(default_spring().visibility_threshold(IntSize::VISIBILITY_THRESHOLD))
}

/// The default spec and color of a veil, as `unveilIn()`'s and `veilOut()`'s.
pub(crate) fn default_veil_spec() -> AnimationSpec {
    AnimationSpec::Spring(default_spring())
}

fn default_veil_color() -> Color {
    Color::BLACK.with_alpha(0.5)
}

/// A scrim over the content as it enters, from half-transparent black to transparent, as
/// `unveilIn()`.
pub fn unveil_in() -> EnterTransition {
    unveil_in_with(default_veil_spec(), default_veil_color(), false)
}

/// A scrim from `initial_color` to its transparent copy along `animation_spec`, over the
/// parent's bounds when `match_parent_size` (see [`VeilConfig`]), as `unveilIn(animationSpec,
/// initialColor, matchParentSize)`.
pub fn unveil_in_with(
    animation_spec: impl Into<AnimationSpec>,
    initial_color: Color,
    match_parent_size: bool,
) -> EnterTransition {
    let veil = VeilConfig {
        initial_color,
        target_color: initial_color.with_alpha(0.0),
        animation_spec: animation_spec.into(),
        match_parent_size,
    };
    enter(EnterExitTransitionConfig { veil: Some(veil), ..EnterExitTransitionConfig::NONE })
}

/// A scrim over the content as it exits, from transparent to half-transparent black, as
/// `veilOut()`.
pub fn veil_out() -> ExitTransition {
    veil_out_with(default_veil_spec(), default_veil_color(), false)
}

/// A scrim from `target_color`'s transparent copy to it along `animation_spec`, as
/// `veilOut(animationSpec, targetColor, matchParentSize)`.
pub fn veil_out_with(
    animation_spec: impl Into<AnimationSpec>,
    target_color: Color,
    match_parent_size: bool,
) -> ExitTransition {
    let veil = VeilConfig {
        initial_color: target_color.with_alpha(0.0),
        target_color,
        animation_spec: animation_spec.into(),
        match_parent_size,
    };
    exit(EnterExitTransitionConfig { veil: Some(veil), ..EnterExitTransitionConfig::NONE })
}

/// Fades in from transparent, as `fadeIn()`.
pub fn fade_in() -> EnterTransition {
    fade_in_with(default_alpha_spec(), 0.0)
}

/// Fades in from `initial_alpha` along `animation_spec`, as `fadeIn(animationSpec,
/// initialAlpha)`.
pub fn fade_in_with(
    animation_spec: impl Into<AnimationSpec>,
    initial_alpha: f32,
) -> EnterTransition {
    let fade = FadeConfig { alpha: initial_alpha, animation_spec: animation_spec.into() };
    enter(EnterExitTransitionConfig { fade: Some(fade), ..EnterExitTransitionConfig::NONE })
}

/// Fades out to transparent, as `fadeOut()`.
pub fn fade_out() -> ExitTransition {
    fade_out_with(default_alpha_spec(), 0.0)
}

/// Fades out to `target_alpha` along `animation_spec`, as `fadeOut(animationSpec,
/// targetAlpha)`.
pub fn fade_out_with(
    animation_spec: impl Into<AnimationSpec>,
    target_alpha: f32,
) -> ExitTransition {
    let fade = FadeConfig { alpha: target_alpha, animation_spec: animation_spec.into() };
    exit(EnterExitTransitionConfig { fade: Some(fade), ..EnterExitTransitionConfig::NONE })
}

/// Slides in from `initial_offset(full size)`, as `slideIn(initialOffset = …)`.
pub fn slide_in(initial_offset: impl Fn(IntSize) -> IntOffset + 'static) -> EnterTransition {
    slide_in_with(default_offset_spec(), initial_offset)
}

/// The same along `animation_spec`, as `slideIn(animationSpec, initialOffset)`.
pub fn slide_in_with(
    animation_spec: impl Into<AnimationSpec>,
    initial_offset: impl Fn(IntSize) -> IntOffset + 'static,
) -> EnterTransition {
    let slide = SlideConfig {
        slide_offset: Rc::new(initial_offset),
        animation_spec: animation_spec.into(),
    };
    enter(EnterExitTransitionConfig { slide: Some(slide), ..EnterExitTransitionConfig::NONE })
}

/// Slides out to `target_offset(full size)`, as `slideOut(targetOffset = …)`.
pub fn slide_out(target_offset: impl Fn(IntSize) -> IntOffset + 'static) -> ExitTransition {
    slide_out_with(default_offset_spec(), target_offset)
}

/// The same along `animation_spec`, as `slideOut(animationSpec, targetOffset)`.
pub fn slide_out_with(
    animation_spec: impl Into<AnimationSpec>,
    target_offset: impl Fn(IntSize) -> IntOffset + 'static,
) -> ExitTransition {
    let slide =
        SlideConfig { slide_offset: Rc::new(target_offset), animation_spec: animation_spec.into() };
    exit(EnterExitTransitionConfig { slide: Some(slide), ..EnterExitTransitionConfig::NONE })
}

/// Half a size, negative, as Compose's default `{ -it / 2 }` (Kotlin's division truncates).
fn minus_half(length: i32) -> i32 {
    -length / 2
}

/// Slides in from half its width to the start, as `slideInHorizontally()`.
pub fn slide_in_horizontally() -> EnterTransition {
    slide_in_horizontally_with(default_offset_spec(), minus_half)
}

/// Slides in horizontally from `initial_offset_x(full width)`, as
/// `slideInHorizontally(animationSpec, initialOffsetX)`.
pub fn slide_in_horizontally_with(
    animation_spec: impl Into<AnimationSpec>,
    initial_offset_x: impl Fn(i32) -> i32 + 'static,
) -> EnterTransition {
    slide_in_with(animation_spec, move |size| IntOffset::new(initial_offset_x(size.width), 0))
}

/// Slides in from half its height above, as `slideInVertically()`.
pub fn slide_in_vertically() -> EnterTransition {
    slide_in_vertically_with(default_offset_spec(), minus_half)
}

/// Slides in vertically from `initial_offset_y(full height)`, as
/// `slideInVertically(animationSpec, initialOffsetY)`.
pub fn slide_in_vertically_with(
    animation_spec: impl Into<AnimationSpec>,
    initial_offset_y: impl Fn(i32) -> i32 + 'static,
) -> EnterTransition {
    slide_in_with(animation_spec, move |size| IntOffset::new(0, initial_offset_y(size.height)))
}

/// Slides out to half its width to the start, as `slideOutHorizontally()`.
pub fn slide_out_horizontally() -> ExitTransition {
    slide_out_horizontally_with(default_offset_spec(), minus_half)
}

/// Slides out horizontally to `target_offset_x(full width)`, as
/// `slideOutHorizontally(animationSpec, targetOffsetX)`.
pub fn slide_out_horizontally_with(
    animation_spec: impl Into<AnimationSpec>,
    target_offset_x: impl Fn(i32) -> i32 + 'static,
) -> ExitTransition {
    slide_out_with(animation_spec, move |size| IntOffset::new(target_offset_x(size.width), 0))
}

/// Slides out to half its height above, as `slideOutVertically()`.
pub fn slide_out_vertically() -> ExitTransition {
    slide_out_vertically_with(default_offset_spec(), minus_half)
}

/// Slides out vertically to `target_offset_y(full height)`, as
/// `slideOutVertically(animationSpec, targetOffsetY)`.
pub fn slide_out_vertically_with(
    animation_spec: impl Into<AnimationSpec>,
    target_offset_y: impl Fn(i32) -> i32 + 'static,
) -> ExitTransition {
    slide_out_with(animation_spec, move |size| IntOffset::new(0, target_offset_y(size.height)))
}

/// Scales in from nothing, around the center, as `scaleIn()`.
pub fn scale_in() -> EnterTransition {
    scale_in_with(default_scale_spec(), 0.0, TransformOrigin::CENTER)
}

/// Scales in from `initial_scale` around `transform_origin` along `animation_spec`, as
/// `scaleIn(animationSpec, initialScale, transformOrigin)`.
pub fn scale_in_with(
    animation_spec: impl Into<AnimationSpec>,
    initial_scale: f32,
    transform_origin: TransformOrigin,
) -> EnterTransition {
    let scale = ScaleConfig {
        scale: initial_scale,
        transform_origin,
        animation_spec: animation_spec.into(),
    };
    enter(EnterExitTransitionConfig { scale: Some(scale), ..EnterExitTransitionConfig::NONE })
}

/// Scales out to nothing, around the center, as `scaleOut()`.
pub fn scale_out() -> ExitTransition {
    scale_out_with(default_scale_spec(), 0.0, TransformOrigin::CENTER)
}

/// Scales out to `target_scale` around `transform_origin` along `animation_spec`, as
/// `scaleOut(animationSpec, targetScale, transformOrigin)`.
pub fn scale_out_with(
    animation_spec: impl Into<AnimationSpec>,
    target_scale: f32,
    transform_origin: TransformOrigin,
) -> ExitTransition {
    let scale = ScaleConfig {
        scale: target_scale,
        transform_origin,
        animation_spec: animation_spec.into(),
    };
    exit(EnterExitTransitionConfig { scale: Some(scale), ..EnterExitTransitionConfig::NONE })
}

/// Grows from nothing, revealing from the bottom end, as `expandIn()`.
pub fn expand_in() -> EnterTransition {
    expand_in_with(default_size_spec(), Alignment::BOTTOM_END, true, |_| IntSize::ZERO)
}

/// Grows from `initial_size(full size)` along `animation_spec`, the `expand_from` side staying,
/// clipped when `clip`, as `expandIn(animationSpec, expandFrom, clip, initialSize)`.
pub fn expand_in_with(
    animation_spec: impl Into<AnimationSpec>,
    expand_from: Alignment,
    clip: bool,
    initial_size: impl Fn(IntSize) -> IntSize + 'static,
) -> EnterTransition {
    let change_size = ChangeSizeConfig {
        alignment: expand_from,
        size: Rc::new(initial_size),
        animation_spec: animation_spec.into(),
        clip,
    };
    enter(EnterExitTransitionConfig {
        change_size: Some(change_size),
        ..EnterExitTransitionConfig::NONE
    })
}

/// Shrinks to nothing, towards the bottom end, as `shrinkOut()`.
pub fn shrink_out() -> ExitTransition {
    shrink_out_with(default_size_spec(), Alignment::BOTTOM_END, true, |_| IntSize::ZERO)
}

/// Shrinks to `target_size(full size)` along `animation_spec`, towards `shrink_towards`,
/// clipped when `clip`, as `shrinkOut(animationSpec, shrinkTowards, clip, targetSize)`.
pub fn shrink_out_with(
    animation_spec: impl Into<AnimationSpec>,
    shrink_towards: Alignment,
    clip: bool,
    target_size: impl Fn(IntSize) -> IntSize + 'static,
) -> ExitTransition {
    let change_size = ChangeSizeConfig {
        alignment: shrink_towards,
        size: Rc::new(target_size),
        animation_spec: animation_spec.into(),
        clip,
    };
    exit(EnterExitTransitionConfig {
        change_size: Some(change_size),
        ..EnterExitTransitionConfig::NONE
    })
}

/// A horizontal alignment as an alignment, as Compose's `toAlignment`.
fn horizontal(alignment: HorizontalAlignment) -> Alignment {
    if alignment == Alignment::START {
        Alignment::CENTER_START
    } else if alignment == Alignment::END {
        Alignment::CENTER_END
    } else {
        Alignment::CENTER
    }
}

/// A vertical alignment as an alignment, as Compose's `toAlignment`.
fn vertical(alignment: VerticalAlignment) -> Alignment {
    if alignment == Alignment::TOP {
        Alignment::TOP_CENTER
    } else if alignment == Alignment::BOTTOM {
        Alignment::BOTTOM_CENTER
    } else {
        Alignment::CENTER
    }
}

/// Grows from no width, revealing from the end, as `expandHorizontally()`.
pub fn expand_horizontally() -> EnterTransition {
    expand_horizontally_with(default_size_spec(), Alignment::END, true, |_| 0)
}

/// Grows from `initial_width(full width)`, as `expandHorizontally(animationSpec, expandFrom,
/// clip, initialWidth)`.
pub fn expand_horizontally_with(
    animation_spec: impl Into<AnimationSpec>,
    expand_from: HorizontalAlignment,
    clip: bool,
    initial_width: impl Fn(i32) -> i32 + 'static,
) -> EnterTransition {
    expand_in_with(animation_spec, horizontal(expand_from), clip, move |size| {
        IntSize::new(initial_width(size.width), size.height)
    })
}

/// Grows from no height, revealing from the bottom, as `expandVertically()`.
pub fn expand_vertically() -> EnterTransition {
    expand_vertically_with(default_size_spec(), Alignment::BOTTOM, true, |_| 0)
}

/// Grows from `initial_height(full height)`, as `expandVertically(animationSpec, expandFrom,
/// clip, initialHeight)`.
pub fn expand_vertically_with(
    animation_spec: impl Into<AnimationSpec>,
    expand_from: VerticalAlignment,
    clip: bool,
    initial_height: impl Fn(i32) -> i32 + 'static,
) -> EnterTransition {
    expand_in_with(animation_spec, vertical(expand_from), clip, move |size| {
        IntSize::new(size.width, initial_height(size.height))
    })
}

/// Shrinks to no width, towards the end, as `shrinkHorizontally()`.
pub fn shrink_horizontally() -> ExitTransition {
    shrink_horizontally_with(default_size_spec(), Alignment::END, true, |_| 0)
}

/// Shrinks to `target_width(full width)`, as `shrinkHorizontally(animationSpec, shrinkTowards,
/// clip, targetWidth)`.
pub fn shrink_horizontally_with(
    animation_spec: impl Into<AnimationSpec>,
    shrink_towards: HorizontalAlignment,
    clip: bool,
    target_width: impl Fn(i32) -> i32 + 'static,
) -> ExitTransition {
    shrink_out_with(animation_spec, horizontal(shrink_towards), clip, move |size| {
        IntSize::new(target_width(size.width), size.height)
    })
}

/// Shrinks to no height, towards the bottom, as `shrinkVertically()`.
pub fn shrink_vertically() -> ExitTransition {
    shrink_vertically_with(default_size_spec(), Alignment::BOTTOM, true, |_| 0)
}

/// Shrinks to `target_height(full height)`, as `shrinkVertically(animationSpec,
/// shrinkTowards, clip, targetHeight)`.
pub fn shrink_vertically_with(
    animation_spec: impl Into<AnimationSpec>,
    shrink_towards: VerticalAlignment,
    clip: bool,
    target_height: impl Fn(i32) -> i32 + 'static,
) -> ExitTransition {
    shrink_out_with(animation_spec, vertical(shrink_towards), clip, move |size| {
        IntSize::new(size.width, target_height(size.height))
    })
}

// ---- The states and the scope ---------------------------------------------------------------

/// Where content is in its enter or exit, as Compose's `EnterExitState`: an enter goes from
/// `PreEnter` to `Visible`, an exit from `Visible` to `PostExit`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EnterExitState {
    PreEnter,
    Visible,
    PostExit,
}

impl ComposeArg for EnterExitState {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// What `AnimatedVisibility`'s content is given, as Compose's `AnimatedVisibilityScope`: the
/// transition of its enter and exit, to animate more values with.
#[derive(Clone)]
pub struct AnimatedVisibilityScope {
    transition: Transition<EnterExitState>,
    /// The content's size, as measured, for `AnimatedContent`'s size animation.
    pub(crate) target_size: State<IntSize>,
}

impl AnimatedVisibilityScope {
    /// The transition of the content's enter and exit, as `transition`.
    pub fn transition(&self) -> &Transition<EnterExitState> {
        &self.transition
    }

    /// A modifier animating a child of the content in and out with the content, along `enter`
    /// and `exit` (`None`: Compose's `fadeIn()` and `fadeOut()`), on the content's transition,
    /// as `Modifier.animateEnterExit(enter, exit, label)`: the content stays until these end
    /// too. Call it on every pass of the content, as a composable (it remembers its
    /// animations); its value goes in the child's modifier.
    pub fn animate_enter_exit(
        &self,
        enter: impl Into<Option<EnterTransition>>,
        exit: impl Into<Option<ExitTransition>>,
        label: &'static str,
    ) -> Modifier {
        let enter = enter.into().unwrap_or_else(fade_in);
        let exit = exit.into().unwrap_or_else(fade_out);
        let (active_enter, active_exit, _) =
            update_enter_exit_modifier_state(&self.transition, enter, exit, None);
        create_enter_exit_modifier(&self.transition, active_enter, active_exit, None, label)
    }
}

impl ComposeArg for AnimatedVisibilityScope {
    fn same(&self, other: &Self) -> bool {
        self.transition.same(&other.transition)
    }
}

/// What `AnimatedVisibility` is told to show by, and where it is: the overloads of Compose's
/// `AnimatedVisibility`, whose default transitions differ.
///
/// - `bool`: `AnimatedVisibility(visible)`, `fade_in() + expand_in()` and `shrink_out() +
///   fade_out()` by default;
/// - `MutableTransitionState<bool>`: `AnimatedVisibility(visibleState)`, `fade_in() +
///   expand_in()` and `fade_out() + shrink_out()`;
/// - `(ColumnScope, bool)` and `(ColumnScope, MutableTransitionState<bool>)`: in a `Column`,
///   expanding and shrinking vertically (`fade_in() + expand_vertically()`, `fade_out() +
///   shrink_vertically()`);
/// - `(RowScope, bool)` and `(RowScope, MutableTransitionState<bool>)`: in a `Row`,
///   horizontally.
pub trait AnimatedVisibilityState: ComposeArg + Clone + 'static {
    /// The transition of the visibility, remembered at the call site.
    fn transition(&self, label: &'static str) -> Transition<bool>;
    /// The enter used when none is given.
    fn default_enter(&self) -> EnterTransition;
    /// The exit used when none is given.
    fn default_exit(&self) -> ExitTransition;
}

impl AnimatedVisibilityState for bool {
    fn transition(&self, label: &'static str) -> Transition<bool> {
        update_transition(*self, label)
    }
    fn default_enter(&self) -> EnterTransition {
        fade_in() + expand_in()
    }
    fn default_exit(&self) -> ExitTransition {
        shrink_out() + fade_out()
    }
}

impl AnimatedVisibilityState for MutableTransitionState<bool> {
    fn transition(&self, label: &'static str) -> Transition<bool> {
        remember_transition(self, label)
    }
    fn default_enter(&self) -> EnterTransition {
        fade_in() + expand_in()
    }
    fn default_exit(&self) -> ExitTransition {
        fade_out() + shrink_out()
    }
}

impl<V: AnimatedVisibilityState> AnimatedVisibilityState for (ColumnScope, V) {
    fn transition(&self, label: &'static str) -> Transition<bool> {
        self.1.transition(label)
    }
    fn default_enter(&self) -> EnterTransition {
        fade_in() + expand_vertically()
    }
    fn default_exit(&self) -> ExitTransition {
        fade_out() + shrink_vertically()
    }
}

impl<V: AnimatedVisibilityState> AnimatedVisibilityState for (RowScope, V) {
    fn transition(&self, label: &'static str) -> Transition<bool> {
        self.1.transition(label)
    }
    fn default_enter(&self) -> EnterTransition {
        fade_in() + expand_horizontally()
    }
    fn default_exit(&self) -> ExitTransition {
        fade_out() + shrink_horizontally()
    }
}

/// Shows `content` while visible, animating it in with `enter` and out with `exit`, as
/// Compose's `AnimatedVisibility`. `visible` is a `bool`, a `MutableTransitionState<bool>`, or
/// either with the `ColumnScope` or `RowScope` it's in, whose defaults expand and shrink along
/// the column or the row (see [`AnimatedVisibilityState`]); `enter` and `exit` default to the
/// overload's. While leaving, the content stays composed; it is removed when the exit ends. The
/// animations write state read while drawing and measuring: their frames redraw and re-measure
/// this node, without recomposing.
#[composable]
#[allow(non_snake_case)]
pub fn AnimatedVisibility<V: AnimatedVisibilityState>(
    visible: V,
    #[default(crate::Modifier)] modifier: Modifier,
    #[default(None)] enter: impl Into<Option<EnterTransition>>,
    #[default(None)] exit: impl Into<Option<ExitTransition>>,
    #[default("AnimatedVisibility")] label: &'static str,
    content: impl Fn(AnimatedVisibilityScope) + 'static,
) {
    let enter: Option<EnterTransition> = enter;
    let enter = enter.unwrap_or_else(|| visible.default_enter());
    let exit: Option<ExitTransition> = exit;
    let exit = exit.unwrap_or_else(|| visible.default_exit());
    let transition = visible.transition(label);
    AnimatedEnterExit(
        transition,
        Rc::new(|visible: &bool| *visible),
        modifier,
        enter,
        exit,
        Rc::new(|current, target| {
            current == EnterExitState::PostExit && target == EnterExitState::PostExit
        }),
        None,
        false,
        compose_runtime::callback(content),
    );
}

impl<S: Clone + PartialEq + 'static> Transition<S> {
    /// Shows `content` while `visible` holds for this transition's target or current state,
    /// animating it in and out on the transition's clock, as Compose's
    /// `Transition.AnimatedVisibility(visible, modifier, enter, exit, content)`. `enter` and
    /// `exit` default (`None`) to Compose's `fadeIn() + expandIn()` and `shrinkOut() +
    /// fadeOut()`. A method, so every parameter is given.
    #[allow(non_snake_case)]
    pub fn AnimatedVisibility(
        &self,
        visible: impl Fn(&S) -> bool + 'static,
        modifier: Modifier,
        enter: impl Into<Option<EnterTransition>>,
        exit: impl Into<Option<ExitTransition>>,
        content: impl Fn(AnimatedVisibilityScope) + 'static,
    ) {
        let enter = enter.into().unwrap_or_else(|| fade_in() + expand_in());
        let exit = exit.into().unwrap_or_else(|| shrink_out() + fade_out());
        AnimatedEnterExit(
            self.clone(),
            Rc::new(visible),
            modifier,
            enter,
            exit,
            Rc::new(|current, target| {
                current == EnterExitState::PostExit && target == EnterExitState::PostExit
            }),
            None,
            false,
            compose_runtime::callback(content),
        );
    }
}

impl<S: Clone + PartialEq + 'static> DeferredTransition<S> {
    /// `AnimatedVisibility` of this transition's states with a deferred phase, as Compose's
    /// `DeferredTransition.DeferredAnimatedVisibility(visible, modifier, enter, exit,
    /// mutableTransform, content)`: while a pending target waits, the content is composed (at its
    /// `enter`'s start) or stays, moved by `mutable_transform` (`None`: not moved). When the
    /// phase ends, its enter or exit takes up from where the transform left it. `enter` and `exit`
    /// default (`None`) to Compose's `fadeIn() + expandIn()` and `shrinkOut() + fadeOut()`. A
    /// method, so every parameter is given.
    #[allow(non_snake_case)]
    pub fn DeferredAnimatedVisibility(
        &self,
        visible: impl Fn(&S) -> bool + 'static,
        modifier: Modifier,
        enter: impl Into<Option<EnterTransition>>,
        exit: impl Into<Option<ExitTransition>>,
        mutable_transform: impl Into<Option<MutableTransform>>,
        content: impl Fn(AnimatedVisibilityScope) + 'static,
    ) {
        let enter = enter.into().unwrap_or_else(|| fade_in() + expand_in());
        let exit = exit.into().unwrap_or_else(|| shrink_out() + fade_out());
        AnimatedEnterExit(
            self.transition.clone(),
            Rc::new(visible),
            modifier,
            enter,
            exit,
            Rc::new(|current, target| {
                current == EnterExitState::PostExit && target == EnterExitState::PostExit
            }),
            mutable_transform.into(),
            false,
            compose_runtime::callback(content),
        );
    }
}

/// Whether the content shows for a parent state.
pub(crate) type VisibleFn<T> = Rc<dyn Fn(&T) -> bool>;
/// Whether the content goes once its exit ends, from its transition's current and target.
pub(crate) type ShouldDispose = Rc<dyn Fn(EnterExitState, EnterExitState) -> bool>;

/// The child transition's state for a parent state, as Compose's `targetEnterExit`: `Visible`
/// when visible, else `PreEnter` while a pending target shows it (a gesture coming back to it),
/// else `PostExit` once it has been visible, `PreEnter` before. Calls `remember`.
pub(crate) fn target_enter_exit<T: Clone + PartialEq + 'static>(
    transition: &Transition<T>,
    visible: &VisibleFn<T>,
    target_state: &T,
) -> EnterExitState {
    let has_been_visible = remember_keyed(transition.clone(), || Rc::new(Cell::new(false)));
    let pending_visible =
        transition.pending_target_state().is_some_and(|pending| visible(&pending));
    if visible(&transition.current_state()) || pending_visible {
        has_been_visible.set(true);
    }
    if visible(target_state) {
        EnterExitState::Visible
    } else if pending_visible {
        // A gesture on its way back to showing it: it goes to where its enter starts, and stays.
        EnterExitState::PreEnter
    } else if has_been_visible.get() {
        EnterExitState::PostExit
    } else {
        EnterExitState::PreEnter
    }
}

/// The content of a parent transition's state, entering and exiting as it's shown and hidden,
/// as Compose's `AnimatedEnterExitImpl`.
#[composable(non_restartable)]
#[allow(non_snake_case)]
pub(crate) fn AnimatedEnterExit<T: Clone + PartialEq + 'static>(
    transition: Transition<T>,
    visible: VisibleFn<T>,
    modifier: Modifier,
    enter: EnterTransition,
    exit: ExitTransition,
    should_dispose: ShouldDispose,
    mutable_transform: Option<MutableTransform>,
    force_visible: bool,
    content: compose_runtime::Callback<dyn Fn(AnimatedVisibilityScope)>,
) {
    // As Compose's: also while values go on from an interrupted seek (the content seeked from
    // stays while they do), and for a pending target (composed early, to be prepared).
    if force_visible
        || visible(&transition.target_state())
        || visible(&transition.current_state())
        || transition.pending_target_state().is_some_and(|pending| visible(&pending))
        || transition.has_initial_value_animations()
    {
        let child = {
            let (parent, visible) = (transition.clone(), visible.clone());
            transition.create_child_transition("EnterExitTransition", move |state| {
                target_enter_exit(&parent, &visible, state)
            })
        };
        // Above the layout, so it lives through the layout's going when an exit ends: a later
        // interruption finds the exit it was on (Compose's hoisted tracking).
        let (active_enter, active_exit, transform_state) =
            update_enter_exit_modifier_state(&child, enter, exit, mutable_transform);
        let current = child.current_state();
        let target = child.target_state();
        let exit_finished =
            current == EnterExitState::PostExit && target == EnterExitState::PostExit;
        if !exit_finished || !should_dispose(current, target) {
            let target_size = remember(|| mutable_state_of(IntSize::ZERO));
            let scope = AnimatedVisibilityScope {
                transition: child.clone(),
                target_size: target_size.as_state(),
            };
            let enter_exit = create_enter_exit_modifier(
                &child,
                active_enter,
                active_exit,
                transform_state,
                "Built-in",
            );
            let policy = remember(move || {
                Rc::new(EnterExitMeasurePolicy { target_size }) as Rc<dyn MeasurePolicy>
            });
            compose_ui::Layout(
                modifier.then(enter_exit),
                policy,
                compose_runtime::callback(move || content(scope.clone())),
            );
        }
    }
}

/// The content's children, stacked at the top start, as large as the largest: their size is the
/// target size.
struct EnterExitMeasurePolicy {
    target_size: compose_runtime::MutableState<IntSize>,
}

impl MeasurePolicy for EnterExitMeasurePolicy {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        let placeables: Vec<_> =
            measurables.iter().map(|&measurable| scope.measure(measurable, constraints)).collect();
        let size = placeables.iter().fold(IntSize::ZERO, |size, placeable| {
            IntSize::new(size.width.max(placeable.width), size.height.max(placeable.height))
        });
        self.target_size.set(size);
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(size.width, size.height)
    }

    fn debug_name(&self) -> &'static str {
        "AnimatedEnterExit"
    }
}

/// The enter and exit animating now, as Compose's `EnterExitTransitionTracker`: once visible
/// and at rest, none; an enter adds to the enter animating (its parts over the old ones); an
/// exit adds to the exit animating, whose other parts go back to their visible values. Of a
/// deferred transition's content (a `transform_state`), also the phase of the manual changes and
/// what keeps them at their hand-off.
struct Tracker {
    enter: RefCell<Option<EnterTransition>>,
    exit: RefCell<Option<ExitTransition>>,
    transform_state: Option<Rc<SharedMutableTransformState>>,
    was_mutating: Cell<bool>,
    last_target: Cell<Option<EnterExitState>>,
    start_catch_up: Cell<bool>,
}

impl Tracker {
    /// As `trackActiveMutableState`.
    fn track_mutable_state(
        &self,
        state: &SharedMutableTransformState,
        mutable_transform: Option<MutableTransform>,
        pending: Option<EnterExitState>,
        current: EnterExitState,
        target: EnterExitState,
    ) {
        state.set_mutable_data(mutable_transform);
        let is_mutating = pending.is_some() && state.has_mutable_data();
        let is_settled = current == target;
        state.update_mutation_state(is_mutating, is_settled);
        let was_mutating = self.was_mutating.get();
        let should_clean_up = self.last_target.get().is_some_and(|last| {
            (is_settled && !is_mutating) || (last != target && !is_mutating && !was_mutating)
        });
        if should_clean_up {
            state.clear();
        }
        if !is_mutating {
            self.start_catch_up.set(false);
        } else if !was_mutating {
            self.start_catch_up.set(!is_settled);
        }
        self.last_target.set(Some(target));
        self.was_mutating.set(is_mutating);
    }
}

pub(crate) fn update_enter_exit_modifier_state(
    transition: &Transition<EnterExitState>,
    enter: EnterTransition,
    exit: ExitTransition,
    mutable_transform: Option<MutableTransform>,
) -> (EnterTransition, ExitTransition, Option<Rc<SharedMutableTransformState>>) {
    let deferred = transition.is_deferred();
    let tracker = remember_keyed(transition.clone(), || {
        Rc::new(Tracker {
            enter: RefCell::new(None),
            exit: RefCell::new(None),
            transform_state: deferred.then(|| Rc::new(SharedMutableTransformState::new())),
            was_mutating: Cell::new(false),
            last_target: Cell::new(None),
            start_catch_up: Cell::new(false),
        })
    });
    let current = transition.current_state();
    let target = transition.target_state();
    let transform_state = tracker.transform_state.clone();
    if let Some(state) = &transform_state {
        let pending = transition.pending_target_state();
        tracker.track_mutable_state(state, mutable_transform, pending, current, target);
    }
    let at_rest_visible = current == target && current == EnterExitState::Visible;
    let active_enter = {
        let mut active = tracker.enter.borrow_mut();
        let previous = active.take().unwrap_or_else(|| enter.clone());
        let next = if at_rest_visible {
            EnterTransition::NONE
        } else if target != EnterExitState::PostExit {
            // What the deferred phase moved stays tracked even if `enter` leaves it out, so an
            // interruption doesn't snap it; `enter`'s parts go over those.
            match &transform_state {
                Some(state) => previous + (state.handoff_enter() + enter),
                None => previous + enter,
            }
        } else {
            previous
        };
        *active = Some(next.clone());
        next
    };
    let active_exit = {
        let mut active = tracker.exit.borrow_mut();
        let previous = active.take().unwrap_or_else(|| exit.clone());
        let next = if at_rest_visible {
            ExitTransition::NONE
        } else if target != EnterExitState::Visible {
            // During a deferred phase, the manual values go on top of the running animations:
            // those aren't neutralized.
            let neutral = if transform_state.as_ref().is_some_and(|state| state.is_mutating()) {
                previous
            } else {
                neutralized(&previous)
            };
            match &transform_state {
                Some(state) => neutral + state.handoff_exit() + exit,
                None => neutral + exit,
            }
        } else {
            previous
        };
        *active = Some(next.clone());
        next
    };
    if let Some(state) = &transform_state {
        let (state, start) = (state.clone(), tracker.start_catch_up.get());
        LaunchedEffect(start, async move {
            if start {
                state.start_catch_up().await;
            }
        });
    }
    (active_enter, active_exit, transform_state)
}

/// An exit's parts at their visible values (alpha and scale 1, no offset, the full size), so a
/// part the next exit doesn't have animates back.
fn neutralized(exit: &ExitTransition) -> ExitTransition {
    thread_local! {
        static NEUTRAL_SLIDE: Rc<dyn Fn(IntSize) -> IntOffset> = Rc::new(|_| IntOffset::ZERO);
        static NEUTRAL_SIZE: Rc<dyn Fn(IntSize) -> IntSize> = Rc::new(|size| size);
    }
    let config = &exit.config;
    ExitTransition {
        config: EnterExitTransitionConfig {
            fade: config.fade.clone().map(|fade| FadeConfig { alpha: 1.0, ..fade }),
            scale: config.scale.clone().map(|scale| ScaleConfig { scale: 1.0, ..scale }),
            slide: config
                .slide
                .clone()
                .map(|slide| SlideConfig { slide_offset: NEUTRAL_SLIDE.with(Rc::clone), ..slide }),
            change_size: config
                .change_size
                .clone()
                .map(|change| ChangeSizeConfig { size: NEUTRAL_SIZE.with(Rc::clone), ..change }),
            veil: config
                .veil
                .clone()
                .map(|veil| VeilConfig { target_color: veil.initial_color, ..veil }),
            hold: config.hold,
        },
    }
}

/// The modifier animating the content's alpha, scale, size and slide, as Compose's
/// `createEnterExitModifier`: a clip while a size animates (unless asked not to), the
/// size and slide in a layout node, the alpha and scale in a graphics layer.
pub(crate) fn create_enter_exit_modifier(
    transition: &Transition<EnterExitState>,
    enter: EnterTransition,
    exit: ExitTransition,
    transform_state: Option<Rc<SharedMutableTransformState>>,
    label: &'static str,
) -> Modifier {
    let parts = EnterExit { enter: Rc::new(enter), exit: Rc::new(exit) };
    let manual = transform_state.as_deref();
    let animate_slide = parts.enter.config.slide.is_some()
        || parts.exit.config.slide.is_some()
        || manual.is_some_and(|state| state.slide_requires_animation());
    let animate_size =
        parts.enter.config.change_size.is_some() || parts.exit.config.change_size.is_some();
    let animate_alpha = parts.enter.config.fade.is_some()
        || parts.exit.config.fade.is_some()
        || manual.is_some_and(|state| state.alpha_requires_animation());
    let animate_scale = parts.enter.config.scale.is_some()
        || parts.exit.config.scale.is_some()
        || manual.is_some_and(|state| state.scale_requires_animation());
    let animate_veil = parts.enter.config.veil.is_some()
        || parts.exit.config.veil.is_some()
        || manual.is_some_and(|state| state.veil_requires_animation());
    let veil = deferred_if(transition, animate_veil, label, " veil").map(|animation| {
        Modifier.then_custom(VeilElement {
            animation,
            parts: parts.clone(),
            transform_state: transform_state.clone(),
        })
    });
    let veil_over_parent = parts
        .enter
        .config
        .veil
        .as_ref()
        .or(parts.exit.config.veil.as_ref())
        .map(|veil| veil.match_parent_size)
        .or_else(|| manual.and_then(|state| state.veil_match_parent_size()))
        .unwrap_or(false);
    let animations = Animations {
        slide: deferred_if(transition, animate_slide, label, " slide"),
        size: deferred_if(transition, animate_size, label, " shrink/expand"),
        offset: deferred_if(transition, animate_size, label, " InterruptionHandlingOffset"),
        alpha: deferred_if(transition, animate_alpha, label, " alpha"),
        scale: deferred_if(transition, animate_scale, label, " scale"),
        transform_origin: deferred_if(
            transition,
            animate_scale,
            "",
            "TransformOriginInterruptionHandling",
        ),
    };
    let no_clip = parts.enter.config.change_size.as_ref().is_some_and(|change| !change.clip)
        || parts.exit.config.change_size.as_ref().is_some_and(|change| !change.clip)
        || !animate_size;
    let layer = graphics_layer_block(&animations, &parts, transform_state.clone());
    let clip = if no_clip { Modifier } else { Modifier.clip(RECTANGLE_SHAPE) };
    // Over the parent: before the other parts (not transformed by them); else after them.
    let (before, after) = match veil {
        Some(veil) if veil_over_parent => (veil, Modifier),
        Some(veil) => (Modifier, veil),
        None => (Modifier, Modifier),
    };
    before
        .then(clip)
        .then_custom(EnterExitElement {
            transition: transition.clone(),
            parts,
            animations,
            transform_state,
        })
        .then(layer)
        .then(after)
}

/// The veil, as Compose's `VeilModifierElement`: a rect of the animated color over the content,
/// read while drawing.
struct VeilElement {
    animation: DeferredAnimation<EnterExitState, Color>,
    parts: EnterExit,
    transform_state: Option<Rc<SharedMutableTransformState>>,
}

struct VeilNode {
    animation: DeferredAnimation<EnterExitState, Color>,
    parts: EnterExit,
    transform_state: Option<Rc<SharedMutableTransformState>>,
}

impl ModifierNodeElement for VeilElement {
    type Node = VeilNode;
    fn create(&self) -> VeilNode {
        VeilNode {
            animation: self.animation.clone(),
            parts: self.parts.clone(),
            transform_state: self.transform_state.clone(),
        }
    }

    fn update(&self, node: &mut VeilNode) {
        node.animation = self.animation.clone();
        node.parts = self.parts.clone();
        node.transform_state = self.transform_state.clone();
    }

    fn same(&self, other: &Self) -> bool {
        other.animation.same(&self.animation)
            && *other.parts.enter == *self.parts.enter
            && *other.parts.exit == *self.parts.exit
            && same_state(&other.transform_state, &self.transform_state)
    }

    fn name(&self) -> &'static str {
        "veil"
    }
}

impl ModifierNode for VeilNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for VeilNode {
    fn draw_after(&mut self, scope: &mut DrawScope<'_>) {
        let (enter, exit) =
            (self.parts.enter.config.veil.clone(), self.parts.exit.config.veil.clone());
        let specs = (
            enter.as_ref().map(|veil| veil.animation_spec.clone()),
            exit.as_ref().map(|veil| veil.animation_spec.clone()),
        );
        let manual = self.transform_state.clone();
        let handoff = manual.as_ref().and_then(|state| state.veil_handoff_value());
        let color = self.animation.animate_forced(
            move |segment| part_spec(segment, &specs.0, &specs.1, default_veil_spec()),
            handoff,
            None,
            move |state| match state {
                EnterExitState::Visible => enter
                    .as_ref()
                    .map(|veil| veil.target_color)
                    .or(exit.as_ref().map(|veil| veil.initial_color))
                    .unwrap_or(Color::TRANSPARENT),
                EnterExitState::PreEnter => {
                    enter.as_ref().map_or(Color::TRANSPARENT, |veil| veil.initial_color)
                }
                EnterExitState::PostExit => {
                    exit.as_ref().map_or(Color::TRANSPARENT, |veil| veil.target_color)
                }
            },
        );
        let color = color.value();
        let color = manual.map_or(color, |state| state.combined_veil(color));
        if color.alpha() != 0 {
            let bounds = scope.bounds();
            scope.draw_rect(bounds, Paint::fill(color));
        }
    }
}

/// A deferred animation of the transition, or none: remembered either way, so it stays with
/// its call.
fn deferred_if<T: Animate>(
    transition: &Transition<EnterExitState>,
    needed: bool,
    label: &'static str,
    part: &'static str,
) -> Option<DeferredAnimation<EnterExitState, T>> {
    // `"{label}{part}"`, made once, as Compose's `remember { "$label alpha" }`.
    let deferred = transition.create_deferred_animation::<T>(|| format!("{label}{part}"));
    // Not needed, Compose's call leaves the composition (its animation leaves the transition
    // once the pass is applied): used again, it starts anew, at its value for the state the
    // transition is at.
    let leaving = deferred.clone();
    compose_runtime::SideEffect(move || {
        if !needed {
            leaving.leave();
        }
    });
    needed.then_some(deferred)
}

/// The enter and exit animating.
#[derive(Clone)]
struct EnterExit {
    enter: Rc<EnterTransition>,
    exit: Rc<ExitTransition>,
}

/// The deferred animations of the parts.
#[derive(Clone)]
struct Animations {
    slide: Option<DeferredAnimation<EnterExitState, IntOffset>>,
    size: Option<DeferredAnimation<EnterExitState, IntSize>>,
    /// The offset making up for a change of the side that stays, as
    /// `InterruptionHandlingOffset`.
    offset: Option<DeferredAnimation<EnterExitState, IntOffset>>,
    alpha: Option<DeferredAnimation<EnterExitState, f32>>,
    scale: Option<DeferredAnimation<EnterExitState, f32>>,
    transform_origin: Option<DeferredAnimation<EnterExitState, TransformOrigin>>,
}

/// A transform origin animates as its two fractions, as Compose's
/// `TransformOriginVectorConverter`.
impl Animate for TransformOrigin {
    fn to_vector(&self) -> AnimationVector {
        AnimationVector::from([self.pivot_fraction_x, self.pivot_fraction_y])
    }
    fn from_vector(vector: AnimationVector) -> TransformOrigin {
        TransformOrigin { pivot_fraction_x: vector[0], pivot_fraction_y: vector[1] }
    }
}

/// The spec of a part: the enter's for `PreEnter → Visible`, the exit's for `Visible →
/// PostExit`, the default otherwise.
fn part_spec(
    segment: &Segment<EnterExitState>,
    enter: &Option<AnimationSpec>,
    exit: &Option<AnimationSpec>,
    default: AnimationSpec,
) -> AnimationSpec {
    if segment.is_transitioning_to(&EnterExitState::PreEnter, &EnterExitState::Visible) {
        enter.clone().unwrap_or(default)
    } else if segment.is_transitioning_to(&EnterExitState::Visible, &EnterExitState::PostExit) {
        exit.clone().unwrap_or(default)
    } else {
        default
    }
}

/// The graphics layer animating the alpha, the scale and its origin, as Compose's
/// `createGraphicsLayerBlock`: read while drawing. A deferred transition's content has one always
/// (its transform can set them any time), showing the manual values over the animated ones.
fn graphics_layer_block(
    animations: &Animations,
    parts: &EnterExit,
    transform_state: Option<Rc<SharedMutableTransformState>>,
) -> Modifier {
    if animations.alpha.is_none() && animations.scale.is_none() && transform_state.is_none() {
        return Modifier;
    }
    let (animations, parts) = (animations.clone(), parts.clone());
    Modifier.graphics_layer(move |layer| {
        let manual = transform_state.as_deref();
        let mut alpha_value = None;
        if let Some(alpha) = &animations.alpha {
            let fade_specs = (
                parts.enter.config.fade.as_ref().map(|fade| fade.animation_spec.clone()),
                parts.exit.config.fade.as_ref().map(|fade| fade.animation_spec.clone()),
            );
            let alphas = (
                parts.enter.config.fade.as_ref().map(|fade| fade.alpha),
                parts.exit.config.fade.as_ref().map(|fade| fade.alpha),
            );
            let value = alpha.animate_forced(
                move |segment| {
                    part_spec(segment, &fade_specs.0, &fade_specs.1, default_alpha_spec())
                },
                manual.and_then(|state| state.alpha_handoff_value()),
                None,
                move |state| match state {
                    EnterExitState::Visible => 1.0,
                    EnterExitState::PreEnter => alphas.0.unwrap_or(1.0),
                    EnterExitState::PostExit => alphas.1.unwrap_or(1.0),
                },
            );
            alpha_value = Some(value.value());
        }
        let mut scale_value = None;
        if let Some(scale) = &animations.scale {
            let scale_specs = (
                parts.enter.config.scale.as_ref().map(|scale| scale.animation_spec.clone()),
                parts.exit.config.scale.as_ref().map(|scale| scale.animation_spec.clone()),
            );
            let scales = (
                parts.enter.config.scale.as_ref().map(|scale| scale.scale),
                parts.exit.config.scale.as_ref().map(|scale| scale.scale),
            );
            let value = scale.animate_forced(
                move |segment| {
                    part_spec(segment, &scale_specs.0, &scale_specs.1, default_scale_spec())
                },
                manual.and_then(|state| state.scale_handoff_value()),
                manual.and_then(|state| state.scale_handoff_velocity()),
                move |state| match state {
                    EnterExitState::Visible => 1.0,
                    EnterExitState::PreEnter => scales.0.unwrap_or(1.0),
                    EnterExitState::PostExit => scales.1.unwrap_or(1.0),
                },
            );
            scale_value = Some(value.value());
        }
        let mut origin_value = None;
        if let Some(origin) = &animations.transform_origin {
            let enter_origin =
                parts.enter.config.scale.as_ref().map(|scale| scale.transform_origin);
            let exit_origin = parts.exit.config.scale.as_ref().map(|scale| scale.transform_origin);
            let current = animations.current_state();
            let when_visible = if current == Some(EnterExitState::PreEnter) {
                enter_origin.or(exit_origin)
            } else {
                exit_origin.or(enter_origin)
            };
            let value = origin.animate_forced(
                |_| AnimationSpec::Spring(spring()),
                manual.and_then(|state| state.transform_origin_handoff_value()),
                None,
                move |state| {
                    match state {
                        EnterExitState::Visible => when_visible,
                        EnterExitState::PreEnter => enter_origin.or(exit_origin),
                        EnterExitState::PostExit => exit_origin,
                    }
                    .unwrap_or(TransformOrigin::CENTER)
                },
            );
            origin_value = Some(value.value());
        }
        match manual {
            Some(state) => {
                layer.alpha = state.combined_alpha(alpha_value.unwrap_or(1.0));
                let scale = state.combined_scale(scale_value.unwrap_or(1.0));
                layer.scale_x = scale;
                layer.scale_y = scale;
                layer.transform_origin = state
                    .combined_transform_origin(origin_value.unwrap_or(TransformOrigin::CENTER));
            }
            None => {
                if let Some(alpha) = alpha_value {
                    layer.alpha = alpha;
                }
                if let Some(scale) = scale_value {
                    layer.scale_x = scale;
                    layer.scale_y = scale;
                }
                if let Some(origin) = origin_value {
                    layer.transform_origin = origin;
                }
            }
        }
    })
}

/// The same deferred state, or none.
fn same_state(
    a: &Option<Rc<SharedMutableTransformState>>,
    b: &Option<Rc<SharedMutableTransformState>>,
) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    }
}

impl Animations {
    /// The state the transition is at (from the scale's animation, which knows it).
    fn current_state(&self) -> Option<EnterExitState> {
        self.transform_origin.as_ref().map(DeferredAnimation::current_state_untracked)
    }
}

/// The node animating the size and the slide, as Compose's `EnterExitTransitionModifierNode`.
struct EnterExitElement {
    transition: Transition<EnterExitState>,
    parts: EnterExit,
    animations: Animations,
    transform_state: Option<Rc<SharedMutableTransformState>>,
}

struct EnterExitNode {
    transition: Transition<EnterExitState>,
    parts: EnterExit,
    animations: Animations,
    transform_state: Option<Rc<SharedMutableTransformState>>,
    /// The side that stays, latched at the start of a change, as `currentAlignment`.
    current_alignment: Option<Alignment>,
}

impl ModifierNodeElement for EnterExitElement {
    type Node = EnterExitNode;
    fn create(&self) -> EnterExitNode {
        EnterExitNode {
            transition: self.transition.clone(),
            parts: self.parts.clone(),
            animations: self.animations.clone(),
            transform_state: self.transform_state.clone(),
            current_alignment: None,
        }
    }

    fn update(&self, node: &mut EnterExitNode) {
        node.transition = self.transition.clone();
        node.parts = self.parts.clone();
        node.animations = self.animations.clone();
        node.transform_state = self.transform_state.clone();
    }

    fn same(&self, other: &Self) -> bool {
        other.transition.same(&self.transition)
            && *other.parts.enter == *self.parts.enter
            && *other.parts.exit == *self.parts.exit
            && same_state(&other.transform_state, &self.transform_state)
    }

    fn name(&self) -> &'static str {
        "enterExitTransition"
    }
}

impl ModifierNode for EnterExitNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }

    fn on_release(&mut self, _context: &mut ReleaseContext<'_>) {}
}

impl EnterExitNode {
    /// The side that stays for the change the transition plays.
    fn alignment(&self) -> Option<Alignment> {
        let enter = self.parts.enter.config.change_size.as_ref().map(|change| change.alignment);
        let exit = self.parts.exit.config.change_size.as_ref().map(|change| change.alignment);
        let segment = self.transition.segment();
        if segment.is_transitioning_to(&EnterExitState::PreEnter, &EnterExitState::Visible) {
            enter.or(exit)
        } else {
            exit.or(enter)
        }
    }
}

impl LayoutModifierNode for EnterExitNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        if self.transition.current_state() == self.transition.target_state() {
            self.current_alignment = None;
        } else if self.current_alignment.is_none() {
            self.current_alignment = Some(self.alignment().unwrap_or(Alignment::TOP_START));
        }
        let placeable = scope.measure(content, constraints);
        let target = placeable.size();
        let size = match &self.animations.size {
            Some(size) => {
                let (enter, exit) = (
                    self.parts.enter.config.change_size.clone(),
                    self.parts.exit.config.change_size.clone(),
                );
                let specs = (
                    enter.as_ref().map(|change| change.animation_spec.clone()),
                    exit.as_ref().map(|change| change.animation_spec.clone()),
                );
                let animated = size.animate(
                    move |segment| part_spec(segment, &specs.0, &specs.1, default_size_spec()),
                    move |state| match state {
                        EnterExitState::Visible => target,
                        EnterExitState::PreEnter => {
                            enter.as_ref().map_or(target, |change| (change.size)(target))
                        }
                        EnterExitState::PostExit => {
                            exit.as_ref().map_or(target, |change| (change.size)(target))
                        }
                    },
                );
                constraints.constrain(animated.value())
            }
            None => constraints.constrain(target),
        };
        let offset_delta = match &self.animations.offset {
            Some(offset) => {
                let (current, alignment) = (self.current_alignment, self.alignment());
                let exit = self.parts.exit.config.change_size.clone();
                let animated = offset.animate(
                    |_| default_offset_spec(),
                    move |state| match (current, alignment) {
                        (Some(current), Some(alignment))
                            if current != alignment && *state == EnterExitState::PostExit =>
                        {
                            exit.as_ref().map_or(IntOffset::ZERO, |change| {
                                let end = (change.size)(target);
                                let to = alignment.align(target, end, LayoutDirection::Ltr);
                                let from = current.align(target, end, LayoutDirection::Ltr);
                                to - from
                            })
                        }
                        _ => IntOffset::ZERO,
                    },
                );
                animated.value()
            }
            None => IntOffset::ZERO,
        };
        let slide = match &self.animations.slide {
            Some(slide) => {
                let (enter, exit) =
                    (self.parts.enter.config.slide.clone(), self.parts.exit.config.slide.clone());
                let specs = (
                    enter.as_ref().map(|slide| slide.animation_spec.clone()),
                    exit.as_ref().map(|slide| slide.animation_spec.clone()),
                );
                let manual = self.transform_state.as_deref();
                let animated = slide.animate_forced(
                    move |segment| part_spec(segment, &specs.0, &specs.1, default_offset_spec()),
                    manual.and_then(|state| state.slide_handoff_value()),
                    manual.and_then(|state| state.slide_handoff_velocity()),
                    move |state| match state {
                        EnterExitState::Visible => IntOffset::ZERO,
                        EnterExitState::PreEnter => enter
                            .as_ref()
                            .map_or(IntOffset::ZERO, |slide| (slide.slide_offset)(target)),
                        EnterExitState::PostExit => exit
                            .as_ref()
                            .map_or(IntOffset::ZERO, |slide| (slide.slide_offset)(target)),
                    },
                );
                animated.value()
            }
            None => IntOffset::ZERO,
        };
        let slide = match &self.transform_state {
            Some(state) => state.combined_slide(slide, target),
            None => slide,
        };
        // The content at its full size, the side that stays at its edge, slid, as Compose's
        // (left to right whatever the direction).
        let aligned = self.current_alignment.map_or(IntOffset::ZERO, |alignment| {
            alignment.align(target, size, LayoutDirection::Ltr)
        });
        let at = aligned + slide + offset_delta;
        scope.place(&placeable, at.x, at.y);
        scope.layout(size.width, size.height)
    }
}
