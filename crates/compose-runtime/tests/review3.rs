//! Regression tests from the third adversarial review of the runtime (after 3733426). Each test
//! demonstrated one finding, fixed since; the comment says what used to happen.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::{Rc, Weak};

use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

fn new_composition() -> Composition {
    Composition::new(NodeTree::<()>::new())
}

/// A frame requester that counts its calls.
fn counting_requester(composition: &Composition) -> Rc<Cell<u32>> {
    let requests = Rc::new(Cell::new(0));
    let counter = requests.clone();
    composition.set_frame_requester(move || counter.set(counter.get() + 1));
    requests
}

/// A host whose frame requester recomposes at once (docs/04, D49: "a frame requester may
/// recompose synchronously").
fn synchronous_host(composition: &Rc<Composition>) {
    let weak: Weak<Composition> = Rc::downgrade(composition);
    composition.set_frame_requester(move || {
        if let Some(composition) = weak.upgrade() {
            composition.recompose();
        }
    });
}

// ---- Finding 1 (major): retired cells are freed before the pass's end-of-pass work ----------

/// Rule R2 (docs/05): a retired cell "is freed at the end of the next pass", and `Pass::finish`
/// documents its order as "releases removed nodes, forgets removed values, dispatches new
/// remember observers, runs side effects, frees retired state cells".
///
/// Expected: a row removed by the pass that follows the removal of its item still sees the
/// item's (retired, detached) state in its `DisposableEffect` cleanup: the cleanup is the row's
/// handler, and it runs in that pass's end-of-pass work.
///
/// Actual: `Pass::finish` (composition.rs ~298-303) calls `cells.end_pass()`, which frees the
/// cells retired before the pass (bumping their generation), *before* `apply_deferred` runs the
/// forgotten callbacks and side effects; only the values' drops wait. The cleanup's
/// `title.try_get()` returns `None` (and `title.get()` would panic with "used after it was freed").
#[test]
fn a_removed_rows_cleanup_still_sees_its_retired_state() {
    let ids = mutable_state_of_with(vec![1u32, 2], never_equal_policy());
    let titles: Rc<RefCell<HashMap<u32, OwnedState<String>>>> = Rc::new(RefCell::new(
        [(1, mutable_state_of("one".to_string())), (2, mutable_state_of("two".to_string()))]
            .into_iter()
            .collect(),
    ));
    let seen: Rc<RefCell<Vec<Option<String>>>> = Rc::default();
    let composition = new_composition();
    let (ids_handle, model, log) = (ids.handle(), titles.clone(), seen.clone());
    composition.set_content(move || {
        for id in ids_handle.get() {
            let title = model.borrow()[&id].handle();
            let log = log.clone();
            key(id, move || {
                DisposableEffect(id, move |scope| {
                    scope.on_dispose(move || log.borrow_mut().push(title.try_get()))
                });
            });
        }
    });

    // A handler removes item 2 from the model: its state owner drops, retiring the cell.
    ids.edit(|ids| ids.retain(|&id| id != 2));
    titles.borrow_mut().remove(&2);
    assert!(composition.recompose());
    assert_eq!(*seen.borrow(), vec![Some("two".to_string())], "the cleanup sees its state");
}

// ---- Finding 2 (major): a panicking effect loses the frame request forever -------------------

/// `Pass::finish` says "a panic in an effect doesn't poison it": the composition is meant to
/// keep working after an effect panics.
///
/// Expected: an effect that writes state the content reads and then panics leaves the
/// composition invalid with a frame requested (now, or at least on the next write).
///
/// Actual: the write happens while the pass is finishing, so `request_frames` holds the request
/// (runtime.rs ~131-136) and relies on the `request_frames()` call at the end of `Pass::finish`
/// (composition.rs ~318), which the panic skips. The `Finishing` guard sees `pending` already
/// set and queues nothing more. The request sits in `runtime.frame_requests` until some other,
/// unrelated invalidation calls `request_frames`; meanwhile `pending` stays true, so every later
/// write to this composition's state requests nothing either. The composition is stuck invalid.
#[test]
fn a_panicking_effect_does_not_lose_the_frame_request() {
    let count = mutable_state_of(0u32);
    let handle = count.handle();
    let composition = new_composition();
    let requests = counting_requester(&composition);
    let armed = Rc::new(Cell::new(false));
    let trigger = armed.clone();
    composition.set_content(move || {
        let _ = handle.get();
        let trigger = trigger.clone();
        SideEffect(move || {
            if trigger.replace(false) {
                handle.set(handle.get() + 1);
                panic!("an effect bug");
            }
        });
    });
    armed.set(true);
    count.set(10);
    assert_eq!(requests.get(), 1);
    assert!(catch_unwind(AssertUnwindSafe(|| composition.recompose())).is_err());
    assert!(!composition.is_poisoned());
    assert!(composition.has_invalidations());
    // Later writes, from handlers: still nothing.
    count.set(20);
    count.set(30);
    assert!(
        requests.get() > 1,
        "the composition is invalid but no frame was requested after the effect's write \
         ({} request(s) in total)",
        requests.get()
    );
}

