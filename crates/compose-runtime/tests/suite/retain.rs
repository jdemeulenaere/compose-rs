//! Compose's `RetainTests` and `RetainedEffectTest`, rewritten, with the store's frame-end callback
//! run by the test's frames. `retain_throwsForRememberObserver_*` have no port: a generic function
//! can't ask what its type implements, so a value with callbacks goes through `retain_observer`.

#![allow(non_snake_case)]
// A call in a closure is a place of its own: that's what the moving test moves.
#![allow(clippy::redundant_closure)]

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::retain::*;
use compose_runtime::*;

use crate::mock::*;

#[derive(Default)]
struct Counting {
    retained: Cell<i32>,
    entered: Cell<i32>,
    exited: Cell<i32>,
    retired: Cell<i32>,
    unused: Cell<i32>,
}

impl Counting {
    fn check(&self) {
        assert!(
            self.retained.get() != 0
                || self.entered.get() + self.exited.get() + self.retired.get() == 0,
            "events without being retained"
        );
        assert!(self.retained.get() >= self.retired.get(), "retired more than retained");
        assert!(self.exited.get() <= self.entered.get(), "exited more than entered");
        assert!(
            self.entered.get() <= self.retained.get() + self.exited.get(),
            "entered without first exiting"
        );
    }

    /// `[retained, entered, exited, retired]`, as `assertCounts`.
    fn counts(&self) -> [i32; 4] {
        [self.retained.get(), self.entered.get(), self.exited.get(), self.retired.get()]
    }
}

impl RetainObserver for Counting {
    fn on_retained(&self) {
        self.retained.set(self.retained.get() + 1);
    }
    fn on_entered_composition(&self) {
        self.entered.set(self.entered.get() + 1);
        self.check();
    }
    fn on_exited_composition(&self) {
        self.exited.set(self.exited.get() + 1);
        self.check();
    }
    fn on_retired(&self) {
        self.retired.set(self.retired.get() + 1);
        self.check();
    }
    fn on_unused(&self) {
        self.unused.set(self.unused.get() + 1);
        self.check();
    }
}

