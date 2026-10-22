//! Adversarial review of D200 (thin generic shims over per-type cores): `remember_keyed` keeps
//! its keys before reserving the value slot, `#[composable]` writes `update_kept`'s steps inline
//! (`take_kept`, `KeptArguments::get`/`set`/`put`), lambdas store through `store_callback`, and
//! `CompositionLocalProvider` starts its provider in `start_provider`. Each test pins a behavior
//! that must be the same as before.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::__private::{
    ArgProbe, Callback, KeyedProbe as _, lambda_probe, resolve_lazy_0,
};
use compose_runtime::*;

type Tree = NodeTree<String>;

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static RUNS: Cell<u32> = const { Cell::new(0) };
}

fn log(entry: impl Into<String>) {
    LOG.with(|log| log.borrow_mut().push(entry.into()));
}

fn take_log() -> Vec<String> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

fn take_runs() -> u32 {
    RUNS.with(|runs| runs.replace(0))
}

struct Observer(&'static str);

impl RememberObserver for Observer {
    fn on_remembered(&self) {
        log(format!("remembered {}", self.0));
    }
    fn on_forgotten(&self) {
        log(format!("forgotten {}", self.0));
    }
    fn on_abandoned(&self) {
        log(format!("abandoned {}", self.0));
    }
}

/// Counts its drops, so a forgotten remembered value shows.
struct Dropped(&'static str);

impl Drop for Dropped {
    fn drop(&mut self) {
        log(format!("dropped {}", self.0));
    }
}

// ---- remember_keyed: slot order ------------------------------------------------------------

/// The keys' type changes between passes at the same position (one key, then two): the keys
/// slot is replaced, the value recomputed, and the sibling `remember` after it keeps its value.
#[test]
fn remember_keyed_whose_key_count_changes_keeps_its_siblings() {
    let two = mutable_state_of(false);
    let handle = two.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let value = if handle.get() {
            remember_keyed((1u32, 2u32), || {
                log("calc two");
                Rc::new(Dropped("two"))
            })
        } else {
            remember_keyed(1u32, || {
                log("calc one");
                Rc::new(Dropped("one"))
            })
        };
        let sibling = remember(|| {
            log("calc sibling");
            Rc::new(7u32)
        });
        log(format!("value {} sibling {sibling}", value.0));
    });
    assert_eq!(take_log(), ["calc one", "calc sibling", "value one sibling 7"]);
    two.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["calc two", "value two sibling 7", "dropped one"]);
    two.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["calc one", "value one sibling 7", "dropped two"]);
    composition.validate();
}

/// The keys stay the same while the value's type changes at that position: the value is
/// recomputed, and the kept keys aren't replaced (a replaced value is dropped at the end of the
/// pass; the keys are `Rc`s counting their holders).
#[test]
fn remember_keyed_with_same_keys_and_a_new_value_type_keeps_the_keys() {
    let other = mutable_state_of(false);
    let handle = other.handle();
    let keys = Rc::new(5u32);
    let probe = keys.clone();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let keys = keys.clone();
        if handle.get() {
            let value = remember_keyed(keys, || Rc::new(String::from("text")));
            log(format!("text {value}"));
        } else {
            let value = remember_keyed(keys, || Rc::new(1u8));
            log(format!("number {value}"));
        }
    });
    assert_eq!(take_log(), ["number 1"]);
    // The composition holds the content's `keys` and the slot's: 3 with `probe`.
    assert_eq!(Rc::strong_count(&probe), 3);
    other.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["text text"]);
    assert_eq!(Rc::strong_count(&probe), 3);
    composition.validate();
}

