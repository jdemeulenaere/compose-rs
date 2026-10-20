//! Regression tests from the third runtime review (after 3733426) that need `#[composable]`.

#![allow(non_snake_case)]
#![allow(clippy::redundant_closure)] // deliberate: a closure, as user code writes it

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::Command;
use std::rc::{Rc, Weak};

use compose::runtime::*;

thread_local! {
    static RUNS: Cell<u32> = const { Cell::new(0) };
}

#[composable]
fn Spinner(loading: MutableState<bool>) {
    RUNS.with(|runs| runs.set(runs.get() + 1));
    let _ = loading.get();
}

/// An RAII "busy" indicator: a common pattern, and a `Drop` that writes state.
struct Busy(MutableState<bool>);

impl Drop for Busy {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

/// The body of the finding below; run in a child process, since it aborts.
fn recompose_while_another_panic_unwinds() {
    let loading = mutable_state_of(false);
    let handle = loading.handle();
    let composition = Rc::new(Composition::new(NodeTree::<()>::new()));
    let weak: Weak<Composition> = Rc::downgrade(&composition);
    // A host whose frame requester recomposes at once (docs/04, D49).
    composition.set_frame_requester(move || {
        if let Some(composition) = weak.upgrade() {
            composition.recompose();
        }
    });
    composition.set_content(move || Spinner(handle));

    // An event handler that panics while a `Busy` guard is alive (the app catches panics per
    // event, as a test harness or a UI toolkit's event loop does).
    let result = catch_unwind(AssertUnwindSafe(|| {
        loading.set(true);
        let _busy = Busy(loading.handle());
        panic!("a handler bug");
    }));
    assert!(result.is_err());
    // The guard's write recomposed the spinner while the handler's panic unwound.
    assert!(!loading.get());
    assert!(!composition.is_poisoned());
}

// ---- Finding (major): a pass run while an unrelated panic unwinds aborts the process --------

/// Expected: a pass that runs while some other panic unwinds on the thread (a `Drop` writing
/// state with a synchronous host, or a `Drop` that recomposes) works like any other: the
/// original panic reaches its `catch_unwind`.
///
/// Actual: the generated group guards (`RestartGroup`, `ReplaceGroup`, `Region` in
/// compose-runtime/src/private.rs) decide whether *they* are being dropped by an unwinding panic
/// with `std::thread::panicking()`, which is true for anything that runs while *any* panic
/// unwinds on the thread. Every guard of the nested pass abandons its group instead of ending
/// it; the walk's `finish_restart` (composer.rs ~431) then panics with "the pass ended with
/// groups still open" inside the `Busy` destructor during unwinding, and the process aborts
/// ("panic in a destructor during cleanup"). A guard should compare with the panicking state at its
/// creation (or a panic count) instead.
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn a_pass_while_another_panic_unwinds_does_not_abort() {
    if std::env::var_os("REVIEW3_CHILD").is_some() {
        recompose_while_another_panic_unwinds();
        return;
    }
    let output = Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", "a_pass_while_another_panic_unwinds_does_not_abort", "--nocapture"])
        .env("REVIEW3_CHILD", "1")
        .output()
        .expect("run the child test");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the child process failed ({:?}):\n{stderr}", output.status);
}

// ---- Finding (nit): a panic caught inside a restarted composable isn't diagnosed by name -----

#[composable]
fn Thrower() {
    panic!("a composable bug");
}

/// A plain helper (no region groups of its own).
fn call_and_catch(tick: u32) {
    if tick > 0 {
        let _ = catch_unwind(AssertUnwindSafe(|| Thrower()));
    }
}

#[composable]
fn Catcher(tick: MutableState<u32>) {
    call_and_catch(tick.get());
}

/// D51: "the runtime diagnoses it by name when the groups turn out unbalanced".
///
/// Expected: catching a panic inside a composable that recomposes alone (a restart from the
/// walk) fails with `ComposeError::CaughtPanic`'s message, as it does in a full pass.
///
/// Actual: `finish_restart` (composer.rs ~430) reports `UnclosedGroups` without looking at
/// `abandoned_groups`: "the pass ended with groups still open".
#[test]
fn a_caught_panic_in_a_restarted_composable_is_diagnosed_by_name() {
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let composition = Composition::new(NodeTree::<()>::new());
    composition.set_content(move || Catcher(handle));
    tick.set(1);
    let payload = catch_unwind(AssertUnwindSafe(|| composition.recompose()))
        .expect_err("a caught panic is a misuse");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_default();
    assert!(message.contains("catch_unwind"), "got: {message}");
}
