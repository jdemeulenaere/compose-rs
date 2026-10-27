//! Compose's `IconButtonScreenshotTest.kt`, its icon toggle buttons' goldens adapted: each
//! checked as what the display list draws, in the light and dark schemes: the 40 dp circle's
//! fill (none for a transparent container), its outline, and the icon's color. The outlined
//! icons are material-icons-core's filled ones (compose-rs has the filled set only).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Modifier, PaintStyle, UnitExt};

/// What a toggle draws: its container's fill, its outline's color, its icon's color.
#[derive(PartialEq, Debug, Default)]
struct Drawn {
    container: Option<Color>,
    outline: Option<Color>,
    icon: Option<Color>,
}

fn drawn(ui: &mut ComposeTestRule) -> Drawn {
    let mut found = Drawn::default();
    for op in &ui.host().display_list().ops {
        match op {
            DrawOp::DrawRoundRect(rect, radii, paint) if rect.width() <= 40.0 && radii[0] > 0.0 => {
                match paint.style {
                    PaintStyle::Fill => found.container = Some(paint.color),
                    PaintStyle::Stroke(_) => found.outline = Some(paint.color),
                }
            }
            DrawOp::DrawPath { paint, .. } => found.icon = Some(paint.color),
            _ => {}
        }
    }
    found
}

#[derive(Clone, Copy)]
enum Kind {
    Standard,
    Filled,
    Tonal,
    Outlined,
}

fn toggle(scheme: ColorScheme, kind: Kind, checked: bool, enabled: bool) -> Drawn {
    let mut ui = compose_test(move || {
        MaterialTheme!(color_scheme = scheme, {
            let icon = || Icon!(icons::filled::favorite(), Some("Localized description"));
            match kind {
                Kind::Standard => IconToggleButton!(checked, |_| {}, enabled = enabled, { icon() }),
                Kind::Filled => {
                    FilledIconToggleButton!(checked, |_| {}, enabled = enabled, { icon() })
                }
                Kind::Tonal => {
                    FilledTonalIconToggleButton!(checked, |_| {}, enabled = enabled, { icon() })
                }
                Kind::Outlined => {
                    OutlinedIconToggleButton!(checked, |_| {}, enabled = enabled, { icon() })
                }
            }
        });
    });
    drawn(&mut ui)
}

/// The default content color outside a surface: Compose's `LocalContentColor` default, black.
const CONTENT: Color = Color::BLACK;

