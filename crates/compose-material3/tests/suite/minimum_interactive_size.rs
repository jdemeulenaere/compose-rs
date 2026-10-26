//! Compose's `InteractiveComponentSizeTest.kt`, and the `…_withoutMinimumTouchTarget` size tests
//! of `RadioButtonTest.kt` and `SwitchTest.kt`, adapted: `minimum_interactive_component_size`
//! and `LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE`. The alignment-line tests wait on alignment
//! lines in compose-ui.

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, Modifier, UnitExt};

fn size(ui: &mut ComposeTestRule, tag: &str) -> (f32, f32) {
    let bounds = ui.on_node_with_tag(tag).fetch_semantics_node().bounds;
    (bounds.width(), bounds.height())
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveComponentSizeTest.kt, minimumInteractiveComponentSize_applyWhenSmallerThanMinSize
#[test]
fn small_content_grows_to_the_minimum_size() {
    let mut ui = compose_test(|| {
        Box!(modifier = minimum_interactive_component_size(Modifier.test_tag("box")).size(24.dp()));
    });
    assert_eq!(size(&mut ui, "box"), (48.0, 48.0));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/InteractiveComponentSizeTest.kt, minimumInteractiveComponentSize_sizeRemainsWhenLargerThanMinSize
#[test]
fn large_content_keeps_its_size() {
    let mut ui = compose_test(|| {
        Box!(modifier = minimum_interactive_component_size(Modifier.test_tag("box")).size(96.dp()));
    });
    assert_eq!(size(&mut ui, "box"), (96.0, 96.0));
}

/// The local turns the minimum off or changes it below it.
#[test]
fn the_local_changes_or_turns_off_the_minimum() {
    let mut ui = compose_test(|| {
        Column!({
            for (tag, minimum) in
                [("none", Dp(0.0)), ("unspecified", Dp::UNSPECIFIED), ("larger", Dp(56.0))]
            {
                CompositionLocalProvider(
                    LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(minimum),
                    || {
                        Box!(
                            modifier = minimum_interactive_component_size(Modifier.test_tag(tag))
                                .size(24.dp())
                        );
                    },
                );
            }
        });
    });
    assert_eq!(size(&mut ui, "none"), (24.0, 24.0));
    assert_eq!(size(&mut ui, "unspecified"), (24.0, 24.0));
    assert_eq!(size(&mut ui, "larger"), (56.0, 56.0));
}

/// A component in a box tagged "component", with the minimum at 48 dp or off.
fn sized(minimum: bool, content: impl Fn() + 'static) -> (f32, f32) {
    let content = std::rc::Rc::new(content);
    let mut ui = compose_test(move || {
        let content = content.clone();
        MaterialTheme!({
            let content = content.clone();
            let minimum = if minimum { 48.dp() } else { 0.dp() };
            CompositionLocalProvider(
                LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(minimum),
                move || {
                    let content = content.clone();
                    Box!(modifier = Modifier.test_tag("component"), { content() });
                },
            );
        });
    });
    size(&mut ui, "component")
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenSelected_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenSelected_withoutMinimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenNotSelected_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenNotSelected_withoutMinimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenSelected_notClickable_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenSelected_notClickable_withoutMinimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenNotSelected_notClickable_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/RadioButtonTest.kt, radioButton_materialSizes_whenNotSelected_notClickable_withoutMinimumTouchTarget
#[test]
fn a_radio_button_is_48_dp_only_when_clickable_with_the_minimum() {
    for selected in [true, false] {
        for clickable in [true, false] {
            for minimum in [true, false] {
                let found = sized(minimum, move || {
                    let on_click = if clickable { Some(|| {}) } else { None };
                    RadioButton!(selected, on_click);
                });
                let expected = if clickable && minimum { 48.0 } else { 24.0 };
                assert_eq!(found, (expected, expected), "{selected} {clickable} {minimum}");
            }
        }
    }
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenChecked_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenChecked_withoutMinimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenUnchecked_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenUnchecked_withoutMinimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenChecked_notClickable_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenChecked_notClickable_withoutMinimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenUnchecked_notClickable_minimumTouchTarget
// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_materialSizes_whenUnchecked_notClickable_withoutMinimumTouchTarget
#[test]
fn a_switch_is_its_track_unless_clickable_with_the_minimum() {
    for checked in [true, false] {
        for clickable in [true, false] {
            for minimum in [true, false] {
                let found = sized(minimum, move || {
                    let on_checked_change = if clickable { Some(|_: bool| {}) } else { None };
                    Switch!(checked, on_checked_change, enabled = false);
                });
                if clickable && minimum {
                    assert!(found.0 >= 48.0 && found.1 >= 48.0, "{found:?}");
                } else {
                    assert_eq!(found, (52.0, 32.0), "{checked} {clickable} {minimum}");
                }
            }
        }
    }
}
