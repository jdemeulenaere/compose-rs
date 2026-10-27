//! The enter and exit transitions' parts, as Compose's `EnterExitTransitionConfig`: what each
//! constructor sets, `+`, and the size transform's defaults.

use compose_animation::*;
use compose_ui::{Alignment, Color, IntOffset, IntSize, TransformOrigin};

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testFadeInConfig
#[test]
fn a_fade_in_sets_only_its_fade() {
    let spec = AnimationSpec::from(tween(300));
    let config = fade_in_with(spec.clone(), 0.5).config().clone();
    let fade = config.fade.expect("a fade");
    assert_eq!((fade.alpha, fade.animation_spec), (0.5, spec));
    assert!(config.slide.is_none() && config.change_size.is_none() && config.scale.is_none());
    assert!(config.veil.is_none());
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testFadeOutConfig
#[test]
fn a_fade_out_sets_its_fade() {
    let spec = AnimationSpec::from(spring());
    let fade = fade_out_with(spec.clone(), 0.2).config().fade.clone().expect("a fade");
    assert_eq!((fade.alpha, fade.animation_spec), (0.2, spec));
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testSlideInConfig
// The offset function is checked by what it gives (Rust closures have no identity to compare).
#[test]
fn a_slide_in_sets_its_slide() {
    let spec = AnimationSpec::from(tween(DEFAULT_DURATION_MILLIS));
    let slide = slide_in_with(spec.clone(), |size| IntOffset::new(size.width / 2, 0))
        .config()
        .slide
        .clone();
    let slide = slide.expect("a slide");
    assert_eq!(slide.animation_spec, spec);
    assert_eq!((slide.slide_offset)(IntSize::new(100, 40)), IntOffset::new(50, 0));
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testSlideOutConfig
#[test]
fn a_slide_out_sets_its_slide() {
    let spec = AnimationSpec::from(spring());
    let slide = slide_out_with(spec.clone(), |size| IntOffset::new(0, size.height / 2))
        .config()
        .slide
        .clone();
    let slide = slide.expect("a slide");
    assert_eq!(slide.animation_spec, spec);
    assert_eq!((slide.slide_offset)(IntSize::new(100, 40)), IntOffset::new(0, 20));
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testChangeSizeConfig
#[test]
fn an_expansion_sets_its_size_change() {
    let spec = AnimationSpec::from(spring());
    let change = expand_in_with(spec.clone(), Alignment::BOTTOM_END, false, |size| {
        IntSize::new(size.width / 4, size.height / 4)
    })
    .config()
    .change_size
    .clone()
    .expect("a size change");
    assert_eq!(change.alignment, Alignment::BOTTOM_END);
    assert_eq!((change.size)(IntSize::new(100, 40)), IntSize::new(25, 10));
    assert_eq!(change.animation_spec, spec);
    assert!(!change.clip);
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testShrinkConfig
#[test]
fn a_shrink_sets_its_size_change() {
    let spec = AnimationSpec::from(tween(DEFAULT_DURATION_MILLIS));
    let change = shrink_out_with(spec.clone(), Alignment::CENTER, true, |_| IntSize::ZERO)
        .config()
        .change_size
        .clone()
        .expect("a size change");
    assert_eq!(change.alignment, Alignment::CENTER);
    assert_eq!((change.size)(IntSize::new(100, 40)), IntSize::ZERO);
    assert_eq!(change.animation_spec, spec);
    assert!(change.clip);
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testScaleConfig
#[test]
fn a_scale_in_sets_its_scale() {
    let spec = AnimationSpec::from(tween(DEFAULT_DURATION_MILLIS));
    let origin = TransformOrigin { pivot_fraction_x: 0.1, pivot_fraction_y: 0.2 };
    let scale = scale_in_with(spec.clone(), 0.8, origin).config().scale.clone().expect("a scale");
    assert_eq!((scale.scale, scale.transform_origin, scale.animation_spec), (0.8, origin, spec));
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testScaleOutConfig
#[test]
fn a_scale_out_sets_its_scale() {
    let spec = AnimationSpec::from(spring());
    let scale = scale_out_with(spec.clone(), 0.5, TransformOrigin::CENTER).config().scale.clone();
    let scale = scale.expect("a scale");
    assert_eq!(
        (scale.scale, scale.transform_origin, scale.animation_spec),
        (0.5, TransformOrigin::CENTER, spec)
    );
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testCombinedConfig
#[test]
fn transitions_added_keep_each_part() {
    let (fade_spec, scale_spec) = (AnimationSpec::from(tween(100)), AnimationSpec::from(spring()));
    let enter = fade_in_with(fade_spec.clone(), 0.0)
        + scale_in_with(scale_spec.clone(), 0.0, TransformOrigin::CENTER);
    let config = enter.config();
    assert_eq!(config.fade.as_ref().unwrap().animation_spec, fade_spec);
    assert_eq!(config.scale.as_ref().unwrap().animation_spec, scale_spec);
    assert!(config.slide.is_none());
}

/// Where both have a part, the right one's wins, as Compose's `plus`.
#[test]
fn the_right_transitions_part_wins() {
    let enter = fade_in_with(tween(100), 0.3) + fade_in_with(tween(200), 0.6);
    assert_eq!(enter.config().fade.as_ref().unwrap().alpha, 0.6);
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testSizeTransformDefaultConfig
#[test]
fn the_default_size_transform_gives_one_spec() {
    let transform = SizeTransform::default();
    let first = transform.create_animation_spec(IntSize::ZERO, IntSize::new(100, 100));
    let second = transform.create_animation_spec(IntSize::new(10, 10), IntSize::new(50, 50));
    assert_eq!(first, second);
    // Stiffness 400, a pixel's threshold.
    let AnimationSpec::Spring(spring) = first else { panic!("{first:?}") };
    assert_eq!(spring.stiffness, Spring::STIFFNESS_MEDIUM_LOW);
    assert_eq!(spring.visibility_threshold, Some(AnimationVector::from([1.0, 1.0])));
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testContentTransformDefaultSizeTransform
#[test]
fn content_transforms_share_the_default_size_transform() {
    let first = fade_in().together_with(fade_out());
    let second = fade_in().together_with(fade_out());
    assert_eq!(first.size_transform, second.size_transform);
    assert!(first.size_transform.is_some());
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testVeilConfig
#[test]
fn an_unveil_sets_its_veil() {
    let spec = AnimationSpec::from(tween(DEFAULT_DURATION_MILLIS));
    let veil =
        unveil_in_with(spec.clone(), Color::RED, false).config().veil.clone().expect("a veil");
    assert_eq!(veil.initial_color, Color::RED);
    assert_eq!(veil.target_color, Color::RED.with_alpha(0.0));
    assert_eq!(veil.animation_spec, spec);
    assert!(!veil.match_parent_size);
}

// Compose: compose/animation/animation/src/androidHostTest/kotlin/androidx/compose/animation/EnterExitTransitionConfigTest.kt, testVeilOutConfig
#[test]
fn a_veil_out_sets_its_veil() {
    let spec = AnimationSpec::from(spring());
    let veil =
        veil_out_with(spec.clone(), Color::BLUE, true).config().veil.clone().expect("a veil");
    assert_eq!(veil.initial_color, Color::BLUE.with_alpha(0.0));
    assert_eq!(veil.target_color, Color::BLUE);
    assert_eq!(veil.animation_spec, spec);
    assert!(veil.match_parent_size);
}
