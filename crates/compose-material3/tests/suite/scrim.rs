//! Compose's `ScrimTest.kt`: the public `Scrim`'s click (semantics and touch), its description,
//! no click or description without `on_click`, nothing without a color, its size.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_test::*;
use compose_ui::{Color, Modifier, UnitExt};

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrimTest.kt, scrim_onClick_callsOnDismissRequest
#[test]
fn a_scrims_click_action_calls_it() {
    let count = Rc::new(Cell::new(0));
    let counted = count.clone();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        Scrim!(
            Some("Close"),
            modifier = Modifier.test_tag("scrimTag"),
            on_click = Some(move || counted.set(counted.get() + 1)),
        );
    });
    ui.on_node_with_tag("scrimTag").assert_has_click_action().perform_click();
    assert_eq!(count.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrimTest.kt, scrim_tapGesture_callsOnDismissRequest
#[test]
fn a_tap_on_a_scrim_calls_it() {
    let count = Rc::new(Cell::new(0));
    let counted = count.clone();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        Box!(modifier = Modifier.size(100.dp()), {
            let counted = counted.clone();
            Scrim!(
                Some("Close"),
                modifier = Modifier.test_tag("scrimTag"),
                on_click = Some(move || counted.set(counted.get() + 1)),
            );
        });
    });
    ui.on_node_with_tag("scrimTag").perform_touch_input(|touch| touch.click());
    assert_eq!(count.get(), 1);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrimTest.kt, scrim_semantics_contentDescription_isSet
#[test]
fn a_scrim_has_its_description() {
    let mut ui = compose_test(|| {
        Scrim!(Some("Close sheet"), modifier = Modifier.test_tag("scrimTag"));
    });
    ui.on_node_with_tag("scrimTag").assert(has_content_description("Close sheet"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrimTest.kt, scrim_nullOnDismissRequest_noSemanticsOrClick
#[test]
fn a_scrim_without_a_click_has_no_click_or_description() {
    let mut ui = compose_test(|| {
        Scrim!(Some("Decor"), modifier = Modifier.test_tag("scrimTag"), on_click = None::<fn()>);
    });
    ui.on_node_with_tag("scrimTag")
        .assert(!has_click_action())
        .assert(!has_content_description("Decor"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrimTest.kt, scrim_unspecifiedColor_doesNotRender
#[test]
fn a_scrim_without_a_color_isnt_there() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.fill_max_size(), {
            Scrim!(
                Some("Close"),
                modifier = Modifier.test_tag("scrimTag"),
                color = Color::UNSPECIFIED
            );
        });
    });
    ui.on_node_with_tag("scrimTag").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/ScrimTest.kt, scrim_fillMaxSize_byDefault
#[test]
fn a_scrim_fills_its_container() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.size(200.dp()), {
            Scrim!(None, modifier = Modifier.test_tag("scrimTag"));
        });
    });
    ui.on_node_with_tag("scrimTag")
        .assert_width_is_equal_to(200.dp())
        .assert_height_is_equal_to(200.dp());
}

/// The scrim draws its color at `alpha` (clamped), by default the scrim color at 32 %.
#[test]
fn a_scrim_draws_its_color_at_its_alpha() {
    let ui = compose_test(|| {
        MaterialTheme!({
            Scrim!(None, alpha = || 2.0);
        });
    });
    let expected = format!("{:?}", light_color_scheme().scrim.with_alpha(0.32));
    let dump = ui.dump_display_list();
    assert!(dump.contains(&expected), "{expected} in {dump}");
    let ui = compose_test(|| {
        Scrim!(None, alpha = || 0.5, color = Color::BLACK);
    });
    let dump = ui.dump_display_list();
    assert!(dump.contains(&format!("{:?}", Color::BLACK.with_alpha(0.5))), "{dump}");
}
