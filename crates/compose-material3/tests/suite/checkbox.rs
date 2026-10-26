//! Compose's `CheckboxTest.kt`, adapted: semantics, toggling, untoggleable checkboxes merging
//! into what's around them, the Material sizes of `Checkbox` and `TriStateCheckbox` with and
//! without the touch target and with thick and thin strokes, and a click in the touch target.
//! Compose runs each test with its `isCheckboxStylingFixEnabled` flag on and off; compose-rs is
//! Compose's default, the flag off (a 20 dp box in 2 dp of padding).
//!
//! Not ported: `checkBoxTest_clickInMinimumTouchTarget` (a click outside a 2 dp checkbox, in its
//! 48 dp touch target: compose-ui hit tests a node's bounds only, ui's feature:hit-test-extensions).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Dp, Modifier, Role, Stroke, ToggleableState, UnitExt};

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_defaultSemantics
#[test]
fn a_checkbox_is_an_enabled_checkbox_on_or_off() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                Checkbox!(false, Some(|_| {}), modifier = Modifier.test_tag("unchecked"));
                Checkbox!(true, Some(|_| {}), modifier = Modifier.test_tag("checked"));
            });
        });
    });
    for (tag, on) in [("unchecked", false), ("checked", true)] {
        let node = ui.on_node_with_tag(tag).fetch_semantics_node();
        assert_eq!(node.config.role, Some(Role::Checkbox));
        ui.on_node_with_tag(tag).assert_is_enabled();
        if on {
            ui.on_node_with_tag(tag).assert_is_on();
        } else {
            ui.on_node_with_tag(tag).assert_is_off();
        }
    }
}

#[composable]
fn Toggling() {
    let checked = remember(|| mutable_state_of(false));
    MaterialTheme!({
        Checkbox!(
            checked.get(),
            Some(move |now| checked.set(now)),
            modifier = Modifier.test_tag("box")
        );
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_toggle
#[test]
fn a_click_checks_it() {
    let mut ui = compose_test(Toggling);
    ui.on_node_with_tag("box").assert_is_off().perform_click().assert_is_on();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_toggle_twice
#[test]
fn a_second_click_unchecks_it() {
    let mut ui = compose_test(Toggling);
    ui.on_node_with_tag("box")
        .assert_is_off()
        .perform_click()
        .assert_is_on()
        .perform_click()
        .assert_is_off();
}

/// Focused by its own semantics: Compose's `semantics { focused = true }` defines the key
/// `isFocusable` looks for; compose-rs's matcher reads `focusable`.
fn focused(modifier: Modifier) -> Modifier {
    modifier.semantics(|config| {
        config.focusable = true;
        config.focused = true;
    })
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_untoggleable_whenEmptyLambda
#[test]
fn a_disabled_checkbox_with_a_callback_is_its_own_node() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.semantics_with(true, |_| {}).test_tag("parent"), {
                Checkbox!(
                    false,
                    Some(|_| {}),
                    enabled = false,
                    modifier = focused(Modifier.test_tag("box")),
                );
            });
        });
    });
    ui.on_node_with_tag("box").assert_has_click_action();
    // Not merged into its parent.
    ui.on_node_with_tag("parent").assert(is_not_focusable());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_untoggleableAndMergeable_whenNullLambda
#[test]
fn a_checkbox_without_a_callback_merges_into_its_parent() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.semantics_with(true, |_| {}).test_tag("parent"), {
                Checkbox!(false, None::<fn(bool)>, modifier = focused(Modifier));
            });
        });
    });
    ui.on_node_with_tag("parent").assert_has_no_click_action().assert(is_focusable());
}

/// The size of a disabled `TriStateCheckbox` in `state`, clickable or not, with the touch
/// target or not (`LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE` 48 or 0 dp) and an optional stroke
/// width, as the test's `materialSizeTestForValue` (in a wrapping box, as
/// `setMaterialContentForSizeAssertions`).
fn material_size(
    state: ToggleableState,
    clickable: bool,
    touch_target: bool,
    stroke: Option<Dp>,
) -> (f32, f32) {
    let mut ui = compose_test(move || {
        MaterialTheme!({
            let minimum = if touch_target { 48.dp() } else { 0.dp() };
            CompositionLocalProvider(
                LOCAL_MINIMUM_INTERACTIVE_COMPONENT_SIZE.provides(minimum),
                || {
                    Box!(modifier = Modifier.test_tag("container"), {
                        let stroke = stroke.map(|width| Stroke::new(width.0));
                        TriStateCheckbox!(
                            state,
                            clickable.then_some(|| {}),
                            enabled = false,
                            checkmark_stroke = stroke,
                            outline_stroke = stroke,
                        );
                    });
                },
            );
        });
    });
    let bounds = ui.on_node_with_tag("container").fetch_semantics_node().bounds;
    (bounds.width(), bounds.height())
}

