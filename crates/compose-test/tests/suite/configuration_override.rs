//! `DeviceConfigurationOverride`: the layout direction and dark mode of one piece of content.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_test::*;
use compose_ui::{LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, UnitExt};

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/deviceconfigurationoverride/DeviceConfigurationOverrideTest.kt, layoutDirectionOverride_toRtl_overridesLayoutDirection, layoutDirectionOverride_toLtr_overridesLayoutDirection
/// The content reads the overridden direction, and lays out in it: a row's first child is at
/// its right in right to left.
#[test]
fn a_layout_direction_override_lays_the_content_out_in_it() {
    for direction in [LayoutDirection::Rtl, LayoutDirection::Ltr] {
        let seen = Rc::new(Cell::new(None));
        let read = seen.clone();
        let mut ui = compose_test(move || {
            let read = read.clone();
            DeviceConfigurationOverride!(
                DeviceConfigurationOverride::layout_direction(direction),
                {
                    read.set(Some(LOCAL_LAYOUT_DIRECTION.current()));
                    Row!(modifier = Modifier.size((100.dp(), 10.dp())), {
                        Box!(modifier = Modifier.size(10.dp()).test_tag("first"));
                    });
                }
            );
        });
        assert_eq!(seen.get(), Some(direction));
        let left = if direction == LayoutDirection::Rtl { 90.dp() } else { 0.dp() };
        ui.on_node_with_tag("first").assert_left_position_in_root_is_equal_to(left);
    }
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/deviceconfigurationoverride/DeviceConfigurationOverrideTest.kt, darkModeOverride_toDark_overridesIsSystemInDarkMode, darkModeOverride_toLight_overridesIsSystemInDarkMode
#[test]
fn a_dark_mode_override_is_what_the_content_sees() {
    for dark in [true, false] {
        let seen = Rc::new(Cell::new(None));
        let read = seen.clone();
        compose_test(move || {
            let read = read.clone();
            DeviceConfigurationOverride!(DeviceConfigurationOverride::dark_mode(dark), {
                read.set(Some(is_system_in_dark_theme()));
            });
        });
        assert_eq!(seen.get(), Some(dark));
    }
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/deviceconfigurationoverride/DeviceConfigurationOverrideTest.kt, combiningDeviceConfigurationOverride_respectsOrder
/// Overrides combine with `then`; where both set a value, the inner one wins.
#[test]
fn combined_overrides_apply_in_order() {
    let seen = Rc::new(Cell::new(None));
    let read = seen.clone();
    compose_test(move || {
        let read = read.clone();
        let combined = DeviceConfigurationOverride::layout_direction(LayoutDirection::Rtl)
            .then(DeviceConfigurationOverride::dark_mode(true))
            .then(DeviceConfigurationOverride::layout_direction(LayoutDirection::Ltr));
        DeviceConfigurationOverride!(combined, {
            read.set(Some((LOCAL_LAYOUT_DIRECTION.current(), is_system_in_dark_theme())));
        });
    });
    assert_eq!(seen.get(), Some((LayoutDirection::Ltr, true)));
}
