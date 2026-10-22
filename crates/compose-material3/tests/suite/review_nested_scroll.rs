//! Review of nested scrolling in Material 3 (D188): failing tests. Each states the input, what
//! Jetpack Compose does, and what happens instead.

#![allow(non_snake_case)]

use std::time::Duration;

use compose_foundation::layout::*;
use compose_foundation::{ScrollModifiers, ScrollState, remember_scroll_state};
use compose_material3::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, Offset, PointerEvent, PointerId, PointerType, UnitExt};

fn finger(id: u64, event: PointerEvent) -> PointerEvent {
    event.with_pointer(PointerId(id), PointerType::Touch)
}

fn bounds(ui: &mut ComposeTestRule, tag: &str) -> compose_ui::Rect {
    ui.on_node_unmerged(has_test_tag(tag)).fetch_semantics_node().bounds
}

thread_local! {
    static OUTER: std::cell::RefCell<Option<ScrollState>> = const { std::cell::RefCell::new(None) };
}

/// A medium exit-until-collapsed bar over a scaffold (the connection on it) whose content is a
/// scrolling column: a 400 dp header, an inner scrolling column 200 dp high (tagged "inner"),
/// then 2000 dp.
#[composable]
fn ThreeLevels() {
    MaterialTheme!({
        let behavior = TopAppBarDefaults::exit_until_collapsed_scroll_behavior();
        let outer = remember_scroll_state();
        OUTER.with(|cell| *cell.borrow_mut() = Some(outer));
        let inner = remember_scroll_state();
        Scaffold!(
            modifier = Modifier.nested_scroll(behavior.nested_scroll_connection()),
            top_bar = {
                MediumTopAppBar!(
                    { Text!("Title") },
                    modifier = Modifier.test_tag("bar"),
                    scroll_behavior = Some(behavior.clone())
                );
            },
            move |padding: PaddingValues| {
                Column!(
                    modifier = Modifier.padding(padding).fill_max_size().vertical_scroll(outer),
                    {
                        Spacer(Modifier.fill_max_width().height(400.dp()));
                        Column!(
                            modifier = Modifier
                                .fill_max_width()
                                .height(200.dp())
                                .test_tag("inner")
                                .vertical_scroll(inner),
                            {
                                for _ in 0..20 {
                                    Spacer(Modifier.fill_max_width().height(40.dp()));
                                }
                            }
                        );
                        Spacer(Modifier.fill_max_width().height(2000.dp()));
                    }
                );
            }
        );
    });
}

/// Input: an exit-until-collapsed bar, collapsed, over an outer scrolling column scrolled 300
/// px down with an inner scrolling column (at its top) under the pointer; a 30 px wheel turn back
/// up over the inner column.
///
/// Compose: the inner column can't scroll back; its post-scroll goes to its nested-scroll
/// parents innermost first: the outer column scrolls back 30 px and nothing is left for the
/// bar, which stays collapsed (it expands only once the content is back at its start).
///
/// Instead: the outer column isn't a nested-scroll parent; the bar's connection gets the 30 px
/// first and expands while the content is still scrolled 300 px down.
#[test]
fn a_bar_doesnt_expand_before_an_outer_scrolling_column_is_back_at_its_start() {
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), ThreeLevels);
    ui.set_auto_advance(false);
    let wheel = |ui: &mut ComposeTestRule, y: f32, delta: f32| {
        ui.host()
            .send_pointer(PointerEvent::scroll(Offset::new(200.0, y), Offset::new(0.0, delta)));
        ui.advance_time_by(Duration::from_millis(32));
    };
    // Below the inner column: the first turn collapses the bar, the second scrolls the content.
    wheel(&mut ui, 150.0, 300.0);
    wheel(&mut ui, 150.0, 300.0);
    assert_eq!(bounds(&mut ui, "bar").height(), 64.0);
    let outer = OUTER.with(|cell| (*cell.borrow()).unwrap());
    assert_eq!(outer.value(), 300);
    let inner = bounds(&mut ui, "inner");
    let y = (inner.top + inner.bottom) / 2.0;
    assert!(y > 64.0 && y < 600.0, "the inner column is on screen: {inner:?}");
    wheel(&mut ui, y, -30.0);
    assert_eq!(outer.value(), 270, "the outer column didn't scroll back");
    assert_eq!(bounds(&mut ui, "bar").height(), 64.0, "the bar expanded first");
}

