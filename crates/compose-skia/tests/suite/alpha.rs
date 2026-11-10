//! Compose's `AlphaTest` (ui, device tests): `Modifier.alpha` at 1, 0 and a half, switched by
//! state, nested in another, and emitted later, drawn by Skia and checked pixel by pixel.

use compose::runtime::*;
use compose::test::*;
use compose::ui::{Color, IntSize, Layout, Modifier, Paint, measure_policy};

/// Fills its layout with `color`.
fn background(modifier: Modifier, color: Color) -> Modifier {
    modifier.draw_behind(move |scope| {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(color));
    })
}

/// At least `size` pixels each way, as the test utilities' `AtLeastSize` (with no children).
fn at_least_size(size: i32, modifier: Modifier) {
    Layout(
        modifier,
        measure_policy(move |scope, _, constraints| {
            scope.layout(size.max(constraints.min_width), size.max(constraints.min_height))
        }),
        callback(|| {}),
    );
}

/// Its children side by side, as the test's `Row`.
fn row(modifier: Modifier, content: impl Fn() + 'static) {
    Layout(
        modifier,
        measure_policy(|scope, measurables, constraints| {
            let (mut width, mut height) = (0, 0);
            for &measurable in measurables {
                let placeable = scope.measure(measurable, constraints);
                scope.place_relative(&placeable, width, 0);
                width += placeable.width;
                height = height.max(placeable.height);
            }
            scope.layout(width, height)
        }),
        callback(content),
    );
}

/// A rule drawing with Skia in a window of `width` × `height`.
fn rule(width: i32, height: i32, content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    ComposeTestRule::sized(IntSize::new(width, height), content)
}

/// The window, `width` × `height`, as the test's `takeScreenShot`.
fn take_screenshot(ui: &mut ComposeTestRule, width: i32, height: i32) -> CapturedImage {
    ui.wait_for_idle();
    let image = ui.on_root().capture_to_image();
    assert_eq!(image.size(), IntSize::new(width, height));
    image
}

/// Every pixel is `color`, as the test utilities' `assertRect(color)`.
#[track_caller]
fn assert_rect(image: &CapturedImage, color: Color) {
    image.assert_pixels(None, |_| Some(color));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/AlphaTest.kt, drawFullAlpha
#[test]
fn draw_full_alpha() {
    let mut ui = rule(10, 10, || {
        let modifier = background(Modifier, Color::WHITE).alpha(1.0);
        at_least_size(10, background(modifier, Color::LIGHT_GRAY));
    });
    assert_rect(&take_screenshot(&mut ui, 10, 10), Color::LIGHT_GRAY);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/AlphaTest.kt, drawZeroAlpha
#[test]
fn draw_zero_alpha() {
    let mut ui = rule(10, 10, || {
        let modifier = background(Modifier, Color::WHITE).alpha(0.0);
        at_least_size(10, background(modifier, Color::LIGHT_GRAY));
    });
    assert_rect(&take_screenshot(&mut ui, 10, 10), Color::WHITE);
}

/// Red under a half alpha over white is the color of red at half alpha over white.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/AlphaTest.kt, drawHalfAlpha
#[test]
fn draw_half_alpha() {
    let color = Color::RED;
    let mut ui = rule(20, 10, move || {
        row(background(Modifier, Color::WHITE), move || {
            let faded = background(Modifier, Color::WHITE).alpha(0.5);
            at_least_size(10, background(faded, color));
            at_least_size(10, background(Modifier, color.with_alpha(0.5)));
        });
    });
    let image = take_screenshot(&mut ui, 20, 10);
    image.assert_pixel_color(image.pixel(15, 5), 5, 5);
    assert_ne!(image.pixel(5, 5), Color::WHITE);
    assert_ne!(image.pixel(5, 5), color);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/AlphaTest.kt, switchFromHalfAlphaToFull
#[test]
fn switch_from_half_alpha_to_full() {
    let alpha = mutable_state_of(0.5f32);
    let a = alpha.handle();
    let mut ui = rule(10, 10, move || {
        let modifier = background(Modifier, Color::WHITE).alpha(a.get());
        at_least_size(10, background(modifier, Color::GREEN));
    });
    // Half green over white first, so the switch is what makes it green.
    let before = take_screenshot(&mut ui, 10, 10);
    assert_ne!(before.pixel(5, 5), Color::GREEN);
    assert_ne!(before.pixel(5, 5), Color::WHITE);
    alpha.set(1.0);
    assert_rect(&take_screenshot(&mut ui, 10, 10), Color::GREEN);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/AlphaTest.kt, switchFromZeroAlphaToFullWithNestedRepaintBoundaries
#[test]
fn switch_from_zero_alpha_to_full_with_nested_repaint_boundaries() {
    let alpha = mutable_state_of(0.0f32);
    let a = alpha.handle();
    let mut ui = rule(10, 10, move || {
        let modifier = background(Modifier, Color::WHITE).alpha(1.0).alpha(a.get());
        at_least_size(10, background(modifier, Color::GREEN));
    });
    assert_rect(&take_screenshot(&mut ui, 10, 10), Color::WHITE);
    alpha.set(1.0);
    assert_rect(&take_screenshot(&mut ui, 10, 10), Color::GREEN);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/AlphaTest.kt, emitDrawWithAlphaLater
#[test]
fn emit_draw_with_alpha_later() {
    let model = mutable_state_of(false);
    let m = model.handle();
    let mut ui = rule(10, 10, move || {
        let modifier = background(Modifier, Color::WHITE);
        let modifier =
            if m.get() { background(modifier.alpha(0.0), Color::GREEN) } else { modifier };
        at_least_size(10, modifier);
    });
    ui.wait_for_idle();
    model.set(true);
    assert_rect(&take_screenshot(&mut ui, 10, 10), Color::WHITE);
}
