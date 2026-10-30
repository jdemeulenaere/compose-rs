//! Compose's `SwitchTest.kt`, adapted: semantics, toggling, untoggleable switches merging into
//! what's around them, the thumb's moves with content in it, state restoration and reuse. The
//! Material size tests are in `suite/minimum_interactive_size.rs`.
//!
//! Not ported: `switch_minTouchTargetArea` (a click outside a 2 dp switch in its touch target:
//! ui's feature:hit-test-extensions).

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{Color, Modifier, Role, UnitExt};

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_defaultSemantics
#[test]
fn a_switch_is_an_enabled_switch_on_or_off() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                Switch!(true, Some(|_| {}), modifier = Modifier.test_tag("checked"));
                Switch!(false, Some(|_| {}), modifier = Modifier.test_tag("unchecked"));
            });
        });
    });
    for (tag, on) in [("checked", true), ("unchecked", false)] {
        let node = ui.on_node_with_tag(tag).fetch_semantics_snapshot();
        assert_eq!(
            node.config.get(&compose_ui::SemanticsProperties::ROLE).copied(),
            Some(Role::Switch)
        );
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
        Box!({
            Switch!(
                checked.get(),
                Some(move |now| checked.set(now)),
                modifier = Modifier.test_tag("switch")
            );
        });
    });
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_toggle
#[test]
fn a_click_turns_it_on() {
    let mut ui = compose_test(Toggling);
    ui.on_node_with_tag("switch").assert_is_off().perform_click().assert_is_on();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_toggleTwice
#[test]
fn a_second_click_turns_it_off() {
    let mut ui = compose_test(Toggling);
    ui.on_node_with_tag("switch")
        .assert_is_off()
        .perform_click()
        .assert_is_on()
        .perform_click()
        .assert_is_off();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_uncheckableWithNoLambda
#[test]
fn a_disabled_switch_with_a_callback_has_a_click_action() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Switch!(false, Some(|_| {}), modifier = Modifier.test_tag("switch"), enabled = false);
        });
    });
    ui.on_node_with_tag("switch").assert_has_click_action();
}

/// Focused by its own semantics, as the test's `semantics { focused = true }` (compose-rs's
/// matcher reads `focusable`).
fn focused(modifier: Modifier) -> Modifier {
    modifier.semantics(|config| config.set_focused(true))
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_untoggleable_whenEmptyLambda
#[test]
fn a_disabled_switch_with_a_callback_is_its_own_node() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.semantics_with(true, |_| {}).test_tag("parent"), {
                Switch!(
                    false,
                    Some(|_| {}),
                    enabled = false,
                    modifier = focused(Modifier.test_tag("switch")),
                );
            });
        });
    });
    ui.on_node_with_tag("switch").assert_has_click_action();
    ui.on_node_with_tag("parent").assert(is_not_focusable());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_untoggleableAndMergeable_whenNullLambda
#[test]
fn a_switch_without_a_callback_merges_into_its_parent() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Box!(modifier = Modifier.semantics_with(true, |_| {}).test_tag("parent"), {
                Switch!(false, None::<fn(bool)>, modifier = focused(Modifier));
            });
        });
    });
    ui.on_node_with_tag("parent").assert_has_no_click_action().assert(is_focusable());
}