fn expect(container: Option<Color>, outline: Option<Color>, icon: Color) -> Drawn {
    Drawn { container, outline, icon: Some(icon) }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, iconToggleButton_lightTheme
#[test]
fn an_unchecked_icon_toggle_button_is_its_icon() {
    assert_eq!(
        toggle(light_color_scheme(), Kind::Standard, false, true),
        expect(None, None, CONTENT)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, iconToggleButton_darkTheme
#[test]
fn an_unchecked_dark_icon_toggle_button_is_its_icon() {
    assert_eq!(
        toggle(dark_color_scheme(), Kind::Standard, false, true),
        expect(None, None, CONTENT)
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, iconToggleButton_checked_lightTheme
#[test]
fn a_checked_icon_toggle_button_is_its_icon_in_the_primary_color() {
    let s = light_color_scheme();
    assert_eq!(toggle(s, Kind::Standard, true, true), expect(None, None, s.primary));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, iconToggleButton_checked_darkTheme
#[test]
fn a_checked_dark_icon_toggle_button_is_its_icon_in_the_primary_color() {
    let s = dark_color_scheme();
    assert_eq!(toggle(s, Kind::Standard, true, true), expect(None, None, s.primary));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, iconToggleButton_largeContentClipped
#[test]
fn an_icon_toggle_buttons_large_content_is_clipped_to_its_circle() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            IconToggleButton!(true, |_| {}, {
                Box!(modifier = Modifier.size(100.dp()).background(Color::BLUE));
            });
        });
    });
    let list = ui.host().display_list();
    let clip = list.ops.iter().position(|op| {
        matches!(op, DrawOp::ClipRoundRect(rect, radii, ..) if rect.width() == 40.0 && radii[0] == 20.0)
    });
    let blue = list
        .ops
        .iter()
        .position(|op| matches!(op, DrawOp::DrawRect(_, paint) if paint.color == Color::BLUE));
    assert!(
        matches!((clip, blue), (Some(clip), Some(blue)) if clip < blue),
        "{}",
        ui.dump_display_list()
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledIconToggleButton_lightTheme
#[test]
fn an_unchecked_filled_icon_toggle_button_is_on_the_surface_container() {
    let s = light_color_scheme();
    let drawn = toggle(s, Kind::Filled, false, true);
    assert_eq!(drawn, expect(Some(s.surface_container), None, s.on_surface_variant));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledIconToggleButton_lightTheme_disabled
#[test]
fn a_disabled_filled_icon_toggle_button_is_faded() {
    let s = light_color_scheme();
    let drawn = toggle(s, Kind::Filled, false, false);
    let expected = expect(Some(s.on_surface.with_alpha(0.1)), None, s.on_surface.with_alpha(0.38));
    assert_eq!(drawn, expected);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledIconToggleButton_darkTheme
#[test]
fn an_unchecked_dark_filled_icon_toggle_button_is_on_the_surface_container() {
    let s = dark_color_scheme();
    let drawn = toggle(s, Kind::Filled, false, true);
    assert_eq!(drawn, expect(Some(s.surface_container), None, s.on_surface_variant));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledIconToggleButton_checked_lightTheme
#[test]
fn a_checked_filled_icon_toggle_button_is_on_the_primary_color() {
    let s = light_color_scheme();
    assert_eq!(toggle(s, Kind::Filled, true, true), expect(Some(s.primary), None, s.on_primary));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledIconToggleButton_checked_darkTheme
#[test]
fn a_checked_dark_filled_icon_toggle_button_is_on_the_primary_color() {
    let s = dark_color_scheme();
    assert_eq!(toggle(s, Kind::Filled, true, true), expect(Some(s.primary), None, s.on_primary));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledTonalIconToggleButton_lightTheme
#[test]
fn an_unchecked_tonal_icon_toggle_button_is_on_the_secondary_container() {
    let s = light_color_scheme();
    let drawn = toggle(s, Kind::Tonal, false, true);
    assert_eq!(drawn, expect(Some(s.secondary_container), None, s.on_secondary_container));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledTonalIconToggleButton_darkTheme
#[test]
fn an_unchecked_dark_tonal_icon_toggle_button_is_on_the_secondary_container() {
    let s = dark_color_scheme();
    let drawn = toggle(s, Kind::Tonal, false, true);
    assert_eq!(drawn, expect(Some(s.secondary_container), None, s.on_secondary_container));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledTonalIconToggleButton_checked_lightTheme
#[test]
fn a_checked_tonal_icon_toggle_button_is_on_the_secondary_color() {
    let s = light_color_scheme();
    assert_eq!(toggle(s, Kind::Tonal, true, true), expect(Some(s.secondary), None, s.on_secondary));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, filledTonalIconToggleButton_checked_darkTheme
#[test]
fn a_checked_dark_tonal_icon_toggle_button_is_on_the_secondary_color() {
    let s = dark_color_scheme();
    assert_eq!(toggle(s, Kind::Tonal, true, true), expect(Some(s.secondary), None, s.on_secondary));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, outlinedIconToggleButton_lightTheme
#[test]
fn an_unchecked_outlined_icon_toggle_button_is_outlined() {
    let drawn = toggle(light_color_scheme(), Kind::Outlined, false, true);
    assert_eq!(drawn, expect(None, Some(CONTENT), CONTENT));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, outlinedIconToggleButton_darkTheme
#[test]
fn an_unchecked_dark_outlined_icon_toggle_button_is_outlined() {
    let drawn = toggle(dark_color_scheme(), Kind::Outlined, false, true);
    assert_eq!(drawn, expect(None, Some(CONTENT), CONTENT));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, outlinedIconToggleButton_checked_lightTheme
#[test]
fn a_checked_outlined_icon_toggle_button_is_on_the_inverse_surface_without_an_outline() {
    let s = light_color_scheme();
    let drawn = toggle(s, Kind::Outlined, true, true);
    assert_eq!(drawn, expect(Some(s.inverse_surface), None, s.inverse_on_surface));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonScreenshotTest.kt, outlinedIconToggleButton_checked_darkTheme
#[test]
fn a_checked_dark_outlined_icon_toggle_button_is_on_the_inverse_surface_without_an_outline() {
    let s = dark_color_scheme();
    let drawn = toggle(s, Kind::Outlined, true, true);
    assert_eq!(drawn, expect(Some(s.inverse_surface), None, s.inverse_on_surface));
}