struct Logging(&'static str, Log);

impl RetainObserver for Logging {
    fn on_retained(&self) {
        self.1.push(format!("Retain({})", self.0));
    }
    fn on_entered_composition(&self) {
        self.1.push(format!("EnterComposition({})", self.0));
    }
    fn on_exited_composition(&self) {
        self.1.push(format!("ExitComposition({})", self.0));
    }
    fn on_retired(&self) {
        self.1.push(format!("Retire({})", self.0));
    }
    fn on_unused(&self) {
        self.1.push(format!("Unused({})", self.0));
    }
}

struct LoggingRemember(&'static str, Log);

impl RememberObserver for LoggingRemember {
    fn on_remembered(&self) {
        self.1.push(format!("Remember({})", self.0));
    }
    fn on_forgotten(&self) {
        self.1.push(format!("Forget({})", self.0));
    }
    fn on_abandoned(&self) {
        self.1.push(format!("Abandon({})", self.0));
    }
}

fn as_dyn(store: &Rc<ManagedRetainedValuesStore>) -> Rc<dyn RetainedValuesStore> {
    store.clone()
}

/// What the tests compose: a store, a scope they invalidate, and values kept to look at.
#[derive(Clone, Default)]
struct Made(Rc<RefCell<Vec<Rc<Counting>>>>);

impl Made {
    fn retain(&self) {
        self.note(retain_observer(Counting::default));
    }

    fn retain_keyed(&self, key: &'static str) {
        let seen = retain_observer_keyed(key.to_string(), Counting::default);
        self.note(seen);
    }

    fn note(&self, seen: Rc<Counting>) {
        let mut made = self.0.borrow_mut();
        if !made.iter().any(|other| Rc::ptr_eq(other, &seen)) {
            made.push(seen);
        }
    }

    fn len(&self) -> usize {
        self.0.borrow().len()
    }

    fn at(&self, index: usize) -> Rc<Counting> {
        self.0.borrow()[index].clone()
    }
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_notRetaining_remember
#[test]
fn a_value_retained_is_told_once() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let (s, m) = (as_dyn(&store), made.clone());
    test.compose(move || {
        let m = m.clone();
        LocalRetainedValuesStoreProvider(s.clone(), move || m.retain());
    });
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_notRetaining_recompose
#[test]
fn a_value_stays_as_it_recomposes() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let scope = ScopeHolder::new();
    let (s, m, h) = (as_dyn(&store), made.clone(), scope.clone());
    test.compose(move || {
        let (m, h) = (m.clone(), h.clone());
        LocalRetainedValuesStoreProvider(s.clone(), move || {
            h.capture();
            m.retain();
        });
    });
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
}

/// `compose { if showContent { provider(store) { retain } } }`, with its switch and scope.
fn shown_under_store(
    test: &mut CompositionTest,
    store: Rc<dyn RetainedValuesStore>,
    made: &Made,
) -> (Rc<Cell<bool>>, ScopeHolder) {
    let (show, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (sh, h, m) = (show.clone(), scope.clone(), made.clone());
    test.compose(move || {
        h.capture();
        if sh.get() {
            let m = m.clone();
            LocalRetainedValuesStoreProvider(store.clone(), move || m.retain());
        }
    });
    (show, scope)
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_notRetaining_reconstruct
#[test]
fn not_retaining_a_value_goes_and_comes_new() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.disable_retaining_exited_values();
    let made = Made::default();
    let (show, scope) = shown_under_store(&mut test, as_dyn(&store), &made);
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
    show.set(false);
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
    show.set(true);
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.len(), 2);
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
    assert_eq!(made.at(1).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_inForgetfulStore_synonymousToRemember
#[test]
fn in_the_forgetful_store_it_s_remember() {
    let mut test = composition_test();
    let made = Made::default();
    let (show, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (sh, h, m) = (show.clone(), scope.clone(), made.clone());
    test.compose(move || {
        assert!(
            Rc::ptr_eq(&LOCAL_RETAINED_VALUES_STORE.current(), &forgetful_retained_values_store()),
            "the forgetful store by default"
        );
        h.capture();
        if sh.get() {
            m.retain();
        }
    });
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
    show.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
    show.set(true);
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 2);
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
    assert_eq!(made.at(1).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_retaining_remember
#[test]
fn retaining_a_value_is_told_once() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let made = Made::default();
    let _ = shown_under_store(&mut test, as_dyn(&store), &made);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_retaining_recompose
#[test]
fn retaining_a_value_stays_as_it_recomposes() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let made = Made::default();
    let (_, scope) = shown_under_store(&mut test, as_dyn(&store), &made);
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_retaining_reconstruct
#[test]
fn retaining_a_value_goes_and_comes_back() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let made = Made::default();
    let (show, scope) = shown_under_store(&mut test, as_dyn(&store), &made);
    let retained = made.at(0);
    assert_eq!(retained.counts(), [1, 1, 0, 0]);
    // The frame that tells the store its content's there.
    test.advance();
    show.set(false);
    scope.invalidate();
    test.expect_changes();
    assert_eq!(retained.counts(), [1, 1, 1, 0]);
    show.set(true);
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.len(), 1, "the factory ran once");
    assert_eq!(retained.counts(), [1, 2, 1, 0]);
}

/// `compose { provider(store) { retain(key) } }`, with its key and scope.
fn keyed_under_store(
    test: &mut CompositionTest,
    store: Rc<dyn RetainedValuesStore>,
    made: &Made,
) -> (Rc<Cell<&'static str>>, ScopeHolder) {
    let (key, scope) = (Rc::new(Cell::new("123")), ScopeHolder::new());
    let (k, h, m) = (key.clone(), scope.clone(), made.clone());
    test.compose(move || {
        h.capture();
        let (k, m) = (k.clone(), m.clone());
        LocalRetainedValuesStoreProvider(store.clone(), move || m.retain_keyed(k.get()));
    });
    (key, scope)
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_recomputesForNewKeys_whenNotRetaining
#[test]
fn new_keys_make_a_new_value() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let (key, scope) = keyed_under_store(&mut test, as_dyn(&store), &made);
    test.advance();
    key.set("456");
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.len(), 2);
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
    assert_eq!(made.at(1).counts(), [1, 1, 0, 0]);
    key.set("123");
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.len(), 3);
    assert_eq!(made.at(1).counts(), [1, 1, 1, 1]);
    assert_eq!(made.at(2).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_reusesForPreviousKeys_whenNotRetaining
#[test]
fn the_same_keys_keep_the_value() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let (key, scope) = keyed_under_store(&mut test, as_dyn(&store), &made);
    key.set("123");
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_forgetsValuesWhenKeysChange_whenNotRetaining
#[test]
fn enabled_and_composed_new_keys_retire_the_value() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let (key, scope) = keyed_under_store(&mut test, as_dyn(&store), &made);
    test.advance();
    store.enable_retaining_exited_values();
    key.set("456");
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.len(), 2);
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
    key.set("123");
    scope.invalidate();
    test.expect_changes();
    assert_eq!(made.len(), 3, "not the first again");
    assert_eq!(made.at(1).counts(), [1, 1, 1, 1]);
    assert_eq!(made.at(2).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_remembersValuesWithSameKeys_whenRetaining
#[test]
fn enabled_the_same_keys_keep_the_value() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let (key, scope) = keyed_under_store(&mut test, as_dyn(&store), &made);
    store.enable_retaining_exited_values();
    key.set("123");
    scope.invalidate();
    test.expect_no_changes();
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainObserver_callbackOrdering
#[test]
fn values_are_told_in_order_and_given_back_last_first() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let log = Log::new();
    let (include, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (s, l, i, h) = (as_dyn(&store), log.clone(), include.clone(), scope.clone());
    test.compose(move || {
        h.capture();
        if i.get() {
            let l = l.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                for name in ["Foo", "Bar", "Baz", "Buzz"] {
                    let l = l.clone();
                    retain_observer(move || Logging(name, l));
                }
            });
        }
    });
    let events = |of: &[&str], kinds: &[&str]| -> Vec<String> {
        of.iter().flat_map(|name| kinds.iter().map(move |kind| format!("{kind}({name})"))).collect()
    };
    let forward = ["Foo", "Bar", "Baz", "Buzz"];
    let backward = ["Buzz", "Baz", "Bar", "Foo"];
    assert_eq!(log.take(), events(&forward, &["Retain", "EnterComposition"]));
    test.advance();
    log.clear();

    store.enable_retaining_exited_values();
    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), events(&backward, &["ExitComposition"]));

    include.set(true);
    scope.invalidate();
    test.advance();
    store.disable_retaining_exited_values();
    assert_eq!(log.take(), events(&forward, &["EnterComposition"]));

    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), events(&backward, &["ExitComposition", "Retire"]));

    include.set(true);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), events(&forward, &["Retain", "EnterComposition"]));

    store.enable_retaining_exited_values();
    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), events(&backward, &["ExitComposition"]));

    store.disable_retaining_exited_values();
    assert_eq!(log.take(), events(&backward, &["Retire"]));
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_callbackOrdering_relativeToRememberObserver
#[test]
fn values_are_told_in_order_among_remembered_ones() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let log = Log::new();
    let (include, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (s, l, i, h) = (as_dyn(&store), log.clone(), include.clone(), scope.clone());
    test.compose(move || {
        h.capture();
        if i.get() {
            let l = l.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                let log = l.clone();
                retain_observer(move || Logging("RetainedFoo", log));
                let log = l.clone();
                remember_observer(move || LoggingRemember("RememberedFoo", log));
                let log = l.clone();
                retain_observer(move || Logging("RetainedBar", log));
                let log = l.clone();
                remember_observer(move || LoggingRemember("RememberedBar", log));
                let log = l.clone();
                retain_observer(move || Logging("RetainedBaz", log));
            });
        }
    });
    let entered_new = [
        "Retain(RetainedFoo)",
        "EnterComposition(RetainedFoo)",
        "Remember(RememberedFoo)",
        "Retain(RetainedBar)",
        "EnterComposition(RetainedBar)",
        "Remember(RememberedBar)",
        "Retain(RetainedBaz)",
        "EnterComposition(RetainedBaz)",
    ];
    let exited = [
        "ExitComposition(RetainedBaz)",
        "Forget(RememberedBar)",
        "ExitComposition(RetainedBar)",
        "Forget(RememberedFoo)",
        "ExitComposition(RetainedFoo)",
    ];
    assert_eq!(log.take(), entered_new);
    test.advance();
    store.enable_retaining_exited_values();
    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), exited);

    include.set(true);
    scope.invalidate();
    test.advance();
    store.disable_retaining_exited_values();
    assert_eq!(
        log.take(),
        [
            "EnterComposition(RetainedFoo)",
            "Remember(RememberedFoo)",
            "EnterComposition(RetainedBar)",
            "Remember(RememberedBar)",
            "EnterComposition(RetainedBaz)",
        ]
    );

    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(
        log.take(),
        [
            "ExitComposition(RetainedBaz)",
            "Retire(RetainedBaz)",
            "Forget(RememberedBar)",
            "ExitComposition(RetainedBar)",
            "Retire(RetainedBar)",
            "Forget(RememberedFoo)",
            "ExitComposition(RetainedFoo)",
            "Retire(RetainedFoo)",
        ]
    );

    include.set(true);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), entered_new);

    store.enable_retaining_exited_values();
    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(log.take(), exited);

    store.disable_retaining_exited_values();
    assert_eq!(log.take(), ["Retire(RetainedBaz)", "Retire(RetainedBar)", "Retire(RetainedFoo)"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, changingRetainedValuesStore_adoptsObjectsToNewScope
#[test]
fn a_new_store_adopts_the_value() {
    let mut test = composition_test();
    let first = ManagedRetainedValuesStore::new();
    first.enable_retaining_exited_values();
    let store: Rc<RefCell<Rc<dyn RetainedValuesStore>>> = Rc::new(RefCell::new(first));
    let made = Made::default();
    let (include, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (s, m, i, h) = (store.clone(), made.clone(), include.clone(), scope.clone());
    test.compose(move || {
        h.capture();
        if i.get() {
            let m = m.clone();
            let current = s.borrow().clone();
            LocalRetainedValuesStoreProvider(current, move || m.retain());
        }
    });
    let second = ManagedRetainedValuesStore::new();
    second.enable_retaining_exited_values();
    *store.borrow_mut() = second;
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 1, "only one object should be retained");
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);

    *store.borrow_mut() = forgetful_retained_values_store();
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);

    let last = ManagedRetainedValuesStore::new();
    *store.borrow_mut() = as_dyn(&last);
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 1);
    assert_eq!(made.at(0).counts(), [1, 1, 0, 0]);

    last.enable_retaining_exited_values();
    include.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(made.at(0).counts(), [1, 1, 1, 0]);
    last.disable_retaining_exited_values();
    assert_eq!(made.at(0).counts(), [1, 1, 1, 1]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_inMovableContent_experiencesOriginRetentionPolicy
#[test]
fn a_value_in_movable_content_goes_by_its_store() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let made = Made::default();
    let (show, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let m = made.clone();
    let content = movable_content_of(move || m.retain());
    let (s, sh, h) = (as_dyn(&store), show.clone(), scope.clone());
    test.compose(move || {
        h.capture();
        if sh.get() {
            let content = content.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                let content = content.clone();
                Linear!(move || content());
            });
        }
    });
    let retained = made.at(0);
    assert_eq!(retained.counts(), [1, 1, 0, 0]);
    test.advance();

    store.enable_retaining_exited_values();
    show.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 1);
    assert_eq!(retained.counts(), [1, 1, 1, 0]);

    show.set(true);
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 1);
    assert_eq!(retained.counts(), [1, 2, 1, 0]);

    store.disable_retaining_exited_values();
    store.enable_retaining_exited_values();
    show.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(retained.counts(), [1, 2, 2, 0]);
    store.disable_retaining_exited_values();
    assert_eq!(retained.counts(), [1, 2, 2, 1]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_inMovableContent_adoptsToDestinationScope
#[test]
fn a_value_in_movable_content_moved_goes_to_the_store_where_it_is() {
    let mut test = composition_test();
    let (store_a, store_b) = (ManagedRetainedValuesStore::new(), ManagedRetainedValuesStore::new());
    store_b.enable_retaining_exited_values();
    let made = Made::default();
    let (move_content, show, scope) =
        (Rc::new(Cell::new(false)), Rc::new(Cell::new(true)), ScopeHolder::new());
    let m = made.clone();
    let content = movable_content_of(move || m.retain());
    let (a, b, mc, sh, h) =
        (as_dyn(&store_a), as_dyn(&store_b), move_content.clone(), show.clone(), scope.clone());
    test.compose(move || {
        h.capture();
        if sh.get() {
            let (c1, c2, m1, m2) = (content.clone(), content.clone(), mc.clone(), mc.clone());
            LocalRetainedValuesStoreProvider(a.clone(), move || {
                let (c1, m1) = (c1.clone(), m1.clone());
                Linear!(move || {
                    if !m1.get() {
                        c1();
                    }
                });
            });
            LocalRetainedValuesStoreProvider(b.clone(), move || {
                let (c2, m2) = (c2.clone(), m2.clone());
                Linear!(move || {
                    if m2.get() {
                        c2();
                    }
                });
            });
        }
    });
    let retained = made.at(0);
    assert_eq!(retained.counts(), [1, 1, 0, 0]);
    test.advance();
    move_content.set(true);
    scope.invalidate();
    test.advance();
    assert_eq!(made.len(), 1);
    assert_eq!(retained.counts(), [1, 1, 0, 0]);
    show.set(false);
    scope.invalidate();
    test.advance();
    assert_eq!(retained.counts(), [1, 1, 1, 0]);
    store_b.disable_retaining_exited_values();
    assert_eq!(retained.counts(), [1, 1, 1, 1]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_duplicateRetainKeys
#[test]
fn values_of_one_key_each_find_their_own() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let seen: Rc<RefCell<Vec<Rc<Counting>>>> = Rc::default();
    let (show, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (s, m, sh, h) = (as_dyn(&store), seen.clone(), show.clone(), scope.clone());
    test.compose(move || {
        h.capture();
        if sh.get() {
            let m = m.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                for _ in 0..5 {
                    m.borrow_mut().push(retain_observer(Counting::default));
                }
            });
        }
    });
    let initial = seen.take();
    assert_eq!(initial.len(), 5);
    test.advance();
    show.set(false);
    scope.invalidate();
    test.expect_changes();
    assert!(seen.borrow().is_empty());
    assert!(initial.iter().all(|value| value.counts() == [1, 1, 1, 0]));
    show.set(true);
    scope.invalidate();
    test.expect_changes();
    let again = seen.take();
    assert_eq!(again.len(), 5);
    assert!(initial.iter().zip(&again).all(|(a, b)| Rc::ptr_eq(a, b)), "the same, in order");
    assert!(initial.iter().all(|value| value.counts() == [1, 2, 1, 0]));
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retain_explicitKey_groupCollision
#[test]
fn two_groups_of_one_key_find_their_own_values() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let (show, scope, banned) =
        (Rc::new(Cell::new(true)), ScopeHolder::new(), Rc::new(Cell::new(false)));
    let (s, sh, h, b) = (as_dyn(&store), show.clone(), scope.clone(), banned.clone());
    test.compose(move || {
        h.capture();
        if sh.get() {
            let b = b.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                let hashes: Rc<RefCell<Vec<u64>>> = Rc::default();
                for name in ["X1", "X2"] {
                    let (hashes, b) = (hashes.clone(), b.clone());
                    key("A", move || {
                        let unique = !hashes.borrow().contains(&current_composite_key_hash_code());
                        assert_eq!(unique, name == "X1");
                        hashes.borrow_mut().push(current_composite_key_hash_code());
                        let b = b.clone();
                        key("X", move || {
                            let text = retain(|| {
                                assert!(!b.get(), "Attempted to execute factory");
                                name.to_string()
                            });
                            Text(&text);
                        });
                    });
                }
            });
        }
    });
    let shown = |v: &mut Expected| {
        v.text("X1");
        v.text("X2");
    };
    test.validate(shown);
    test.advance();
    show.set(false);
    banned.set(true);
    scope.invalidate();
    test.expect_changes();
    test.validate(|_| {});
    show.set(true);
    scope.invalidate();
    test.expect_changes();
    test.validate(shown);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, abandonCompositionTest_gapComposer
