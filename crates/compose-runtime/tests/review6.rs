//! Regression tests from the sixth adversarial review of the runtime (after 474995f). Each test
//! demonstrated a finding, fixed since; its comment says what used to happen.

use std::cell::RefCell;

use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

/// Runs the named test of this binary in a child process (for scenarios that abort), and
/// asserts that the child succeeded.
fn in_child_process(name: &str, body: fn()) {
    if std::env::var_os("REVIEW6_CHILD").is_some() {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", name, "--nocapture"])
        .env("REVIEW6_CHILD", "1")
        .output()
        .expect("run the child test");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the child process failed ({:?}):\n{stderr}", output.status);
}

// ---- Finding: the other destruction order: the handle's owner outlives the runtime ----------

/// review4's host: it owns its composition and uses it when dropped.
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

fn run_a_host_stored_first() {
    let thread = std::thread::spawn(|| {
        // The most direct way to keep a host in a thread-local: the thread's first use of
        // anything is `HOST`, so `HOST` registers its destructor first, then the runtime (in
        // `Composition::new`), then the exit guard. Destroyed in reverse order: the guard, the
        // runtime, then `HOST`.
        HOST.with(|host| {
            let composition = Composition::new(NodeTree::<()>::new());
            composition.set_content(|| {});
            *host.borrow_mut() = Some(Host { composition });
        });
    });
    assert!(thread.join().is_ok());
}

/// Expected (D55, docs/04 "Disposal"): "a handle still owned (by a thread-local) stays usable as
/// an empty composition until its owner drops it", whatever the order thread-locals are
/// destroyed in (unspecified; here the platform's reverse registration order).
///
/// Actual: review4's test only covers the order guard, `HOST`, runtime. When the owner's
/// thread-local was registered before the runtime (it was first touched before any runtime
/// use, as above), the runtime's thread-local is destroyed before `HOST`: every method of the
/// handle (`with_applier`, `has_invalidations`, `recompose`, `set_content`, ...) goes through
/// `with_runtime` (runtime.rs:99), whose `RUNTIME.with` panics ("cannot access a Thread Local
/// Storage value during or after destruction") inside a TLS destructor, and the process aborts.
/// The guard closed the composition, but the closed handle is only usable while the runtime's
/// thread-local lives (e.g. the runtime could live in a thread-local without a destructor,
/// released by the guard, so it stays reachable from every other thread-local's destructor).
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn a_handle_owned_by_a_thread_local_registered_before_the_runtime_stays_usable() {
    in_child_process(
        "a_handle_owned_by_a_thread_local_registered_before_the_runtime_stays_usable",
        run_a_host_stored_first,
    );
}