/// A 16 dp spacer in the thumb.
fn spacer() {
    Spacer!(modifier = Modifier.size(16.dp()).test_tag("spacer"));
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_stateChange_movesThumb
#[test]
fn checking_moves_the_thumb_to_the_end() {
    let checked = mutable_state_of(false);
    let handle = checked.handle();
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Switch!(
                handle.get(),
                Some(move |now| handle.set(now)),
                modifier = Modifier.test_tag("switch"),
                thumb_content = Some(spacer),
            );
        });
    });
    // The 24 dp thumb (with content) 4 dp in, the spacer centered in it.
    ui.on_node_unmerged(has_test_tag("spacer")).assert_left_position_in_root_is_equal_to(8.dp());
    checked.set(true);
    ui.on_node_unmerged(has_test_tag("spacer")).assert_left_position_in_root_is_equal_to(28.dp());
    checked.set(false);
    ui.on_node_unmerged(has_test_tag("spacer")).assert_left_position_in_root_is_equal_to(8.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_constantState_doesNotAnimate
#[test]
fn a_click_on_a_switch_kept_off_leaves_its_thumb() {
    let mut ui = compose_test(|| {
        MaterialTheme!({
            Switch!(
                false,
                Some(|_| {}),
                modifier = Modifier.test_tag("switch"),
                thumb_content = Some(spacer),
            );
        });
    });
    ui.on_node_with_tag("switch").perform_touch_input(|touch| touch.click());
    ui.on_node_unmerged(has_test_tag("spacer")).assert_left_position_in_root_is_equal_to(8.dp());
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_stateRestoration_stateChangeWhileSaved
#[test]
fn switches_changed_while_their_screen_was_saved_show_the_change() {
    let screen_two = mutable_state_of(false);
    let items = mutable_state_of(vec![(1, false), (2, true)]);
    let (screen, list) = (screen_two.handle(), items.handle());
    let mut ui = compose_test(move || {
        MaterialTheme!({
            Column!({
                Button!(move || screen.set(!screen.get()), { Text!("switch screen") });
                let holder = remember_saveable_state_holder();
                holder.SaveableStateProvider(screen.get(), || {
                    if screen.get() {
                        Text!("Second screen");
                    } else {
                        Column!({
                            Text!("screen one");
                            for (index, (id, on)) in list.get().into_iter().enumerate() {
                                Row!({
                                    Text!("Item {}", id);
                                    Switch!(
                                        on,
                                        Some(move |_| {
                                            let mut all = list.get();
                                            all[index].1 = !on;
                                            list.set(all);
                                        }),
                                        modifier = Modifier.test_tag(id.to_string()),
                                    );
                                });
                            }
                        });
                    }
                });
            });
        });
    });
    ui.on_node_with_tag("1").assert_is_off();
    ui.on_node_with_tag("2").assert_is_on();
    screen_two.set(true);
    ui.wait_for_idle();
    items.set(vec![(1, true), (2, false)]);
    ui.wait_for_idle();
    screen_two.set(false);
    ui.on_node_with_tag("1").assert_is_on();
    ui.on_node_with_tag("2").assert_is_off();
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwitchTest.kt, switch_onReset_resetsState
#[test]
fn a_reused_switch_shows_its_new_value_without_animating() {
    let checked = mutable_state_of(false);
    let reuse_key = mutable_state_of(0);
    let (on, key) = (checked.handle(), reuse_key.handle());
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        MaterialTheme!({
            ReusableContent(key.get(), || {
                Switch!(
                    on.get(),
                    Some(move |now| on.set(now)),
                    modifier = Modifier.test_tag("switch"),
                    thumb_content = Some(|| {
                        Box!(
                            modifier =
                                Modifier.size(10.dp()).test_tag("thumb").background(Color::BLUE)
                        );
                    }),
                );
            });
        });
    });
    checked.set(true);
    reuse_key.set(1);
    ui.advance_time_by_frame();
    let before = ui.on_node_unmerged(has_test_tag("thumb")).fetch_semantics_snapshot().bounds;
    for _ in 0..10 {
        ui.advance_time_by_frame();
    }
    let after = ui.on_node_unmerged(has_test_tag("thumb")).fetch_semantics_snapshot().bounds;
    assert_eq!(before, after, "the thumb doesn't move");
}

/// The thumb's content takes the icon color: on-primary-container checked, the highest
/// surface container unchecked (`SwitchDefaults.colors()`), and the thumb is 24 dp with it.
#[test]
fn the_thumb_content_takes_the_icon_color() {
    thread_local! {
        static COLORS: std::cell::Cell<[Option<Color>; 2]> = const { std::cell::Cell::new([None; 2]) };
    }
    let _ui = compose_test(|| {
        MaterialTheme!({
            Column!({
                for (index, on) in [true, false].into_iter().enumerate() {
                    Switch!(
                        on,
                        Some(|_| {}),
                        thumb_content = Some(move || {
                            let color = LOCAL_CONTENT_COLOR.current();
                            COLORS.with(|colors| {
                                let mut all = colors.get();
                                all[index] = Some(color);
                                colors.set(all);
                            });
                        }),
                    );
                }
            });
        });
    });
    let s = light_color_scheme();
    assert_eq!(
        COLORS.with(std::cell::Cell::get),
        [Some(s.on_primary_container), Some(s.surface_container_highest)]
    );
}
