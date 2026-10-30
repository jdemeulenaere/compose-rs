//! Compose's `IconButtonTest.kt`, its icon toggle buttons' tests adapted: the standard,
//! filled, filled tonal and outlined toggles' sizes (with and without the touch target),
//! semantics, icon positions and default colors. Sizes are the bounds of a box around the
//! button (`setMaterialContentForSizeAssertions`); the touch bounds' assertions aren't in
//! compose-test.
//!
//! Not ported: the vibrant colors and `shapes` overloads (feature:button-shapes),
//! `iconToggleButton_clickInMinimumTouchTarget` (ui's feature:hit-test-extensions).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Modifier, Role, UnitExt};

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Standard,
    Filled,
    Tonal,
    Outlined,
}

/// A toggle button of `kind`, `checked`, enabled or not, its content `content`.
fn toggle(
    kind: Kind,
    checked: bool,
    on_checked_change: impl Fn(bool) + Clone + 'static,
    enabled: bool,
    content: impl Fn() + Clone + 'static,
) {
    match kind {
        Kind::Standard => {
            IconToggleButton!(checked, on_checked_change, enabled = enabled, { content() })
        }
        Kind::Filled => {
            FilledIconToggleButton!(checked, on_checked_change, enabled = enabled, { content() })
        }
        Kind::Tonal => {
            FilledTonalIconToggleButton!(checked, on_checked_change, enabled = enabled, {
                content()
            })
        }
        Kind::Outlined => {
            OutlinedIconToggleButton!(checked, on_checked_change, enabled = enabled, { content() })
        }
    }
}

fn favorite() {
    Icon!(icons::filled::favorite(), Some("Localized description"));
}

/// The size of a checked toggle of `kind`, with the touch target (48 dp) or without (0 dp).
fn size(kind: Kind, touch_target: bool) -> (f32, f32) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            let minimum = if touch_target { 48.dp() } else { 0.dp() };
            CompositionLocalProvider(
                LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(minimum),
                || {
                    Box!(modifier = Modifier.test_tag("container"), {
                        toggle(kind, true, |_| {}, true, favorite);
                    });
                },
            );
        });
    });
    let bounds = ui.on_node_with_tag("container").fetch_semantics_snapshot().bounds;
    (bounds.width(), bounds.height())
}

/// A click toggles it on; it's a checkbox, enabled, off at first.
fn check_semantics(kind: Kind) {
    let mut ui = compose_test(move || {
        let checked = remember(|| mutable_state_of(false));
        MaterialTheme!({
            toggle(kind, checked.get(), move |now| checked.set(now), true, favorite);
        });
    });
    let node = ui.on_node(is_toggleable()).fetch_semantics_snapshot();
    assert_eq!(
        node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
        Some(Role::Checkbox)
    );
    ui.on_node(is_toggleable()).assert_is_enabled().assert_is_off().perform_click().assert_is_on();
}

/// Disabled, it's a disabled checkbox, off.
fn check_disabled_semantics(kind: Kind) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            toggle(kind, false, |_| {}, false, || {});
        });
    });
    let node = ui.on_node(is_toggleable()).fetch_semantics_snapshot();
    assert_eq!(
        node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
        Some(Role::Checkbox)
    );
    ui.on_node(is_toggleable()).assert_is_not_enabled().assert_is_off();
}