// ---- Finding 3 (minor): a panicking frame requester loses the other compositions' requests --

/// Expected: when one frame requester panics (a synchronous host whose recomposition panics), the
/// other compositions invalidated by the same write still get their frame.
///
/// Actual: `request_frames` (runtime.rs ~131-145) takes the whole queue and calls the requesters
/// in a loop; the panic drops the rest of the taken vector. Their compositions keep `pending`
/// set, so no later write requests a frame for them again: they are stuck invalid.
#[test]
fn a_panicking_frame_requester_does_not_lose_the_others() {
    let shared = mutable_state_of(0u32);
    let handle = shared.handle();
    let first = new_composition();
    let second = new_composition();
    first.set_frame_requester(|| panic!("the first host's frame failed"));
    let requests = counting_requester(&second);
    first.set_content(move || {
        let _ = handle.get();
    });
    second.set_content(move || {
        let _ = handle.get();
    });
    assert!(catch_unwind(AssertUnwindSafe(|| shared.set(1))).is_err());
    shared.set(2);
    assert!(second.has_invalidations());
    assert!(requests.get() >= 1, "the second composition never got a frame request");
}

// ---- Finding 4 (major): with a synchronous host, R2's "later in the same handler" breaks -----

/// R2 (docs/05) promises that code later in the same handler still sees a live, detached cell,
/// "the next pass" being the one that runs after the handler. A frame requester that recomposes
/// synchronously (D49) runs that pass inside the handler, at its next write: no finite deferral
/// survives that, so the guarantee holds for hosts that run frames after handlers, and docs/05
/// says so (D53). This checks both sides of the documented behaviour.
#[test]
fn a_dropped_state_lives_until_the_pass_after_the_handler() {
    let tick = mutable_state_of(0u32);
    let tick_handle = tick.handle();
    // A host that runs the frame after the handler: the cell is live for the whole handler.
    let composition = new_composition();
    composition.set_content(move || {
        let _ = tick_handle.get();
    });
    let owner = mutable_state_of("detached".to_string());
    let title = owner.handle();
    drop(owner);
    tick.set(1);
    assert_eq!(title.try_get(), Some("detached".to_string()), "later in the same handler");
    composition.recompose();
    assert_eq!(title.try_get(), None, "freed at the end of the next pass");

    // A synchronous host runs that pass inside the write.
    let composition = Rc::new(new_composition());
    synchronous_host(&composition);
    composition.set_content(move || {
        let _ = tick_handle.get();
    });
    let owner = mutable_state_of("detached".to_string());
    let title = owner.handle();
    drop(owner);
    tick.set(2);
    assert_eq!(title.try_get(), None, "the synchronous pass ran inside the handler");
}

// ---- Finding 5 (minor): cells retired by a disposal are freed before anyone's next pass ------

/// R2 (docs/05) and D34: "dropping a composition frees the cells *it* retired". The
/// implementation frees every cell retired while a disposal runs, including cells owned by the
/// app or by other compositions that a cleanup happened to drop.
///
/// Expected: a popup whose `DisposableEffect` cleanup clears the app's selection (dropping a
/// state owner the main composition's rows still hold handles to) retires that cell as any
/// handler would: it stays alive until the end of the main composition's next pass, so a row's
/// click handler invoked before that pass still works.
///
/// Actual: `retire` (state.rs ~451) puts it in `held_by_disposals`, and `Disposal::run`
/// (composition.rs ~445-457) frees it as soon as the disposal ends.
#[test]
fn a_cell_dropped_by_a_popups_cleanup_survives_until_the_next_pass() {
    let selection: Rc<RefCell<Option<OwnedState<String>>>> =
        Rc::new(RefCell::new(Some(mutable_state_of("item".to_string()))));
    let selected = selection.borrow().as_ref().unwrap().handle();

    let main = new_composition();
    main.set_content(move || {
        let _ = selected.get();
    });
    let popup = new_composition();
    let model = selection.clone();
    popup.set_content(move || {
        let model = model.clone();
        DisposableEffect(0u32, move |scope| {
            scope.on_dispose(move || {
                model.borrow_mut().take();
            })
        });
    });

    drop(popup);
    // A row's click handler in `main`, before `main` recomposes.
    assert_eq!(selected.try_get(), Some("item".to_string()), "alive until main's next pass");
}

// ---- Finding 6 (minor): a misuse drops a remembered value while the runtime is borrowed -----

