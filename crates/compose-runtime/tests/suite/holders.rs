//! State holders as `Copy` handles (D240): an object owned by an `Owned` (a `remember` slot),
//! freed a pass after its owner goes, and `CoroutineScope` built on it.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::*;

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => payload.downcast_ref::<&str>().map(|s| s.to_string()).unwrap_or_default(),
    }
}

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

    fn increment(&self) -> i32 {
        let object = self.object.get();
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
    let message =
        panic_message(catch_unwind(AssertUnwindSafe(|| counter.increment())).expect_err("panics"));
    assert!(message.contains("CounterObject"), "{message}");
    assert!(message.contains("after it was freed"), "{message}");
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
    let object = counter.object.get();
    drop(owner);
    // What a method fetched before its owner went stays usable.
    object.count.set(5);
    assert_eq!(object.count.get(), 5);
    assert!(!counter.object.is_alive());
}

#[test]
fn a_forgotten_scope_is_cancelled_then_freed_and_launching_in_it_panics() {
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
    executor::run_ready();
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
    // Freed: cancelling does nothing, launching panics naming the scope.
    assert!(!scope.is_active());
    scope.cancel();
    let message = panic_message(
        catch_unwind(AssertUnwindSafe(|| scope.launch(async {}))).expect_err("panics"),
    );
    assert!(message.contains("ScopeInner"), "{message}");
}

#[test]
fn a_scope_of_its_own_dropped_at_once_keeps_its_tasks() {
    let ran = Rc::new(Cell::new(false));
    let flag = ran.clone();
    CoroutineScope::new().launch(async move { flag.set(true) });
    executor::run_ready();
    assert!(ran.get());
}
