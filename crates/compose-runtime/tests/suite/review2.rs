//! Regression tests from the second adversarial review of the runtime (after 6c8734e and
//! b78b74e). Each test demonstrated one finding, fixed since; the comment says what is expected
//! and what used to happen instead.

#![allow(non_snake_case)]

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::{Rc, Weak};

use compose_runtime::platform::*;
use compose_runtime::*;

// ---- Test support --------------------------------------------------------------------------

#[derive(Clone, PartialEq)]
struct Label(String);

impl fmt::Debug for Label {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type Tree = NodeTree<Label>;

fn tree(composition: &Composition) -> String {
    composition.with_applier(|tree: &mut Tree| tree.dump())
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_default(),
    }
}

/// A leaf node group that never touches the applier after inserting its node (so nothing
/// flushes a pending move on its behalf).
fn Leaf(label: &str) {
    let composer = current();
    let node = composer.start_node(key_of("review2::Leaf"), || Label(label.to_string()));
    composer.with_applier(|tree: &mut Tree| {
        let value = tree.get_mut(node).expect("live node");
        if value.0 != label {
            value.0 = label.to_string();
        }
    });
    composer.end_group();
}

/// A host whose frame requester recomposes at once (a test host, an immediate scheduler):
/// docs/04 says "a frame requester may recompose at once".
fn synchronous_host(composition: &Rc<Composition>) {
    let weak: Weak<Composition> = Rc::downgrade(composition);
    composition.set_frame_requester(move || {
        if let Some(composition) = weak.upgrade() {
            composition.recompose();
        }
    });
}

// ---- Finding 1 (critical): a move past node-less siblings is a `from == to` move ------------

/// Expected: reordering keyed items keeps the node tree in composition order.
///
/// Actual: `start_group` (composer.rs ~152-168) moves a matched group that isn't at the cursor
/// with `NodeOp::Move { from: node_index + nodes_before, to: node_index }`. When every skipped
/// sibling contributes no nodes (an item that emits nothing right now, a group holding only
/// state or effects), `nodes_before` is 0 and the op is `from == to`. Under the documented
/// `Applier::move_nodes` semantics (Compose's `MutableList.move`, "`to` is an index before the
/// move") that is NOT a no-op: the destination is `to - count`, so the node swaps places with its
/// predecessor. Here the tree ends up "two, first" while composition says "first, two".
#[test]
fn moving_a_keyed_item_past_nodeless_items_keeps_the_node_order() {
    let order = mutable_state_of_with(vec![1u32, 2], never_equal_policy());
    let handle = order.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        Leaf("first");
        handle.with(|order| {
            for &item in order {
                key(item, || {
                    if item == 2 {
                        Leaf("two");
                    }
                });
            }
        });
    });
    assert_eq!(tree(&composition), "first\ntwo\n");
    order.set(vec![2, 1]);
    composition.recompose();
    composition.validate();
    assert_eq!(tree(&composition), "first\ntwo\n");
}

/// The most common shape of the same defect, no keys needed: a branch whose content emits no
/// node (`if logging { SideEffect(…) }`, a `DisposableEffect`, a composable holding only
/// state), followed by a node. When the branch goes away, matching the next node skips over the
/// node-less region group, emitting `Move { from: 1, to: 1 }`, and the following node swaps with
/// the one before it. Expected "a, b"; actual "b, a".
#[test]
fn removing_a_nodeless_branch_keeps_the_following_nodes_in_place() {
    let show = mutable_state_of(true);
    let handle = show.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let composer = current();
        Leaf("a");
        if handle.get() {
            composer.start_replace_group(key_of("review2::effect_only"));
            SideEffect(|| {});
            composer.end_group();
        }
        Leaf("b");
    });
    assert_eq!(tree(&composition), "a\nb\n");
    show.set(false);
    composition.recompose();
    assert_eq!(tree(&composition), "a\nb\n");
}