thread_local! {
    static ALIVE_IN_DROP: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct Probe(MutableState<u32>);

impl Drop for Probe {
    fn drop(&mut self) {
        // User code in a `Drop`, touching the runtime as user code may.
        let alive = self.0.is_alive();
        ALIVE_IN_DROP.with(|log| log.borrow_mut().push(alive));
    }
}

/// The runtime's soundness rule (runtime.rs): "no user code runs while the runtime is borrowed
/// ... Values the runtime gives up are moved out first and dropped after the borrow ends."
///
/// Expected: the misuse the `SlotOutOfOrder` message describes ("remember's calculation must not
/// call remember") panics cleanly, and the remembered value's `Drop` runs outside the borrow.
///
/// Actual: `slot_set` (remember.rs ~60, composer.rs ~553) moves the value into the
/// `with_runtime` closure, and `misuse` panics inside it: the value drops while unwinding with
/// the runtime still borrowed. Its `Drop` sees a live cell as dead (`is_alive()` is false, since
/// `try_with_runtime` can't borrow); a `Drop` calling a non-`try` API (`get`, `set`) panics
/// with "the runtime was re-entered" while unwinding, which aborts the process. The same holds
/// for `put_kept` (composer.rs ~491) and any other `with_runtime` closure that owns a user value
/// when a misuse check fires.
#[test]
fn a_misuse_does_not_drop_user_values_inside_the_runtime() {
    let live = mutable_state_of(0u32);
    let handle = live.handle();
    let composition = new_composition();
    let result = catch_unwind(AssertUnwindSafe(|| {
        composition.set_content(move || {
            let _: u32 = remember(|| {
                let _ = remember(|| Probe(handle));
                0
            });
        })
    }));
    assert!(result.is_err(), "the nested remember is a misuse");
    let alive = ALIVE_IN_DROP.with(|log| std::mem::take(&mut *log.borrow_mut()));
    assert!(!alive.is_empty());
    assert!(
        alive.iter().all(|&alive| alive),
        "a Drop saw the live cell as dead, so it ran inside the runtime's borrow: {alive:?}"
    );
}

// ---- Finding 7 (minor): a frame requester set after an invalidation never fires -------------

/// Expected: a host that sets its frame requester after the first `set_content` (a natural
/// order: build the composition, compose it, then hook it to the event loop) gets a frame for an
/// invalidation the first pass's effects made, as a Compose `Recomposer` would.
///
/// Actual: `invalidate_scope_parts` (runtime.rs ~173-179) sets `pending` even when there is no
/// requester, and `set_frame_requester` (composition.rs ~71) doesn't look at `pending`: the new
/// requester is never called for the invalidation, and since `pending` stays set, it isn't
/// called for later writes either. The composition stays invalid until the host polls.
#[test]
fn a_frame_requester_set_after_an_invalidation_gets_a_frame() {
    let loaded = mutable_state_of(false);
    let handle = loaded.handle();
    let composition = new_composition();
    composition.set_content(move || {
        if !handle.get() {
            SideEffect(move || handle.set(true));
        }
    });
    assert!(composition.has_invalidations());
    let requests = counting_requester(&composition);
    loaded.set(false);
    loaded.set(true);
    assert!(requests.get() >= 1, "the composition is invalid and no frame is ever requested");
}

// ---- Finding 8 (minor): a composition kept in a thread-local aborts the thread's exit -------

/// A remembered value whose `Drop` writes state (the Applier docs allow nodes that "own state
/// and write it"; the same holds for remembered values).
struct WritesOnDrop(MutableState<u32>);

impl Drop for WritesOnDrop {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

thread_local! {
    /// A host keeping its composition in a thread-local, a common shape for UI hosts.
    static HOST: RefCell<Option<Composition>> = const { RefCell::new(None) };
}

fn compose_into_a_thread_local_and_exit() {
    let thread = std::thread::spawn(|| {
        HOST.with(|host| {
            let composition = new_composition();
            let state = mutable_state_of(0u32);
            let handle = state.handle();
            composition.set_content(move || {
                remember(|| Rc::new(WritesOnDrop(handle)));
            });
            std::mem::forget(state);
            *host.borrow_mut() = Some(composition);
        });
        // The thread ends: its thread-locals are destroyed, the runtime (registered after
        // HOST) first.
    });
    assert!(thread.join().is_ok());
}

/// Expected: a thread that ends with a composition still alive in a thread-local exits cleanly
/// (review 2 made `is_alive` and `try_*` safe at thread exit for that reason).
///
/// Actual: the runtime's thread-local is destroyed with the composition's slot table in it
/// (`Runtime` has no `Drop` that disposes compositions outside its storage), so user `Drop`s
/// run during its destruction; a `Drop` that uses `get`/`set` hits `RUNTIME.with` on a
/// destroyed thread-local and panics inside a TLS destructor, which aborts the process.
/// (`HOST`'s own `Composition::drop`, later, finds no runtime and leaks quietly.)
#[test]
fn a_thread_exits_cleanly_with_a_composition_in_a_thread_local() {
    if std::env::var_os("REVIEW3_CHILD").is_some() {
        compose_into_a_thread_local_and_exit();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args(["--exact", "a_thread_exits_cleanly_with_a_composition_in_a_thread_local"])
        .env("REVIEW3_CHILD", "1")
        .output()
        .expect("run the child test");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the child process failed ({:?}):\n{stderr}", output.status);
}
