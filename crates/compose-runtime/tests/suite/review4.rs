//! Findings of the fourth adversarial review of the runtime (after 10a535b). Each test fails;
//! its comment says what is expected and what happens instead.

use std::cell::RefCell;

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

/// Runs the named test of this binary in a child process (for scenarios that abort), and
/// asserts that the child succeeded.
fn in_child_process(name: &str, body: fn()) {
    if std::env::var_os("REVIEW4_CHILD").is_some() {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", name, "--nocapture"])
        .env("REVIEW4_CHILD", "1")
        .output()
        .expect("run the child test");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the child process failed ({:?}):\n{stderr}", output.status);
}

// ---- Finding: the exit guard disposes a composition its owner still uses --------------------

/// A host that owns its composition and unmounts its tree when dropped: a `Drop` that uses the
/// composition it owns.
struct Host {
    composition: Composition,
}

impl Drop for Host {
    fn drop(&mut self) {
        // E.g. detach the platform views the tree holds.
        self.composition.with_applier(|tree: &mut NodeTree<()>| tree.len());
    }
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn run_a_host_in_a_thread_local() {
    let thread = std::thread::spawn(|| {
        // `HOST` is registered first; the app's state, created inside, registers the exit guard
        // after it. Thread-locals are destroyed in reverse order: the guard (which closes the
        // composition), then `HOST`, whose `Drop` uses the closed handle. (The runtime itself has
        // no destructor.)
        HOST.with(|host| {
            let count = mutable_state_of(0u32);
            let handle = count.handle();
            std::mem::forget(count);
            let composition = Composition::new(NodeTree::<()>::new());
            composition.set_content(move || {
                let _ = handle.get();
            });
            *host.borrow_mut() = Some(Host { composition });
        });
    });
    assert!(thread.join().is_ok());
}

/// Expected: a `Composition` stays usable for as long as its owner holds it: the `Host` is
/// dropped at thread exit while the runtime's thread-local is still alive, so its `Drop` can use
/// the composition, as it could before the exit guard existed.
///
/// Actual: `ExitGuard::drop` (composition.rs ~182) disposes every live composition, including
/// those still owned by a `Composition` value that is dropped later (here by `HOST`, destroyed
/// after the guard and before the runtime). `Host::drop` then calls `with_applier`, which
/// panics in `Composition::data` ("a live composition", composition.rs ~170) inside a TLS
/// destructor, and the process aborts. Any method of the still-owned handle does the same
/// (`recompose`, `has_invalidations`, `is_poisoned`, `set_frame_requester`, ...).
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn a_composition_owned_by_a_thread_local_stays_usable_until_its_owner_drops() {
    in_child_process(
        "a_composition_owned_by_a_thread_local_stays_usable_until_its_owner_drops",
        run_a_host_in_a_thread_local,
    );
}