#[test]
fn a_composition_abandoned_says_who_was_unused() {
    let (store1, store2) = (ManagedRetainedValuesStore::new(), ManagedRetainedValuesStore::new());
    let events = Log::new();
    let fail = mutable_state_of(false);
    let (s1, s2, l, f) = (as_dyn(&store1), as_dyn(&store2), events.clone(), fail.handle());
    {
        let mut test = composition_test();
        test.compose(move || {
            let (a, b) = (l.clone(), l.clone());
            LocalRetainedValuesStoreProvider(s1.clone(), move || {
                let log = a.clone();
                retain_observer(move || Logging("A", log));
                if f.get() {
                    let log = a.clone();
                    retain_observer(move || Logging("B", log));
                }
            });
            LocalRetainedValuesStoreProvider(s2.clone(), move || {
                let log = b.clone();
                retain_observer(move || Logging("C", log));
                if f.get() {
                    let log = b.clone();
                    retain_observer(move || Logging("D", log));
                }
            });
            if f.get() {
                l.push("throw");
                panic!("Abandoning composition");
            }
        });
        assert_eq!(
            events.entries(),
            ["Retain(A)", "EnterComposition(A)", "Retain(C)", "EnterComposition(C)"]
        );
        test.advance();
        fail.set(true);
        events.push("recompose");
        let caught = catch_unwind(AssertUnwindSafe(|| test.advance()));
        assert!(caught.is_err());
    }
    let seen = events.take();
    assert_eq!(
        seen[..6],
        [
            "Retain(A)",
            "EnterComposition(A)",
            "Retain(C)",
            "EnterComposition(C)",
            "recompose",
            "throw"
        ]
    );
    let mut unused = seen[6..8].to_vec();
    unused.sort();
    assert_eq!(unused, ["Unused(B)", "Unused(D)"]);
    assert_eq!(seen[8..], ["ExitComposition(C)", "ExitComposition(A)"]);
    store2.disable_retaining_exited_values();
    store1.disable_retaining_exited_values();
    assert_eq!(events.take(), ["Retire(C)", "Retire(A)"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainedContentHostInitializationTest
#[test]
fn nested_stores_are_there_at_once_and_not_retaining() {
    let mut test = composition_test();
    let (parent, child) = (ManagedRetainedValuesStore::new(), ManagedRetainedValuesStore::new());
    let counter = Rc::new(Cell::new(0));
    let (p, c, n) = (as_dyn(&parent), as_dyn(&child), counter.clone());
    test.compose(move || {
        let (c, n) = (c.clone(), n.clone());
        LocalRetainedValuesStoreProvider(p.clone(), move || {
            let n = n.clone();
            LocalRetainedValuesStoreProvider(c.clone(), move || {
                for _ in 0..3 {
                    let n = n.clone();
                    Text(&retain(move || n.replace(n.get() + 1)).to_string());
                }
            });
        });
    });
    test.validate(|v| {
        v.text("0");
        v.text("1");
        v.text("2");
    });
    test.expect_no_changes();
    assert!(!child.is_retaining_exited_values());
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainLocalRetainedValuesStoreTest
#[test]
fn a_retained_store_follows_its_parent() {
    let mut test = composition_test();
    let parent = ManagedRetainedValuesStore::new();
    let child: Rc<RefCell<Option<Rc<ManagedRetainedValuesStore>>>> = Rc::default();
    let (show_content, show_parent) = (mutable_state_of(true), mutable_state_of(true));
    let counter = Rc::new(Cell::new(0));
    let (p, ch, sc, sp, n) = (
        as_dyn(&parent),
        child.clone(),
        show_content.handle(),
        show_parent.handle(),
        counter.clone(),
    );
    test.compose(move || {
        if sp.get() {
            let (ch, n) = (ch.clone(), n.clone());
            LocalRetainedValuesStoreProvider(p.clone(), move || {
                let retained = retain_managed_retained_values_store();
                if let Some(child) = ch.borrow().as_ref() {
                    assert!(Rc::ptr_eq(child, &retained));
                }
                *ch.borrow_mut() = Some(retained.clone());
                if sc.get() {
                    let n = n.clone();
                    LocalRetainedValuesStoreProvider(as_dyn(&retained), move || {
                        for _ in 0..3 {
                            let n = n.clone();
                            Text(&retain(move || n.replace(n.get() + 1)).to_string());
                        }
                    });
                }
            });
        }
    });
    let shown = |v: &mut Expected| {
        v.text("0");
        v.text("1");
        v.text("2");
    };
    test.expect_no_changes();
    let child = child.borrow().clone().expect("composed");
    assert!(!child.is_retaining_exited_values(), "not retaining while it's there");
    test.validate(shown);

    show_content.set(false);
    test.expect_changes();
    test.validate(|_| {});
    assert!(child.is_retaining_exited_values(), "retaining while its content's out");

    show_content.set(true);
    test.expect_changes();
    test.validate(shown);
    assert!(!child.is_retaining_exited_values(), "not after its content's restored");

    show_parent.set(false);
    test.expect_changes();
    test.validate(|_| {});
    assert!(parent.is_retaining_exited_values());
    assert!(child.is_retaining_exited_values(), "as its parent");

    parent.disable_retaining_exited_values();
    assert!(!parent.is_retaining_exited_values());
    assert!(!child.is_retaining_exited_values(), "retired with its parent");
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainedValuesStoreRegistry_retainsScopes
#[test]
fn a_registry_keeps_a_store_per_key() {
    let mut test = composition_test();
    let known = ["A", "B", "C", "D", "E", "F"];
    let visible =
        mutable_state_of(known[..3].iter().map(|key| key.to_string()).collect::<Vec<_>>());
    let values: Rc<[Made; 6]> = Rc::new(Default::default());
    let (v, m) = (visible.handle(), values.clone());
    test.compose(move || {
        let registry = retain_retained_values_store_registry();
        for name in v.get() {
            let (registry, m) = (registry.clone(), m.clone());
            let inner = name.clone();
            key(name, move || {
                let at =
                    ["A", "B", "C", "D", "E", "F"].iter().position(|key| *key == inner).unwrap();
                let m = m.clone();
                registry.LocalRetainedValuesStoreProvider(inner.clone(), move || {
                    for _ in 0..5 {
                        m[at].retain_new();
                    }
                });
            });
        }
    });
    let check = |values: &[Made; 6], counts: [[i32; 4]; 6]| {
        for (made, counts) in values.iter().zip(counts) {
            if counts == [0; 4] {
                assert_eq!(made.len(), 0);
            } else {
                assert_eq!(made.len(), 5);
                assert!(made.0.borrow().iter().all(|value| value.counts() == counts));
            }
        }
    };
    let (once, out, back) = ([1, 1, 0, 0], [1, 1, 1, 0], [1, 2, 1, 0]);
    check(&values, [once, once, once, [0; 4], [0; 4], [0; 4]]);
    visible.set(known.iter().map(|key| key.to_string()).collect());
    test.advance();
    check(&values, [once; 6]);
    visible.set(known[3..].iter().map(|key| key.to_string()).collect());
    test.advance();
    check(&values, [out, out, out, once, once, once]);
    visible.set(known.iter().map(|key| key.to_string()).collect());
    test.advance();
    check(&values, [back, back, back, once, once, once]);
}

impl Made {
    /// A value per call in one place, each noted once.
    fn retain_new(&self) {
        let value = retain_observer(Counting::default);
        self.note(value);
    }
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainedValuesStoreRegistry_clearChild_removesScope
#[test]
fn a_registry_s_child_cleared_is_made_again() {
    let mut test = composition_test();
    let provider = RetainedValuesStoreRegistry::new();
    let counter = Rc::new(Cell::new(0));
    let (include_a, include_b) = (mutable_state_of(true), mutable_state_of(true));
    type Stores = [Option<Rc<dyn RetainedValuesStore>>; 2];
    let stores: Rc<RefCell<Stores>> = Rc::default();
    let (p, n, ia, ib, st) =
        (provider.clone(), counter.clone(), include_a.handle(), include_b.handle(), stores.clone());
    test.compose(move || {
        for (at, (name, there)) in [("A", ia), ("B", ib)].into_iter().enumerate() {
            let (n, st, p) = (n.clone(), st.clone(), p.clone());
            // A group each, as each `if` of Compose's.
            key(name, move || {
                if there.get() {
                    p.LocalRetainedValuesStoreProvider(name, move || {
                        st.borrow_mut()[at] = Some(LOCAL_RETAINED_VALUES_STORE.current());
                        let n = n.clone();
                        let value = retain(move || n.replace(n.get() + 1));
                        Text(&format!("{name}:{value}"));
                    });
                }
            });
        }
    });
    test.validate(|v| {
        v.text("A:0");
        v.text("B:1");
    });
    test.advance();
    include_a.set(false);
    test.expect_changes();
    test.validate(|v| v.text("B:1"));
    let store = |at: usize| stores.borrow()[at].clone().expect("composed");
    let (original, b) = (store(0), store(1));
    assert!(original.is_retaining_exited_values(), "out, it's retaining");
    assert!(!b.is_retaining_exited_values(), "there, it isn't");
    provider.clear_child("A");
    assert!(!original.is_retaining_exited_values(), "cleared, it stops");
    include_a.set(true);
    test.expect_changes();
    test.validate(|v| {
        v.text("A:2");
        v.text("B:1");
    });
    assert!(!Rc::ptr_eq(&original, &store(0)), "made again");
}

/// What its keys say goes, its values retired; the rest stays.
#[test]
fn a_registry_clears_the_children_it_s_told() {
    let registry = RetainedValuesStoreRegistry::new();
    let log = Log::new();
    let mut test = composition_test();
    let (r, l) = (registry.clone(), log.clone());
    let shown = mutable_state_of(true);
    let s = shown.handle();
    test.compose(move || {
        if s.get() {
            for name in ["A", "B"] {
                let l = l.clone();
                r.LocalRetainedValuesStoreProvider(name, move || {
                    retain_observer(move || Logging(name, l));
                });
            }
        }
    });
    test.advance();
    log.clear();
    shown.set(false);
    test.advance();
    assert_eq!(log.take(), ["ExitComposition(B)", "ExitComposition(A)"]);
    registry.clear_children(|key| key.downcast_ref::<&str>() == Some(&"A"));
    assert_eq!(log.take(), ["Retire(A)"]);
    registry.dispose();
    assert_eq!(log.take(), ["Retire(B)"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainRetainedValuesStoreRegistry_retireDispose
#[test]
fn a_retained_registry_goes_with_its_store() {
    let mut test = composition_test();
    let parent = ManagedRetainedValuesStore::new();
    let child: Rc<RefCell<Option<Rc<dyn RetainedValuesStore>>>> = Rc::default();
    let events = Log::new();
    let (include_parent, include_child) = (mutable_state_of(true), mutable_state_of(true));
    let (p, c, l, ip, ic) = (
        as_dyn(&parent),
        child.clone(),
        events.clone(),
        include_parent.handle(),
        include_child.handle(),
    );
    test.compose(move || {
        if ip.get() {
            let (c, l) = (c.clone(), l.clone());
            LocalRetainedValuesStoreProvider(p.clone(), move || {
                let log = l.clone();
                retain_observer(move || Logging("A", log));
                let registry = retain_retained_values_store_registry();
                if ic.get() {
                    let (c, l) = (c.clone(), l.clone());
                    registry.LocalRetainedValuesStoreProvider("B", move || {
                        *c.borrow_mut() = Some(LOCAL_RETAINED_VALUES_STORE.current());
                        let log = l.clone();
                        retain_observer(move || Logging("B", log));
                    });
                }
            });
        }
    });
    test.expect_no_changes();
    assert_eq!(
        events.entries(),
        ["Retain(A)", "EnterComposition(A)", "Retain(B)", "EnterComposition(B)"]
    );
    let child = child.borrow().clone().expect("composed");
    assert!(!child.is_retaining_exited_values());
    events.clear();
    include_child.set(false);
    test.advance();
    assert_eq!(events.take(), ["ExitComposition(B)"]);
    assert!(child.is_retaining_exited_values());
    include_parent.set(false);
    test.advance();
    assert_eq!(events.take(), ["ExitComposition(A)"]);
    assert!(parent.is_retaining_exited_values());
    assert!(child.is_retaining_exited_values());
    parent.disable_retaining_exited_values();
    assert!(!child.is_retaining_exited_values());
    let mut retired = events.take();
    retired.sort();
    assert_eq!(retired, ["Retire(A)", "Retire(B)"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, retainedValuesStoreRegistry_manualDispose_gapComposer
#[test]
#[should_panic(
    expected = "Cannot get a RetainedValuesStore after a RetainedValuesStoreRegistry has been disposed."
)]
fn a_registry_disposed_gives_no_store() {
    let mut test = composition_test();
    let parent = ManagedRetainedValuesStore::new();
    let registry_seen: Rc<RefCell<Option<Rc<RetainedValuesStoreRegistry>>>> = Rc::default();
    let include_child = mutable_state_of(true);
    let (p, r, ic) = (as_dyn(&parent), registry_seen.clone(), include_child.handle());
    test.compose(move || {
        let r = r.clone();
        LocalRetainedValuesStoreProvider(p.clone(), move || {
            let registry = retain_retained_values_store_registry();
            *r.borrow_mut() = Some(registry.clone());
            if ic.get() {
                registry.LocalRetainedValuesStoreProvider("B", || {
                    retain_observer(Counting::default);
                });
            }
        });
    });
    test.expect_no_changes();
    include_child.set(false);
    test.advance();
    registry_seen.borrow().as_ref().expect("composed").dispose();
    include_child.set(true);
    test.advance();
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, provideRetainedValuesStore_inPausableComposition
#[test]
fn a_store_in_paused_content_tells_at_apply() {
    let store = ManagedRetainedValuesStore::new();
    let events = Log::new();
    let (enable, key) = (mutable_state_of(true), mutable_state_of("A"));
    let (s, l, en, k) = (as_dyn(&store), events.clone(), enable.handle(), key.handle());
    let composition =
        Composition::pausable(ViewApplier::new(), Recomposer::current_thread().as_context());
    let paused = composition.set_pausable_content(move || {
        if en.get() {
            let l = l.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                let (name, l) = (k.get(), l.clone());
                retain_observer_keyed(name, move || Logging(name, l));
            });
        }
    });
    let settle = |paused: &PausedComposition| {
        snapshots::Snapshot::send_apply_notifications();
        while !paused.is_complete() {
            paused.resume(|| false);
        }
    };
    settle(&paused);
    enable.set(false);
    settle(&paused);
    enable.set(true);
    settle(&paused);
    key.set("B");
    settle(&paused);
    enable.set(false);
    settle(&paused);
    enable.set(true);
    settle(&paused);
    paused.apply();
    let seen = events.take();
    assert_eq!(seen[..2], ["Retain(B)", "EnterComposition(B)"]);
    let mut unused = seen[2..].to_vec();
    unused.sort();
    assert_eq!(unused, ["Unused(A)", "Unused(A)", "Unused(B)"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, provideRetainedValuesStore_inMovableContent
#[test]
fn a_store_in_movable_content_moves_with_it() {
    let mut test = composition_test();
    let events = Log::new();
    let store = ManagedRetainedValuesStore::new();
    let (s, l) = (as_dyn(&store), events.clone());
    let content = movable_content_of(move || {
        let l = l.clone();
        LocalRetainedValuesStoreProvider(s.clone(), move || {
            let l = l.clone();
            retain_observer(move || Logging("Movable", l));
        });
    });
    let (include, moved) = (mutable_state_of(true), mutable_state_of(false));
    let (i, m, s, l) = (include.handle(), moved.handle(), as_dyn(&store), events.clone());
    test.compose(move || {
        if !i.get() {
            let l = l.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                let l = l.clone();
                retain_observer(move || Logging("NotMovable", l));
            });
        } else {
            let (a, b) = (content.clone(), content.clone());
            Linear!(move || {
                if !m.get() {
                    a();
                }
            });
            Linear!(move || {
                if m.get() {
                    b();
                }
            });
        }
    });
    test.advance();
    assert_eq!(events.entries(), ["Retain(Movable)", "EnterComposition(Movable)"]);
    assert!(!store.is_retaining_exited_values());

    moved.set(true);
    test.advance();
    assert_eq!(events.entries(), ["Retain(Movable)", "EnterComposition(Movable)"]);
    assert!(!store.is_retaining_exited_values());

    include.set(false);
    test.advance();
    assert_eq!(
        events.entries(),
        [
            "Retain(Movable)",
            "EnterComposition(Movable)",
            "Retain(NotMovable)",
            "EnterComposition(NotMovable)",
            "ExitComposition(Movable)",
            "Retire(Movable)",
        ]
    );
    assert!(!store.is_retaining_exited_values());

    include.set(true);
    test.advance();
    assert_eq!(
        events.entries(),
        [
            "Retain(Movable)",
            "EnterComposition(Movable)",
            "Retain(NotMovable)",
            "EnterComposition(NotMovable)",
            "ExitComposition(Movable)",
            "Retire(Movable)",
            "ExitComposition(NotMovable)",
            "Retain(Movable)",
            "EnterComposition(Movable)",
            "Retire(NotMovable)",
        ]
    );
    assert!(!store.is_retaining_exited_values());
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainTests.kt, provideRetainedValuesStore_withReturnPassthrough
#[test]
fn a_store_passes_back_what_its_content_returns() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    let (retains, remembers, invocations) =
        (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (captured, show) = (mutable_state_of(0), mutable_state_of(true));
    let (s, r1, r2, i, c, sh) = (
        as_dyn(&store),
        retains.clone(),
        remembers.clone(),
        invocations.clone(),
        captured.handle(),
        show.handle(),
    );
    test.compose(move || {
        if sh.get() {
            let (r1, r2, i) = (r1.clone(), r2.clone(), i.clone());
            let result = with_local_retained_values_store(s.clone(), move || {
                let retained = retain(move || r1.replace(r1.get() + 1));
                let remembered = remember(move || r2.replace(r2.get() + 1));
                format!(
                    "Retain: {retained}, Remembered: {remembered}, Captured: {}, Invocations: {}",
                    c.get(),
                    i.replace(i.get() + 1)
                )
            });
            Text(&result);
        }
    });
    let expect =
        |test: &mut CompositionTest, text: &'static str| test.validate(move |v| v.text(text));
    expect(&mut test, "Retain: 0, Remembered: 0, Captured: 0, Invocations: 0");
    captured.update(|value| value + 1);
    test.advance();
    expect(&mut test, "Retain: 0, Remembered: 0, Captured: 1, Invocations: 1");
    show.set(false);
    test.advance();
    test.validate(|_| {});
    show.set(true);
    test.advance();
    expect(&mut test, "Retain: 0, Remembered: 1, Captured: 1, Invocations: 2");
    store.disable_retaining_exited_values();
    show.set(false);
    test.advance();
    test.validate(|_| {});
    show.set(true);
    test.advance();
    expect(&mut test, "Retain: 1, Remembered: 2, Captured: 1, Invocations: 3");
}

/// A value goes back to where it was, not to another place's retain.
#[test]
fn a_value_finds_its_own_place() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let places = Rc::new(RefCell::new(vec!["a", "b"]));
    let scope = ScopeHolder::new();
    let counter = Rc::new(Cell::new(0));
    let (s, p, h, n) = (as_dyn(&store), places.clone(), scope.clone(), counter.clone());
    test.compose(move || {
        h.capture();
        if p.borrow().is_empty() {
            return;
        }
        let (p, n) = (p.clone(), n.clone());
        LocalRetainedValuesStoreProvider(s.clone(), move || {
            for place in p.borrow().iter().copied() {
                let n = n.clone();
                key(place, move || {
                    let value = retain(move || n.replace(n.get() + 1));
                    Text(&format!("{place}:{value}"));
                });
            }
        });
    });
    test.advance();
    places.borrow_mut().clear();
    scope.invalidate();
    test.advance();
    // The first kept is the last it would be given, by what was kept alone.
    places.borrow_mut().push("b");
    scope.invalidate();
    test.advance();
    test.validate(|v| v.text("b:1"));
}

/// Values of two types in one place each find their own.
#[test]
fn values_of_two_types_in_one_place_find_their_own() {
    let mut test = composition_test();
    let store = ManagedRetainedValuesStore::new();
    store.enable_retaining_exited_values();
    let (show, scope) = (Rc::new(Cell::new(true)), ScopeHolder::new());
    let (s, sh, h) = (as_dyn(&store), show.clone(), scope.clone());
    let f = Rc::new(Cell::new(true));
    test.compose(move || {
        h.capture();
        if sh.get() {
            let f = f.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                // In the other order, the second time: each by its type.
                if f.replace(false) {
                    let number = retain(|| 1);
                    let text = retain(|| String::from("text"));
                    Text(&format!("{number} {text}"));
                } else {
                    let text = retain(|| String::from("new"));
                    let number = retain(|| 2);
                    Text(&format!("{number} {text}"));
                }
            });
        }
    });
    test.advance();
    show.set(false);
    scope.invalidate();
    test.advance();
    show.set(true);
    scope.invalidate();
    test.advance();
    test.validate(|v| v.text("1 text"));
}

// ---- RetainedEffectTest ---------------------------------------------------------------------

fn retain_logger(log: &Log, name: String) {
    RetainLogger(log.clone(), name);
}

#[composable(non_restartable)]
fn RetainLogger(log: Log, name: String) {
    let (retained, retired) = (log.clone(), log.clone());
    let key = name.clone();
    RetainedEffect(key, move |scope| {
        retained.push(format!("Retain:{name}"));
        scope.on_retire(move || retired.push(format!("Retire:{name}")))
    });
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainedEffectTest.kt, testRetainedEffect_retained
#[test]
fn a_retained_effect_runs_when_it_s_retained() {
    let mut test = composition_test();
    let mount = mutable_state_of(false);
    let log = Log::new();
    let (m, l) = (mount.handle(), log.clone());
    test.compose(move || {
        retain_logger(&l, "1".into());
        if m.get() {
            retain_logger(&l, "2".into());
        }
    });
    assert_eq!(log.entries(), ["Retain:1"]);
    mount.set(true);
    log.push("recompose");
    test.expect_changes();
    assert_eq!(log.entries(), ["Retain:1", "recompose", "Retain:2"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainedEffectTest.kt, testRetainedEffect_keysChanged
#[test]
fn a_retained_effect_with_new_keys_runs_again() {
    let mut test = composition_test();
    let delta = mutable_state_of(0);
    let log = Log::new();
    let (d, l) = (delta.handle(), log.clone());
    test.compose(move || {
        retain_logger(&l, (d.get() * 2).to_string());
        retain_logger(&l, (d.get() * 2 + 1).to_string());
    });
    assert_eq!(log.entries(), ["Retain:0", "Retain:1"]);
    delta.update(|value| value + 1);
    log.push("recompose");
    test.expect_changes();
    assert_eq!(
        log.entries(),
        ["Retain:0", "Retain:1", "recompose", "Retire:1", "Retire:0", "Retain:2", "Retain:3"]
    );
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainedEffectTest.kt, testRetainedEffect_retireWhenRemovedNotRetainingExitedValues
#[test]
fn a_retained_effect_removed_with_nothing_keeping_it_retires() {
    let mut test = composition_test();
    let mount = mutable_state_of(true);
    let log = Log::new();
    let (m, l) = (mount.handle(), log.clone());
    test.compose(move || {
        if m.get() {
            retain_logger(&l, "1".into());
            retain_logger(&l, "2".into());
        }
    });
    assert_eq!(log.entries(), ["Retain:1", "Retain:2"]);
    mount.set(false);
    log.push("recompose");
    test.expect_changes();
    assert_eq!(log.entries(), ["Retain:1", "Retain:2", "recompose", "Retire:2", "Retire:1"]);
}

// Compose: compose/runtime/runtime-retain/src/commonTest/kotlin/androidx/compose/runtime/retain/RetainedEffectTest.kt, testRetainedEffect_retireWhenRetainExitedValuesEnds
#[test]
fn a_retained_effect_retires_when_its_store_stops_keeping() {
    let mut test = composition_test();
    let mount = mutable_state_of(true);
    let log = Log::new();
    let store = ManagedRetainedValuesStore::new();
    let (m, l, s) = (mount.handle(), log.clone(), as_dyn(&store));
    test.compose(move || {
        if m.get() {
            let l = l.clone();
            LocalRetainedValuesStoreProvider(s.clone(), move || {
                retain_logger(&l, "1".into());
                retain_logger(&l, "2".into());
            });
        }
    });
    assert_eq!(log.entries(), ["Retain:1", "Retain:2"]);
    test.advance();
    mount.set(false);
    log.push("recompose");
    test.expect_changes();
    assert_eq!(log.entries(), ["Retain:1", "Retain:2", "recompose"]);
    store.disable_retaining_exited_values();
    assert_eq!(log.entries(), ["Retain:1", "Retain:2", "recompose", "Retire:2", "Retire:1"]);
}
