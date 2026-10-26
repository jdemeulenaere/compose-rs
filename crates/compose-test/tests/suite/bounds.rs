//! Bounds assertions in dp, as Compose's `BoundsAssertions.kt`.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::FixedTextMeasurer;
use compose_ui::{Alignment, Color, Density, Dp, IntSize, Modifier, PaddingValues, UnitExt};

const TAG: &str = "box";

/// A black box of `width` by `height` dp, `start` and `top` dp from the root's top left, as
/// Compose's `composeBox`.
#[composable]
fn PlacedBox(start: Dp, top: Dp, width: Dp, height: Dp) {
    Box!(modifier = Modifier.fill_max_size().wrap_content_size_with(Alignment::TOP_START, false), {
        Box!(modifier = Modifier.padding(PaddingValues { start, top, ..PaddingValues::ZERO }), {
            Box!(
                modifier = Modifier
                    .test_tag(TAG)
                    .required_size((width, height))
                    .background(Color::BLACK)
            );
        });
    });
}

fn compose_box() -> ComposeTestRule {
    compose_test(|| PlacedBox(50.dp(), 100.dp(), 80.dp(), 100.dp()))
}

/// The same box in whole pixels at density 2, as Compose's `composeBoxWithPixelBounds`.
fn compose_box_with_pixel_bounds() -> ComposeTestRule {
    compose_test_with(
        IntSize::new(400, 800),
        Density::new(2.0),
        Box::new(FixedTextMeasurer::new()),
        || PlacedBox(Dp(25.0), Dp(50.0), Dp(40.0), Dp(50.0)),
    )
}

fn px(ui: &ComposeTestRule, pixels: f32) -> Dp {
    ui.density().to_dp(pixels)
}

#[track_caller]
fn expect_error(action: impl FnOnce()) -> String {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let panic = failed.expect_err("an assertion error");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertSizeEquals
#[test]
fn a_nodes_size_is_asserted_in_dp() {
    let mut ui = compose_box();
    ui.on_node_with_tag(TAG).assert_width_is_equal_to(80.dp()).assert_height_is_equal_to(100.dp());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertSizeAtLeast
#[test]
fn a_nodes_size_is_at_least_its_own_and_less() {
    let mut ui = compose_box();
    ui.on_node_with_tag(TAG)
        .assert_width_is_at_least(80.dp())
        .assert_width_is_at_least(79.dp())
        .assert_height_is_at_least(100.dp())
        .assert_height_is_at_least(99.dp());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertSizeEquals_fail
#[test]
fn a_wrong_size_fails_naming_it() {
    let mut ui = compose_box();
    let message = expect_error(|| {
        ui.on_node_with_tag(TAG).assert_width_is_equal_to(70.dp());
    });
    assert!(message.contains("Actual width is 80.0.dp, expected 70.0.dp"), "{message}");
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_height_is_equal_to(90.dp());
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertSizeAtLeast_fail
#[test]
fn a_size_below_the_least_fails() {
    let mut ui = compose_box();
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_width_is_at_least(81.dp());
    });
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_height_is_at_least(101.dp());
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertWidthIsEqualTo_withCustomTolerance, assertHeightIsEqualTo_withCustomTolerance
#[test]
fn a_size_is_compared_within_the_tolerance_given() {
    let mut ui = compose_box_with_pixel_bounds();
    let (width, height) = (px(&ui, 81.0), px(&ui, 101.0));
    let (success, failure) = (px(&ui, 1.1), px(&ui, 0.9));
    ui.on_node_with_tag(TAG)
        .assert_width_is_equal_to_with_tolerance(width, success)
        .assert_height_is_equal_to_with_tolerance(height, success);
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_width_is_equal_to_with_tolerance(width, failure);
    });
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_height_is_equal_to_with_tolerance(height, failure);
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertPosition
#[test]
fn a_nodes_position_in_the_root_is_asserted_in_dp() {
    let mut ui = compose_box();
    ui.on_node_with_tag(TAG)
        .assert_position_in_root_is_equal_to(50.dp(), 100.dp())
        .assert_left_position_in_root_is_equal_to(50.dp())
        .assert_top_position_in_root_is_equal_to(100.dp());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertPosition_fail
#[test]
fn a_wrong_position_fails() {
    let mut ui = compose_box();
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_position_in_root_is_equal_to(51.dp(), 101.dp());
    });
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_position_in_root_is_equal_to(49.dp(), 99.dp());
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertPositionInRootIsEqualTo_withCustomTolerance, assertLeftPositionInRootIsEqualTo_withCustomTolerance, assertTopPositionInRootIsEqualTo_withCustomTolerance
#[test]
fn a_position_is_compared_within_the_tolerance_given() {
    let mut ui = compose_box_with_pixel_bounds();
    let (left, top) = (px(&ui, 51.0), px(&ui, 101.0));
    let (success, failure) = (px(&ui, 1.1), px(&ui, 0.9));
    ui.on_node_with_tag(TAG)
        .assert_position_in_root_is_equal_to_with_tolerance(left, top, success)
        .assert_left_position_in_root_is_equal_to_with_tolerance(left, success)
        .assert_top_position_in_root_is_equal_to_with_tolerance(top, success);
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_position_in_root_is_equal_to_with_tolerance(
            left, top, failure,
        );
    });
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_left_position_in_root_is_equal_to_with_tolerance(
            left, failure,
        );
    });
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_top_position_in_root_is_equal_to_with_tolerance(
            top, failure,
        );
    });
}

