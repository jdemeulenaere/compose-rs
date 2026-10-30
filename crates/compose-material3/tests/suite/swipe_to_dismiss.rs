//! Compose's `SwipeToDismissTest.kt`, adapted: the saved state, the dismissed offsets (left to
//! right and right to left), the background's size, swipes in each direction and with
//! directions or gestures disabled, and `on_dismiss`.
//!
//! Swipes are a private helper over `send_pointer` (the test harness has no gesture DSL yet):
//! across the node in 200 ms, as Compose's `swipeRight()`/`swipeLeft()`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compose_foundation::layout::*;
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{
    LOCAL_LAYOUT_DIRECTION, LayoutDirection, Modifier, Offset, PointerEvent, UnitExt,
    platform::UiHost,
};

const ROOT_WIDTH: f32 = 400.0;

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_with_tag(tag).fetch_semantics_snapshot().bounds
}

/// Swipes across the node tagged `tag` at its middle, from its left edge to its right one
/// (`rightwards`) or back, in 200 ms, then lets everything settle.
fn swipe(ui: &mut ComposeTestRule, tag: &str, rightwards: bool) {
    let node = bounds(ui, tag);
    let y = (node.top + node.bottom) / 2.0;
    let (from, to) = if rightwards {
        (node.left + 1.0, node.right - 1.0)
    } else {
        (node.right - 1.0, node.left + 1.0)
    };
    ui.host().send_pointer(PointerEvent::press(Offset::new(from, y)));
    const STEPS: usize = 10;
    for step in 1..=STEPS {
        ui.advance_time_by(Duration::from_millis(20));
        let x = from + (to - from) * step as f32 / STEPS as f32;
        ui.host().send_pointer(PointerEvent::moved(Offset::new(x, y), true));
    }
    ui.host().send_pointer(PointerEvent::release(Offset::new(to, y)));
    ui.wait_for_idle();
}

thread_local! {
    static STATE: Cell<Option<SwipeToDismissBoxState>> = const { Cell::new(None) };
}

fn state() -> SwipeToDismissBoxState {
    STATE.with(Cell::get).expect("composed")
}

/// A full-size swipe-to-dismiss box tagged "swipe", its content "content", remembering its
/// state from `initial`.
fn swipe_box(
    initial: SwipeToDismissBoxValue,
    start_to_end: bool,
    end_to_start: bool,
    gestures_enabled: bool,
    direction: LayoutDirection,
) -> impl Fn() {
    move || {
        let state = remember_swipe_to_dismiss_box_state!(initial);
        STATE.with(|cell| cell.set(Some(state)));
        CompositionLocalProvider(LOCAL_LAYOUT_DIRECTION.provides(direction), || {
            SwipeToDismissBox!(
                state,
                |_| {},
                modifier = Modifier.test_tag("swipe"),
                enable_dismiss_from_start_to_end = start_to_end,
                enable_dismiss_from_end_to_start = end_to_start,
                gestures_enabled = gestures_enabled,
                |_| {
                    Box!(modifier = Modifier.fill_max_size().test_tag("content"));
                }
            );
        });
    }
}