/// Changed keys recompute the value, which is forgotten at the end of the pass; the same keys
/// skip `calc`. Observers get one `on_forgotten` each.
#[test]
fn remember_observer_keyed_recreates_once_per_key_change() {
    let key = mutable_state_of(0u32);
    let handle = key.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let key = handle.get();
        let observer = remember_observer_keyed(key, move || {
            log(format!("calc {key}"));
            Observer(["a", "b", "c"][key as usize])
        });
        let after = remember(|| Rc::new(Dropped("after")));
        log(format!("compose {} {}", observer.0, after.0));
    });
    assert_eq!(take_log(), ["calc 0", "compose a after", "remembered a"]);
    key.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["calc 1", "compose b after", "forgotten a", "remembered b"]);
    key.set(1);
    composition.recompose();
    assert_eq!(take_log(), Vec::<String>::new());
    key.set(2);
    composition.recompose();
    assert_eq!(take_log(), ["calc 2", "compose c after", "forgotten b", "remembered c"]);
    drop(composition);
    assert_eq!(take_log(), ["forgotten c", "dropped after"]);
}

/// A `remember` in a branch that flips (no group around it): typed slots recreate what moved
/// under another type, never misread it.
#[test]
fn remember_in_a_flipping_branch_recreates_by_type() {
    let flag = mutable_state_of(true);
    let handle = flag.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        if handle.get() {
            let first = remember(|| Rc::new(String::from("first")));
            log(format!("first {first}"));
        }
        let second = remember(|| {
            log("calc second");
            Rc::new(2u64)
        });
        log(format!("second {second}"));
    });
    assert_eq!(take_log(), ["first first", "calc second", "second 2"]);
    flag.set(false);
    composition.recompose();
    // `second` reads slot 0, which holds a `String`: recreated, and slot 1 is dropped.
    assert_eq!(take_log(), ["calc second", "second 2"]);
    flag.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["first first", "calc second", "second 2"]);
    composition.validate();
}

/// A keyed `remember_keyed` inside `key` groups that move: each value moves with its group.
#[test]
fn remember_keyed_moves_with_its_keyed_group() {
    let order = mutable_state_of(vec![1u32, 2, 3]);
    let handle = order.handle();
    let calcs = Rc::new(Cell::new(0));
    let counted = calcs.clone();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        for id in handle.get() {
            let counted = counted.clone();
            key(id, || {
                let value = remember_keyed(id * 10, || {
                    counted.set(counted.get() + 1);
                    Rc::new(id * 10)
                });
                log(format!("{id}:{value}"));
            });
        }
    });
    assert_eq!(calcs.get(), 3);
    take_log();
    order.set(vec![3, 1, 2]);
    composition.recompose();
    assert_eq!(calcs.get(), 3);
    assert_eq!(take_log(), ["3:30", "1:10", "2:20"]);
    composition.validate();
}

/// A `calc` reading state subscribes the composing scope, as before.
#[test]
fn remember_calc_reading_state_subscribes_the_scope() {
    let state = mutable_state_of(1u32);
    let other = mutable_state_of(0u32);
    let (handle, other_handle) = (state.handle(), other.handle());
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let value = remember_keyed(other_handle.get(), || Rc::new(handle.get()));
        log(format!("value {value}"));
    });
    assert_eq!(take_log(), ["value 1"]);
    state.set(2);
    composition.recompose();
    // The scope re-ran (it read `state` in `calc`), but the keys are the same: the value stays.
    assert_eq!(take_log(), ["value 1"]);
    other.set(1);
    composition.recompose();
    assert_eq!(take_log(), ["value 2"]);
}

/// A `calc` that panics poisons the pass, and the observers it remembered are abandoned.
#[test]
fn remember_keyed_with_a_panicking_calc_poisons_the_pass() {
    let key = mutable_state_of(0u32);
    let handle = key.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let key = handle.get();
        let _first = remember_observer(|| Observer("first"));
        let value = remember_keyed(key, || {
            if key == 1 {
                panic!("calc failed");
            }
            Rc::new(key)
        });
        log(format!("value {value}"));
    });
    assert_eq!(take_log(), ["value 0", "remembered first"]);
    key.set(1);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| composition.recompose()));
    assert!(result.is_err());
    drop(composition);
    assert_eq!(take_log(), ["forgotten first"]);
}

// ---- Kept arguments, as `#[composable]` expands them ---------------------------------------