/// Same defect at index 0: `move_nodes(parent, 0, 0, 1)` makes the destination `0 - 1`, an
/// arithmetic overflow panic in `NodeTree` (and in any applier implementing Compose's move), which
/// poisons the composition. Expected: the reorder works.
#[test]
fn moving_the_first_node_past_a_nodeless_item_does_not_panic() {
    let order = mutable_state_of_with(vec![1u32, 2], never_equal_policy());
    let handle = order.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        handle.with(|order| {
            for &item in order {
                key(item, || {
                    if item == 2 {
                        Leaf("two");
                    }
                });
            }
        });
    });
    order.set(vec![2, 1]);
    let result = catch_unwind(AssertUnwindSafe(|| composition.recompose()));
    if let Err(payload) = result {
        panic!("reordering panicked: {}", panic_message(payload));
    }
    assert_eq!(tree(&composition), "two\n");
}

// ---- Finding 2 (major): a released node that writes state, with an immediate requester -------

/// A node whose `Drop` writes state: a focus target clearing the focus owner's state, a
/// semantics node updating a counter. The `Applier` docs allow it: "Releases run after
/// composition, so the nodes it drops may own state and write it."
struct Tracked {
    released: Option<MutableState<u32>>,
}

impl Drop for Tracked {
    fn drop(&mut self) {
        if let Some(released) = self.released {
            released.update(|count| count + 1);
        }
    }
}

/// Expected: the release writes `released`, the immediate requester recomposes, and the tree
/// shows `released=1`.
///
/// Actual: `apply_deferred` (composition.rs ~326-333) keeps the applier mutably borrowed while
/// it calls `Applier::release` (and `on_end_changes`). The node's `Drop` runs inside
/// `NodeTree::release`, its write invalidates the content scope, the requester calls
/// `recompose()`, and `Pass::begin` panics with "the applier was used while it was already in
/// use" — after setting `runtime.active`, so the `Pass` guard poisons the composition.
#[test]
fn a_released_node_may_write_state_with_an_immediate_frame_requester() {
    let show = mutable_state_of(true);
    let released = mutable_state_of(0u32);
    let (show_handle, released_handle) = (show.handle(), released.handle());
    let composition = Rc::new(Composition::new(NodeTree::<Tracked>::new()));
    synchronous_host(&composition);
    composition.set_content(move || {
        let composer = current();
        let count = released_handle.get();
        composer.start_node(key_of("review2::Count"), || Tracked { released: None });
        composer.end_group();
        if show_handle.get() {
            composer.start_replace_group(key_of("review2::then"));
            composer.start_node(key_of("review2::Tracked"), || Tracked {
                released: Some(released_handle),
            });
            composer.end_group();
            composer.end_group();
        }
        let _ = count;
    });
    let result = catch_unwind(AssertUnwindSafe(|| show.set(false)));
    if let Err(payload) = result {
        panic!("hiding the node panicked: {}", panic_message(payload));
    }
    assert!(!composition.is_poisoned());
    assert_eq!(released.get(), 1);
}

// ---- Finding 3 (major): invalidating during composition with an immediate requester ---------

/// Expected: a scope may invalidate itself (or another) during composition — docs/04's known
/// limitations describe exactly that case ("costs one extra empty pass"), and Compose allows
/// `currentRecomposeScope.invalidate()` in a body. The host is asked for a frame, and that frame
/// runs after the pass.
///
/// Actual: `invalidate_scope_parts` queues the requester (the pass reset `pending`), and
/// `RecomposeScope::invalidate` calls `request_frames()` at once, in the middle of the pass. The
/// immediate requester's `recompose()` panics with "a composition pass is already running", from
/// inside the body, which poisons the composition. The same happens for a state write during
/// composition in release builds (D40 lets it through) and for an applier invalidating a scope.
/// Frame requests made while composing should wait for the end of the pass.
#[test]
fn invalidating_during_composition_with_an_immediate_frame_requester() {
    let runs = Rc::new(Cell::new(0u32));
    let composition = Rc::new(Composition::new(Tree::new()));
    synchronous_host(&composition);
    let counter = runs.clone();
    let result = catch_unwind(AssertUnwindSafe(|| {
        composition.set_content(move || {
            let run = counter.get();
            counter.set(run + 1);
            if run == 0 {
                current_recompose_scope().invalidate();
            }
            Leaf(&format!("run{run}"));
        })
    }));
    if let Err(payload) = result {
        panic!("invalidating during composition panicked: {}", panic_message(payload));
    }
    assert!(!composition.is_poisoned());
    assert_eq!(tree(&composition), "run1\n");
}

