//! `assert_is_displayed` and `assert_is_not_displayed`: placed, clipped by the clips around,
//! inside the window, as Compose's `checkIsDisplayed`.

#![allow(non_snake_case)]

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    Color, Dp, Layout, Modifier, SizeSpec, UnitExt, measure_policy,
};

const COLORS: [Color; 3] = [Color::RED, Color::GREEN, Color::BLUE];

/// An item tagged `item{i}`: a node of its own around a box of `width` and `height` (filling
/// what it's given where `None`), as Compose's test's `Item` in a `BoundaryNode`.
#[composable]
fn Item(i: usize, width: Option<Dp>, height: Option<Dp>) {
    Box!(modifier = Modifier.test_tag(format!("item{i}")), {
        let mut modifier = match width {
            Some(width) => Modifier.size_in(SizeSpec::exact(width, Dp::UNSPECIFIED, true)),
            None => Modifier.fill_max_width(),
        };
        modifier = match height {
            Some(height) => modifier.size_in(SizeSpec::exact(Dp::UNSPECIFIED, height, true)),
            None => modifier.fill_max_height(),
        };
        Box!(modifier = modifier.background(COLORS[i % COLORS.len()]));
    });
}

/// Places its content, or nothing (a zero size), as the test's `PlaceConditionally`.
#[composable]
fn PlaceConditionally(place: bool, content: impl Fn() + 'static) {
    let policy = measure_policy(move |scope, measurables, constraints| {
        if place {
            let placeable = scope.measure(measurables[0], constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        } else {
            scope.layout(0, 0)
        }
    });
    Layout(Modifier, policy, content);
}

#[composable]
fn ScrollingItems() {
    let state = remember_scroll_state(0);
    Column!(modifier = Modifier.required_size(100.dp()).vertical_scroll(state), {
        for i in 0..10 {
            Item(i, None, Some(30.dp()));
        }
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, componentInScrollable_isDisplayed
#[test]
fn an_item_in_a_scroll_viewport_is_displayed() {
    let mut ui = compose_test(ScrollingItems);
    ui.on_node_with_tag("item0").assert_is_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, componentInScrollable_isNotDisplayed
/// An item scrolled out of the viewport (still placed, and inside the window) isn't
/// displayed: the viewport clips it.
#[test]
fn an_item_past_a_scroll_viewport_is_not_displayed() {
    let mut ui = compose_test(ScrollingItems);
    ui.on_node_with_tag("item4").assert_is_not_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, componentInLazyColumn_notComposedAndPrefetched_isNotDisplayed
#[test]
fn a_lazy_item_not_composed_or_only_prefetched_is_not_displayed() {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = compose_test(move || {
        LazyColumn!(
            modifier = Modifier.required_size(150.dp()),
            state = given,
            |scope: LazyListScope| {
                scope.items(10, |i| Item(i, None, Some(100.dp())));
            }
        );
    });
    ui.on_node_with_tag("item0").assert_is_displayed();
    ui.on_node_with_tag("item2").assert_does_not_exist();
    ui.on_node_with_tag("item2").assert_is_not_displayed();
    state.scroll_by(5.0);
    ui.wait_for_idle();
    ui.on_node_with_tag("item2").assert_is_not_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, togglePlacement
#[test]
fn an_item_no_longer_placed_is_not_displayed() {
    let place = mutable_state_of(true);
    let handle = place.handle();
    let mut ui = compose_test(move || PlaceConditionally!(handle.get(), { Item(0, None, None); }));
    ui.on_node_with_tag("item0").assert_is_displayed();
    ui.run_on_idle(|_| place.set(false));
    ui.on_node_with_tag("item0").assert_is_not_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, toggleParentPlacement
#[test]
fn an_item_whose_parent_is_no_longer_placed_is_not_displayed() {
    let place = mutable_state_of(true);
    let handle = place.handle();
    let mut ui = compose_test(move || {
        PlaceConditionally!(handle.get(), {
            Box!({
                Item(0, None, None);
            });
        })
    });
    ui.on_node_with_tag("item0").assert_is_displayed();
    ui.run_on_idle(|_| place.set(false));
    ui.on_node_with_tag("item0").assert_is_not_displayed();
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, rowTooSmall
#[test]
fn an_item_past_a_row_too_small_is_not_displayed() {
    let mut ui = compose_test(|| {
        Row!(modifier = Modifier.required_size(100.dp()), {
            for i in 0..10 {
                Item(i, Some(30.dp()), None);
            }
        });
    });
    ui.on_node_with_tag("item9").assert_is_not_displayed();
}

#[track_caller]
fn expect_error(action: impl FnOnce()) -> String {
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(action));
    let panic = failed.expect_err("an assertion error");
    panic
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| panic.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default()
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, isDisplayed_throws_whenMultipleNodesMatch, isNotDisplayed_throws_whenMultipleNodesMatch
#[test]
fn checking_whether_several_nodes_are_displayed_fails() {
    let mut ui = compose_test(|| {
        Item(0, None, None);
        Item(0, None, None);
    });
    let message = expect_error(|| {
        ui.on_node_with_tag("item0").is_displayed();
    });
    assert!(message.contains("expected at most 1 node but found 2"), "{message}");
    expect_error(|| {
        ui.on_node_with_tag("item0").is_not_displayed();
    });
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, isDisplayed_returnsFalse_whenNodeNotFound, isNotDisplayed_returnsTrue_whenNodeNotFound
#[test]
fn a_missing_node_is_not_displayed() {
    let mut ui = compose_test(|| {});
    assert!(!ui.on_node_with_tag("doesn't exist").is_displayed());
    assert!(ui.on_node_with_tag("doesn't exist").is_not_displayed());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, isDisplayed_returnsFalse_whenNodeIsFullyHidden, isNotDisplayed_returnsTrue_whenNodeIsFullyHidden
/// An item in a zero-sized box: its node takes the box's size, nothing.
#[test]
fn a_node_of_no_size_is_not_displayed() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.required_size(0.dp()), {
            Item(0, Some(10.dp()), Some(10.dp()));
        });
    });
    assert!(!ui.on_node_with_tag("item0").is_displayed());
    assert!(ui.on_node_with_tag("item0").is_not_displayed());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, isDisplayed_returnsTrue_whenNodeIsPartiallyVisible, isNotDisplayed_returnsFalse_whenNodeIsPartiallyVisible
/// A 10 dp item in a 5 dp box, centered so it's partly outside the window: displayed (in
/// part), and not "not displayed" (which takes it wholly hidden or partly out).
#[test]
fn a_node_partly_in_the_window_is_displayed() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.required_size(5.dp()), {
            Item(0, Some(10.dp()), Some(10.dp()));
        });
    });
    assert!(ui.on_node_with_tag("item0").is_displayed());
    assert!(!ui.on_node_with_tag("item0").is_not_displayed());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, isDisplayed_returnsTrue_whenNodeIsFullyVisible, isNotDisplayed_returnsFalse_whenNodeIsFullyVisible
#[test]
fn a_node_wholly_in_the_window_is_displayed() {
    let mut ui = compose_test(|| Item(0, None, None));
    assert!(ui.on_node_with_tag("item0").is_displayed());
    assert!(!ui.on_node_with_tag("item0").is_not_displayed());
}

// Compose: compose/ui/ui-test/src/androidDeviceTest/kotlin/androidx/compose/ui/test/IsDisplayedTest.kt, isDisplayed_throws_whenNodeIsNotVisible, isNotDisplayed_throws_whenNodeIsVisible, isDisplayed_throws_whenNodeIsNotVisibleWithText, isNotDisplayed_throws_whenNodeIsVisibleWithText
#[test]
fn the_displayed_assertions_fail_naming_the_node() {
    let mut ui = compose_test(|| Item(0, None, None));
    let message = expect_error(|| {
        ui.on_node_with_tag("item1").assert_is_displayed();
    });
    assert!(message.contains("Assert failed: the component with tag \"item1\" is not displayed!"), "{message}");
    let message = expect_error(|| {
        ui.on_node_with_tag("item0").assert_is_not_displayed();
    });
    assert!(message.contains("Assert failed: the component with tag \"item0\" is displayed!"), "{message}");
    let mut text = compose_test(|| BasicText!("item0"));
    let message = expect_error(|| {
        text.on_node_with_text("item1").assert_is_displayed();
    });
    assert!(message.contains("with text \"item1\" is not displayed!"), "{message}");
    let message = expect_error(|| {
        text.on_node_with_text("item0").assert_is_not_displayed();
    });
    assert!(message.contains("with text \"item0\" is displayed!"), "{message}");
}

/// A node clipped away by a `clip_to_bounds` ancestor (still placed, inside the window) isn't
/// displayed.
#[test]
fn a_node_clipped_away_by_an_ancestor_is_not_displayed() {
    let mut ui = compose_test(|| {
        Box!(modifier = Modifier.size(50.dp()).clip_to_bounds(), {
            Box!(modifier = Modifier.offset(60.dp(), 0.dp()).size(20.dp()).test_tag("away"));
            Box!(modifier = Modifier.offset(40.dp(), 0.dp()).size(20.dp()).test_tag("half"));
        });
    });
    ui.on_node_with_tag("away").assert_is_not_displayed();
    ui.on_node_with_tag("half").assert_is_displayed();
    let bounds = ui.on_node_with_tag("half").get_bounds_in_root();
    assert_eq!((bounds.left, bounds.right), (40.dp(), 50.dp()));
}
