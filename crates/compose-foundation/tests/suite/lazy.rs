//! `LazyColumn` and `LazyRow`: only the items in view are composed; scrolling composes the next
//! ones within the frame; item state goes with keys.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Offset, PointerEvent, UnitExt};

thread_local! {
    static COMPOSED: Cell<usize> = const { Cell::new(0) };
}

#[composable]
fn Row(index: usize) {
    COMPOSED.with(|composed| composed.set(composed.get() + 1));
    Box!(modifier = Modifier.size((100.dp(), 20.dp())).test_tag(format!("row {index}")), {
        BasicText!("item {}", index);
    });
}

#[composable]
fn List(state: LazyListState) {
    LazyColumn!(
        modifier = Modifier.size((100.dp(), 100.dp())),
        state = state,
        |scope: LazyListScope| {
            scope.items(1000, |index| Row(index));
        }
    );
}

fn list() -> (ComposeTestRule, Owned<LazyListState>) {
    let state = LazyListState::new(0, 0);
    let given = *state;
    let ui = ComposeTestRule::sized(IntSize::new(200, 200), move || List(given));
    (ui, state)
}

#[test]
fn only_the_items_in_view_and_a_few_beyond_are_composed() {
    COMPOSED.with(|composed| composed.set(0));
    let (mut ui, _) = list();
    ui.on_node_with_text("item 0").assert_exists();
    ui.on_node_with_text("item 4").assert_exists();
    // 5 in view (100 / 20), and a viewport's worth beyond.
    let composed = COMPOSED.with(Cell::get);
    assert!((5..=12).contains(&composed), "{composed} items composed");
    ui.on_node_with_text("item 50").assert_does_not_exist();
}

#[test]
fn scrolling_composes_the_items_it_reaches_in_the_same_frame() {
    let (mut ui, state) = list();
    // A wheel scroll of 210 px: item 10 at the top, 10 px off.
    ui.host().send_pointer(PointerEvent::scroll(Offset::new(50.0, 50.0), Offset::new(0.0, 210.0)));
    let frames = ui.host().settle(10);
    assert_eq!(state.first_visible_item_index(), 10);
    assert_eq!(state.first_visible_item_scroll_offset(), 10);
    assert!(frames <= 2, "{frames} frames to settle a scroll");
    let row = ui.on_node_with_tag("row 10").fetch_semantics_node().bounds;
    assert_eq!(row.top, -10.0);
    ui.on_node_with_text("item 0").assert_does_not_exist();
}

#[test]
fn scroll_to_item_jumps_far_and_clamps_at_the_end() {
    let (mut ui, state) = list();
    state.scroll_to_item(500, 0);
    ui.wait_for_idle();
    ui.on_node_with_text("item 500").assert_exists();
    assert_eq!(ui.on_node_with_tag("row 500").fetch_semantics_node().bounds.top, 0.0);
    // Past the end: the last item ends the viewport.
    state.scroll_to_item(999, 0);
    ui.wait_for_idle();
    assert_eq!(ui.on_node_with_tag("row 999").fetch_semantics_node().bounds.bottom, 100.0);
    assert_eq!(state.first_visible_item_index(), 995);
    assert!(!state.can_scroll_forward());
    assert!(state.can_scroll_backward());
}

#[test]
fn a_drag_scrolls_the_list() {
    let (mut ui, state) = list();
    let host = ui.host();
    host.send_pointer(PointerEvent::press(Offset::new(50.0, 90.0)));
    for y in [80.0, 70.0, 60.0, 50.0, 40.0] {
        host.send_pointer(PointerEvent::moved(Offset::new(50.0, y), true));
    }
    host.send_pointer(PointerEvent::release(Offset::new(50.0, 40.0)));
    ui.wait_for_idle();
    // Moved 50 px, the first 8 within the slop.
    assert_eq!(state.first_visible_item_index(), 2);
}

#[test]
fn keyed_items_keep_their_state_when_items_before_them_go() {
    #[composable]
    fn Counter(label: String) {
        let count = remember(|| mutable_state_of(0));
        Box!(
            modifier = Modifier
                .size((100.dp(), 20.dp()))
                .clickable(move || count.update(|count| count + 1))
                .test_tag(label.clone()),
            {
                BasicText!("{} {}", label, count.get());
            }
        );
    }
    let items = mutable_state_of(vec![String::from("a"), String::from("b"), String::from("c")]);
    let handle = items.handle();
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        let items = Rc::new(handle.get());
        LazyColumn!(modifier = Modifier.size((100.dp(), 100.dp())), |scope: LazyListScope| {
            let (keys, labels) = (items.clone(), items.clone());
            scope.items_keyed(
                items.len(),
                move |index| keys[index].clone(),
                move |index| {
                    Counter(labels[index].clone());
                },
            );
        });
    });
    ui.on_node_with_tag("b").perform_click();
    ui.on_node_with_text("b 1").assert_exists();
    items.edit(|items| {
        items.remove(0);
    });
    ui.wait_for_idle();
    ui.on_node_with_text("b 1").assert_exists();
    assert_eq!(ui.on_node_with_tag("b").fetch_semantics_node().bounds.top, 0.0);
}

#[test]
fn a_lazy_row_lays_out_sideways_with_padding_and_spacing() {
    let mut ui = ComposeTestRule::sized(IntSize::new(300, 100), || {
        LazyRow!(
            modifier = Modifier.size((200.dp(), 50.dp())),
            content_padding = PaddingValues::new(10.dp(), 0.dp(), 10.dp(), 0.dp()),
            horizontal_arrangement = Arrangement::spaced_by(5.dp()),
            |scope: LazyListScope| {
                scope.item(|| {
                    Box!(modifier = Modifier.size((40.dp(), 40.dp())).test_tag("first"));
                });
                scope.items(50, |index| {
                    Box!(
                        modifier =
                            Modifier.size((40.dp(), 40.dp())).test_tag(format!("cell {index}"))
                    );
                });
            }
        );
    });
    assert_eq!(ui.on_node_with_tag("first").fetch_semantics_node().bounds.left, 10.0);
    assert_eq!(ui.on_node_with_tag("cell 0").fetch_semantics_node().bounds.left, 55.0);
    ui.on_node_with_tag("cell 10").assert_does_not_exist();
}

/// Scrolling moves the items without drawing them again: each item's repaint boundary keeps
/// what it drew and its version (docs/06-ui.md, "Repaint boundaries").
#[test]
fn scrolling_keeps_what_the_items_drew() {
    let (mut ui, state) = list();
    let before = ui.host().display_list();
    assert!(before.groups.len() >= 5, "{}", before.dump());
    // Less than an item: the same items, moved.
    assert!(state.scroll_by(13.0));
    ui.wait_for_idle();
    assert_eq!(state.first_visible_item_scroll_offset(), 13);
    let after = ui.host().display_list();
    let mut kept = 0;
    for group in &after.groups {
        if let Some(earlier) = before.groups.iter().find(|earlier| earlier.key == group.key) {
            assert_eq!(group.version, earlier.version, "{group:?} drawn again");
            assert_eq!(after.ops[group.start..group.end], before.ops[earlier.start..earlier.end]);
            kept += 1;
        }
    }
    assert!(kept >= 5, "{kept} items kept\n{}", after.dump());
    assert_ne!(after.dump(), before.dump());
}
