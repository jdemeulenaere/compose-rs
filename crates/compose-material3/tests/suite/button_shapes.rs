//! `ButtonShapes`, `IconButtonShapes` and `IconToggleButtonShapes`, and the expressive sizes: Compose's
//! `ButtonTest.kt`'s positions and animated shapes, `ButtonScreenshotTest.kt`'s `withAnimatedShape`
//! goldens as the display list's corners, and `IconButtonTest.kt`'s sizes, shapes and vibrant colors.
//!
//! Not ported: the precision pointer's (dep:precision-pointer), and the baselines (they ask for the
//! Android font's metrics).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, CornerSize, Dp, DpSize, IntSize, Modifier, Offset, Outline, Press, PressInteraction,
    Shape, UnitExt, platform::DrawOp,
};

const TAG: &str = "button";

fn rule(content: impl Fn() + 'static) -> ComposeTestRule {
    let content = std::rc::Rc::new(content);
    ComposeTestRule::sized(IntSize::new(600, 600), move || {
        let content = content.clone();
        MaterialTheme!({
            content();
        });
    })
}

fn round(dp: f32) -> Shape {
    Shape::Rounded([CornerSize::Dp(dp.dp()); 4])
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_snapshot().bounds
}

/// The first corner of the first thing in `color`.
fn corner_of(ui: &mut ComposeTestRule, color: Color) -> f32 {
    ui.host()
        .display_list()
        .ops
        .iter()
        .find_map(|op| match op {
            DrawOp::DrawRoundRect(_, radii, paint) if paint.color == color => Some(radii[0]),
            DrawOp::DrawRect(_, paint) if paint.color == color => Some(0.0),
            _ => None,
        })
        .unwrap_or_else(|| panic!("nothing in {color:?}: {}", ui.dump_display_list()))
}

/// The corner of a clip: an outlined and a text button have no fill.
fn clip_corner(ui: &mut ComposeTestRule) -> f32 {
    let list = ui.host().display_list();
    let layers = list.groups.iter().filter_map(|group| group.layer.as_ref());
    let outline = layers.filter(|layer| layer.clip).find_map(|layer| layer.outline.clone());
    match outline {
        Some(Outline::Rounded(_, radii)) => radii[0],
        other => panic!("{other:?}"),
    }
}

/// `TAG`'s five buttons with `shapes`, the container in blue where it has one.
fn each_button(index: usize, source: compose_ui::MutableInteractionSource) -> ComposeTestRule {
    rule(move || {
        let source = Some(source.clone());
        let shapes = ButtonDefaults::shapes();
        let blue = |base: ButtonColors| ButtonColors { container_color: Color::BLUE, ..base };
        let modifier = Modifier.test_tag(TAG);
        match index {
            0 => Button!(
                || {},
                shapes = shapes,
                modifier = modifier,
                colors = blue(ButtonDefaults::button_colors()),
                interaction_source = source,
                |_: RowScope| Text!("Button")
            ),
            1 => ElevatedButton!(
                || {},
                shapes = shapes,
                modifier = modifier,
                colors = blue(ButtonDefaults::elevated_button_colors()),
                interaction_source = source,
                |_: RowScope| Text!("Button")
            ),
            2 => FilledTonalButton!(
                || {},
                shapes = shapes,
                modifier = modifier,
                colors = blue(ButtonDefaults::filled_tonal_button_colors()),
                interaction_source = source,
                |_: RowScope| Text!("Button")
            ),
            3 => OutlinedButton!(
                || {},
                shapes = shapes,
                modifier = modifier,
                colors = blue(ButtonDefaults::outlined_button_colors()),
                interaction_source = source,
                |_: RowScope| Text!("Button")
            ),
            _ => TextButton!(
                || {},
                shapes = shapes,
                modifier = modifier,
                colors = blue(ButtonDefaults::text_button_colors()),
                interaction_source = source,
                |_: RowScope| Text!("Button")
            ),
        }
    })
}