fn check_size(state: ToggleableState, clickable: bool, touch_target: bool, stroke: Option<Dp>) {
    let expected = if clickable && touch_target { 48.0 } else { 2.0 * 2.0 + 20.0 };
    assert_eq!(material_size(state, clickable, touch_target, stroke), (expected, expected));
}

const ON: ToggleableState = ToggleableState::On;
const OFF: ToggleableState = ToggleableState::Off;
const INDETERMINATE: ToggleableState = ToggleableState::Indeterminate;
const THICK: Option<Dp> = Some(Dp(6.0));
const THIN: Option<Dp> = Some(Dp(2.0 / 3.0));

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenChecked_minimumTouchTarget
#[test]
fn a_clickable_checked_checkbox_is_48_dp() {
    check_size(ON, true, true, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenChecked_withoutMinimumTouchTarget
#[test]
fn a_clickable_checked_checkbox_without_the_touch_target_is_24_dp() {
    check_size(ON, true, false, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenUnchecked_minimumTouchTarget
#[test]
fn a_clickable_unchecked_checkbox_is_48_dp() {
    check_size(OFF, true, true, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenUnchecked_withoutMinimumTouchTarget
#[test]
fn a_clickable_unchecked_checkbox_without_the_touch_target_is_24_dp() {
    check_size(OFF, true, false, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenIndeterminate_minimumTouchTarget
#[test]
fn a_clickable_indeterminate_checkbox_is_48_dp() {
    check_size(INDETERMINATE, true, true, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenIndeterminate_withoutMinimumTouchTarget
#[test]
fn a_clickable_indeterminate_checkbox_without_the_touch_target_is_24_dp() {
    check_size(INDETERMINATE, true, false, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenChecked_notClickable_minimumTouchTarget
#[test]
fn a_checked_checkbox_without_a_callback_has_no_touch_target() {
    check_size(ON, false, true, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenChecked_notClickable_withoutMinimumTouchTarget
#[test]
fn a_checked_checkbox_without_a_callback_or_touch_target_is_24_dp() {
    check_size(ON, false, false, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenUnchecked_notClickable_minimumTouchTarget
#[test]
fn an_unchecked_checkbox_without_a_callback_has_no_touch_target() {
    check_size(OFF, false, true, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenUnchecked_notClickable_withoutMinimumTouchTarget
#[test]
fn an_unchecked_checkbox_without_a_callback_or_touch_target_is_24_dp() {
    check_size(OFF, false, false, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenIndeterminate_notClickable_minimumTouchTarget
#[test]
fn an_indeterminate_checkbox_without_a_callback_has_no_touch_target() {
    check_size(INDETERMINATE, false, true, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenIndeterminate_notClickable_withoutMinimumTouchTarget
#[test]
fn an_indeterminate_checkbox_without_a_callback_or_touch_target_is_24_dp() {
    check_size(INDETERMINATE, false, false, None);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenChecked_withThickStroke_withoutMinimumTouchTarget
#[test]
fn a_thick_stroke_keeps_a_checked_checkbox_24_dp() {
    check_size(ON, false, false, THICK);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenChecked_withThinStroke_withoutMinimumTouchTarget
#[test]
fn a_thin_stroke_keeps_a_checked_checkbox_24_dp() {
    check_size(ON, false, false, THIN);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenUnchecked_withThickStroke_withoutMinimumTouchTarget
#[test]
fn a_thick_stroke_keeps_an_unchecked_checkbox_24_dp() {
    check_size(OFF, false, false, THICK);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenUnchecked_withThinStroke_withoutMinimumTouchTarget
#[test]
fn a_thin_stroke_keeps_an_unchecked_checkbox_24_dp() {
    check_size(OFF, false, false, THIN);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenIndeterminate_withThickStroke_withoutMinimumTouchTarget
#[test]
fn a_thick_stroke_keeps_an_indeterminate_checkbox_24_dp() {
    check_size(INDETERMINATE, false, false, THICK);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/CheckboxTest.kt, checkBoxTest_MaterialSize_WhenIndeterminate_withThinStroke_withoutMinimumTouchTarget
#[test]
fn a_thin_stroke_keeps_an_indeterminate_checkbox_24_dp() {
    check_size(INDETERMINATE, false, false, THIN);
}