fn ltr_box(initial: SwipeToDismissBoxValue) -> impl Fn() {
    swipe_box(initial, true, true, true, LayoutDirection::Ltr)
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, test_stateSavedAndRestored
#[test]
fn a_remembered_state_is_restored_in_a_new_host() {
    let mut ui = compose_test(ltr_box(SwipeToDismissBoxValue::Settled));
    let snapping = state();
    CoroutineScope::new()
        .launch(async move { snapping.snap_to(SwipeToDismissBoxValue::StartToEnd).await });
    ui.wait_for_idle();
    assert_eq!(state().settled_value(), SwipeToDismissBoxValue::StartToEnd);
    let saved = ui.host().save_state();
    drop(ui);
    let host = UiHost::headless(400, 800);
    host.restore_state(saved);
    let _ui = ComposeTestRule::with_host(host, ltr_box(SwipeToDismissBoxValue::Settled));
    assert_eq!(state().settled_value(), SwipeToDismissBoxValue::StartToEnd);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_testOffset_whenDefault
#[test]
fn a_settled_box_shows_its_content_in_place() {
    let mut ui = compose_test(ltr_box(SwipeToDismissBoxValue::Settled));
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_testOffset_whenDismissedToEnd
#[test]
fn a_box_dismissed_to_the_end_has_its_content_a_width_right() {
    let mut ui = compose_test(ltr_box(SwipeToDismissBoxValue::StartToEnd));
    assert_eq!(bounds(&mut ui, "content").left, ROOT_WIDTH);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_testOffset_whenDismissedToEnd_rtl
#[test]
fn right_to_left_a_box_dismissed_to_the_end_has_its_content_a_width_left() {
    let mut ui = compose_test(swipe_box(
        SwipeToDismissBoxValue::StartToEnd,
        true,
        true,
        true,
        LayoutDirection::Rtl,
    ));
    assert_eq!(bounds(&mut ui, "content").left, -ROOT_WIDTH);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_testOffset_whenDismissedToStart
#[test]
fn a_box_dismissed_to_the_start_has_its_content_a_width_left() {
    let mut ui = compose_test(ltr_box(SwipeToDismissBoxValue::EndToStart));
    assert_eq!(bounds(&mut ui, "content").left, -ROOT_WIDTH);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_testOffset_whenDismissedToStart_rtl
#[test]
fn right_to_left_a_box_dismissed_to_the_start_has_its_content_a_width_right() {
    let mut ui = compose_test(swipe_box(
        SwipeToDismissBoxValue::EndToStart,
        true,
        true,
        true,
        LayoutDirection::Rtl,
    ));
    assert_eq!(bounds(&mut ui, "content").left, ROOT_WIDTH);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_testBackgroundMatchesContentSize
#[test]
fn the_background_is_as_large_as_the_content() {
    let mut ui = compose_test(|| {
        let state = remember_swipe_to_dismiss_box_state!();
        SwipeToDismissBox!(
            state,
            |_| {
                Box!(modifier = Modifier.fill_max_size().test_tag("background"));
            },
            |_| {
                Box!(modifier = Modifier.size(100.dp()));
            }
        );
    });
    let background = bounds(&mut ui, "background");
    assert_eq!((background.width(), background.height()), (100.0, 100.0));
}

/// Swipes a box allowing the given directions; the rule is kept so the state lives on.
fn swiped(
    rightwards: bool,
    start_to_end: bool,
    end_to_start: bool,
    direction: LayoutDirection,
) -> ComposeTestRule {
    let mut ui = compose_test(swipe_box(
        SwipeToDismissBoxValue::Settled,
        start_to_end,
        end_to_start,
        true,
        direction,
    ));
    swipe(&mut ui, "swipe", rightwards);
    ui
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_dismissBySwipe_toEnd
#[test]
fn a_swipe_right_dismisses_to_the_end() {
    let _ui = swiped(true, true, false, LayoutDirection::Ltr);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::StartToEnd);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_dismissBySwipe_toStart
#[test]
fn a_swipe_left_dismisses_to_the_start() {
    let _ui = swiped(false, false, true, LayoutDirection::Ltr);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::EndToStart);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_dismissBySwipe_toEnd_rtl
#[test]
fn right_to_left_a_swipe_left_dismisses_to_the_end() {
    let _ui = swiped(false, true, false, LayoutDirection::Rtl);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::StartToEnd);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_dismissBySwipe_toStart_rtl
#[test]
fn right_to_left_a_swipe_right_dismisses_to_the_start() {
    let _ui = swiped(true, false, true, LayoutDirection::Rtl);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::EndToStart);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_dismissBySwipe_disabled
#[test]
fn a_box_with_both_directions_disabled_stays_in_place() {
    let mut ui = compose_test(swipe_box(
        SwipeToDismissBoxValue::StartToEnd,
        false,
        false,
        true,
        LayoutDirection::Ltr,
    ));
    swipe(&mut ui, "swipe", true);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::Settled);
    swipe(&mut ui, "swipe", false);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::Settled);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_containsSettledAnchor_dismissAnchorsDisabled
// (without the `LookaheadScope`, which compose-rs lacks: the state comes back to its one anchor)
#[test]
fn a_dismissed_state_without_its_anchor_comes_back_in_place() {
    let mut ui = compose_test(swipe_box(
        SwipeToDismissBoxValue::StartToEnd,
        false,
        false,
        true,
        LayoutDirection::Ltr,
    ));
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::Settled);
    assert_eq!(state().settled_value(), SwipeToDismissBoxValue::Settled);
    assert_eq!(bounds(&mut ui, "content").left, 0.0);
}

// Compose: compose/material3/material3/src/androidDeviceTest/kotlin/androidx/compose/material3/SwipeToDismissTest.kt, swipeDismiss_respectsGesturesEnabled
#[test]
fn a_box_without_gestures_ignores_swipes() {
    let mut ui = compose_test(swipe_box(
        SwipeToDismissBoxValue::Settled,
        true,
        true,
        false,
        LayoutDirection::Ltr,
    ));
    swipe(&mut ui, "swipe", true);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::Settled);
    swipe(&mut ui, "swipe", false);
    assert_eq!(state().current_value(), SwipeToDismissBoxValue::Settled);
}

/// `on_dismiss` hears the direction once the box settles away, and only then (the
/// `onDismiss` parameter Compose's tests don't cover).
#[test]
fn on_dismiss_hears_the_direction_once_settled_away() {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let log = heard.clone();
    let mut ui = compose_test(move || {
        let state = remember_swipe_to_dismiss_box_state!();
        STATE.with(|cell| cell.set(Some(state)));
        let log = log.clone();
        SwipeToDismissBox!(
            state,
            |_| {},
            modifier = Modifier.test_tag("swipe"),
            on_dismiss = move |direction| log.borrow_mut().push(direction),
            |_| {
                Box!(modifier = Modifier.fill_max_size());
            }
        );
    });
    assert!(heard.borrow().is_empty());
    swipe(&mut ui, "swipe", false);
    assert_eq!(*heard.borrow(), vec![SwipeToDismissBoxValue::EndToStart]);
    // Dismissed, it can't be swiped back.
    swipe(&mut ui, "swipe", true);
    assert_eq!(state().settled_value(), SwipeToDismissBoxValue::EndToStart);
    assert_eq!(*heard.borrow(), vec![SwipeToDismissBoxValue::EndToStart]);
}

/// Counts its layer's measurements.
struct Measures(std::rc::Rc<std::cell::Cell<u32>>);

impl compose_ui::ModifierNode for Measures {
    fn as_layout_aware(&mut self) -> Option<&mut dyn compose_ui::LayoutAwareModifierNode> {
        Some(self)
    }
}

impl compose_ui::LayoutAwareModifierNode for Measures {
    fn on_remeasured(&mut self, _: compose_ui::IntSize, _: &mut compose_ui::NodeContext<'_>) {
        self.0.set(self.0.get() + 1);
    }
}

struct MeasuresElement(std::rc::Rc<std::cell::Cell<u32>>);

impl compose_ui::ModifierNodeElement for MeasuresElement {
    type Node = Measures;
    fn create(&self) -> Measures {
        Measures(self.0.clone())
    }
    fn update(&self, _: &mut Measures) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

// Compose: compose/foundation/foundation/src/commonMain/kotlin/androidx/compose/foundation/gestures/AnchoredDraggable.kt, offset read while placing, as SwipeToDismissBox's `Modifier.offset { }`
#[test]
fn a_swipe_places_the_content_and_measures_nothing() {
    use compose_foundation::layout::*;
    let measures = std::rc::Rc::new(std::cell::Cell::new(0));
    let m = measures.clone();
    let shown: std::rc::Rc<std::cell::Cell<Option<SwipeToDismissBoxState>>> = Default::default();
    let s = shown.clone();
    let mut ui =
        compose_test::ComposeTestRule::sized(compose_ui::IntSize::new(200, 100), move || {
            let state = remember_swipe_to_dismiss_box_state!(SwipeToDismissBoxValue::Settled);
            s.set(Some(state));
            SwipeToDismissBox!(
                state,
                |_| {},
                modifier = Modifier.then_custom(MeasuresElement(m.clone())),
                |_| {
                    Box!(modifier = Modifier.fill_max_size().test_tag("content"));
                }
            );
        });
    let measured = measures.get();
    let state = shown.get().expect("composed");
    compose_runtime::platform::launch(async move {
        state.snap_to(SwipeToDismissBoxValue::StartToEnd).await;
    });
    ui.wait_for_idle();
    assert_eq!(measures.get(), measured);
    ui.on_node_with_tag("content").assert_left_position_in_root_is_equal_to(compose_ui::Dp(200.0));
}
