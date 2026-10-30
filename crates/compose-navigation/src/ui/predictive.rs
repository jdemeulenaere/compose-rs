//! `PredictivePopGesture.kt`.

use std::rc::Rc;

use compose_animation::{
    AnimationSpec, ContentTransform, Easing, LINEAR_EASING, MutableContentTransform,
    MutableTransform,
};
use compose_ui::{Color, IntOffset};

fn extract_easing(spec: &AnimationSpec) -> Easing {
    match spec {
        AnimationSpec::Tween(tween) => tween.easing.clone(),
        _ => LINEAR_EASING,
    }
}

fn lerp(start: f32, stop: f32, fraction: f32) -> f32 {
    (1.0 - fraction) * start + fraction * stop
}

fn lerp_int(start: i32, stop: i32, fraction: f32) -> i32 {
    start + (f64::from(stop - start) * f64::from(fraction)).round() as i32
}

/// `transform` followed by hand along `progress`, each part eased as its spec, as
/// `toMutableTransform`.
pub(crate) fn to_mutable_transform(
    transform: &ContentTransform,
    progress: Rc<dyn Fn() -> f32>,
) -> MutableContentTransform {
    let enter = transform.target_content_enter.config().clone();
    let exit = transform.initial_content_exit.config().clone();
    let exit_veil_matches = exit.veil.as_ref().is_some_and(|veil| veil.match_parent_size);
    let enter_veil_matches = enter.veil.as_ref().is_some_and(|veil| veil.match_parent_size);
    let target_progress = progress.clone();
    MutableContentTransform::of(
        MutableTransform::empty().veil_match_parent_size(exit_veil_matches),
        MutableTransform::empty().veil_match_parent_size(enter_veil_matches),
    )
    .target_content_transform(move |scope, full_size| {
        let along = |spec: &AnimationSpec| extract_easing(spec).transform(target_progress());
        if let Some(fade) = &enter.fade {
            scope.set_alpha(lerp(fade.alpha, 1.0, along(&fade.animation_spec)));
        }
        if let Some(scale) = &enter.scale {
            scope.set_scale(lerp(scale.scale, 1.0, along(&scale.animation_spec)));
            scope.set_transform_origin(scale.transform_origin);
        }
        if let Some(slide) = &enter.slide {
            let fraction = along(&slide.animation_spec);
            let initial = (slide.slide_offset)(full_size);
            scope.set_offset(IntOffset::new(
                lerp_int(initial.x, 0, fraction),
                lerp_int(initial.y, 0, fraction),
            ));
        }
        if let Some(veil) = &enter.veil {
            let fraction = along(&veil.animation_spec);
            scope.set_veil(Color::lerp(veil.initial_color, veil.target_color, fraction));
        }
    })
    .initial_content_transform(move |scope, full_size| {
        let along = |spec: &AnimationSpec| extract_easing(spec).transform(progress());
        if let Some(fade) = &exit.fade {
            scope.set_alpha(lerp(1.0, fade.alpha, along(&fade.animation_spec)));
        }
        if let Some(scale) = &exit.scale {
            scope.set_scale(lerp(1.0, scale.scale, along(&scale.animation_spec)));
            scope.set_transform_origin(scale.transform_origin);
        }
        if let Some(slide) = &exit.slide {
            let fraction = along(&slide.animation_spec);
            let target = (slide.slide_offset)(full_size);
            scope.set_offset(IntOffset::new(
                lerp_int(0, target.x, fraction),
                lerp_int(0, target.y, fraction),
            ));
        }
        if let Some(veil) = &exit.veil {
            let fraction = along(&veil.animation_spec);
            scope.set_veil(Color::lerp(veil.initial_color, veil.target_color, fraction));
        }
    })
}
