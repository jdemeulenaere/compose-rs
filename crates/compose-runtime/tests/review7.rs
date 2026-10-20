//! Regression test from the seventh adversarial review of the runtime (after ee102b9): it
//! demonstrated a finding, fixed since; its comment says what used to happen.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, Ordering};

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
    if std::env::var_os("REVIEW7_CHILD").is_some() {
        body();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env("REVIEW7_CHILD", "1")
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

// ---- Finding: every thread that used the runtime leaks its arenas at exit -------------------

/// A window with `cells` remembered states, composed then dropped: everything it made is
/// released before the thread exits.
fn a_window_on_its_own_thread(cells: usize) {
    std::thread::spawn(move || {
        let composition = Composition::new(NodeTree::<u32>::new());
        composition.set_content(move || {
            for index in 0..cells {
                key(index, || {
                    let state = remember(|| mutable_state_of(index as u32));
                    let _ = state.get();
                });
            }
        });
        drop(composition);
        assert_eq!(live_state_count(), 0);
        assert_eq!(live_scope_count(), 0);
    })
    .join()
    .expect("the window's thread");
}

fn run_windows_on_threads() {
    // Warm up std's own per-process allocations (thread bookkeeping, ...).
    a_window_on_its_own_thread(1000);
    a_window_on_its_own_thread(1000);
    let before = LIVE_BYTES.load(Ordering::SeqCst);
    for _ in 0..20 {
        a_window_on_its_own_thread(1000);
    }
    let leaked = LIVE_BYTES.load(Ordering::SeqCst) - before;
    assert!(leaked < 20 * 1024, "20 threads that dropped everything leaked {leaked} bytes");
}

/// Expected: a thread that drops its compositions and state before exiting leaves nothing
/// behind (as before ee102b9: 0 bytes with this test); allocations.rs's
/// `dropping_a_composition_frees_what_it_allocated` only checks one long-lived thread.
///
/// Actual: the runtime's thread-local is `ManuallyDrop` (runtime.rs:27) and nothing ever frees
/// its contents, so every thread that touched the runtime leaks its arenas at exit: the cell
/// arena (one `CellEntry` per peak cell, each with its `readers` buffer), the scope and
/// composition arenas, the free lists, and the pass scratch vectors (`frames`,
/// `removal_stack`, `frame_requests`). Here ~135 KB per thread for a 1000-state window that
/// was fully dropped; it grows with each thread's peak size, without bound across threads (a
/// host running each window, document or job on its own thread). D55 and docs/04 describe the
/// leak as "what remains", but the arenas leak even when nothing remains. E.g. the exit guard,
/// and any later drop that leaves no composition and no cell live, could reset the runtime to
/// `Runtime::new()` (stale handles then read as freed, which they are).
#[test]
#[cfg_attr(miri, ignore = "spawns a process")]
fn threads_that_drop_everything_they_composed_leak_nothing() {
    in_child_process(
        "threads_that_drop_everything_they_composed_leak_nothing",
        run_windows_on_threads,
    );
}
