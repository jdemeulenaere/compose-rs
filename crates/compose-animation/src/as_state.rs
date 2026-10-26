//! `animate_*_as_state`, as Compose's `animate*AsState`: a state that follows a target, animating
//! to each new one.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{SideEffect, State, composable, remember, remember_coroutine_scope};
use compose_ui::{Color, Dp, IntOffset, IntSize, Offset, Rect, Size};

use crate::animatable::{Animatable, AnimationEndReason};
use crate::spec::{AnimationSpec, SpringSpec};
use crate::vector::{Animate, default_spring};

/// `spec` with `visibility_threshold` when it's a spring of another threshold, as Compose's
/// `resolveAnimationSpec`.
fn resolve_spec<T: Animate>(
    spec: AnimationSpec,
    visibility_threshold: &Option<T>,
) -> AnimationSpec {
    match (spec, visibility_threshold) {
        (AnimationSpec::Spring(spring), Some(threshold))
            if spring.visibility_threshold != Some(threshold.to_vector()) =>
        {
            AnimationSpec::Spring(SpringSpec {
                visibility_threshold: Some(threshold.to_vector()),
                ..spring
            })
        }
        (spec, _) => spec,
    }
}

/// A state animating to `target_value` whenever it changes, along `animation_spec`, as Compose's
/// `animateValueAsState`. A new target interrupts the animation running, which keeps its
/// velocity, the new one starting at the frame the other last set the value at; a spring
/// ends within `visibility_threshold` of the target when given (it replaces a spring spec's
/// own); `finished_listener` hears the value each animation that isn't interrupted ends at.
#[composable]
pub fn animate_value_as_state<T: Animate>(
    target_value: T,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default(None)] visibility_threshold: Option<T>,
    #[default("ValueAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(T) + 'static>,
) -> State<T> {
    let animation_spec = resolve_spec(animation_spec.into(), &visibility_threshold);
    let animatable = remember(|| {
        Animatable::new_with(target_value.clone(), visibility_threshold.clone(), label)
    });
    let scope = remember_coroutine_scope();
    // The latest listener, as Compose's `finishedListener` field: an animation that finishes
    // calls the one given last.
    type Listener<T> = Option<compose_runtime::Callback<dyn Fn(T)>>;
    let listener = remember(|| Rc::new(RefCell::new(None::<compose_runtime::Callback<dyn Fn(T)>>)));
    let state = animatable.as_state();
    SideEffect(move || {
        let latest: Listener<T> = finished_listener.clone();
        *listener.borrow_mut() = latest;
        if crate::vector::same_value(&target_value, &animatable.target_value()) {
            return;
        }
        // The animation running is interrupted by the new one (it reads the frame and the
        // velocity the running one is at), not cancelled first.
        let listener = listener.clone();
        // An interruption-aware spec's interruption spec when it interrupts, as Compose's
        // `restartAnimation`.
        let animation_spec = animation_spec.resolve(animatable.is_running());
        scope.launch(async move {
            let result = animatable.animate_to(target_value, animation_spec).await;
            if result.end_reason != AnimationEndReason::Interrupted {
                let latest = listener.borrow().clone();
                if let Some(latest) = latest {
                    latest(animatable.value());
                }
            }
        });
    });
    state
}

/// As Compose's `animateFloatAsState`. A `visibility_threshold` other than the default 0.01
/// replaces a spring spec's own.
#[composable]
pub fn animate_float_as_state(
    target_value: f32,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default(crate::Spring::DEFAULT_DISPLACEMENT_THRESHOLD)] visibility_threshold: f32,
    #[default("FloatAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(f32) + 'static>,
) -> State<f32> {
    let threshold = (visibility_threshold != crate::spec::Spring::DEFAULT_DISPLACEMENT_THRESHOLD)
        .then_some(visibility_threshold);
    animate_value_as_state(target_value, animation_spec.into(), threshold, label, finished_listener)
}

/// The default spring of `animate_dp_as_state`: 0.4 dp's threshold.
#[doc(hidden)]
pub fn __dp_default_spring() -> AnimationSpec {
    default_spring::<Dp>()
}

/// The default spring of `animate_size_as_state`: a pixel's threshold.
#[doc(hidden)]
pub fn __size_default_spring() -> AnimationSpec {
    default_spring::<Size>()
}

/// The default spring of `animate_offset_as_state`: a pixel's threshold.
#[doc(hidden)]
pub fn __offset_default_spring() -> AnimationSpec {
    default_spring::<Offset>()
}

/// The default spring of `animate_rect_as_state`: a pixel's threshold.
#[doc(hidden)]
pub fn __rect_default_spring() -> AnimationSpec {
    default_spring::<Rect>()
}

/// The default spring of `animate_int_as_state`: a threshold of 1.
#[doc(hidden)]
pub fn __int_default_spring() -> AnimationSpec {
    default_spring::<i32>()
}

/// The default spring of `animate_int_offset_as_state`: a pixel's threshold.
#[doc(hidden)]
pub fn __int_offset_default_spring() -> AnimationSpec {
    default_spring::<IntOffset>()
}

/// The default spring of `animate_int_size_as_state`: a pixel's threshold.
#[doc(hidden)]
pub fn __int_size_default_spring() -> AnimationSpec {
    default_spring::<IntSize>()
}

/// As Compose's `animateDpAsState`: its default spring ends within 0.4 dp.
#[composable]
pub fn animate_dp_as_state(
    target_value: Dp,
    #[default(crate::__dp_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("DpAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Dp) + 'static>,
) -> State<Dp> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateColorAsState`: in the Oklab color space.
#[composable]
pub fn animate_color_as_state(
    target_value: Color,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("ColorAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Color) + 'static>,
) -> State<Color> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateIntAsState`: its default spring ends within 1.
#[composable]
pub fn animate_int_as_state(
    target_value: i32,
    #[default(crate::__int_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("IntAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(i32) + 'static>,
) -> State<i32> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateOffsetAsState`: its default spring ends within a pixel.
#[composable]
pub fn animate_offset_as_state(
    target_value: Offset,
    #[default(crate::__offset_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("OffsetAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Offset) + 'static>,
) -> State<Offset> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateIntOffsetAsState`: its default spring ends within a pixel.
#[composable]
pub fn animate_int_offset_as_state(
    target_value: IntOffset,
    #[default(crate::__int_offset_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("IntOffsetAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(IntOffset) + 'static>,
) -> State<IntOffset> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateIntSizeAsState`: its default spring ends within a pixel.
#[composable]
pub fn animate_int_size_as_state(
    target_value: IntSize,
    #[default(crate::__int_size_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("IntSizeAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(IntSize) + 'static>,
) -> State<IntSize> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateSizeAsState`: its default spring ends within a pixel.
#[composable]
pub fn animate_size_as_state(
    target_value: Size,
    #[default(crate::__size_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("SizeAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Size) + 'static>,
) -> State<Size> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}

/// As Compose's `animateRectAsState`: its default spring ends within a pixel.
#[composable]
pub fn animate_rect_as_state(
    target_value: Rect,
    #[default(crate::__rect_default_spring())] animation_spec: impl Into<AnimationSpec>,
    #[default("RectAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Rect) + 'static>,
) -> State<Rect> {
    animate_value_as_state(target_value, animation_spec.into(), None, label, finished_listener)
}
