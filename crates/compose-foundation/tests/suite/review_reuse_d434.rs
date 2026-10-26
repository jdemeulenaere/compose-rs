//! Adversarial review of D434 (the fixes to lazy item reuse).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::{IntSize, Modifier, UnitExt};

fn top_of(ui: &mut ComposeTestRule, tag: &str) -> f32 {
    ui.on_node_with_tag(tag).fetch_semantics_node().bounds.top
}

/// A deactivated child between two others in a Column: the one after it is laid out as if it
/// weren't there, is hit where it's drawn, and comes after the first in the semantics.
/// Reactivated, it's laid out between them again, with fresh state.
#[test]
fn a_deactivated_child_in_the_middle_of_a_column() {
    let active = mutable_state_of(true);
    let handle = active.handle();
    let clicks: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let counted = clicks.clone();
    let mut ui = compose_test(move || {
        let counted = counted.clone();
        Column!({
            Box!(modifier = Modifier.size(10.dp()).test_tag("a"));
            let inner = counted.clone();
            ReusableContentHost(handle.get(), move || {
                let inner = inner.clone();
                let label = remember(|| mutable_state_of(0u32));
                Box!(
                    modifier = Modifier
                        .size(20.dp())
                        .test_tag(format!("b{}", label.get()))
                        .clickable(move || {
                            inner.borrow_mut().push("b");
                            label.set(label.get() + 1);
                        })
                );
            });
            let after = counted.clone();
            Box!(
                modifier = Modifier
                    .size(30.dp())
                    .test_tag("c")
                    .clickable(move || after.borrow_mut().push("c"))
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "c"), 30.0);
    ui.on_node_with_tag("b0").perform_click();
    ui.wait_for_idle();
    ui.on_node_with_tag("b1").assert_exists();
    active.set(false);
    ui.wait_for_idle();
    assert_eq!(top_of(&mut ui, "c"), 10.0, "{}", ui.dump_semantics());
    ui.on_node_with_tag("c").perform_click();
    assert_eq!(*clicks.borrow(), vec!["b", "c"]);
    active.set(true);
    ui.wait_for_idle();
    // Reused: its state computed anew.
    ui.on_node_with_tag("b0").assert_exists();
    assert_eq!(top_of(&mut ui, "c"), 30.0, "{}", ui.dump_semantics());
    ui.on_node_with_tag("b0").perform_click();
    ui.on_node_with_tag("c").perform_click();
    assert_eq!(*clicks.borrow(), vec!["b", "c", "b", "c"]);
}

/// The same in a Row with weights: the weights are shared among the active children only.
#[test]
fn a_deactivated_weighted_child_in_a_row() {
    let active = mutable_state_of(false);
    let handle = active.handle();
    let mut ui = compose_test(move || {
        Row!(modifier = Modifier.size((90.dp(), 10.dp())), |scope: RowScope| {
            Box!(modifier = Modifier.weight(scope, 1.0).test_tag("a"));
            ReusableContentHost(handle.get(), move || {
                Box!(modifier = Modifier.weight(scope, 1.0).test_tag("b"));
            });
            Box!(modifier = Modifier.weight(scope, 1.0).test_tag("c"));
        });
    });
    ui.wait_for_idle();
    active.set(true);
    ui.wait_for_idle();
    let width = ui.on_node_with_tag("a").fetch_semantics_node().bounds.width();
    assert_eq!(width, 30.0);
    active.set(false);
    ui.wait_for_idle();
    let width = ui.on_node_with_tag("a").fetch_semantics_node().bounds.width();
    let c = ui.on_node_with_tag("c").fetch_semantics_node().bounds;
    assert_eq!((width, c.left, c.width()), (45.0, 45.0, 45.0), "{}", ui.dump_semantics());
}

thread_local! {
    static INNER_STATES: RefCell<HashMap<usize, LazyListState>> = RefCell::new(HashMap::new());
    static WRONG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A feed of carousels, rows keyed: an inner list's scroll position and its items' state
/// belong to its row, whichever row's node it reuses (Compose resets them: the inner
/// `rememberLazyListState` is a `rememberSaveable` of the row's key).
#[test]
fn a_reused_rows_inner_list_starts_at_its_own_position_with_its_own_items() {
    INNER_STATES.with(|s| s.borrow_mut().clear());
    WRONG.with(|w| w.borrow_mut().clear());
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            |scope: LazyListScope| scope.items_keyed(
                1000,
                |row| row as u64,
                |row| {
                    let inner = remember_lazy_list_state!();
                    INNER_STATES.with(|s| s.borrow_mut().insert(row, inner));
                    LazyRow!(
                        modifier = Modifier.size((100.dp(), 20.dp())),
                        state = inner,
                        move |scope: LazyListScope| {
                            scope.items(50, move |index| {
                                let label = remember(move || (row, index));
                                if label != (row, index) {
                                    WRONG.with(|w| {
                                        w.borrow_mut().push(format!("{label:?} in {row}/{index}"))
                                    });
                                }
                                Box!(modifier = Modifier.size(20.dp()));
                            })
                        }
                    );
                }
            )
        );
    });
    ui.wait_for_idle();
    // Row 0's carousel scrolled on.
    INNER_STATES.with(|s| s.borrow()[&0]).request_scroll_to_item(20, 0);
    ui.wait_for_idle();
    for _ in 0..30 {
        state.dispatch_raw_delta(20.0);
        ui.wait_for_idle();
    }
    let positions: Vec<(usize, usize)> = INNER_STATES.with(|s| {
        let s = s.borrow();
        (state.first_visible_item_index()..state.first_visible_item_index() + 5)
            .map(|row| (row, s[&row].first_visible_item_index()))
            .collect()
    });
    assert!(positions.iter().all(|(_, index)| *index == 0), "{positions:?}");
    // Back to row 0: its carousel where it was (saved).
    state.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    let back = INNER_STATES.with(|s| s.borrow()[&0].first_visible_item_index());
    assert_eq!(back, 20);
    let wrong = WRONG.with(|w| w.borrow().clone());
    assert!(wrong.is_empty(), "inner items' state from another row: {wrong:?}");
}

/// The focused item is pinned (kept composed and placed wherever it scrolls) until it reports
/// losing the focus. Removed from the data while focused, its node is deactivated (the tree
/// drops the focus) but its `on_focus_changed` never hears it (deactivated content reports
/// nothing; `on_reset` forgets what it reported): the list keeps its key pinned, and the item,
/// once back in the data far from the viewport, is composed and placed for good.
#[test]
fn a_focused_item_removed_from_the_data_does_not_stay_pinned() {
    let requester = compose_ui::FocusRequester::new();
    let removed = mutable_state_of(false);
    let removed_handle = removed.handle();
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::sized(IntSize::new(200, 200), move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = given,
            move |scope: LazyListScope| {
                let gone = removed_handle.get();
                let keys: Vec<u64> = (0..200u64).filter(|key| !(gone && *key == 2)).collect();
                let count = keys.len();
                let keys = Rc::new(keys);
                let by_index = keys.clone();
                scope.items_keyed(
                    count,
                    move |index| by_index[index],
                    move |index| {
                        let key = keys[index];
                        let mut modifier =
                            Modifier.size((100.dp(), 20.dp())).test_tag(format!("i{key}"));
                        if key == 2 {
                            modifier = modifier.focus_requester(requester);
                        }
                        Box!(modifier = modifier.focusable());
                    },
                )
            }
        );
    });
    ui.wait_for_idle();
    requester.request_focus();
    ui.wait_for_idle();
    ui.on_node_with_tag("i2").assert_is_focused();
    removed.set(true);
    ui.wait_for_idle();
    state.request_scroll_to_item(100, 0);
    ui.wait_for_idle();
    removed.set(false);
    ui.wait_for_idle();
    let shown = ui.count_nodes(&has_test_tag("i2"));
    assert_eq!(shown, 0, "item 2 composed and placed 100 items away: still pinned");
}