/// The rule's tolerance applies to the assertions given none, as
/// `ComposeUiTestConfig(boundsAssertionTolerance)`.
#[test]
fn the_rules_tolerance_applies_to_assertions_given_none() {
    let mut ui = compose_box();
    expect_error(|| {
        ui.on_node_with_tag(TAG).assert_width_is_equal_to(81.dp());
    });
    ui.set_bounds_assertion_tolerance(1.5.dp());
    ui.on_node_with_tag(TAG).assert_width_is_equal_to(81.dp());
}

/// A box shifted 30 dp left and 10 dp up inside a clipping root: clipped to 50 by 90.
fn compose_clipped_box() -> ComposeTestRule {
    compose_test(|| {
        Box!(
            modifier = Modifier
                .fill_max_size()
                .clip_to_bounds()
                .wrap_content_size_with(Alignment::TOP_START, false),
            {
                Box!(
                    modifier = Modifier
                        .offset(Dp(-30.0), Dp(-10.0))
                        .test_tag(TAG)
                        .required_size((80.dp(), 100.dp()))
                        .background(Color::BLACK)
                );
            }
        );
    })
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertClippedPosition
#[test]
fn a_clipped_nodes_position_is_unclipped() {
    let mut ui = compose_clipped_box();
    ui.on_node_with_tag(TAG)
        .assert_position_in_root_is_equal_to(Dp(-30.0), Dp(-10.0))
        .assert_left_position_in_root_is_equal_to(Dp(-30.0))
        .assert_top_position_in_root_is_equal_to(Dp(-10.0));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertClippedSize
#[test]
fn a_clipped_nodes_size_is_unclipped() {
    let mut ui = compose_clipped_box();
    ui.on_node_with_tag(TAG).assert_width_is_equal_to(80.dp()).assert_height_is_equal_to(100.dp());
}

/// The bounds in the root clip; the unclipped bounds don't, as `getBoundsInRoot` and
/// `getUnclippedBoundsInRoot`.
#[test]
fn the_bounds_in_the_root_are_clipped_and_the_unclipped_ones_not() {
    let mut ui = compose_clipped_box();
    let unclipped = ui.on_node_with_tag(TAG).get_unclipped_bounds_in_root();
    assert_eq!(
        (unclipped.left, unclipped.top, unclipped.right, unclipped.bottom),
        (Dp(-30.0), Dp(-10.0), 50.dp(), 90.dp())
    );
    let clipped = ui.on_node_with_tag(TAG).get_bounds_in_root();
    assert_eq!((clipped.left, clipped.top, clipped.right, clipped.bottom), (0.dp(), 0.dp(), 50.dp(), 90.dp()));
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/assertions/BoundsAssertionsTest.kt, assertBoundsInRootUsesLocalDensity (at the root's density)
/// Bounds are in dp at the root's density: 20 dp of padding is 40 px at density 2.
#[test]
fn bounds_are_in_dp_at_the_density() {
    let mut ui = compose_test_with(
        IntSize::new(400, 800),
        Density::new(2.0),
        Box::new(FixedTextMeasurer::new()),
        || {
            Box!(modifier = Modifier.padding(20.dp()).test_tag(TAG).required_size(10.dp()));
        },
    );
    let bounds = ui.on_node_with_tag(TAG).get_bounds_in_root();
    bounds.left.assert_is_equal_to(20.dp(), "left", DEFAULT_BOUNDS_TOLERANCE);
    bounds.top.assert_is_equal_to(20.dp(), "top", DEFAULT_BOUNDS_TOLERANCE);
    ui.on_node_with_tag(TAG).assert_width_is_equal_to(10.dp());
    let pixels = ui.on_node_with_tag(TAG).fetch_semantics_node().bounds;
    assert_eq!(pixels.width(), 20.0);
}

/// `Dp::assert_is_equal_to`: within the tolerance; an unspecified value only matches an
/// unspecified one, an infinite one only itself.
#[test]
fn dp_values_compare_within_a_tolerance() {
    10.dp().assert_is_equal_to(10.4.dp(), "width", 0.5.dp());
    Dp::UNSPECIFIED.assert_is_equal_to(Dp::UNSPECIFIED, "width", 0.5.dp());
    Dp(f32::INFINITY).assert_is_equal_to(Dp(f32::INFINITY), "width", 0.5.dp());
    let message = expect_error(|| 10.dp().assert_is_equal_to(11.dp(), "width", 0.5.dp()));
    assert!(message.contains("Actual width is 10.0.dp, expected 11.0.dp (tolerance: 0.5.dp)"), "{message}");
    expect_error(|| 10.dp().assert_is_equal_to(Dp::UNSPECIFIED, "width", 0.5.dp()));
}
