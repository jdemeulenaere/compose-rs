//! Compose's `SurfaceTest.kt`, adapted: tonal elevation, absolute elevation, and the clickable,
//! selectable and toggleable overloads. The screenshot checks of the surface's color read the
//! display list's fills instead.

use std::cell::Cell;

use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Dp, Modifier, UnitExt};

/// The colors of the rectangles the display list fills, in drawing order.
fn rect_fills(ui: &mut ComposeTestRule) -> Vec<Color> {
    ui.host()
        .display_list()
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawRect(_, paint) => Some(paint.color),
            _ => None,
        })
        .collect()
}

thread_local! {
    static ELEVATION: Cell<Option<Dp>> = const { Cell::new(None) };
}

fn record_elevation() {
    ELEVATION.with(|cell| cell.set(Some(LOCAL_ABSOLUTE_TONAL_ELEVATION.current())));
}

fn recorded_elevation() -> Dp {
    ELEVATION.with(Cell::get).expect("composed")
}

/// The baseline light scheme with a surface tint of its own, so a tint taken from `primary`
/// shows.
fn tinted_scheme() -> ColorScheme {
    ColorScheme { surface_tint: Color::from_argb(0xFF00FF00), ..light_color_scheme() }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, noTonalElevationColorIsSetOnNonElevatedSurfaceColor
#[test]
fn a_surface_without_elevation_keeps_the_surface_color() {
    let mut ui = compose_test(|| {
        MaterialTheme!(color_scheme = tinted_scheme(), {
            Surface!(modifier = Modifier.size(10.dp()), {
                record_elevation();
            });
        });
    });
    assert_eq!(recorded_elevation(), Dp(0.0));
    assert_eq!(rect_fills(&mut ui), vec![tinted_scheme().surface]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, tonalElevationColorIsSetOnElevatedSurfaceColor
// (with a surface tint differing from the primary color, so the tint's role shows)
#[test]
fn an_elevated_surface_is_tinted_with_the_surface_tint() {
    let mut ui = compose_test(|| {
        MaterialTheme!(color_scheme = tinted_scheme(), {
            Surface!(tonal_elevation = 2.dp(), modifier = Modifier.size(10.dp()), {
                record_elevation();
            });
        });
    });
    assert_eq!(recorded_elevation(), Dp(2.0));
    let scheme = tinted_scheme();
    let alpha = (4.5 * 3.0f32.ln() + 2.0) / 100.0;
    let expected = scheme.surface_tint.with_alpha(alpha).composite_over(scheme.surface);
    assert_eq!(scheme.surface_color_at_elevation(2.dp()), expected);
    assert_eq!(rect_fills(&mut ui), vec![expected]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, tonalElevationColorIsNotSetOnNonSurfaceColor
#[test]
fn an_elevated_surface_of_another_color_is_not_tinted() {
    let green = Color::from_argb(0xFF00FF00);
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Surface!(color = green, tonal_elevation = 2.dp(), modifier = Modifier.size(10.dp()), {
                record_elevation();
            });
        });
    });
    assert_eq!(recorded_elevation(), Dp(2.0));
    assert_eq!(rect_fills(&mut ui), vec![green]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, absoluteElevationCompositionLocalIsSet
#[test]
fn nested_surfaces_add_their_elevations_up() {
    thread_local! { static OUTER: Cell<Option<Dp>> = const { Cell::new(None) }; }
    compose_test(|| {
        MaterialTheme!({
            Surface!(tonal_elevation = 2.dp(), {
                OUTER.with(|cell| cell.set(Some(LOCAL_ABSOLUTE_TONAL_ELEVATION.current())));
                Surface!(tonal_elevation = 4.dp(), {
                    record_elevation();
                });
            });
        });
    });
    assert_eq!(OUTER.with(Cell::get), Some(Dp(2.0)));
    assert_eq!(recorded_elevation(), Dp(6.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SurfaceTest.kt, contentColorSetBeforeModifier
// (compose-rs has no composed modifiers: the content color is read in the content)
#[test]
fn a_surface_provides_its_content_color() {
    thread_local! { static CONTENT: Cell<Option<Color>> = const { Cell::new(None) }; }
    compose_test(|| {
        MaterialTheme!({
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::RED), || {
                Surface!(tonal_elevation = 2.dp(), content_color = Color::BLUE, {
                    CONTENT.with(|cell| cell.set(Some(LOCAL_CONTENT_COLOR.current())));
                });
            });
        });
    });
    assert_eq!(CONTENT.with(Cell::get), Some(Color::BLUE));
}