/// `#[composable] fn Shown<T>(value: T)`, expanded as D200 does.
fn Shown<T: ComposeArg + Clone + std::fmt::Debug + 'static>(value: T) {
    let composer = current();
    composer.start_restart_group(key_of("review_monomorphization::Shown"));
    let mut kept = composer.take_kept();
    let changed = match kept.get::<(T,)>() {
        Some(kept) => !kept.0.same(&value),
        None => true,
    };
    if changed {
        kept.set((value.clone(),));
    }
    if !kept.put(changed) && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        RUNS.with(|runs| runs.set(runs.get() + 1));
        log(format!("{value:?}"));
    }
    composer
        .end_restart_group(|kept| Shown::<T>(kept.downcast_ref::<(T,)>().expect("kept").0.clone()));
}

/// Skips iff every argument is `same`: an `Rc` by identity (an equal value in a new `Rc` runs),
/// a `String` or an integer by value.
#[test]
fn inline_kept_arguments_skip_exactly_when_same() {
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    let shared = Rc::new(1u32);
    let first = shared.clone();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let tick = handle.get();
        // Same `Rc` each pass, then a new `Rc` holding an equal value.
        let rc = if tick >= 2 { Rc::new(1u32) } else { first.clone() };
        Shown(rc);
        Shown(String::from("same"));
        Shown(tick / 2);
    });
    assert_eq!(take_runs(), 3);
    take_log();
    tick.set(1);
    composition.recompose();
    // Same `Rc`, same string, 1/2 == 0: all skip.
    assert_eq!(take_runs(), 0);
    tick.set(2);
    composition.recompose();
    // A new `Rc` (identity), and 2/2 == 1: two run.
    assert_eq!(take_runs(), 2);
    drop(shared);
    composition.validate();
}

/// The composable recomposes itself (its own scope invalid) with the kept arguments: the
/// restart passes clones of the kept tuple, which compare same, and the body runs.
#[test]
fn inline_kept_arguments_restart_with_their_kept_values() {
    let state = mutable_state_of(0u32);
    let handle = state.handle();
    fn Reader(name: String, state: MutableState<u32>) {
        let composer = current();
        composer.start_restart_group(key_of("review_monomorphization::Reader"));
        let mut kept = composer.take_kept();
        let changed = match kept.get::<(String, MutableState<u32>)>() {
            Some(kept) => !kept.0.same(&name) || !kept.1.same(&state),
            None => true,
        };
        if changed {
            kept.set((name.clone(), state));
        }
        if !kept.put(changed) && composer.skipping() {
            composer.skip_to_group_end();
        } else {
            RUNS.with(|runs| runs.set(runs.get() + 1));
            log(format!("{name}={}", state.get()));
        }
        composer.end_restart_group(|kept| {
            let (name, state) =
                kept.downcast_ref::<(String, MutableState<u32>)>().expect("kept").clone();
            Reader(name, state)
        });
    }
    let composition = Composition::new(Tree::new());
    composition.set_content(move || Reader(String::from("r"), handle));
    assert_eq!(take_log(), ["r=0"]);
    for round in 1..5 {
        state.set(round);
        composition.recompose();
        assert_eq!(take_log(), [format!("r={round}")]);
    }
    composition.validate();
}

/// A `KeptArguments` dropped without `put` (hand-written misuse, or a panic in `same` or a
/// clone): the group loses its kept arguments, and the next pass runs the body as for a new
/// group, rather than skipping on stale arguments.
#[test]
fn kept_arguments_lost_by_a_panicking_comparison_rerun_the_body() {
    #[derive(Clone, Debug)]
    struct Fussy(u32);
    impl ComposeArg for Fussy {
        fn same(&self, other: &Self) -> bool {
            if self.0 == 13 || other.0 == 13 {
                panic!("unlucky");
            }
            self.0 == other.0
        }
    }
    let value = mutable_state_of(1u32);
    let handle = value.handle();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || Shown(Fussy(handle.get())));
    assert_eq!(take_log(), ["Fussy(1)"]);
    value.set(13);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| composition.recompose()));
    assert!(result.is_err());
    // The pass is poisoned: further passes panic (04, "Panics").
    take_log();
}

