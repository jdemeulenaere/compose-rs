//! Animations, as Compose's `androidx.compose.animation.core`: `Animatable`, the animation specs
//! (`tween`, `spring`, `snap`, `repeatable`, `infinite_repeatable`), easings,
//! `animate_*_as_state` and infinite transitions. Animations run as tasks on the UI thread's
//! executor, a step per frame (`with_frame_nanos`), and write state, so what reads their values
//! recomposes, re-measures or redraws.

#![forbid(unsafe_code)]
// Every type reachable from the public API can be named (D260): a type in a private
// module that a public signature uses must be re-exported.
#![deny(unnameable_types)]

extern crate compose_runtime as compose;

mod animatable;
mod animation;
mod as_state;
mod content;
mod content_size;
mod crossfade;
mod decay;
mod deferred_target;
mod easing;
mod infinite;
mod keyframes;
mod spec;
mod spring;
mod transition;
mod vector;
mod visibility;

pub use animatable::{Animatable, AnimationEndReason, AnimationResult};
pub use animation::{Animation, TargetBasedAnimation};
#[doc(hidden)]
pub use as_state::{
    __dp_default_spring, __int_default_spring, __int_offset_default_spring,
    __int_size_default_spring, __offset_default_spring, __rect_default_spring,
    __size_default_spring,
};
pub use as_state::{
    animate_color_as_state, animate_dp_as_state, animate_float_as_state, animate_int_as_state,
    animate_int_offset_as_state, animate_int_size_as_state, animate_offset_as_state,
    animate_rect_as_state, animate_size_as_state, animate_value_as_state,
};
/// As animation-core's `withInfiniteAnimationFrameNanos`/`Millis` (the runtime's, D901).
pub use compose_runtime::{
    with_infinite_animation_frame_millis, with_infinite_animation_frame_nanos,
};
pub use content::{
    AnimatedContent, AnimatedContentScope, AnimatedContentTransitionScope, ContentKey,
    ContentTransform, SizeTransform, SlideDirection, default_content_transform, size_transform,
};
pub use content_size::AnimationModifiers;
pub use crossfade::Crossfade;
pub use decay::{
    DecayAnimation, DecayAnimationSpec, FloatDecayAnimationSpec, FloatExponentialDecaySpec,
    VectorizedDecayAnimationSpec, exponential_decay, float_decay_animation,
    generate_decay_animation_spec,
};
pub use deferred_target::DeferredTargetAnimation;
pub use easing::{
    CubicBezierEasing, EASE, EASE_IN, EASE_IN_BACK, EASE_IN_BOUNCE, EASE_IN_CIRC, EASE_IN_CUBIC,
    EASE_IN_ELASTIC, EASE_IN_EXPO, EASE_IN_OUT, EASE_IN_OUT_BACK, EASE_IN_OUT_BOUNCE,
    EASE_IN_OUT_CIRC, EASE_IN_OUT_CUBIC, EASE_IN_OUT_ELASTIC, EASE_IN_OUT_EXPO, EASE_IN_OUT_QUAD,
    EASE_IN_OUT_QUART, EASE_IN_OUT_QUINT, EASE_IN_OUT_SINE, EASE_IN_QUAD, EASE_IN_QUART,
    EASE_IN_QUINT, EASE_IN_SINE, EASE_OUT, EASE_OUT_BACK, EASE_OUT_BOUNCE, EASE_OUT_CIRC,
    EASE_OUT_CUBIC, EASE_OUT_ELASTIC, EASE_OUT_EXPO, EASE_OUT_QUAD, EASE_OUT_QUART, EASE_OUT_QUINT,
    EASE_OUT_SINE, Easing, FAST_OUT_LINEAR_IN_EASING, FAST_OUT_SLOW_IN_EASING, LINEAR_EASING,
    LINEAR_OUT_SLOW_IN_EASING,
};
pub use infinite::{InfiniteTransition, remember_infinite_transition};
pub use keyframes::{KeyframeEntry, KeyframesBuilder, KeyframesSpec, keyframes};
pub use spec::{
    AnimationConstants, AnimationSpec, CustomDurationBasedSpec, CustomSpec,
    DEFAULT_DURATION_MILLIS, DurationBasedSpec, FloatAnimationSpec, FloatSpringSpec,
    FloatTweenSpec, InterruptionAwareAnimationSpec, RepeatMode, RepeatableSpec, SnapSpec, Spring,
    SpringSpec, StartOffset, StartOffsetType, TweenSpec, VectorizedAnimationSpec,
    VectorizedDurationBasedAnimationSpec, infinite_repeatable, infinite_repeatable_with,
    repeatable, repeatable_with, snap, spring, spring_with, tween,
};
#[doc(hidden)]
pub use spring::{
    SpringSimulation, estimate_animation_duration_millis,
    estimate_animation_duration_millis_of_mass,
};
pub use transition::{
    AnyTransition, MutableTransitionState, Segment, Transition, TransitionAnimation,
    remember_transition, update_transition,
};
#[doc(hidden)]
pub use vector::default_spring;
pub use vector::{Animate, AnimationVector, VisibilityThreshold};
pub use visibility::{
    AnimatedVisibility, AnimatedVisibilityScope, AnimatedVisibilityState, ChangeSizeConfig,
    EnterExitState, EnterExitTransitionConfig, EnterTransition, ExitTransition, FadeConfig,
    ScaleConfig, SlideConfig, VeilConfig, expand_horizontally, expand_horizontally_with, expand_in,
    expand_in_with, expand_vertically, expand_vertically_with, fade_in, fade_in_with, fade_out,
    fade_out_with, scale_in, scale_in_with, scale_out, scale_out_with, shrink_horizontally,
    shrink_horizontally_with, shrink_out, shrink_out_with, shrink_vertically,
    shrink_vertically_with, slide_in, slide_in_horizontally, slide_in_horizontally_with,
    slide_in_vertically, slide_in_vertically_with, slide_in_with, slide_out,
    slide_out_horizontally, slide_out_horizontally_with, slide_out_vertically,
    slide_out_vertically_with, slide_out_with, unveil_in, unveil_in_with, veil_out, veil_out_with,
};

#[doc(hidden)]
pub use compose_ui::{Alignment, Modifier};