/// Each button with `ButtonDefaults::shapes()`: a pill, and the theme's small corners pressed.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_withAnimatedShape_defaultShape
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_withAnimatedShape_pressedShape
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, button_withAnimatedShape_default_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, elevatedButton_withAnimatedShape_default_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, filledTonalButton_withAnimatedShape_default_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, outlinedButton_withAnimatedShape_default_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, textButton_withAnimatedShape_default_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, button_withAnimatedShape_pressed_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, elevatedButton_withAnimatedShape_pressed_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, filledTonalButton_withAnimatedShape_pressed_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, outlinedButton_withAnimatedShape_pressed_lightTheme
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonScreenshotTest.kt, textButton_withAnimatedShape_pressed_lightTheme
#[test]
fn each_buttons_corners_come_in_when_pressed() {
    let small = 8.0;
    for index in 0..5 {
        let source = compose_ui::MutableInteractionSource::new();
        let mut ui = each_button(index, source.clone());
        let height = bounds(&mut ui, TAG).height();
        // A pill: half its height.
        assert_eq!(clip_corner(&mut ui), height / 2.0, "button {index}");
        source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
        ui.wait_for_idle();
        assert_eq!(clip_corner(&mut ui), small, "button {index}");
    }
}

/// A real press, as `button_withAnimatedShape_pressedShape`'s `down(center)`.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_withAnimatedShape_pressedShape
#[test]
fn a_finger_down_brings_a_buttons_corners_in() {
    let mut ui = rule(|| {
        Box!({
            Button!(
                || {},
                shapes = ButtonDefaults::shapes(),
                modifier = Modifier.test_tag(TAG),
                colors = ButtonColors {
                    container_color: Color::BLUE,
                    ..ButtonDefaults::button_colors()
                },
                |_: RowScope| {
                    Text!("Button");
                }
            );
        });
    });
    let height = bounds(&mut ui, TAG).height();
    assert_eq!(corner_of(&mut ui, Color::BLUE), height / 2.0);
    ui.on_node_with_tag(TAG).perform_touch_input(|touch| {
        let center = touch.center();
        touch.down(center);
    });
    ui.wait_for_idle();
    assert_eq!(corner_of(&mut ui, Color::BLUE), 8.0);
}

/// Without `shapes`, it stays the shape, however it's pressed.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Button.kt, Button
#[test]
fn without_shapes_a_button_stays_its_shape() {
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        Button!(
            || {},
            shape = round(5.0),
            interaction_source = Some(hoisted.clone()),
            |_: RowScope| {
                Text!("Button");
            }
        );
    });
    assert_eq!(clip_corner(&mut ui), 5.0);
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    assert_eq!(clip_corner(&mut ui), 5.0);
}

/// Shapes with no corners: at once.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Button.kt, shapeByInteraction
#[test]
fn shapes_without_corners_snap() {
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        Button!(
            || {},
            shapes = ButtonShapes { shape: Shape::Rectangle, pressed_shape: round(9.0) },
            colors =
                ButtonColors { container_color: Color::BLUE, ..ButtonDefaults::button_colors() },
            interaction_source = Some(hoisted.clone()),
            |_: RowScope| {
                Text!("Button");
            }
        );
    });
    assert_eq!(corner_of(&mut ui, Color::BLUE), 0.0);
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.advance_time_by_frame();
    assert_eq!(corner_of(&mut ui, Color::BLUE), 9.0);
}

