//! Compose's `MenuScreenshotTest.kt`, its plain menus (the segmented ones need the expressive
//! menus), as checks of what the goldens show: the container (its shape, color, shadow and
//! border), each item's text and icon colors, the divider; enabled, disabled and restyled.

#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;

use compose_foundation::BorderStroke;
use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Dp, Modifier, PaintStyle, Shape, UnitExt};

thread_local! {
    /// The content color each slot saw, by slot name.
    static SEEN: RefCell<HashMap<String, Color>> = RefCell::new(HashMap::new());
}

fn see(slot: String) {
    let color = LOCAL_CONTENT_COLOR.current();
    SEEN.with(|seen| seen.borrow_mut().insert(slot, color));
}

fn seen(slot: &str) -> Color {
    SEEN.with(|seen| *seen.borrow().get(slot).unwrap_or_else(|| panic!("no {slot}")))
}

#[derive(Clone)]
struct Appearance {
    shape: Option<Shape>,
    container_color: Option<Color>,
    tonal_elevation: Dp,
    shadow_elevation: Dp,
    border: Option<BorderStroke>,
}

const DEFAULT: Appearance = Appearance {
    shape: None,
    container_color: None,
    tonal_elevation: MenuDefaults::TONAL_ELEVATION,
    shadow_elevation: MenuDefaults::SHADOW_ELEVATION,
    border: None,
};

/// An item whose text and icons record the content color they're given.
#[composable]
fn RecordingItem(
    label: &'static str,
    enabled: bool,
    leading: bool,
    trailing: Option<&'static str>,
) {
    let leading_icon = leading.then_some(move || {
        see(format!("{label} leading"));
        Icon!(icons::filled::edit(), None);
    });
    // A trailing text, or the lock icon.
    let trailing_icon = trailing.map(|trailing| {
        move || {
            see(format!("{label} trailing"));
            if trailing == "Lock" {
                Icon!(icons::filled::lock(), None);
            } else {
                Text!("{}", trailing);
            }
        }
    });
    DropdownMenuItem!(
        {
            see(format!("{label} text"));
            Text!("{}", label);
        },
        || {},
        enabled = enabled,
        leading_icon = leading_icon,
        trailing_icon = trailing_icon,
    );
}

/// The goldens' menu: "Edit" with a leading icon, "Settings" with an "F11" trailing text, a
/// divider, "Send Feedback" with both icons.
fn test_menu(scheme: ColorScheme, enabled: bool, appearance: Appearance) -> ComposeTestRule {
    SEEN.with(|seen| seen.borrow_mut().clear());
    let mut ui = compose_test(move || {
        let appearance = appearance.clone();
        MaterialTheme!(color_scheme = scheme, {
            let appearance = appearance.clone();
            Box!(modifier = Modifier.test_tag("dropdown_menu").padding(20.dp()), {
                let appearance = appearance.clone();
                let shape = appearance.shape.unwrap_or_else(MenuDefaults::shape);
                let container =
                    appearance.container_color.unwrap_or_else(MenuDefaults::container_color);
                DropdownMenu!(
                    true,
                    || {},
                    shape = shape,
                    container_color = container,
                    tonal_elevation = appearance.tonal_elevation,
                    shadow_elevation = appearance.shadow_elevation,
                    border = appearance.border,
                    {
                        RecordingItem("Edit", enabled, true, None);
                        RecordingItem("Settings", enabled, true, Some("F11"));
                        HorizontalDivider!();
                        RecordingItem("Send Feedback", enabled, true, Some("Lock"));
                    }
                );
            });
        });
    });
    ui.wait_for_idle();
    ui
}

fn ops(ui: &mut ComposeTestRule) -> Vec<DrawOp> {
    ui.host().display_list().ops.clone()
}

/// The container: a rounded rectangle of `radius` in `color`.
fn assert_container(ui: &mut ComposeTestRule, radius: f32, color: Color) {
    let found = ops(ui).iter().any(|op| {
        matches!(op, DrawOp::DrawRoundRect(_, radii, paint)
            if *radii == [radius; 4] && paint.color == color && paint.style == PaintStyle::Fill)
    });
    assert!(found, "no {radius} px container in {color:?}: {}", ui.dump_display_list());
}

fn shadows(ui: &mut ComposeTestRule) -> Vec<f32> {
    ops(ui)
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawShadow { elevation, .. } => Some(*elevation),
            _ => None,
        })
        .collect()
}