// ---- Finding 4 (major): a nested pass during a disposal frees the cells it retired ----------

thread_local! {
    static TRIGGER: Cell<Option<MutableState<u32>>> = const { Cell::new(None) };
}

/// Writes the trigger when dropped: another composition reads it and recomposes at once.
struct WritesOnDrop;

impl Drop for WritesOnDrop {
    fn drop(&mut self) {
        if let Some(trigger) = TRIGGER.with(Cell::get) {
            trigger.update(|value| value + 1);
        }
    }
}

/// Reads a state remembered earlier in the same group when dropped.
struct ReadsOnDrop(MutableState<u32>);

impl Drop for ReadsOnDrop {
    fn drop(&mut self) {
        assert_eq!(self.0.get(), 5);
    }
}

/// Expected (docs/04, "Disposal"): "state cells those values retire stay alive until the
/// disposal ends, so values of one group may rely on each other in `Drop`." That holds unless a
/// value's `Drop` or `on_forgotten` happens to make another composition recompose.
///
/// Actual: `Disposal::run` relies on `retired[retired_mark..]` (composition.rs ~375, ~401), but
/// a pass started while the values drop (`WritesOnDrop` → the other composition's immediate
/// requester) runs `Cells::begin_pass`, which moves *every* retired cell, including the ones this
/// disposal just retired, to `freeing`, and frees them at its end. `ReadsOnDrop` then reads a
/// freed cell and panics out of `Composition::drop` ("state … was used … after it was freed").
/// The same nested pass breaks the mark in the other direction: cells retired by unrelated code
/// after it land past the mark and are freed early. The same applies to the deferred drops of a
/// normal pass, whose frame requester recomposes another composition between two drops.
#[test]
fn a_nested_pass_during_disposal_keeps_the_cells_it_retired() {
    let trigger = mutable_state_of(0u32);
    TRIGGER.with(|slot| slot.set(Some(trigger.handle())));
    let trigger_handle = trigger.handle();
    let observer = Rc::new(Composition::new(Tree::new()));
    synchronous_host(&observer);
    observer.set_content(move || Leaf(&format!("trigger={}", trigger_handle.get())));

    let disposed = Composition::new(Tree::new());
    disposed.set_content(|| {
        let owned = remember(|| mutable_state_of(5u32));
        let _writer = remember(|| Rc::new(WritesOnDrop));
        let _reader = remember(|| Rc::new(ReadsOnDrop(owned)));
        Leaf("disposed");
    });
    let result = catch_unwind(AssertUnwindSafe(|| drop(disposed)));
    TRIGGER.with(|slot| slot.set(None));
    if let Err(payload) = result {
        panic!("disposal panicked: {}", panic_message(payload));
    }
    assert_eq!(tree(&observer), "trigger=1\n");
}

// ---- Finding 5 (minor): an observer of a completed pass gets `on_abandoned` -----------------