/// A `width` by `height` icon is centered in the 48 dp button.
fn check_icon_position(kind: Kind, width: f32, height: f32) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Box!({
                toggle(
                    kind,
                    false,
                    |_| {},
                    true,
                    move || {
                        Box!(modifier = Modifier.size((width.dp(), height.dp())).test_tag("icon"));
                    },
                );
            });
        });
    });
    ui.on_node_unmerged(has_test_tag("icon"))
        .assert_left_position_in_root_is_equal_to(((48.0 - width) / 2.0).dp())
        .assert_top_position_in_root_is_equal_to(((48.0 - height) / 2.0).dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_size
#[test]
fn an_icon_toggle_button_is_48_dp() {
    assert_eq!(size(Kind::Standard, true), (48.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_sizeWithoutMinTargetEnforcement
#[test]
fn an_icon_toggle_button_without_the_touch_target_is_40_dp() {
    assert_eq!(size(Kind::Standard, false), (40.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_defaultSemantics
#[test]
fn an_icon_toggle_button_is_a_checkbox_toggled_by_a_click() {
    check_semantics(Kind::Standard);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_disabledSemantics
#[test]
fn a_disabled_icon_toggle_button_is_a_disabled_checkbox() {
    check_disabled_semantics(Kind::Standard);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_materialIconSize_iconPositioning
#[test]
fn an_icon_toggle_button_centers_its_icon() {
    check_icon_position(Kind::Standard, 24.0, 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_customIconSize_iconPositioning
#[test]
fn an_icon_toggle_button_centers_an_icon_of_any_size() {
    check_icon_position(Kind::Standard, 36.0, 14.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, iconToggleButton_defaultLocalContentColors
#[test]
fn an_icon_toggle_button_takes_the_content_color_and_the_primary_when_checked() {
    thread_local! {
        static READ: std::cell::Cell<Option<(IconToggleButtonColors, Color)>> =
            const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            let colors = IconButtonDefaults::icon_toggle_button_colors();
            READ.with(|read| read.set(Some((colors, LOCAL_CONTENT_COLOR.current()))));
        });
    });
    let (colors, content) = READ.with(std::cell::Cell::get).expect("composed");
    assert_eq!(
        colors,
        IconToggleButtonColors {
            container_color: Color::TRANSPARENT,
            content_color: content,
            disabled_container_color: Color::TRANSPARENT,
            disabled_content_color: content.with_alpha(0.38),
            checked_container_color: Color::TRANSPARENT,
            checked_content_color: light_color_scheme().primary,
        }
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_size
#[test]
fn a_filled_icon_toggle_button_is_48_dp() {
    assert_eq!(size(Kind::Filled, true), (48.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_sizeWithoutMinTargetEnforcement
#[test]
fn a_filled_icon_toggle_button_without_the_touch_target_is_40_dp() {
    assert_eq!(size(Kind::Filled, false), (40.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_defaultSemantics
#[test]
fn a_filled_icon_toggle_button_is_a_checkbox_toggled_by_a_click() {
    check_semantics(Kind::Filled);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_disabledSemantics
#[test]
fn a_disabled_filled_icon_toggle_button_is_a_disabled_checkbox() {
    check_disabled_semantics(Kind::Filled);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_materialIconSize_iconPositioning
#[test]
fn a_filled_icon_toggle_button_centers_its_icon() {
    check_icon_position(Kind::Filled, 24.0, 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_customIconSize_iconPositioning
#[test]
fn a_filled_icon_toggle_button_centers_an_icon_of_any_size() {
    check_icon_position(Kind::Filled, 36.0, 14.0);
}

/// The colors `colors` gives in the light theme.
fn read_colors(colors: fn() -> IconToggleButtonColors) -> IconToggleButtonColors {
    thread_local! {
        static READ: std::cell::Cell<Option<IconToggleButtonColors>> =
            const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(move || {
        MaterialTheme!({
            READ.with(|read| read.set(Some(colors())));
        });
    });
    READ.with(std::cell::Cell::get).expect("composed")
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledIconToggleButton_defaultColors
#[test]
fn a_filled_icon_toggle_button_is_the_surface_container_then_the_primary_color() {
    let s = light_color_scheme();
    assert_eq!(
        read_colors(IconButtonDefaults::filled_icon_toggle_button_colors),
        IconToggleButtonColors {
            container_color: s.surface_container,
            content_color: s.on_surface_variant,
            disabled_container_color: s.on_surface.with_alpha(0.1),
            disabled_content_color: s.on_surface.with_alpha(0.38),
            checked_container_color: s.primary,
            checked_content_color: s.content_color_for(s.primary),
        }
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, filledTonalIconToggleButton_defaultColors
#[test]
fn a_filled_tonal_icon_toggle_button_is_the_secondary_container_then_the_secondary_color() {
    let s = light_color_scheme();
    assert_eq!(
        read_colors(IconButtonDefaults::filled_tonal_icon_toggle_button_colors),
        IconToggleButtonColors {
            container_color: s.secondary_container,
            content_color: s.content_color_for(s.secondary_container),
            disabled_container_color: s.on_surface.with_alpha(0.1),
            disabled_content_color: s.on_surface.with_alpha(0.38),
            checked_container_color: s.secondary,
            checked_content_color: s.on_secondary,
        }
    );
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_size
#[test]
fn an_outlined_icon_toggle_button_is_48_dp() {
    assert_eq!(size(Kind::Outlined, true), (48.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_sizeWithoutMinTargetEnforcement
#[test]
fn an_outlined_icon_toggle_button_without_the_touch_target_is_40_dp() {
    assert_eq!(size(Kind::Outlined, false), (40.0, 40.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_defaultSemantics
#[test]
fn an_outlined_icon_toggle_button_is_a_checkbox_toggled_by_a_click() {
    check_semantics(Kind::Outlined);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_disabledSemantics
#[test]
fn a_disabled_outlined_icon_toggle_button_is_a_disabled_checkbox() {
    check_disabled_semantics(Kind::Outlined);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_materialIconSize_iconPositioning
#[test]
fn an_outlined_icon_toggle_button_centers_its_icon() {
    check_icon_position(Kind::Outlined, 24.0, 24.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_customIconSize_iconPositioning
#[test]
fn an_outlined_icon_toggle_button_centers_an_icon_of_any_size() {
    check_icon_position(Kind::Outlined, 36.0, 14.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/IconButtonTest.kt, outlinedIconToggleButton_useLocalContentColors
#[test]
fn the_outlined_colors_follow_the_content_color() {
    thread_local! {
        static READ: std::cell::Cell<Option<(IconButtonColors, IconButtonColors)>> =
            const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            let mut blue = None;
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::BLUE), || {
                blue = Some(IconButtonDefaults::outlined_icon_button_colors());
            });
            CompositionLocalProvider(LOCAL_CONTENT_COLOR.provides(Color::RED), || {
                let red = IconButtonColors {
                    container_color: Color::GREEN,
                    ..IconButtonDefaults::outlined_icon_button_colors()
                };
                READ.with(|read| read.set(Some((blue.expect("read"), red))));
            });
        });
    });
    let (blue, red) = READ.with(std::cell::Cell::get).expect("composed");
    assert_eq!(blue.content_color, Color::BLUE);
    assert_eq!(red.container_color, Color::GREEN);
    assert_eq!(red.content_color, Color::RED);
    assert_eq!(red.disabled_content_color, Color::RED.with_alpha(0.38));
}

/// The outlined toggle's outline goes when it's checked; its checked container is the inverse
/// surface (`outlinedIconToggleButtonBorder`, `outlinedIconToggleButtonColors`).
#[test]
fn an_outlined_icon_toggle_button_loses_its_outline_when_checked() {
    thread_local! {
        static READ: std::cell::Cell<Option<(bool, bool, IconToggleButtonColors)>> =
            const { std::cell::Cell::new(None) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            let unchecked = IconButtonDefaults::outlined_icon_toggle_button_border(true, false);
            let checked = IconButtonDefaults::outlined_icon_toggle_button_border(true, true);
            let colors = IconButtonDefaults::outlined_icon_toggle_button_colors();
            READ.with(|read| read.set(Some((unchecked.is_some(), checked.is_some(), colors))));
        });
    });
    let (unchecked, checked, colors) = READ.with(std::cell::Cell::get).expect("composed");
    assert!(unchecked && !checked);
    let s = light_color_scheme();
    assert_eq!(
        (colors.checked_container_color, colors.checked_content_color),
        (s.inverse_surface, s.inverse_on_surface)
    );
}

/// The tonal toggle, which Compose's tests only check the colors of, is a checkbox too.
#[test]
fn a_filled_tonal_icon_toggle_button_is_a_checkbox_toggled_by_a_click() {
    check_semantics(Kind::Tonal);
    check_disabled_semantics(Kind::Tonal);
}