/// The colors of the paths drawn (the icons).
fn path_colors(ui: &mut ComposeTestRule) -> Vec<Color> {
    ops(ui)
        .iter()
        .filter_map(|op| match op {
            DrawOp::DrawPath { paint, .. } => Some(paint.color),
            _ => None,
        })
        .collect()
}

fn divider_color(ui: &mut ComposeTestRule) -> Color {
    let ops = ops(ui);
    let line = ops.iter().find_map(|op| match op {
        DrawOp::DrawLine(_, _, paint) => Some(paint.color),
        _ => None,
    });
    line.expect("a divider")
}

/// An enabled menu's colors: on the surface container, the texts in the on-surface color, the
/// icons (leading and trailing) in the on-surface variant, the divider in the outline variant.
fn check_enabled(scheme: ColorScheme) {
    let mut ui = test_menu(scheme, true, DEFAULT);
    assert_container(&mut ui, 4.0, scheme.surface_container);
    assert_eq!(shadows(&mut ui), vec![3.0]);
    for label in ["Edit", "Settings", "Send Feedback"] {
        assert_eq!(seen(&format!("{label} text")), scheme.on_surface, "{label}");
        assert_eq!(seen(&format!("{label} leading")), scheme.on_surface_variant, "{label}");
    }
    assert_eq!(seen("Settings trailing"), scheme.on_surface_variant);
    assert_eq!(seen("Send Feedback trailing"), scheme.on_surface_variant);
    assert_eq!(path_colors(&mut ui), vec![scheme.on_surface_variant; 4]);
    assert_eq!(divider_color(&mut ui), scheme.outline_variant);
    ui.on_node_with_text("F11").assert_exists();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuScreenshotTest.kt, dropdownMenu_lightTheme
#[test]
fn the_light_menu_golden() {
    check_enabled(light_color_scheme());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuScreenshotTest.kt, dropdownMenu_darkTheme
#[test]
fn the_dark_menu_golden() {
    check_enabled(dark_color_scheme());
}

/// A disabled menu's texts and icons: the on-surface color at 38 %.
fn check_disabled(scheme: ColorScheme) {
    let mut ui = test_menu(scheme, false, DEFAULT);
    let disabled = scheme.on_surface.with_alpha(0.38);
    assert_container(&mut ui, 4.0, scheme.surface_container);
    for label in ["Edit", "Settings", "Send Feedback"] {
        assert_eq!(seen(&format!("{label} text")), disabled, "{label}");
        assert_eq!(seen(&format!("{label} leading")), disabled, "{label}");
    }
    assert_eq!(seen("Settings trailing"), disabled);
    assert_eq!(path_colors(&mut ui), vec![disabled; 4]);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuScreenshotTest.kt, dropdownMenu_disabled_lightTheme
#[test]
fn the_light_disabled_menu_golden() {
    check_disabled(light_color_scheme());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuScreenshotTest.kt, dropdownMenu_disabled_darkTheme
#[test]
fn the_dark_disabled_menu_golden() {
    check_disabled(dark_color_scheme());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuScreenshotTest.kt, dropdownMenu_customAppearance
#[test]
fn a_restyled_menu_golden() {
    let appearance = Appearance {
        shape: Some(compose_foundation::CutCornerShape(12.dp())),
        container_color: Some(Color::YELLOW),
        tonal_elevation: 0.dp(),
        shadow_elevation: 0.dp(),
        border: Some(BorderStroke::new(1.dp(), Color::BLACK)),
    };
    let mut ui = test_menu(light_color_scheme(), true, appearance);
    // Cut corners are a path: the container's, and the border stroked inside it (a 1 px stroke
    // twice as wide, clipped to the path, as Compose's `drawGenericBorder`).
    let container = ops(&mut ui).iter().any(|op| {
        matches!(op, DrawOp::DrawPath { paint, .. }
            if paint.color == Color::YELLOW && paint.style == PaintStyle::Fill)
    });
    assert!(container, "no cut container in yellow: {}", ui.dump_display_list());
    assert!(shadows(&mut ui).is_empty(), "no shadow");
    let border = ops(&mut ui).iter().any(|op| {
        matches!(op, DrawOp::DrawPath { paint, .. }
            if paint.color == Color::BLACK && paint.style == PaintStyle::Stroke(2.0))
    });
    assert!(border, "a 1 px black border: {}", ui.dump_display_list());
}
