//! State holders as `Copy` handles (D240): an object owned by an `Owned` (a `remember` slot),
//! freed a pass after its owner goes, and `CoroutineScope` built on it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::*;

fn new_composition() -> Composition {
    Composition::new(NodeTree::<()>::new())
}

/// A state holder, as a toolkit writes one: a `Copy` handle of an object.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Counter {
    object: ObjectHandle<CounterObject>,
}

struct CounterObject {
    count: Cell<i32>,
}

impl Counter {
    fn new(count: i32) -> Owned<Counter> {
        Owned::new(CounterObject { count: Cell::new(count) }, |object| Counter { object })
    }

    /// The new count, or 0 (doing nothing) once the counter's owner went.
    fn increment(&self) -> i32 {
        let Some(object) = self.object.get() else { return 0 };
        object.count.set(object.count.get() + 1);
        object.count.get()
    }
}

#[test]
fn a_remembered_holder_is_a_handle_freed_a_pass_after_its_group_leaves() {
    thread_local! {
        static COUNTER: Cell<Option<Counter>> = const { Cell::new(None) };
    }
    let composition = new_composition();
    let show = mutable_state_of(true);
    let shown = show.handle();
    composition.set_content(move || {
        if shown.get() {
            current().start_replace_group(1);
            // `remember` keeps the owner and returns the handle.
            let counter: Counter = remember(|| Counter::new(1));
            COUNTER.with(|slot| slot.set(Some(counter)));
            current().end_group();
        }
    });
    let counter = COUNTER.with(Cell::get).expect("composed");
    // Copied into two closures without a clone.
    let (first, second) = (move || counter.increment(), move || counter.increment());
    assert_eq!((first(), second()), (2, 3));
    show.set(false);
    composition.recompose();
    // Rule R2: retired at the end of this pass, not freed.
    assert!(counter.object.is_alive());
    assert_eq!(counter.increment(), 4);
    show.set(true);
    composition.recompose();
    assert!(!counter.object.is_alive(), "freed at the end of the next pass");
    assert!(counter.object.try_get().is_none());
    // A stale handle doesn't panic: the holder's method does nothing (debug builds say so).
    assert_eq!(counter.increment(), 0);
    assert_eq!(counter.increment(), 0);
}

#[test]
fn an_owned_holder_outside_a_composition_is_freed_when_dropped() {
    let owner = Counter::new(0);
    let counter = *owner;
    assert_eq!(counter.increment(), 1);
    assert_eq!(owner.increment(), 2, "the owner derefs to its handle");
    drop(owner);
    assert!(!counter.object.is_alive(), "no composition on this thread: freed at once");
}

#[test]
fn an_async_method_keeps_its_object_across_awaits() {
    let owner = Counter::new(0);
    let counter = *owner;
    let object = counter.object.get().expect("live");
    drop(owner);
    // What a method fetched before its owner went stays usable.
    object.count.set(5);
    assert_eq!(object.count.get(), 5);
    assert!(!counter.object.is_alive());
}

#[test]
fn a_forgotten_scope_is_cancelled_then_freed_and_stays_cancelled() {
    thread_local! {
        static SCOPE: Cell<Option<CoroutineScope>> = const { Cell::new(None) };
    }
    let composition = new_composition();
    let show = mutable_state_of(true);
    let shown = show.handle();
    composition.set_content(move || {
        if shown.get() {
            current().start_replace_group(1);
            let scope = remember_coroutine_scope();
            SCOPE.with(|slot| slot.set(Some(scope)));
            current().end_group();
        }
    });
    let scope = SCOPE.with(Cell::get).expect("composed");
    let ran = Rc::new(RefCell::new(Vec::new()));
    let log = ran.clone();
    let job = scope.launch(async move {
        log.borrow_mut().push("started");
        delay(std::time::Duration::from_secs(1)).await;
        log.borrow_mut().push("finished");
    });
    platform::run_ready();
    assert_eq!(*ran.borrow(), ["started"]);
    assert!(scope.is_active());
    show.set(false);
    composition.recompose();
    // Forgotten: cancelled (its task with it), but still there until the next pass ends.
    assert!(!job.is_active());
    assert!(!scope.is_active());
    let late = scope.launch(async {});
    assert!(!late.is_active(), "a cancelled scope launches nothing");
    show.set(true);
    composition.recompose();
    // Freed: a cancelled scope still, as Kotlin's: cancelling does nothing, launching starts
    // nothing.
    assert!(!scope.is_active());
    scope.cancel();
    let ran_late = Rc::new(Cell::new(false));
    let flag = ran_late.clone();
    let late = scope.launch(async move { flag.set(true) });
    platform::run_ready();
    assert!(!late.is_active());
    assert!(!ran_late.get());
}

#[test]
fn a_scope_of_its_own_dropped_at_once_keeps_its_tasks() {
    let ran = Rc::new(Cell::new(false));
    let flag = ran.clone();
    CoroutineScope::new().launch(async move { flag.set(true) });
    platform::run_ready();
    assert!(ran.get());
}