/// A size's padding, icon and spacing around its label.
#[track_caller]
fn check_size(height: Dp, expected: (f32, f32, f32)) {
    let (padding, icon, spacing) = expected;
    let mut ui = rule(move || {
        Box!({
            Button!(
                || {},
                modifier = Modifier.height_in(height, Dp::UNSPECIFIED).test_tag(TAG),
                content_padding = ButtonDefaults::content_padding_for(height, true, false),
                |_: RowScope| {
                    Box!(
                        modifier =
                            Modifier.size(ButtonDefaults::icon_size_for(height)).test_tag("icon")
                    );
                    Spacer(Modifier.size(ButtonDefaults::icon_spacing_for(height)));
                    Text!("Label", modifier = Modifier.test_tag("label"));
                }
            );
        });
    });
    let button = bounds(&mut ui, TAG);
    let (icon_bounds, label) = (bounds(&mut ui, "icon"), bounds(&mut ui, "label"));
    assert_eq!(icon_bounds.left - button.left, padding, "start");
    assert_eq!(icon_bounds.width(), icon, "icon");
    assert_eq!(label.left - icon_bounds.right, spacing, "spacing");
    assert_eq!(button.right - label.right, padding, "end");
    assert_eq!(button.height(), height.0, "height");
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_xSmall_positioning
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_small_positioning
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_small_withIcon_positioning
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_medium_positioning
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_large_positioning
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, button_xLarge_positioning
#[test]
fn each_size_has_its_padding_icon_and_spacing() {
    check_size(ButtonDefaults::EXTRA_SMALL_CONTAINER_HEIGHT, (12.0, 20.0, 4.0));
    check_size(ButtonDefaults::MIN_HEIGHT, (16.0, 20.0, 8.0));
    check_size(ButtonDefaults::MEDIUM_CONTAINER_HEIGHT, (24.0, 24.0, 8.0));
    check_size(ButtonDefaults::LARGE_CONTAINER_HEIGHT, (48.0, 32.0, 12.0));
    check_size(ButtonDefaults::EXTRA_LARGE_CONTAINER_HEIGHT, (64.0, 40.0, 16.0));
}

/// With `shapes`, a text button's padding is a small button's.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, text_button_shapesRequired_positioning
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ButtonTest.kt, text_button_shapesRequired_withIcon_positioning
#[test]
fn a_shaped_text_buttons_padding_is_16_dp() {
    let mut ui = rule(|| {
        Box!({
            TextButton!(
                || {},
                shapes = ButtonDefaults::shapes(),
                modifier = Modifier.test_tag(TAG),
                |_: RowScope| {
                    Text!("Label", modifier = Modifier.test_tag("label"));
                }
            );
        });
    });
    let (button, label) = (bounds(&mut ui, TAG), bounds(&mut ui, "label"));
    assert_eq!(label.left - button.left, 16.0);
    assert_eq!(button.right - label.right, 16.0);
    // The plain one's is 12.
    let mut plain = rule(|| {
        Box!({
            TextButton!(|| {}, modifier = Modifier.test_tag(TAG), |_: RowScope| {
                Text!("Label", modifier = Modifier.test_tag("label"));
            });
        });
    });
    let (button, label) = (bounds(&mut plain, TAG), bounds(&mut plain, "label"));
    assert_eq!(label.left - button.left, 12.0);
    // A shaped button's the same, the others' 24 being the plain one's.
    let mut shaped = rule(|| {
        Box!({
            Button!(
                || {},
                shapes = ButtonDefaults::shapes(),
                modifier = Modifier.test_tag(TAG),
                |_: RowScope| {
                    Text!("Label", modifier = Modifier.test_tag("label"));
                }
            );
        });
    });
    let (button, label) = (bounds(&mut shaped, TAG), bounds(&mut shaped, "label"));
    assert_eq!(label.left - button.left, 16.0);
    assert_eq!(label.top - button.top, 10.0);
}

/// `shapes_for` by height, its limits halfway between sizes; each `*For` by the same steps, its limits
/// the sizes themselves.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/Button.kt, shapesFor
#[test]
fn a_heights_defaults_go_by_its_size() {
    let _ui = rule(|| {
        let shapes = MaterialTheme::shapes();
        let pressed = |height: f32| ButtonDefaults::shapes_for(Dp(height)).pressed_shape;
        assert_eq!(pressed(36.0), shapes.small);
        assert_eq!(pressed(48.0), shapes.small);
        assert_eq!(pressed(49.0), shapes.medium);
        assert_eq!(pressed(76.0), shapes.medium);
        assert_eq!(pressed(77.0), shapes.large);
        assert_eq!(pressed(116.0), shapes.large);
        assert_eq!(pressed(117.0), shapes.large);
        assert_eq!(ButtonDefaults::shapes_for(Dp(40.0)).shape, ButtonDefaults::shape());
        let typography = MaterialTheme::typography();
        let style = |height: f32| ButtonDefaults::text_style_for(Dp(height));
        assert_eq!(style(55.0), typography.label_large);
        assert_eq!(style(56.0), typography.title_medium);
        assert_eq!(style(96.0), typography.headline_small);
        assert_eq!(style(136.0), typography.headline_large);
    });
    assert_eq!(ButtonDefaults::icon_size_for(Dp(39.0)), ButtonDefaults::EXTRA_SMALL_ICON_SIZE);
    assert_eq!(ButtonDefaults::icon_size_for(Dp(40.0)), ButtonDefaults::SMALL_ICON_SIZE);
    assert_eq!(ButtonDefaults::icon_size_for(Dp(55.0)), ButtonDefaults::SMALL_ICON_SIZE);
    assert_eq!(ButtonDefaults::icon_size_for(Dp(56.0)), ButtonDefaults::MEDIUM_ICON_SIZE);
    assert_eq!(ButtonDefaults::icon_spacing_for(Dp(39.0)), Dp(4.0));
    assert_eq!(ButtonDefaults::icon_spacing_for(Dp(40.0)), Dp(8.0));
    assert_eq!(ButtonDefaults::icon_spacing_for(Dp(96.0)), Dp(12.0));
    assert_eq!(ButtonDefaults::icon_spacing_for(Dp(136.0)), Dp(16.0));
}

// ---- Icon buttons -----------------------------------------------------------------------------

/// Each size's default container and its options.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconButton_xsmall_visualBounds
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconButton_small_visualBounds
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconButton_large_size
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconButton_xlarge_size
#[test]
fn each_sizes_container_is_its_tokens() {
    use IconButtonWidthOption::*;
    type Size = fn(IconButtonWidthOption) -> DpSize;
    let check = |size: Size, widths: [f32; 3], height: f32| {
        for (option, width) in [Narrow, Uniform, Wide].into_iter().zip(widths) {
            assert_eq!(size(option), DpSize::new(Dp(width), Dp(height)), "{option:?}");
        }
    };
    check(IconButtonDefaults::extra_small_container_size, [28.0, 32.0, 40.0], 32.0);
    check(IconButtonDefaults::small_container_size, [32.0, 40.0, 52.0], 40.0);
    check(IconButtonDefaults::medium_container_size, [48.0, 56.0, 72.0], 56.0);
    check(IconButtonDefaults::large_container_size, [64.0, 96.0, 128.0], 96.0);
    check(IconButtonDefaults::extra_large_container_size, [104.0, 136.0, 184.0], 136.0);
}

/// A filled icon button in a given size, square: the blue thing's corners.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconButton_medium_squareShape
#[test]
fn a_medium_square_icon_button_has_the_large_corners() {
    let mut ui = rule(|| {
        Box!({
            FilledIconButton!(
                || {},
                modifier = Modifier.test_tag(TAG).size(IconButtonDefaults::medium_container_size(
                    IconButtonWidthOption::Uniform
                )),
                shape = IconButtonDefaults::medium_square_shape(),
                colors = IconButtonColors {
                    container_color: Color::BLUE,
                    ..IconButtonDefaults::icon_button_vibrant_colors()
                },
                {}
            );
        });
    });
    assert_eq!(bounds(&mut ui, TAG).width(), 56.0);
    assert_eq!(corner_of(&mut ui, Color::BLUE), 16.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconButton_medium_pressedShape
#[test]
fn a_medium_icon_buttons_corners_come_in_when_pressed() {
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        let hoisted = hoisted.clone();
        Box!({
            FilledIconButton!(
                || {},
                shapes = IconButtonShapes {
                    shape: IconButtonDefaults::medium_round_shape(),
                    pressed_shape: IconButtonDefaults::medium_pressed_shape(),
                },
                modifier = Modifier.test_tag(TAG).size(IconButtonDefaults::medium_container_size(
                    IconButtonWidthOption::Uniform
                )),
                colors = IconButtonColors {
                    container_color: Color::BLUE,
                    ..IconButtonDefaults::icon_button_vibrant_colors()
                },
                interaction_source = Some(hoisted.clone()),
                {}
            );
        });
    });
    assert_eq!(corner_of(&mut ui, Color::BLUE), 28.0);
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    assert_eq!(corner_of(&mut ui, Color::BLUE), 12.0);
}

/// The plain one, with a clip and no fill of its own, follows too; the same two shapes read no press.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/IconButton.kt, shapeForInteraction
#[test]
fn a_plain_icon_buttons_corners_come_in_when_pressed() {
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        IconButton!(
            || {},
            shapes = IconButtonDefaults::shapes(),
            colors = IconButtonColors {
                container_color: Color::BLUE,
                ..IconButtonDefaults::icon_button_vibrant_colors()
            },
            interaction_source = Some(hoisted.clone()),
            {}
        );
    });
    assert_eq!(corner_of(&mut ui, Color::BLUE), 20.0);
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    assert_eq!(corner_of(&mut ui, Color::BLUE), 8.0);
    // With no hoisted source it still has one to read.
    let mut own = rule(|| {
        IconButton!(
            || {},
            modifier = Modifier.test_tag(TAG),
            shapes = IconButtonDefaults::shapes(),
            colors = IconButtonColors {
                container_color: Color::BLUE,
                ..IconButtonDefaults::icon_button_vibrant_colors()
            },
            {}
        );
    });
    own.on_node_with_tag(TAG).perform_touch_input(|touch| touch.down(Offset::new(20.0, 20.0)));
    own.wait_for_idle();
    assert_eq!(corner_of(&mut own, Color::BLUE), 8.0);
}

