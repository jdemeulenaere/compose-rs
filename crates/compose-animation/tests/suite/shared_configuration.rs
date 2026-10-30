//! What shared transitions are configured with (no Compose test targets these alone: its
//! `SharedTransitionTest` goes through the scope, which waits for ui lookahead). Against the values
//! in Compose's `SharedTransitionScope.kt`.

use compose_animation::*;
use compose_ui::{Alignment, ContentScale, IntSize, Rect};

#[test]
fn the_default_bounds_transform_is_a_spring_of_stiffness_400_and_a_pixel_s_threshold() {
    let transform = SharedTransitionDefaults::bounds_transform();
    assert_eq!(transform, SharedTransitionDefaults::bounds_transform(), "one shared value");
    let spec = transform
        .create_animation_spec(Rect::new(0.0, 0.0, 1.0, 1.0), Rect::new(0.0, 0.0, 10.0, 10.0));
    let expected = spring_with(Spring::DAMPING_RATIO_NO_BOUNCY, Spring::STIFFNESS_MEDIUM_LOW)
        .visibility_threshold(Rect::VISIBILITY_THRESHOLD);
    assert_eq!(spec, AnimationSpec::Spring(expected));
}

#[test]
fn a_bounds_transform_is_given_the_two_bounds() {
    let transform = BoundsTransform::new(|initial, target| {
        tween(if initial.width() < target.width() { 100 } else { 200 }).into()
    });
    let (small, large) = (Rect::new(0.0, 0.0, 1.0, 1.0), Rect::new(0.0, 0.0, 2.0, 2.0));
    assert_eq!(transform.create_animation_spec(small, large), AnimationSpec::from(tween(100)));
    assert_eq!(transform.create_animation_spec(large, small), AnimationSpec::from(tween(200)));
}

#[test]
fn the_placeholder_sizes_are_the_animated_one_and_the_content_s() {
    let (content, animated) = (IntSize::new(1, 2), IntSize::new(3, 4));
    assert_eq!(PlaceholderSize::animated_size().calculate_size(content, animated), animated);
    assert_eq!(PlaceholderSize::content_size().calculate_size(content, animated), content);
    assert_eq!(PlaceholderSize::animated_size(), PlaceholderSize::animated_size());
    assert_ne!(PlaceholderSize::animated_size(), PlaceholderSize::content_size());
}

#[test]
fn the_default_resize_mode_scales_to_fill_the_width_centred() {
    assert_eq!(
        ResizeMode::default(),
        ResizeMode::scale_to_bounds(ContentScale::FillWidth, Alignment::CENTER)
    );
    assert_ne!(ResizeMode::default(), ResizeMode::RemeasureToBounds);
}

#[test]
fn the_default_shared_content_config_allows_everything() {
    let config = SharedTransitionDefaults::shared_content_config();
    assert!(config.should_keep_enabled_for_ongoing_animation());
    assert!(config.permit_transform_during_deferred_transition());
}