/// Plain `remember` at the position of a `remember_across_reuse` of the same type, in content
/// reused: it's handed the kept value (the slot filter takes any `KeptOnReuse`).
#[test]
fn a_plain_remember_in_reused_content_does_not_get_a_kept_value() {
    let key = mutable_state_of(0u32);
    let handle = key.handle();
    let seen: Rc<RefCell<Vec<u32>>> = Rc::default();
    let given = seen.clone();
    let mut ui = compose_test(move || {
        let given = given.clone();
        let k = handle.get();
        ReusableContent(k, move || {
            if k == 0 {
                let value =
                    compose_runtime::platform::remember_across_reuse(|| Rc::new(Cell::new(7u32)));
                given.borrow_mut().push(value.get());
            } else {
                let value = remember(|| Rc::new(Cell::new(k)));
                given.borrow_mut().push(value.get());
            }
        });
    });
    ui.wait_for_idle();
    key.set(1);
    ui.wait_for_idle();
    assert_eq!(*seen.borrow().last().unwrap(), 1, "{:?}", seen.borrow());
}

/// Tab moves focus past deactivated content; a requester in it focuses nothing until it's
/// active again; a field focused when it's deactivated takes no more text.
#[test]
fn focus_skips_deactivated_content_and_comes_back_with_it() {
    let active = mutable_state_of(true);
    let handle = active.handle();
    let requester = compose_ui::FocusRequester::new();
    let text = mutable_state_of(String::new());
    let text_handle = text.handle();
    let mut ui = compose_test(move || {
        Column!({
            Box!(modifier = Modifier.size(10.dp()).test_tag("a").focusable());
            ReusableContentHost(handle.get(), move || {
                BasicTextField!(
                    value = text_handle.get(),
                    on_value_change = move |value| text_handle.set(value),
                    modifier = Modifier
                        .size((100.dp(), 20.dp()))
                        .focus_requester(requester)
                        .test_tag("field"),
                );
            });
            Box!(modifier = Modifier.size(10.dp()).test_tag("c").focusable());
        });
    });
    ui.wait_for_idle();
    assert!(requester.request_focus());
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_focused();
    ui.type_text("x");
    ui.wait_for_idle();
    assert_eq!(text.get(), "x");
    active.set(false);
    ui.wait_for_idle();
    ui.type_text("y");
    ui.wait_for_idle();
    assert_eq!(text.get(), "x", "a deactivated field took text");
    assert!(!requester.request_focus(), "a deactivated field's requester is attached");
    ui.on_node_with_tag("a").request_focus();
    ui.press_key(compose_ui::Key::Tab);
    ui.wait_for_idle();
    ui.on_node_with_tag("c").assert_is_focused();
    active.set(true);
    ui.wait_for_idle();
    assert!(requester.request_focus(), "the reactivated field's requester isn't attached");
    ui.wait_for_idle();
    ui.on_node_with_tag("field").assert_is_focused();
    ui.type_text("z");
    ui.wait_for_idle();
    // The cursor at the start, as a new field's.
    assert_eq!(text.get(), "zx");
}