#[composable]
fn ScrollingSheet(state: SheetState) {
    MaterialTheme!({
        ModalBottomSheet!(|| {}, sheet_state = state, {
            Column!(
                modifier = Modifier.fill_max_width().vertical_scroll(remember_scroll_state()),
                {
                    for index in 0..40 {
                        let tag = if index == 0 { "first" } else { "other" };
                        Text!("Item {}", index, modifier = Modifier.height(40.dp()).test_tag(tag));
                    }
                }
            );
        });
    });
}

/// Input: a half-expanded sheet over scrolling content; a finger drags the content up 100 px (the
/// sheet follows it 92 px, between its anchors), a second finger goes down on the content, the
/// first lifts, then the second.
///
/// Compose: the content's drag ends when the last finger lifts and its post-fling settles the
/// sheet at an anchor (its first item at 48 or 348).
///
/// Instead: the second press replaces the content's drag and the first's release ends it
/// without dispatching the fling: the sheet stays where the finger left it, between anchors.
#[test]
fn a_sheet_dragged_through_its_content_settles_after_a_second_finger() {
    let state = SheetState::new(false, SheetValue::Hidden, |_| true, false);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || ScrollingSheet(given));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(600));
    assert_eq!(state.current_value(), SheetValue::PartiallyExpanded);
    let first = |ui: &mut ComposeTestRule| bounds(ui, "first").top;
    assert_eq!(first(&mut ui), 348.0);
    ui.host().send_pointer(finger(1, PointerEvent::press(Offset::new(200.0, 500.0))));
    for y in [492.0, 450.0, 420.0, 400.0] {
        ui.host().send_pointer(finger(1, PointerEvent::moved(Offset::new(200.0, y), true)));
        ui.advance_time_by(Duration::from_millis(100));
    }
    assert_eq!(first(&mut ui), 348.0 - 92.0);
    ui.host().send_pointer(finger(2, PointerEvent::press(Offset::new(200.0, 550.0))));
    ui.host().send_pointer(finger(1, PointerEvent::release(Offset::new(200.0, 400.0))));
    ui.advance_time_by(Duration::from_millis(100));
    ui.host().send_pointer(finger(2, PointerEvent::release(Offset::new(200.0, 550.0))));
    ui.advance_time_by(Duration::from_millis(1000));
    let top = first(&mut ui);
    assert!(top == 48.0 || top == 348.0, "the sheet was left between anchors: {top}");
}

/// Input: a half-expanded sheet over scrolling content; a finger drags the content up 100 px (the
/// sheet follows, between its anchors), then the system cancels the touch (Android taking the
/// gesture, the desktop window losing the pointer).
///
/// Compose: a cancelled drag stops as a release without velocity (`onDragStopped(Zero)`): the
/// post-fling settles the sheet at an anchor. (The sheet's own `draggable` does settle here.)
///
/// Instead: the content's drag is dropped without a post-fling; the sheet stays between
/// anchors.
#[test]
fn a_sheet_dragged_through_its_content_settles_when_the_touch_is_cancelled() {
    let state = SheetState::new(false, SheetValue::Hidden, |_| true, false);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(400, 600), move || ScrollingSheet(given));
    ui.set_auto_advance(false);
    ui.advance_time_by(Duration::from_millis(600));
    let first = |ui: &mut ComposeTestRule| bounds(ui, "first").top;
    assert_eq!(first(&mut ui), 348.0);
    ui.host().send_pointer(finger(1, PointerEvent::press(Offset::new(200.0, 500.0))));
    for y in [492.0, 450.0, 420.0, 400.0] {
        ui.host().send_pointer(finger(1, PointerEvent::moved(Offset::new(200.0, y), true)));
        ui.advance_time_by(Duration::from_millis(100));
    }
    assert_eq!(first(&mut ui), 348.0 - 92.0);
    ui.host().with_tree(|tree| tree.cancel_pointer(PointerId(1)));
    ui.advance_time_by(Duration::from_millis(1000));
    let top = first(&mut ui);
    assert!(top == 48.0 || top == 348.0, "the sheet was left between anchors: {top}");
}
