// Adversarial review tests (D220–D226).

use super::*;
use std::collections::HashMap;

thread_local! {
    static OPEN: RefCell<HashMap<u32, MutableState<bool>>> = RefCell::new(HashMap::new());
    static NUM: RefCell<HashMap<u32, MutableState<u32>>> = RefCell::new(HashMap::new());
}

#[composable]
fn Expandable(id: u32) {
    let open = remember_saveable(|| mutable_state_of(false));
    OPEN.with(|map| map.borrow_mut().insert(id, open));
    if open.get() {
        let n = remember_saveable(|| mutable_state_of(0u32));
        NUM.with(|map| map.borrow_mut().insert(id, n));
        BasicText!("{id}: {}", n.get());
    }
}

#[composable]
fn TwoExpandables() {
    Column!({
        Expandable(1);
        Expandable(2);
    });
}

/// Two calls of one composable at two call sites (Compose: two keys). The second is expanded
/// first, so its nested value registers first: restored by order, they swap.
#[test]
fn sibling_calls_whose_nested_values_register_out_of_order_keep_their_values() {
    let mut ui = compose_test(TwoExpandables);
    OPEN.with(|map| map.borrow()[&2].set(true));
    ui.wait_for_idle();
    NUM.with(|map| map.borrow()[&2].set(22));
    OPEN.with(|map| map.borrow()[&1].set(true));
    ui.wait_for_idle();
    NUM.with(|map| map.borrow()[&1].set(11));
    ui.wait_for_idle();
    ui.on_node_with_text("1: 11").assert_exists();
    ui.on_node_with_text("2: 22").assert_exists();
    let mut ui = recreate(ui, TwoExpandables);
    ui.on_node_with_text("1: 11").assert_exists();
    ui.on_node_with_text("2: 22").assert_exists();
}

thread_local! {
    static SHOW_DETAIL: Cell<bool> = const { Cell::new(true) };
    static DETAIL: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
}

/// A pane shown by something not saved (a configuration: landscape only).
#[composable]
fn MaybeDetail() {
    if SHOW_DETAIL.get() {
        let value = remember_saveable(|| mutable_state_of(0));
        DETAIL.set(Some(value));
        BasicText!("detail {}", value.get());
    }
    BasicText!("list");
}

/// Compose's `performSave` starts from the values restored and not yet taken back, so a pane
/// hidden during one recreation still gets its value back after the next one.
#[test]
fn values_restored_but_not_yet_taken_survive_the_next_save() {
    SHOW_DETAIL.set(true);
    let mut ui = compose_test(MaybeDetail);
    DETAIL.get().unwrap().set(9);
    ui.wait_for_idle();
    ui.on_node_with_text("detail 9").assert_exists();
    // Recreated in portrait: no detail pane.
    SHOW_DETAIL.set(false);
    let ui = recreate(ui, MaybeDetail);
    // Recreated in landscape again.
    SHOW_DETAIL.set(true);
    let mut ui = recreate(ui, MaybeDetail);
    ui.on_node_with_text("detail 9").assert_exists();
}

#[test]
fn restored_values_of_another_type_fall_back_to_init() {
    let mut ui = compose_test(SavedCounter);
    COUNT.get().unwrap().set(5);
    ui.wait_for_idle();
    let mut state = ui.host().save_state();
    for values in state.values.values_mut() {
        for value in values.iter_mut() {
            *value = match value {
                SavedValue::Int(_) => SavedValue::Text("x".into()),
                _ => SavedValue::List(vec![SavedValue::Bool(true)]),
            };
        }
    }
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(state);
    let mut ui = ComposeTestRule::with_host(host, SavedCounter);
    ui.on_node_with_text("Count: 0").assert_exists();
    ui.on_node_with_text("Name: first").assert_exists();
}