/// A selection in a `SelectionContainer` around a list, in an item's text: the item leaves and
/// its text node is reused by another item of the same text. `TextNode::on_reset` clears the
/// node's drawn selection but keeps its `version` (bumped only when the text changes), and the
/// tree drops the selection only when the reset node is the container: the selection's points
/// hold again, on the other item.
#[test]
fn a_selection_does_not_move_to_the_item_reusing_the_selected_ones_text() {
    use compose_ui::{Offset, PointerEvent};
    let state = LazyListState::new(0, 0);
    let given = *state;
    let mut ui = ComposeTestRule::with_manual_clock(move || {
        SelectionContainer!({
            LazyColumn!(
                modifier = Modifier.size((200.dp(), 100.dp())),
                state = given,
                |scope: LazyListScope| scope.items(1000, |index| {
                    BasicText!("Hello world", modifier = Modifier.test_tag(format!("t{index}")));
                })
            );
        });
    });
    ui.wait_for_idle();
    let bounds = ui.on_node_with_tag("t0").fetch_semantics_node().bounds;
    let point = Offset::new(bounds.left + 7.0 * 8.0 + 1.0, (bounds.top + bounds.bottom) / 2.0);
    for _ in 0..2 {
        ui.host().send_pointer(PointerEvent::press(point));
        ui.host().send_pointer(PointerEvent::release(point));
        ui.advance_time_by(std::time::Duration::from_millis(50));
    }
    assert_eq!(ui.host().with_tree(|tree| tree.selected_text()).as_deref(), Some("world"));
    let before = ui.dump_display_list();
    // Item 0 leaves; the items coming take the slots of those leaving.
    state.request_scroll_to_item(500, 0);
    ui.wait_for_idle();
    let display = ui.dump_display_list();
    let _ = before;
    let selected = ui.host().with_tree(|tree| tree.selected_text());
    // A key extending the selection shows it again, on the other item.
    let modifiers = compose_ui::KeyModifiers { shift: true, ..Default::default() };
    for kind in [compose_ui::KeyEventType::KeyDown, compose_ui::KeyEventType::KeyUp] {
        ui.host().send_key(compose_ui::KeyEvent {
            key: compose_ui::Key::DirectionRight,
            kind,
            modifiers,
        });
    }
    ui.wait_for_idle();
    let shown = ui.dump_display_list().contains("#664286F4");
    let extended = ui.host().with_tree(|tree| tree.selected_text());
    assert_eq!(
        (selected.as_deref(), shown),
        (None, false),
        "item 0's selection on another item (extended: {extended:?}):\n{display}"
    );
}
