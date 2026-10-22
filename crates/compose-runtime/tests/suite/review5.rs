//! Regression tests from the fifth adversarial review of the runtime (after 4d7102c). Each test
//! demonstrated a finding, fixed since; its comment says what used to happen.

use std::cell::RefCell;
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

/// Runs the named test of this binary in a child process (for scenarios that abort), and
/// asserts that the child succeeded.
fn in_child_process(name: &str, body: fn()) {
    if std::env::var_os("REVIEW5_CHILD").is_some() {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", name, "--nocapture"])
        .env("REVIEW5_CHILD", "1")
        .output()
        .expect("run the child test");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the child process failed ({:?}):\n{stderr}", output.status);
}

// ---- Finding: closing at thread exit drops the frame requester inside the runtime's borrow ---

thread_local! {
    static HOST: RefCell<Option<Composition>> = const { RefCell::new(None) };
}

fn run_a_host_with_a_stateful_frame_requester() {
    let thread = std::thread::spawn(|| {
        // As in review4: `HOST` first, then the exit guard (registered by the first state), so
        // the guard closes the composition before `HOST` drops its handle.
        HOST.with(|host| {
            let ticks = mutable_state_of(0u32);
            let handle = ticks.handle();
            let composition = Composition::new(NodeTree::<()>::new());
            // The host's frame requester owns a piece of state (a frame counter).
            composition.set_frame_requester(move || ticks.set(ticks.get() + 1));
            composition.set_content(move || {
                let _ = handle.get();
            });
            *host.borrow_mut() = Some(composition);
        });
    });
    assert!(thread.join().is_ok());
}

/// Expected: a composition owned by a thread-local is emptied at thread exit and its handle
/// dropped later, without aborting, whatever its frame requester captures.
///
/// Actual: `Disposal::close` (composition.rs ~470-478) replaces the composition's data with
/// `CompositionData::closed(..)` and then sets `disposal.data = None`, dropping the old data,
/// and with it the frame requester closure, *inside* `try_with_runtime`. The closure owns an
/// `OwnedState`, whose `Drop` (`retire`) calls `with_runtime` again: "the runtime was re-entered
/// while busy" panics inside a TLS destructor and the process aborts. `Disposal::take` (the
/// path before f843d9d) drops the data in `Disposal::run`, outside the borrow.
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn closing_at_thread_exit_drops_the_frame_requester_outside_the_runtime() {
    in_child_process(
        "closing_at_thread_exit_drops_the_frame_requester_outside_the_runtime",
        run_a_host_with_a_stateful_frame_requester,
    );
}

/// A host kept alive by a reference cycle through its frame requester (the requester schedules
/// a frame on the host that owns the composition).
struct App {
    composition: RefCell<Option<Composition>>,
}

fn run_a_leaked_app() {
    let thread = std::thread::spawn(|| {
        let app = Rc::new(App { composition: RefCell::new(None) });
        let composition = Composition::new(NodeTree::<()>::new());
        let scheduler = app.clone();
        composition.set_frame_requester(move || {
            let _ = scheduler.composition.borrow().as_ref().map(Composition::has_invalidations);
        });
        composition.set_content(|| {});
        *app.composition.borrow_mut() = Some(composition);
    });
    assert!(thread.join().is_ok());
}

/// Expected: a composition leaked through an `Rc` cycle is disposed at thread exit, as the exit
/// guard documents ("one kept in a thread-local, or leaked"), without aborting.
///
/// Actual: closing drops the frame requester inside the runtime's borrow (see above); it holds
/// the last strong reference to `App`, whose `Composition` drops, and `Composition::drop` calls
/// `try_with_runtime` while the runtime is borrowed and not unwinding: `reentered()` panics in a
/// TLS destructor and the process aborts. Before f843d9d, the data (and its requester) dropped
/// in `Disposal::run`, outside the borrow, and the inner `Composition::drop` found the
/// composition already removed.
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn a_composition_leaked_through_its_frame_requester_is_disposed_at_thread_exit() {
    in_child_process(
        "a_composition_leaked_through_its_frame_requester_is_disposed_at_thread_exit",
        run_a_leaked_app,
    );
}
