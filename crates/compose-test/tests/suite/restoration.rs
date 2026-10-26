//! `StateRestorationTester`: an activity's recreation, emulated.

#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;

thread_local! {
    static STATE: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
    static PLAIN: Cell<Option<MutableState<i32>>> = const { Cell::new(None) };
}

#[composable]
fn Counters() {
    let saved = remember_saveable(|| mutable_state_of(0));
    let plain = remember(|| mutable_state_of(0));
    STATE.with(|cell| cell.set(Some(saved)));
    PLAIN.with(|cell| cell.set(Some(plain)));
    BasicText!("saved {} plain {}", saved.get(), plain.get());
}

// Compose: compose/runtime/runtime-saveable/src/androidDeviceTest/kotlin/androidx/compose/runtime/saveable/RememberSaveableWithMutableStateTest.kt, simpleRestore
/// A `remember_saveable` state comes back with its value; a `remember` one starts over.
#[test]
fn a_saveable_state_comes_back_and_a_remembered_one_starts_over() {
    let mut ui = ComposeTestRule::empty(default_host());
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut ui, Counters);
    ui.run_on_ui_thread(|_| {
        let saved = STATE.with(|cell| cell.take()).expect("composed");
        assert_eq!(saved.get(), 0);
        saved.set(1);
        PLAIN.with(|cell| cell.take()).expect("composed").set(5);
    });
    ui.on_node_with_text("saved 1 plain 5").assert_exists();
    tester.emulate_save_and_restore(&mut ui);
    ui.run_on_ui_thread(|_| {
        assert_eq!(STATE.with(|cell| cell.get()).expect("composed again").get(), 1);
        assert_eq!(PLAIN.with(|cell| cell.get()).expect("composed again").get(), 0);
    });
    ui.on_node_with_text("saved 1 plain 0").assert_exists();
    // Twice: what was restored is saved again.
    STATE.with(|cell| cell.get()).unwrap().set(2);
    tester.emulate_save_and_restore(&mut ui);
    ui.on_node_with_text("saved 2 plain 0").assert_exists();
}

/// The content is disposed in between: its effects end and start again.
#[test]
fn the_content_is_disposed_and_composed_again() {
    let (starts, ends) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (started, ended) = (starts.clone(), ends.clone());
    let mut ui = ComposeTestRule::empty(default_host());
    let mut tester = StateRestorationTester::new();
    tester.set_content(&mut ui, move || {
        let (started, ended) = (started.clone(), ended.clone());
        DisposableEffect((), move |scope| {
            started.set(started.get() + 1);
            let ended = ended.clone();
            scope.on_dispose(move || ended.set(ended.get() + 1))
        });
    });
    assert_eq!((starts.get(), ends.get()), (1, 0));
    tester.emulate_save_and_restore(&mut ui);
    assert_eq!((starts.get(), ends.get()), (2, 1));
}

#[test]
#[should_panic(expected = "set_content should be called first")]
fn restoring_before_any_content_fails() {
    let mut ui = ComposeTestRule::empty(default_host());
    StateRestorationTester::new().emulate_save_and_restore(&mut ui);
}