#[test]
fn corrupt_saved_bytes_never_panic() {
    let mut state = SavedState::default();
    state.values.insert(
        7,
        vec![
            SavedValue::Text("hello".into()),
            SavedValue::List(vec![SavedValue::Int(1), SavedValue::Float(2.0)]),
            SavedValue::Bool(false),
        ],
    );
    state.values.insert(u64::MAX, vec![SavedValue::Int(-1)]);
    let bytes = state.to_bytes();
    for cut in 0..bytes.len() {
        let _ = SavedState::from_bytes(&bytes[..cut]);
    }
    for at in 0..bytes.len() {
        for value in [0u8, 1, 4, 0x7f, 0x80, 0xff] {
            let mut corrupt = bytes.clone();
            corrupt[at] = value;
            let _ = SavedState::from_bytes(&corrupt);
        }
    }
    // Deep nesting and huge lengths.
    let mut deep = b"CRS1\x01\x00\x00\x00".to_vec();
    deep.extend_from_slice(&0u64.to_le_bytes());
    deep.extend_from_slice(&1u32.to_le_bytes());
    for _ in 0..10_000 {
        deep.push(4);
        deep.extend_from_slice(&1u32.to_le_bytes());
    }
    assert_eq!(SavedState::from_bytes(&deep), None);
    let mut huge = b"CRS1\xff\xff\xff\xff".to_vec();
    huge.extend_from_slice(&0u64.to_le_bytes());
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    huge.push(3);
    huge.extend_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(SavedState::from_bytes(&huge), None);
}

// ---- Back ----

#[composable]
fn ScreenUnderStickyDialog() {
    BackHandler!(on_back = || log("screen"));
    Dialog!(
        on_dismiss_request = || log("dismissed"),
        properties = compose_ui::DialogProperties {
            dismiss_on_back_press: false,
            dismiss_on_click_outside: true,
        },
        {
            Box!(modifier = Modifier.size(50.dp()));
        }
    );
}

/// Compose's dialog window takes back while it's shown, dismissing only if
/// `dismiss_on_back_press`: back never reaches the screen beneath (nor finishes the activity).
#[test]
fn a_dialog_that_ignores_back_still_keeps_it_from_the_screen_beneath() {
    let mut ui = compose_test(ScreenUnderStickyDialog);
    ui.host().on_back_pressed();
    assert_eq!(take_log(), Vec::<&str>::new());
}

thread_local! {
    static OUTER: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
    static INNER: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
}

#[composable]
fn NestedDialogs() {
    let outer = remember(|| mutable_state_of(true));
    let inner = remember(|| mutable_state_of(true));
    OUTER.set(Some(outer));
    INNER.set(Some(inner));
    if outer.get() {
        Dialog!(
            on_dismiss_request = move || {
                log("outer");
                outer.set(false)
            },
            {
                Box!(modifier = Modifier.size(50.dp()));
                if inner.get() {
                    Dialog!(
                        on_dismiss_request = move || {
                            log("inner");
                            inner.set(false)
                        },
                        {
                            Box!(modifier = Modifier.size(20.dp()));
                        }
                    );
                }
            }
        );
    }
}

#[test]
fn nested_dialogs_go_back_innermost_first() {
    let mut ui = compose_test(NestedDialogs);
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert!(ui.host().on_back_pressed());
    ui.wait_for_idle();
    assert_eq!(take_log(), ["inner", "outer"]);
    assert!(!ui.host().on_back_pressed());
}

// ---- image_resource in a branch without a region ----

thread_local! {
    static SHOWN: Cell<Option<MutableState<bool>>> = const { Cell::new(None) };
    static AFTER: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
}

#[composable]
fn OptionalIcon() {
    let shown = remember(|| mutable_state_of(false));
    SHOWN.set(Some(shown));
    // Not a likely composable call by name: no region group around it.
    let _icon = shown.get().then(|| compose_ui::remember_image_resource(&[2, 2, 9]));
    let after = remember(|| mutable_state_of(0));
    AFTER.set(Some(after));
    BasicText!("after {}", after.get());
}

#[test]
fn an_optional_image_resource_does_not_reset_later_state() {
    compose_ui::platform::set_image_decoder(Arc::new(FakeDecoder));
    let mut ui = compose_test(OptionalIcon);
    AFTER.get().unwrap().set(5);
    ui.wait_for_idle();
    ui.on_node_with_text("after 5").assert_exists();
    SHOWN.get().unwrap().set(true);
    ui.wait_for_idle();
    ui.on_node_with_text("after 5").assert_exists();
}
