//! Regression test from the eighth adversarial review of the runtime (after 5901236): it
//! demonstrated a finding, fixed since; its comment says what used to happen.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

/// Counts the bytes live on every thread (thread exit must be visible, so not thread-local).
struct Counting;

static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);

// SAFETY: forwards every call to the system allocator unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(layout.size() as i64, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as i64, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE_BYTES.fetch_add(new_size as i64 - layout.size() as i64, Ordering::Relaxed);
        // SAFETY: same contract as the caller's.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs the named test of this binary alone in a child process (so no other test allocates
/// meanwhile), and asserts that the child succeeded.
fn in_child_process(name: &str, body: fn()) {
    if std::env::var_os("REVIEW8_CHILD").is_some() {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("REVIEW8_CHILD", "1")
        .output()
        .expect("run the child test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the child process failed ({:?}):\n{stdout}\n{stderr}",
        output.status
    );
}

// ---- Finding: a thread that only uses state (no composition) still leaks its arena ---------

/// A worker that keeps `cells` states (a view model, a test's fixtures), then drops them all.
fn states_on_their_own_thread(cells: usize) {
    std::thread::spawn(move || {
        let states: Vec<OwnedState<u32>> =
            (0..cells).map(|index| mutable_state_of(index as u32)).collect();
        let sum: u32 = states.iter().map(|state| state.get()).sum();
        assert!(sum > 0);
        drop(states);
        assert_eq!(live_state_count(), 0);
    })
    .join()
    .expect("the worker's thread");
}

fn run_states_on_threads() {
    // Warm up std's own per-process allocations (thread bookkeeping, ...).
    states_on_their_own_thread(1000);
    states_on_their_own_thread(1000);
    let before = LIVE_BYTES.load(Ordering::SeqCst);
    for _ in 0..20 {
        states_on_their_own_thread(1000);
    }
    let leaked = LIVE_BYTES.load(Ordering::SeqCst) - before;
    assert!(leaked < 20 * 1024, "20 threads that dropped every state leaked {leaked} bytes");
}

/// Expected (docs/04 "Disposal", D55, 60d0c39): "a thread that dropped everything leaks
/// nothing".
///
/// Actual: the reset (`release_if_empty`, runtime.rs) only runs once `Runtime::exiting` is set,
/// and only the exit guard sets it; the guard is registered by `Composition::new` alone
/// (composition.rs:44). A thread that creates state cells but never a composition never
/// registers the guard, so `exiting` stays false and the cell arena (one `CellEntry` per peak
/// cell, plus the free list) leaks at thread exit: here ~78 KB per thread for 1000 dropped
/// states, without bound across threads. E.g. `alloc_cell` could register the guard too.
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn threads_that_drop_every_state_they_made_leak_nothing() {
    in_child_process("threads_that_drop_every_state_they_made_leak_nothing", run_states_on_threads);
}