// ---- Lambdas -------------------------------------------------------------------------------

/// A memoized lambda whose capture is `same` returns the kept callback; a changed capture
/// stores a new one. The captures' type changing at that slot (two lambdas in a flipping
/// branch) is a miss that replaces the slot, never a misread.
#[test]
fn lambda_memoization_through_store_callback_is_exact() {
    let tick = mutable_state_of(0u32);
    let handle = tick.handle();
    type Seen = Rc<RefCell<Vec<Callback<dyn Fn() -> String>>>>;
    let seen: Seen = Rc::default();
    let pushed = seen.clone();
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        let tick = handle.get();
        let callback: Callback<dyn Fn() -> String> = if tick % 4 == 3 {
            let text = String::from("text");
            resolve_lazy_0(lambda_probe(1, ((&&ArgProbe(&text)).arg_key(),)), || {
                let text = text.clone();
                move || text.clone()
            })
        } else {
            let half = tick / 2;
            resolve_lazy_0(lambda_probe(1, ((&&ArgProbe(&half)).arg_key(),)), || {
                move || half.to_string()
            })
        };
        pushed.borrow_mut().push(callback);
    });
    for round in 1..8 {
        tick.set(round);
        composition.recompose();
    }
    let seen = seen.borrow();
    let values: Vec<String> = seen.iter().map(|callback| callback()).collect();
    assert_eq!(values, ["0", "0", "1", "text", "2", "2", "3", "text"]);
    // 0 and 1 share captures (half 0); 2 differs; 3 is the other lambda; 4 is a miss (the slot
    // held the other type); 5 shares 4's; 6 differs; 7 is the other lambda again.
    let same: Vec<bool> = seen.windows(2).map(|pair| pair[0].ptr_eq(&pair[1])).collect();
    assert_eq!(same, [true, false, false, false, true, false, false]);
    composition.validate();
}

/// Outside composition, a probe has no slot: the lambda is wrapped and not remembered.
#[test]
fn lambda_probe_outside_composition_wraps_without_storing() {
    let value = 3u32;
    let callback: Callback<dyn Fn() -> u32> =
        resolve_lazy_0(lambda_probe(2, ((&&ArgProbe(&value)).arg_key(),)), || move || value);
    assert_eq!(callback(), 3);
}

// ---- Composition locals --------------------------------------------------------------------

static LOCAL_NAME: CompositionLocal<String> =
    CompositionLocal::new("review_monomorphization::LocalName", || String::from("default"));

/// Nested providers of the same local, the inner one removed then restored, the outer value
/// changed: readers see the nearest provider's value.
#[test]
fn nested_providers_of_the_same_local_and_their_removal() {
    let inner = mutable_state_of(true);
    let outer_value = mutable_state_of(String::from("outer"));
    let (inner_handle, outer_handle) = (inner.handle(), outer_value.handle());
    let composition = Composition::new(Tree::new());
    composition.set_content(move || {
        log(format!("top {}", LOCAL_NAME.current()));
        CompositionLocalProvider(LOCAL_NAME.provides(outer_handle.get()), || {
            log(format!("outer {}", LOCAL_NAME.current()));
            if inner_handle.get() {
                CompositionLocalProvider(LOCAL_NAME.provides(String::from("inner")), || {
                    log(format!("inner {}", LOCAL_NAME.current()));
                });
            } else {
                log(format!("no inner {}", LOCAL_NAME.current()));
            }
        });
    });
    assert_eq!(take_log(), ["top default", "outer outer", "inner inner"]);
    inner.set(false);
    composition.recompose();
    assert_eq!(take_log(), ["top default", "outer outer", "no inner outer"]);
    outer_value.set(String::from("changed"));
    composition.recompose();
    assert_eq!(take_log(), ["top default", "outer changed", "no inner changed"]);
    inner.set(true);
    composition.recompose();
    assert_eq!(take_log(), ["top default", "outer changed", "inner inner"]);
    composition.validate();
}
