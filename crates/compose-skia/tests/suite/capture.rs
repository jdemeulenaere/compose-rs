//! compose-test's `capture_to_image` and the pixel assertions, as Compose's `captureToImage`
//! with its test utilities' `assertPixels`, drawn by `raster_colors`: here, as only this crate's
//! tests link Skia (D904).

#![allow(non_snake_case)]

use std::cell::Cell;

use compose::foundation::layout::*;
use compose::runtime::*;
use compose::test::*;
use compose::ui::{Color, Dialog, IntOffset, IntSize, Modifier, PaddingValues, Popup, UnitExt};

/// A test rule capturing with Skia.
fn compose_test(content: impl Fn() + 'static) -> ComposeTestRule {
    set_capture_renderer(compose_skia::raster_colors);
    compose::test::compose_test(content)
}

const TOP_LEFT: Color = Color::from_argb(0xFFFF0000);
const TOP_RIGHT: Color = Color::from_argb(0xFF00FF00);
const BOTTOM_LEFT: Color = Color::from_argb(0xFF0000FF);
const BOTTOM_RIGHT: Color = Color::from_argb(0xFFFFFF00);
const BACKGROUND: Color = Color::from_argb(0xFF00FFFF);

/// Four 100 by 50 px boxes in two rows, 20 px down, over a background, as the test's
/// `composeCheckerboard`.
#[composable]
fn Checkerboard() {
    Box!(modifier = Modifier.background(BACKGROUND), {
        Box!(
            modifier = Modifier
                .padding(PaddingValues { top: 20.dp(), ..PaddingValues::ZERO })
                .background(BACKGROUND),
            {
                Column!(modifier = Modifier.test_tag("root"), {
                    Row!({
                        Box!(
                            modifier = Modifier
                                .test_tag("topLeft")
                                .size((100.dp(), 50.dp()))
                                .background(TOP_LEFT)
                        );
                        Box!(
                            modifier = Modifier
                                .test_tag("topRight")
                                .size((100.dp(), 50.dp()))
                                .background(TOP_RIGHT)
                        );
                    });
                    Row!({
                        Box!(
                            modifier = Modifier
                                .test_tag("bottomLeft")
                                .size((100.dp(), 50.dp()))
                                .background(BOTTOM_LEFT)
                        );
                        Box!(
                            modifier = Modifier
                                .test_tag("bottomRight")
                                .size((100.dp(), 50.dp()))
                                .background(BOTTOM_RIGHT)
                        );
                    });
                });
            }
        );
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/BitmapCapturingTest.kt, captureIndividualRects_checkSizeAndColors
#[test]
fn each_node_captures_its_own_pixels() {
    let mut ui = compose_test(Checkerboard);
    let called = Cell::new(0);
    ui.on_node_with_tag("topLeft").capture_to_image().assert_pixels(
        Some(IntSize::new(100, 50)),
        |_| {
            called.set(called.get() + 1);
            Some(TOP_LEFT)
        },
    );
    assert_eq!(called.get(), 100 * 50);
    for (tag, color) in
        [("topRight", TOP_RIGHT), ("bottomLeft", BOTTOM_LEFT), ("bottomRight", BOTTOM_RIGHT)]
    {
        ui.on_node_with_tag(tag)
            .capture_to_image()
            .assert_pixels(Some(IntSize::new(100, 50)), |_| Some(color));
    }
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/BitmapCapturingTest.kt, captureRootContainer_checkSizeAndColors
#[test]
fn a_container_captures_what_is_drawn_in_it() {
    let mut ui = compose_test(Checkerboard);
    ui.on_node_with_tag("root").capture_to_image().assert_pixels(
        Some(IntSize::new(200, 100)),
        |at: IntOffset| {
            Some(match (at.x < 100, at.y < 50) {
                (true, true) => TOP_LEFT,
                (false, true) => TOP_RIGHT,
                (true, false) => BOTTOM_LEFT,
                (false, false) => BOTTOM_RIGHT,
            })
        },
    );
}

/// A wrong pixel fails naming it and its color; a wrong size fails.
#[test]
fn a_wrong_pixel_or_size_fails() {
    let mut ui = compose_test(Checkerboard);
    let image = ui.on_node_with_tag("topLeft").capture_to_image();
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        image.assert_pixels(None, |at| (at == IntOffset::new(3, 4)).then_some(TOP_RIGHT));
    }));
    let panic = failed.expect_err("a wrong pixel");
    let message = panic.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(message.contains("Pixel(3, 4) expected to be"), "{message}");
    assert!(
        std::panic::catch_unwind(|| {
            image.assert_pixels(Some(IntSize::new(10, 10)), |_| None);
        })
        .is_err()
    );
    image.assert_contains_color(TOP_LEFT).assert_does_not_contain_color(TOP_RIGHT);
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/BitmapCapturingTest.kt, capturePopup_verifyBackground, capturePopup_verifySize
#[test]
fn a_popup_captures_its_own_window() {
    let mut ui = compose_test(|| {
        Box!({
            Popup!({
                Box!(modifier = Modifier.size(200.dp()).background(Color::RED));
            });
        });
    });
    let image = ui.on_node(is_popup()).capture_to_image();
    image.assert_contains_color(Color::RED);
    assert_eq!(image.size(), IntSize::new(200, 200));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/BitmapCapturingTest.kt, captureComposable_withPopUp_verifyBackground, captureComposable_withDialog_verifyBackground
/// A node under a popup or a dialog captures its own window, without them.
#[test]
fn a_node_under_a_popup_or_a_dialog_captures_without_them() {
    let mut popup = compose_test(|| {
        Box!(modifier = Modifier.test_tag("root").size(300.dp()).background(BOTTOM_RIGHT), {
            Popup!({
                Box!(modifier = Modifier.size(50.dp()).background(Color::RED));
            });
        });
    });
    popup
        .on_node_with_tag("root")
        .capture_to_image()
        .assert_contains_color(BOTTOM_RIGHT)
        .assert_does_not_contain_color(Color::RED);
    let mut dialog = compose_test(|| {
        Box!(modifier = Modifier.test_tag("root").size(300.dp()).background(BOTTOM_RIGHT), {
            Dialog!(on_dismiss_request = || {}, {
                Box!(
                    modifier =
                        Modifier.size(300.dp()).background(Color::RED).test_tag("dialog content")
                );
            });
        });
    });
    dialog
        .on_node_with_tag("root")
        .capture_to_image()
        .assert_contains_color(BOTTOM_RIGHT)
        .assert_does_not_contain_color(Color::RED);
    // The dialog's own window has its content.
    dialog.on_node(is_dialog()).capture_to_image().assert_contains_color(Color::RED);
}

// Compose: compose/test-utils/src/androidMain/kotlin/androidx/compose/testutils/ImageAssertions.android.kt, assertShape (as foundation's ShapeTest and BackgroundTest use it)
/// A red circle filling a white box is that shape (its anti-aliased edge left out); a square
/// isn't; a 20 dp padded square in it is.
#[test]
fn a_drawn_shape_is_asserted_by_its_outline() {
    let density = compose::ui::Density::default();
    let mut ui = compose_test(|| {
        Column!({
            Box!(modifier = Modifier.test_tag("circle").size(100.dp()).background(Color::WHITE), {
                Box!(
                    modifier = Modifier
                        .fill_max_size()
                        .clip(compose::foundation::CIRCLE_SHAPE)
                        .background(Color::RED)
                );
            });
            Box!(modifier = Modifier.test_tag("square").size(100.dp()).background(Color::WHITE), {
                Box!(modifier = Modifier.padding(20.dp()).fill_max_size().background(Color::BLUE));
            });
        });
    });
    let circle = ui.on_node_with_tag("circle").capture_to_image();
    circle.assert_shape(density, compose::foundation::CIRCLE_SHAPE, Color::RED, Some(Color::WHITE));
    let wrong = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        circle.assert_shape(density, compose::ui::Shape::Rectangle, Color::RED, Some(Color::WHITE));
    }));
    assert!(wrong.is_err(), "a circle isn't a square");
    let square = ui.on_node_with_tag("square").capture_to_image();
    square.assert_shape_padded(
        density,
        20.dp(),
        20.dp(),
        Color::WHITE,
        Color::BLUE,
        compose::ui::Shape::Rectangle,
    );
}
