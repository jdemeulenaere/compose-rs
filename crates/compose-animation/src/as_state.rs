//! `animate_*_as_state`, as Compose's `animate*AsState`: a state that follows a target, animating
//! to each new one.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::{SideEffect, State, composable, remember, remember_coroutine_scope};
use compose_ui::{Color, Dp, IntOffset, Offset, Size};

use crate::animatable::{Animatable, AnimationEndReason};
use crate::spec::AnimationSpec;
use crate::vector::Animate;

/// A state animating to `target_value` whenever it changes, along `animation_spec`, as Compose's
/// `animateValueAsState`. A new target interrupts the animation running, which keeps its
/// velocity; `finished_listener` hears the value each animation that finishes ends at.
#[composable]
pub fn animate_value_as_state<T: Animate>(
    target_value: T,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("ValueAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(T) + 'static>,
) -> State<T> {
    let _ = label;
    let animation_spec = animation_spec.into();
    let animatable = remember(|| Animatable::new(target_value.clone()));
    let scope = remember_coroutine_scope();
    // The target the last animation went to: set when it is launched, after the pass.
    let launched = remember(|| Rc::new(RefCell::new(target_value.clone())));
    // The latest listener, as Compose's `rememberUpdatedState`: an animation that finishes
    // calls the one given last.
    type Listener<T> = Option<compose_runtime::Callback<dyn Fn(T)>>;
    let listener = remember(|| Rc::new(RefCell::new(None::<compose_runtime::Callback<dyn Fn(T)>>)));
    let state = animatable.as_state();
    SideEffect(move || {
        let latest: Listener<T> = finished_listener.clone();
        *listener.borrow_mut() = latest;
        if crate::vector::same_value(&*launched.borrow(), &target_value) {
            return;
        }
        *launched.borrow_mut() = target_value.clone();
        let listener = listener.clone();
        scope.launch(async move {
            let result = animatable.animate_to(target_value, animation_spec).await;
            if result.end_reason == AnimationEndReason::Finished {
                let latest = listener.borrow().clone();
                if let Some(latest) = latest {
                    latest(result.end_value);
                }
            }
        });
    });
    state
}

/// As Compose's `animateFloatAsState`. `visibility_threshold` is the default spring's.
#[composable]
pub fn animate_float_as_state(
    target_value: f32,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default(crate::Spring::DEFAULT_DISPLACEMENT_THRESHOLD)] visibility_threshold: f32,
    #[default("FloatAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(f32) + 'static>,
) -> State<f32> {
    let spec = match animation_spec.into() {
        AnimationSpec::Spring(spring) if spring.visibility_threshold.is_none() => {
            AnimationSpec::Spring(crate::SpringSpec {
                visibility_threshold: Some(visibility_threshold),
                ..spring
            })
        }
        spec => spec,
    };
    animate_value_as_state(target_value, spec, label, finished_listener)
}

/// As Compose's `animateDpAsState`.
#[composable]
pub fn animate_dp_as_state(
    target_value: Dp,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("DpAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Dp) + 'static>,
) -> State<Dp> {
    animate_value_as_state(target_value, animation_spec.into(), label, finished_listener)
}

/// As Compose's `animateColorAsState`: in the Oklab color space.
#[composable]
pub fn animate_color_as_state(
    target_value: Color,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("ColorAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Color) + 'static>,
) -> State<Color> {
    animate_value_as_state(target_value, animation_spec.into(), label, finished_listener)
}

/// As Compose's `animateIntAsState`.
#[composable]
pub fn animate_int_as_state(
    target_value: i32,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("IntAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(i32) + 'static>,
) -> State<i32> {
    animate_value_as_state(target_value, animation_spec.into(), label, finished_listener)
}

/// As Compose's `animateOffsetAsState`.
#[composable]
pub fn animate_offset_as_state(
    target_value: Offset,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("OffsetAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Offset) + 'static>,
) -> State<Offset> {
    animate_value_as_state(target_value, animation_spec.into(), label, finished_listener)
}

/// As Compose's `animateIntOffsetAsState`.
#[composable]
pub fn animate_int_offset_as_state(
    target_value: IntOffset,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("IntOffsetAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(IntOffset) + 'static>,
) -> State<IntOffset> {
    animate_value_as_state(target_value, animation_spec.into(), label, finished_listener)
}

/// As Compose's `animateSizeAsState`.
#[composable]
pub fn animate_size_as_state(
    target_value: Size,
    #[default(crate::AnimationSpec::Spring(crate::spring()))] animation_spec: impl Into<AnimationSpec>,
    #[default("SizeAnimation")] label: &'static str,
    #[default(None)] finished_listener: Option<impl Fn(Size) + 'static>,
) -> State<Size> {
    animate_value_as_state(target_value, animation_spec.into(), label, finished_listener)
}
