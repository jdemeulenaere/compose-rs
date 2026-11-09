//! Compose's `ActivityRecreationTest`: what `remember` keeps is lost when the activity is
//! recreated, what `remember_saveable` keeps comes back, in a `SaveableStateHolder` too and
//! through list and map savers. The activity is a `UiHost`: its state saved to bytes, it's dropped,
//! and a new one composes the same content from them.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_runtime::*;
use compose_test::*;
use compose_ui::platform::UiHost;

/// The activity recreated: its host's state saved, the host dropped, and a new one composing
/// `content` from what was saved, through bytes.
fn recreate(mut ui: ComposeTestRule, content: impl Fn() + 'static) -> ComposeTestRule {
    let bytes = ui.host().save_state().to_bytes();
    drop(ui);
    let host = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host.restore_state(SavedState::from_bytes(&bytes).expect("saved state bytes"));
    ComposeTestRule::with_host(host, content)
}

thread_local! {
    static ARRAY: Cell<Option<Rc<Cell<i32>>>> = const { Cell::new(None) };
    static STATE: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
    static STATE_AS_MAP: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
}

fn array() -> Rc<Cell<i32>> {
    let array = ARRAY.take().expect("composed");
    ARRAY.set(Some(array.clone()));
    array
}

fn state() -> MutableState<i32> {
    STATE.get().expect("composed")
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, valueStoredViaRememberIsLost
#[test]
fn a_remembered_value_is_lost() {
    let content = || ARRAY.set(Some(remember(|| Rc::new(Cell::new(0)))));
    let ui = compose_test(content);
    assert_eq!(array().get(), 0);
    array().set(1);
    let _ui = recreate(ui, content);
    assert_eq!(array().get(), 0);
}

/// Compose's `IntArray` changed in place, saved as the number it holds.
fn array_saver() -> impl Saver<Rc<Cell<i32>>> {
    saver(
        |array: &Rc<Cell<i32>>| Some(SavedValue::Int(i64::from(array.get()))),
        |saved| match saved {
            SavedValue::Int(value) => Some(Rc::new(Cell::new(*value as i32))),
            _ => None,
        },
    )
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, valueStoredViaRememberSavedInstanceStateRestored
#[test]
fn a_saveable_value_comes_back() {
    let content =
        || ARRAY.set(Some(remember_saveable_with(array_saver(), || Rc::new(Cell::new(0)))));
    let ui = compose_test(content);
    assert_eq!(array().get(), 0);
    array().set(1);
    let _ui = recreate(ui, content);
    assert_eq!(array().get(), 1);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, valueStoredInMutableStateIsRestored
#[test]
fn a_saveable_state_comes_back() {
    let content = || STATE.set(Some(remember_saveable(|| mutable_state_of(0))));
    let ui = compose_test(content);
    assert_eq!(state().get(), 0);
    state().set(1);
    let _ui = recreate(ui, content);
    assert_eq!(state().get(), 1);
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, valueStoredInMutableStateInsideSaveableStateHolderIsRestored
#[test]
fn a_saveable_state_in_a_holder_comes_back() {
    let content = || {
        let holder = remember_saveable_state_holder();
        holder.SaveableStateProvider(0, || {
            STATE.set(Some(remember_saveable(|| mutable_state_of(0))))
        });
    };
    let ui = compose_test(content);
    assert_eq!(state().get(), 0);
    state().set(1);
    let _ui = recreate(ui, content);
    assert_eq!(state().get(), 1);
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Count(i32);

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, valueStoredInMutableStateWithCustomSaverInsideSaveableStateHolderIsRestored
#[test]
fn a_state_saved_by_its_saver_in_a_holder_comes_back() {
    thread_local! {
        static COUNT: Cell<Option<MutableState<Count>>> = const { Cell::new(None) };
    }
    let count_saver = || {
        saver(
            |count: &Count| Some(SavedValue::Int(i64::from(count.0))),
            |saved| match saved {
                SavedValue::Int(value) => Some(Count(*value as i32)),
                _ => None,
            },
        )
    };
    let content = move || {
        let holder = remember_saveable_state_holder();
        holder.SaveableStateProvider(0, || {
            COUNT.set(Some(remember_saveable_with(count_saver(), || mutable_state_of(Count(0)))));
        });
    };
    let ui = compose_test(content);
    assert_eq!(COUNT.get().expect("composed").get(), Count(0));
    COUNT.get().expect("composed").set(Count(1));
    let _ui = recreate(ui, content);
    assert_eq!(COUNT.get().expect("composed").get(), Count(1));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, stateStoredViaListSaverOrMapSaverIsRestored
#[test]
fn states_saved_through_a_list_saver_and_a_map_saver_come_back() {
    let content = || {
        let holder = remember_saveable_state_holder();
        holder.SaveableStateProvider(0, || {
            let as_list =
                list_saver(|value: &i32| vec![*value], |items: Vec<i32>| items.first().copied());
            STATE.set(Some(remember_saveable_with(as_list, || mutable_state_of(0))));
            let as_map = map_saver(
                |value: &i32| vec![("value".to_string(), SavedValue::Int(i64::from(*value)))],
                |entries: HashMap<String, SavedValue>| match entries.get("value") {
                    Some(SavedValue::Int(value)) => Some(*value as i32),
                    _ => None,
                },
            );
            STATE_AS_MAP.set(Some(remember_saveable_with(as_map, || mutable_state_of(0))));
        });
    };
    let ui = compose_test(content);
    assert_eq!((state().get(), STATE_AS_MAP.get().expect("composed").get()), (0, 0));
    state().set(1);
    STATE_AS_MAP.get().expect("composed").set(2);
    let _ui = recreate(ui, content);
    assert_eq!((state().get(), STATE_AS_MAP.get().expect("composed").get()), (1, 2));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, valueStoredInTwoViewsBothRestored
#[test]
fn values_in_two_hosts_both_come_back() {
    thread_local! {
        static ARRAY1: Cell<Option<Rc<Cell<i32>>>> = const { Cell::new(None) };
        static ARRAY2: Cell<Option<Rc<Cell<i32>>>> = const { Cell::new(None) };
    }
    let content1 =
        || ARRAY1.set(Some(remember_saveable_with(array_saver(), || Rc::new(Cell::new(0)))));
    let content2 =
        || ARRAY2.set(Some(remember_saveable_with(array_saver(), || Rc::new(Cell::new(0)))));

    let mut host1 = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    let mut host2 = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    host1.set_content(content1);
    host2.set_content(content2);
    let _ = host1.frame();
    let _ = host2.frame();

    let take1 = || {
        let a = ARRAY1.take().expect("composed 1");
        ARRAY1.set(Some(a.clone()));
        a
    };
    let take2 = || {
        let a = ARRAY2.take().expect("composed 2");
        ARRAY2.set(Some(a.clone()));
        a
    };
    assert_eq!((take1().get(), take2().get()), (0, 0));
    take1().set(1);
    take2().set(2);

    let bytes1 = host1.save_state().to_bytes();
    let bytes2 = host2.save_state().to_bytes();
    drop(host1);
    drop(host2);
    ARRAY1.set(None);
    ARRAY2.set(None);

    let mut restored1 = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    let mut restored2 = UiHost::headless(DEFAULT_SIZE.width, DEFAULT_SIZE.height);
    restored1.restore_state(SavedState::from_bytes(&bytes1).expect("saved state 1"));
    restored2.restore_state(SavedState::from_bytes(&bytes2).expect("saved state 2"));
    restored1.set_content(content1);
    restored2.set_content(content2);
    let _ = restored1.frame();
    let _ = restored2.frame();

    assert_eq!((take1().get(), take2().get()), (1, 2));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, stateInsidePopupIsRestored
#[test]
fn state_inside_popups_comes_back() {
    use compose_ui::Popup;
    thread_local! {
        static NEXT: Cell<i32> = const { Cell::new(10) };
        static POPUP1: Cell<Option<i32>> = const { Cell::new(None) };
        static POPUP2: Cell<Option<i32>> = const { Cell::new(None) };
    }
    let next_int = || {
        let value = NEXT.get();
        NEXT.set(value + 1);
        value
    };
    let content = move || {
        Popup!({
            POPUP1.set(Some(remember_saveable(next_int)));
        });
        Popup!({
            POPUP2.set(Some(remember_saveable(next_int)));
        });
    };
    let ui = compose_test(content);
    let initial1 = POPUP1.take().expect("popup 1 composed");
    let initial2 = POPUP2.take().expect("popup 2 composed");
    assert_ne!(initial1, initial2);

    let _ui = recreate(ui, content);
    assert_eq!(POPUP1.take(), Some(initial1));
    assert_eq!(POPUP2.take(), Some(initial2));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, stateInsideDialogIsRestored
#[test]
fn state_inside_dialogs_comes_back() {
    use compose_ui::Dialog;
    thread_local! {
        static NEXT: Cell<i32> = const { Cell::new(100) };
        static DIALOG1: Cell<Option<i32>> = const { Cell::new(None) };
        static DIALOG2: Cell<Option<i32>> = const { Cell::new(None) };
    }
    let next_int = || {
        let value = NEXT.get();
        NEXT.set(value + 1);
        value
    };
    let content = move || {
        Dialog!(on_dismiss_request = || {}, {
            DIALOG1.set(Some(remember_saveable(next_int)));
        });
        Dialog!(on_dismiss_request = || {}, {
            DIALOG2.set(Some(remember_saveable(next_int)));
        });
    };
    let ui = compose_test(content);
    let initial1 = DIALOG1.take().expect("dialog 1 composed");
    let initial2 = DIALOG2.take().expect("dialog 2 composed");
    assert_ne!(initial1, initial2);

    let _ui = recreate(ui, content);
    assert_eq!(DIALOG1.take(), Some(initial1));
    assert_eq!(DIALOG2.take(), Some(initial2));
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/ActivityRecreationTest.kt, movableContentInAdaptiveLayoutUseCaseIsRestored
#[test]
fn movable_content_in_an_adaptive_layout_comes_back() {
    use compose_foundation::BasicText;
    use compose_foundation::layout::*;
    use compose_ui::Modifier;

    thread_local! {
        static IS_TABLET: Cell<bool> = const { Cell::new(false) };
        static COMPOSED_AS_TABLET: Cell<bool> = const { Cell::new(false) };
    }

    #[composable]
    fn AdaptiveScreen() {
        let list_content = remember(|| {
            movable_content_of(|| {
                ARRAY.set(Some(remember_saveable_with(array_saver(), || Rc::new(Cell::new(0)))));
                BasicText!("List screen");
            })
        });
        if !IS_TABLET.get() {
            COMPOSED_AS_TABLET.set(false);
            list_content();
        } else {
            COMPOSED_AS_TABLET.set(true);
            Row!(|scope: RowScope| {
                let list_content = list_content.clone();
                Box!(modifier = Modifier.weight(scope, 1.0).fill_max_height(), {
                    list_content();
                });
                Box!(modifier = Modifier.weight(scope, 1.0).fill_max_height(), {
                    BasicText!("Details screen");
                });
            });
        }
    }

    IS_TABLET.set(false);
    let ui = compose_test(AdaptiveScreen);
    assert!(!COMPOSED_AS_TABLET.get());
    assert_eq!(array().get(), 0);
    array().set(1);

    IS_TABLET.set(true);
    let mut ui = recreate(ui, AdaptiveScreen);
    assert!(COMPOSED_AS_TABLET.get());
    assert_eq!(array().get(), 1);
    ui.on_node_with_text("List screen").assert_exists();
    ui.on_node_with_text("Details screen").assert_exists();
}
