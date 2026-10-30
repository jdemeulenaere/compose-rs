//! Compose's `MenuTest.kt`: a menu opening and closing (with its scale and fade), its size, its
//! scrolled content, an item's click and its trailing icon without a leading one.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::DrawOp;
use compose_ui::{Color, Dp, Modifier, Offset, UnitExt};

/// A menu of one tagged item, at a 20 dp blue anchor, open while `expanded`.
#[composable]
fn OneItemMenu(expanded: MutableState<bool>) {
    MaterialTheme!({
        Box!(modifier = Modifier.required_size(20.dp()).background(Color::BLUE), {
            DropdownMenu!(expanded.get(), || {}, {
                DropdownMenuItem!(
                    { Text!("Option 1") },
                    || {},
                    modifier = Modifier.test_tag("MenuContent")
                );
            });
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuTest.kt, menu_canBeTriggered
#[test]
fn a_menu_opens_and_closes_with_its_animation() {
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let mut ui = compose_test(move || OneItemMenu(handle));
    ui.on_node_with_tag("MenuContent").assert_does_not_exist();
    ui.set_auto_advance(false);

    expanded.set(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(300));
    ui.on_node_with_tag("MenuContent").assert_exists();

    expanded.set(false);
    ui.advance_time_by_frame();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(300));
    ui.advance_time_by_frame();
    ui.on_node_with_tag("MenuContent").assert_does_not_exist();

    expanded.set(true);
    ui.advance_time_by_frame();
    ui.wait_for_idle();
    ui.advance_time_by_frame();
    ui.advance_time_by(Duration::from_millis(300));
    ui.on_node_with_tag("MenuContent").assert_exists();
}

/// The menu's layer in the frame: its pivot, scale and alpha (none at full size and opacity).
fn menu_layer(ui: &mut ComposeTestRule) -> Option<(Offset, f32, f32)> {
    let ops = ui.host().display_list().ops.clone();
    let pivot_scale = ops.iter().find_map(|op| match op {
        DrawOp::Transform { pivot, scale, .. } => Some((*pivot, scale.0)),
        _ => None,
    });
    let alpha = ops.iter().find_map(|op| match op {
        DrawOp::SaveLayerAlpha { alpha, .. } => Some(*alpha),
        _ => None,
    });
    match (pivot_scale, alpha) {
        (None, None) => None,
        (pivot_scale, alpha) => {
            let (pivot, scale) = pivot_scale.unwrap_or((Offset::ZERO, 1.0));
            Some((pivot, scale, alpha.unwrap_or(1.0)))
        }
    }
}

/// A menu opened grows in from 0.8 of its size and fades in, as Compose's `ClosedScaleTarget`
/// and `ClosedAlphaTarget`, from the side of its anchor (`calculateTransformOrigin`): its top
/// (it's below the 20 px anchor) and the middle of where they overlap (10 px in).
#[test]
fn an_opening_menu_grows_from_its_anchor() {
    let expanded = mutable_state_of(false);
    let handle = expanded.handle();
    let mut ui = compose_test(move || OneItemMenu(handle));
    ui.set_auto_advance(false);
    expanded.set(true);
    let mut layers = Vec::new();
    for _ in 0..30 {
        ui.advance_time_by_frame();
        match menu_layer(&mut ui) {
            Some(layer) => layers.push(layer),
            None if !layers.is_empty() => break,
            None => {}
        }
    }
    assert!(layers.len() > 3, "{layers:?}");
    for (pivot, scale, alpha) in &layers {
        assert_eq!(*pivot, Offset::new(10.0, 0.0));
        assert!((0.8..1.0).contains(scale) && (0.0..=1.0).contains(alpha), "{layers:?}");
    }
    assert!(layers.windows(2).all(|pair| pair[0].1 <= pair[1].1 && pair[0].2 <= pair[1].2));
    // In place at its full size once opened: 48 dp from the window's top (`MenuVerticalMargin`).
    let bounds = ui.on_node_with_tag("MenuContent").get_unclipped_bounds_in_root();
    assert_eq!((bounds.left, bounds.top, bounds.width()), (Dp(0.0), Dp(56.0), Dp(112.0)));
}

/// Compose's menu animates out before it leaves (its transition's current state is still
/// expanded): it's there, shrinking and fading, frames after it's collapsed, then gone.
#[test]
fn a_closing_menu_stays_until_its_animation_ends() {
    let expanded = mutable_state_of(true);
    let handle = expanded.handle();
    let mut ui = compose_test(move || OneItemMenu(handle));
    assert_eq!(menu_layer(&mut ui), None);
    ui.set_auto_advance(false);
    expanded.set(false);
    for _ in 0..3 {
        ui.advance_time_by_frame();
    }
    ui.on_node_with_tag("MenuContent").assert_exists();
    let (_, scale, alpha) = menu_layer(&mut ui).expect("a layer animating out");
    assert!(scale < 1.0 && alpha < 1.0, "{scale} {alpha}");
    ui.set_auto_advance(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("MenuContent").assert_does_not_exist();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuTest.kt, menu_hasExpectedSize
#[test]
fn a_menu_is_as_wide_as_its_widest_content_and_8_dp_more_above_and_below() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.required_size(20.dp()).background(Color::BLUE), {
                DropdownMenu!(true, || {}, {
                    Box!(modifier = Modifier.test_tag("MenuContent1").size(70.dp()));
                    Box!(modifier = Modifier.test_tag("MenuContent2").size(130.dp()));
                });
            });
        });
    });
    ui.on_node_with_tag("MenuContent1").assert_exists();
    ui.on_node_with_tag("MenuContent2").assert_exists();
    let node = ui
        .on_node(
            is_popup()
                & has_any_descendant(has_test_tag("MenuContent1"))
                & has_any_descendant(has_test_tag("MenuContent2")),
        )
        .assert_exists()
        .fetch_semantics_snapshot();
    assert_eq!(node.bounds.width(), 130.0);
    assert_eq!(node.bounds.height(), 8.0 * 2.0 + 200.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuTest.kt, menu_scrolledContent
#[test]
fn a_menu_scrolls_its_content() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.required_size(20.dp()).background(Color::BLUE), {
                let scroll_state = remember_scroll_state(0);
                DropdownMenu!(true, || {}, scroll_state = scroll_state, {
                    for index in 0..100 {
                        let tag = format!("MenuContent {}", index + 1);
                        Box!(modifier = Modifier.test_tag(tag).size(70.dp()));
                    }
                });
                LaunchedEffect((), async move {
                    let _ = scroll_state.scroll_to(scroll_state.max_value()).await;
                });
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("MenuContent 1").assert_is_not_displayed();
    ui.on_node_with_tag("MenuContent 100").assert_is_displayed();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuTest.kt, dropdownMenuItem_onClick
#[test]
fn a_menu_item_clicks() {
    let clicked = Rc::new(Cell::new(false));
    let on_click = clicked.clone();
    let mut ui = compose_test(move || {
        let (item_click, modifier_click) = (on_click.clone(), on_click.clone());
        DropdownMenuItem!(
            { Box!(modifier = Modifier.required_size(40.dp())) },
            move || item_click.set(true),
            modifier = Modifier.test_tag("MenuItem").clickable(move || modifier_click.set(true)),
        );
    });
    ui.on_node_with_tag("MenuItem").perform_click();
    ui.run_on_idle(|_| assert!(clicked.get()));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/MenuTest.kt, dropdownMenuItem_noLeadingIcon_trailingIconPosition
#[test]
fn a_trailing_icon_without_a_leading_one_is_at_the_end() {
    let mut ui = compose_test(|| {
        DropdownMenuItem!(
            { Box!(modifier = Modifier.size(40.dp()).test_tag("Text")) },
            || {},
            modifier = Modifier.test_tag("MenuItem"),
            trailing_icon =
                Some(|| Box!(modifier = Modifier.size(20.dp()).test_tag("TrailingIcon"))),
        );
    });
    let item = ui.on_node_with_tag("MenuItem").get_unclipped_bounds_in_root();
    let text = ui.on_node_with_tag_unmerged("Text").get_unclipped_bounds_in_root();
    let trailing = ui.on_node_with_tag_unmerged("TrailingIcon").get_unclipped_bounds_in_root();
    assert!(text.left < trailing.left, "{text:?} {trailing:?}");
    // The selectable items' 12 dp padding, and the icon at the start of its 24 dp slot.
    let expected = item.right.0 - 12.0 - 4.0;
    assert!((trailing.right.0 - expected).abs() <= 2.0, "{item:?} {trailing:?}");
}