/// Checked, the toggleable shapes' checked one; pressed over that.
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_checked_medium_squareShape
#[test]
fn an_icon_toggle_buttons_shape_goes_by_its_state() {
    let checked = mutable_state_of(false);
    let read = checked.handle();
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        let hoisted = hoisted.clone();
        Box!({
            FilledIconToggleButton!(
                read.get(),
                |_| {},
                shapes = IconToggleButtonShapes {
                    shape: round(2.0),
                    pressed_shape: round(6.0),
                    checked_shape: round(10.0),
                },
                colors = IconToggleButtonColors {
                    container_color: Color::BLUE,
                    checked_container_color: Color::BLUE,
                    ..IconButtonDefaults::icon_toggle_button_vibrant_colors()
                },
                interaction_source = Some(hoisted.clone()),
                {}
            );
        });
    });
    assert_eq!(corner_of(&mut ui, Color::BLUE), 2.0);
    checked.set(true);
    ui.wait_for_idle();
    assert_eq!(corner_of(&mut ui, Color::BLUE), 10.0);
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    assert_eq!(corner_of(&mut ui, Color::BLUE), 6.0);
}

/// The defaults of each size, from the theme's shapes.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/IconButtonDefaults.kt, toggleableShapes
#[test]
fn the_icon_button_shapes_are_the_tokens() {
    let _ui = rule(|| {
        let shapes = MaterialTheme::shapes();
        let circle = compose_foundation::CIRCLE_SHAPE;
        use IconButtonDefaults as D;
        for shape in [
            D::extra_small_round_shape(),
            D::small_round_shape(),
            D::medium_round_shape(),
            D::large_round_shape(),
            D::extra_large_round_shape(),
            D::extra_small_selected_square_shape(),
            D::small_selected_square_shape(),
            D::medium_selected_square_shape(),
            D::large_selected_square_shape(),
            D::extra_large_selected_square_shape(),
        ] {
            assert_eq!(shape, circle);
        }
        assert_eq!(D::extra_small_square_shape(), shapes.medium);
        assert_eq!(D::small_square_shape(), shapes.medium);
        assert_eq!(D::medium_square_shape(), shapes.large);
        assert_eq!(D::large_square_shape(), shapes.extra_large);
        assert_eq!(D::extra_large_square_shape(), shapes.extra_large);
        assert_eq!(D::extra_small_pressed_shape(), shapes.small);
        assert_eq!(D::small_pressed_shape(), shapes.small);
        assert_eq!(D::medium_pressed_shape(), shapes.medium);
        assert_eq!(D::large_pressed_shape(), shapes.large);
        assert_eq!(D::extra_large_pressed_shape(), shapes.large);
        assert_eq!(D::extra_small_selected_round_shape(), shapes.medium);
        assert_eq!(D::small_selected_round_shape(), shapes.medium);
        assert_eq!(D::medium_selected_round_shape(), shapes.large);
        assert_eq!(D::large_selected_round_shape(), shapes.extra_large);
        assert_eq!(D::extra_large_selected_round_shape(), shapes.extra_large);
        let toggle = D::toggleable_shapes();
        assert_eq!(
            (toggle.shape, toggle.pressed_shape, toggle.checked_shape),
            (circle.clone(), shapes.small.clone(), shapes.medium.clone())
        );
        let shaped = D::shapes();
        assert_eq!((shaped.shape, shaped.pressed_shape), (circle, shapes.small.clone()));
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconButton_defaultVibrantColors
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconButtonVibrantColors_ignoreLocalContentColor
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_defaultVibrantColors
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconButton_defaultVibrantColors
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_defaultVibrantColors
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_borderStroke_defaultVibrantColor
#[test]
fn the_vibrant_colors_are_the_tokens() {
    let _ui = rule(|| {
        CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::RED), || {
            let scheme = light_color_scheme();
            let dim = scheme.on_surface.with_alpha(0.38);
            let plain = IconButtonColors {
                container_color: Color::TRANSPARENT,
                content_color: scheme.on_surface_variant,
                disabled_container_color: Color::TRANSPARENT,
                disabled_content_color: dim,
            };
            assert_eq!(IconButtonDefaults::icon_button_vibrant_colors(), plain);
            assert_eq!(IconButtonDefaults::outlined_icon_button_vibrant_colors(), plain);
            let toggle = IconToggleButtonColors {
                container_color: Color::TRANSPARENT,
                content_color: scheme.on_surface_variant,
                disabled_container_color: Color::TRANSPARENT,
                disabled_content_color: dim,
                checked_container_color: Color::TRANSPARENT,
                checked_content_color: scheme.primary,
            };
            assert_eq!(IconButtonDefaults::icon_toggle_button_vibrant_colors(), toggle);
            assert_eq!(
                IconButtonDefaults::outlined_icon_toggle_button_vibrant_colors(),
                IconToggleButtonColors {
                    checked_container_color: scheme.inverse_surface,
                    checked_content_color: scheme.inverse_on_surface,
                    ..toggle
                }
            );
            assert_eq!(
                IconButtonDefaults::outlined_icon_toggle_button_vibrant_border(false, false),
                Some(compose_foundation::BorderStroke::new(
                    1.dp(),
                    scheme.outline_variant.with_alpha(0.38)
                ))
            );
            assert_eq!(
                IconButtonDefaults::outlined_icon_toggle_button_vibrant_border(true, true),
                None
            );
            assert_eq!(
                IconButtonDefaults::outlined_icon_button_vibrant_border(true),
                compose_foundation::BorderStroke::new(1.dp(), scheme.outline_variant)
            );
        });
    });
}

/// The same two shapes are as good as none: a press composes nothing, as `isStatic`.
// Compose: compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/IconButton.kt, shapeForInteraction
#[test]
fn the_same_two_shapes_read_no_press() {
    let source = compose_ui::MutableInteractionSource::new();
    let hoisted = source.clone();
    let mut ui = rule(move || {
        IconButton!(
            || {},
            shapes = IconButtonShapes { shape: round(2.0), pressed_shape: round(2.0) },
            interaction_source = Some(hoisted.clone()),
            {}
        );
    });
    let before = ui.recomposition_counts();
    source.try_emit(PressInteraction::Press(Press::new(Offset::ZERO)));
    ui.wait_for_idle();
    let recomposed = ui.recomposition_counts().since(&before);
    assert_eq!(recomposed.scopes_named("IconButton"), 0, "{:?}", recomposed.names());
}
