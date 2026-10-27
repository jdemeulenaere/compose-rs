//! Compose's `DividerTest.kt` and `DividerScreenshotTest.kt`, adapted: the dividers' sizes
//! (default, custom, indented, hairline) and what they draw (a line of their thickness through
//! their middle, in the outline variant), at density 1 in a 400 by 800 window.

use std::cell::Cell;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Dp, Modifier, Offset, PaintStyle, UnitExt};

/// The size of `content` in the window, as `setMaterialContentForSizeAssertions`.
fn size(content: fn()) -> (f32, f32) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Box!(modifier = Modifier.test_tag("container"), { content() });
        });
    });
    let bounds = ui.on_node_with_tag("container").fetch_semantics_node().bounds;
    (bounds.width(), bounds.height())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerTest.kt, horizontalDivider_defaultSize
#[test]
fn a_horizontal_divider_is_as_wide_as_the_window_and_1_dp_high() {
    assert_eq!(size(|| HorizontalDivider!()), (400.0, 1.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerTest.kt, horizontalDivider_customSize
#[test]
fn a_horizontal_divider_takes_its_thickness() {
    assert_eq!(size(|| HorizontalDivider!(thickness = 20.dp())), (400.0, 20.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerTest.kt, verticalDivider_defaultSize
#[test]
fn a_vertical_divider_is_as_tall_as_the_window_and_1_dp_wide() {
    assert_eq!(size(|| VerticalDivider!()), (1.0, 800.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerTest.kt, verticalDivider_customSize
#[test]
fn a_vertical_divider_takes_its_thickness() {
    assert_eq!(size(|| VerticalDivider!(thickness = 20.dp())), (20.0, 800.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerTest.kt, divider_withIndent_doesNotChangeSize
#[test]
fn an_indent_keeps_the_dividers_size() {
    let size = size(|| {
        HorizontalDivider!(
            modifier = Modifier.padding((75.dp(), 0.dp(), 0.dp(), 0.dp())),
            thickness = 21.dp(),
        )
    });
    assert_eq!(size, (400.0, 21.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerTest.kt, divider_hairlineThickness
#[test]
fn a_hairline_divider_has_no_height() {
    thread_local! { static HEIGHT: Cell<i32> = const { Cell::new(-1) }; }
    let _ui = compose_test(|| {
        MaterialTheme!({
            let record = Modifier.layout(|scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                HEIGHT.with(|height| height.set(placeable.height));
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            });
            HorizontalDivider!(modifier = record, thickness = Dp::HAIRLINE);
        });
    });
    assert_eq!(HEIGHT.with(Cell::get), 0);
}

/// The line a divider draws: its ends (in the window) and width, its color.
fn line(scheme: ColorScheme, content: fn()) -> (Offset, Offset, f32, compose_ui::Color) {
    let mut ui = compose_test(move || {
        MaterialTheme!(color_scheme = scheme, { content() });
    });
    let mut at = vec![Offset::ZERO];
    for op in &ui.host().display_list().ops {
        let here = *at.last().expect("a translation");
        match op {
            DrawOp::Save => at.push(here),
            DrawOp::Restore => {
                at.pop();
            }
            DrawOp::Translate(by) => *at.last_mut().expect("a translation") = here + *by,
            DrawOp::DrawLine(from, to, paint) => {
                let width = match paint.style {
                    PaintStyle::Stroke(width) => width,
                    PaintStyle::Fill => 0.0,
                };
                return (here + *from, here + *to, width, paint.color);
            }
            _ => {}
        }
    }
    panic!("no line: {}", ui.dump_display_list());
}

fn spaced_horizontal() {
    Column!({
        Spacer!(modifier = Modifier.size(10.dp()));
        HorizontalDivider!();
        Spacer!(modifier = Modifier.size(10.dp()));
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerScreenshotTest.kt, horizontalDivider_lightTheme
#[test]
fn a_horizontal_divider_draws_a_line_through_its_middle() {
    let s = light_color_scheme();
    let found = line(s, spaced_horizontal);
    assert_eq!(found, (Offset::new(0.0, 10.5), Offset::new(400.0, 10.5), 1.0, s.outline_variant));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerScreenshotTest.kt, horizontalDivider_darkTheme
#[test]
fn a_dark_horizontal_divider_is_the_dark_outline_variant() {
    let s = dark_color_scheme();
    assert_eq!(line(s, spaced_horizontal).3, s.outline_variant);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerScreenshotTest.kt, verticalDivider_lightTheme
#[test]
fn a_vertical_divider_draws_a_line_down_its_middle() {
    let s = light_color_scheme();
    let found = line(s, || {
        Row!(modifier = Modifier.height(300.dp()), {
            Spacer!(modifier = Modifier.size(10.dp()));
            VerticalDivider!();
            Spacer!(modifier = Modifier.size(10.dp()));
        });
    });
    assert_eq!(found, (Offset::new(10.5, 0.0), Offset::new(10.5, 300.0), 1.0, s.outline_variant));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/DividerScreenshotTest.kt, horizontalDivider_hairlineThickness
#[test]
fn a_hairline_divider_draws_a_one_pixel_line() {
    let s = light_color_scheme();
    let found = line(s, || {
        Column!({
            Spacer!(modifier = Modifier.size(10.dp()));
            HorizontalDivider!(thickness = Dp::HAIRLINE);
            Spacer!(modifier = Modifier.size(10.dp()));
        });
    });
    // A stroke of no width: the renderers' hairline, one pixel.
    assert_eq!(found, (Offset::new(0.0, 10.0), Offset::new(400.0, 10.0), 0.0, s.outline_variant));
}