thread_local! {
    static EVENTS: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

struct Logger;

impl RememberObserver for Logger {
    fn on_remembered(&self) {
        EVENTS.with(|events| events.borrow_mut().push("remembered"));
    }
    fn on_forgotten(&self) {
        EVENTS.with(|events| events.borrow_mut().push("forgotten"));
    }
    fn on_abandoned(&self) {
        EVENTS.with(|events| events.borrow_mut().push("abandoned"));
    }
}

/// Expected (docs/04, `RememberObserver` docs): each value gets "exactly one of `on_forgotten`
/// (after `on_remembered`) or `on_abandoned` (its pass never completed)". The first pass
/// completed, so `Logger` gets `on_remembered` then `on_forgotten`, as in Compose, which
/// dispatches all of a pass's remembers and effects before the next recomposition.
///
/// Actual: since the pass now ends before its deferred work (D49), an earlier observer's
/// `on_remembered` (a `DisposableEffect` writing state, with an immediate requester) recomposes
/// the same composition before `Logger`'s `on_remembered` is dispatched. The nested pass removes
/// `Logger`'s group and dispatches `on_forgotten` to a still-`New` observer, which becomes
/// `on_abandoned`; the outer loop then skips it. Likewise the outer pass's remaining side effects
/// run after the nested pass has torn their groups down (a `SideEffect` running after its
/// sibling `DisposableEffect`'s `on_dispose`).
#[test]
fn an_observer_of_a_completed_pass_is_not_abandoned() {
    EVENTS.with(|events| events.borrow_mut().clear());
    let show = mutable_state_of(true);
    let handle = show.handle();
    let composition = Rc::new(Composition::new(Tree::new()));
    synchronous_host(&composition);
    composition.set_content(move || {
        let composer = current();
        DisposableEffect((), move |scope| {
            handle.set(false);
            scope.on_dispose(|| {})
        });
        if handle.get() {
            composer.start_replace_group(key_of("review2::shown"));
            remember_observer(|| Logger);
            composer.end_group();
        }
    });
    let events = EVENTS.with(|events| events.borrow().clone());
    assert_eq!(events, ["remembered", "forgotten"]);
}

// ---- Finding 6 (minor): a panic during disposal leaks every later state cell ----------------

struct PanicsOnForget;

impl RememberObserver for PanicsOnForget {
    fn on_forgotten(&self) {
        panic!("on_forgotten failed");
    }
}

/// Expected: after a panic out of `Composition::drop` (a buggy `on_forgotten`, caught by a test
/// harness or a host that keeps running), the runtime is consistent: with no composition left, a
/// dropped `OwnedState` frees its cell at once.
///
/// Actual: `Disposal::run` increments `runtime.disposing` in `take` and decrements it only on
/// the normal path (composition.rs ~369, ~396). The panic skips the decrement, so `disposing`
/// stays 1 forever: `retire` (state.rs ~426) then never frees at once, and with no composition
/// there is no pass to free the retired list: every cell retired from then on leaks (and
/// `free_retired`'s "last" branch is never taken again).
#[test]
fn a_panic_during_disposal_does_not_leak_later_cells() {
    let composition = Composition::new(Tree::new());
    composition.set_content(|| {
        remember_observer(|| PanicsOnForget);
    });
    let result = catch_unwind(AssertUnwindSafe(|| drop(composition)));
    assert!(panic_message(result.expect_err("on_forgotten panics")).contains("on_forgotten"));
    let before = live_state_count();
    drop(mutable_state_of(1u32));
    assert_eq!(live_state_count(), before, "a cell dropped with no composition left leaked");
}

// ---- Finding 7 (nit): `Composition::with_applier` during a pass sees an unflushed move ------

thread_local! {
    static HOST: RefCell<Option<Rc<Composition>>> = const { RefCell::new(None) };
    static SEEN: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Expected: every applier access during a pass sees the moves applied so far, as
/// `Composer::with_applier` does (it flushes the coalesced move first).
///
/// Actual: `Composition::with_applier` (composition.rs ~78) doesn't call `flush_node_moves`,
/// so code holding the composition (a host-side helper called from a body, as Compose's
/// `LocalView`-style escapes do) sees the tree before the pending move. Here the body sees
/// "1 2 3" while the composition has already placed "2 3 1".
#[test]
fn composition_with_applier_during_a_pass_sees_applied_moves() {
    let order = mutable_state_of_with(vec![1u32, 2, 3], never_equal_policy());
    let handle = order.handle();
    let composition = Rc::new(Composition::new(Tree::new()));
    HOST.with(|host| *host.borrow_mut() = Some(composition.clone()));
    composition.set_content(move || {
        handle.with(|order| {
            for &item in order {
                key(item, || Leaf(&item.to_string()));
            }
        });
        let host = HOST.with(|host| host.borrow().clone()).expect("host");
        let seen = host.with_applier(|tree: &mut Tree| tree.dump());
        SEEN.with(|cell| *cell.borrow_mut() = seen);
    });
    order.set(vec![2, 3, 1]);
    composition.recompose();
    HOST.with(|host| host.borrow_mut().take());
    assert_eq!(SEEN.with(|cell| cell.borrow().clone()), "2\n3\n1\n");
}
