//! Compose's `CompoundHashKeyTests`, adapted: `current_composite_key_hash_code` is unique for
//! adjacent, repeated and conditional calls, stable across recompositions and recreation, and
//! moves with keys. Its values differ from Compose's by design (D290). The subcomposition tests
//! are in composition_context.rs.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

/// What `A` records, as the file's `hashTrace` and `markerToHash`.
struct TraceData {
    recompose: OwnedState<i32>,
    hashes: RefCell<Vec<u64>>,
    markers: RefCell<HashMap<i32, u64>>,
    next_marker: Cell<i32>,
}

#[derive(Clone)]
struct Trace(Rc<TraceData>);

impl ComposeArg for Trace {
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Trace {
    fn new() -> Trace {
        Trace(Rc::new(TraceData {
            recompose: mutable_state_of(0),
            hashes: RefCell::default(),
            markers: RefCell::default(),
            next_marker: Cell::new(100),
        }))
    }

    fn new_marker(&self) -> i32 {
        let marker = self.0.next_marker.get();
        self.0.next_marker.set(marker + 1);
        marker
    }

    /// The hashes and markers recorded since the last call.
    fn take(&self) -> (Vec<u64>, HashMap<i32, u64>) {
        (
            std::mem::take(&mut *self.0.hashes.borrow_mut()),
            std::mem::take(&mut *self.0.markers.borrow_mut()),
        )
    }
}

#[composable]
fn A(trace: Trace, marker: i32) {
    trace.0.recompose.get();
    let hash = current_composite_key_hash_code();
    trace.0.hashes.borrow_mut().push(hash);
    trace.0.markers.borrow_mut().insert(marker, hash);
}

/// `A()` with its marker remembered where it's called, as `A(marker = remember { newMarker() })`.
fn a(trace: &Trace) {
    let t = trace.clone();
    let marker = remember(move || t.new_marker());
    A(trace.clone(), marker);
}

#[composable]
fn C(trace: Trace, condition: bool) {
    let markers = [(); 3].map(|_| {
        let t = trace.clone();
        remember(move || t.new_marker())
    });
    if condition {
        for marker in markers {
            A(trace.clone(), marker);
        }
    }
}

/// Recomposes every `A`, returning what they recorded, as `retrace()`.
fn retrace(test: &mut CompositionTest, trace: &Trace) -> (Vec<u64>, HashMap<i32, u64>) {
    trace.take();
    trace.0.recompose.update(|count| count + 1);
    test.advance();
    trace.take()
}

/// Merges `markers` with a retrace's, failing on a marker whose hash changed.
fn retrace_consistent_with(
    test: &mut CompositionTest,
    trace: &Trace,
    markers: HashMap<i32, u64>,
) -> HashMap<i32, u64> {
    let (_, recomposed) = retrace(test, trace);
    let mut merged = markers;
    for (marker, hash) in recomposed {
        if let Some(existing) = merged.insert(marker, hash) {
            assert_eq!(existing, hash, "inconsistent marker {marker}");
        }
    }
    merged
}

/// Composes `content`, checks `duplicates` hashes repeat and a recomposition traces the same
/// hashes; returns the markers.
fn expect_hash_codes(
    test: &mut CompositionTest,
    trace: &Trace,
    duplicates: usize,
    content: impl Fn(&Trace) + 'static,
) -> HashMap<i32, u64> {
    let t = trace.clone();
    test.compose(move || content(&t));
    let (hashes, markers) = trace.take();
    let mut unique = hashes.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(hashes.len(), unique.len() + duplicates, "duplicate hash codes: {hashes:?}");
    let (recomposed, recomposed_markers) = retrace(test, trace);
    assert_eq!(recomposed, hashes, "the same hash codes after a recomposition");
    let mut merged = markers;
    for (marker, hash) in recomposed_markers {
        if let Some(existing) = merged.insert(marker, hash) {
            assert_eq!(existing, hash, "inconsistent marker {marker}");
        }
    }
    merged
}

fn expect_unique_hash_codes(
    test: &mut CompositionTest,
    trace: &Trace,
    content: impl Fn(&Trace) + 'static,
) -> HashMap<i32, u64> {
    expect_hash_codes(test, trace, 0, content)
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, adjacentCallsProduceUniqueKeys
#[test]
fn adjacent_calls_produce_unique_keys() {
    let (mut test, trace) = (composition_test(), Trace::new());
    expect_unique_hash_codes(&mut test, &trace, |trace| {
        a(trace);
        a(trace);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, indirectConditionalCallsProduceUniqueKeys
#[test]
fn indirect_conditional_calls_produce_unique_keys() {
    let (mut test, trace) = (composition_test(), Trace::new());
    expect_unique_hash_codes(&mut test, &trace, |trace| {
        C(trace.clone(), true);
        C(trace.clone(), true);
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, repeatedCallsProduceUniqueKeys
#[test]
fn repeated_calls_produce_unique_keys() {
    #[composable]
    fn Content(trace: Trace) {
        for _ in 0..10 {
            a(&trace);
        }
    }
    let (mut test, trace) = (composition_test(), Trace::new());
    expect_unique_hash_codes(&mut test, &trace, |trace| Content(trace.clone()));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, uniqueKeysGenerateUniqueCompositeKeys
#[test]
fn unique_keys_generate_unique_composite_keys() {
    let (mut test, trace) = (composition_test(), Trace::new());
    expect_unique_hash_codes(&mut test, &trace, |trace| {
        key(1, || a(trace));
        key(2, || a(trace));
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, duplicateKeysGenerateDuplicateCompositeKeys
#[test]
fn duplicate_keys_generate_duplicate_composite_keys() {
    let (mut test, trace) = (composition_test(), Trace::new());
    expect_hash_codes(&mut test, &trace, 4, |trace| {
        for item in [1, 2, 1, 2, 1, 2] {
            key(item, || a(trace));
        }
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, compositeKeysAreConsistentBetweenOnRecreate
#[test]
fn composite_keys_are_consistent_when_content_is_recreated() {
    #[composable]
    fn Content(trace: Trace, state: MutableState<bool>) {
        C(trace.clone(), state.get());
        C(trace, !state.get());
    }
    let (mut test, trace) = (composition_test(), Trace::new());
    let state = mutable_state_of(true);
    let s = state.handle();
    let mut markers =
        expect_unique_hash_codes(&mut test, &trace, move |trace| Content(trace.clone(), s));
    for _ in 0..4 {
        state.update(|state| !state);
        markers = retrace_consistent_with(&mut test, &trace, markers);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, compositeKeysMoveWithKeys
#[test]
fn composite_keys_move_with_their_keys() {
    #[composable]
    fn Content(trace: Trace, list: compose_runtime::snapshots::SnapshotStateList<i32>) {
        for item in list.iter_cloned() {
            key(item, || a(&trace));
        }
    }
    let (mut test, trace) = (composition_test(), Trace::new());
    let list = mutable_state_list_from(1..=10);
    let l = list.handle();
    let mut markers =
        expect_unique_hash_codes(&mut test, &trace, move |trace| Content(trace.clone(), l));
    let mut seed: u64 = 7;
    for step in 0..4 {
        list.edit(|items| {
            if step < 2 {
                items.reverse();
            } else {
                for index in (1..items.len()).rev() {
                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                    items.swap(index, (seed >> 33) as usize % (index + 1));
                }
            }
        });
        markers = retrace_consistent_with(&mut test, &trace, markers);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, testEnumKeys
#[test]
fn a_composable_under_an_enum_key_keeps_its_hash() {
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, ComposeArg)]
    enum Config {
        A,
    }
    #[composable]
    fn Child(scope: ScopeHolder, current_key: Rc<Cell<u64>>) {
        scope.capture();
        current_key.set(current_composite_key_hash_code());
    }
    let mut test = composition_test();
    let (scope, current_key) = (ScopeHolder::new(), Rc::new(Cell::new(0)));
    let (s, k) = (scope.clone(), current_key.clone());
    test.compose(move || key(Config::A, || Child(s.clone(), k.clone())));
    let original = current_key.get();
    scope.invalidate();
    test.advance();
    assert_eq!(current_key.get(), original);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, testReusableContentNodeKeys
#[test]
fn the_hash_is_the_same_on_both_sides_of_reused_content() {
    #[composable]
    fn Content(content_key: MutableState<i32>, keys: Rc<Cell<(u64, u64)>>) {
        let current = content_key.get();
        ReusableContent(current, || {
            let on_enter = current_composite_key_hash_code();
            NonReusableText(current.to_string());
            keys.set((on_enter, current_composite_key_hash_code()));
        });
    }
    let mut test = composition_test();
    let content_key = mutable_state_of(0);
    let keys = Rc::new(Cell::new((0, 1)));
    let (c, k) = (content_key.handle(), keys.clone());
    test.compose(move || Content(c, k.clone()));
    assert_eq!(keys.get().0, keys.get().1);
    content_key.set(1);
    test.advance();
    assert_eq!(keys.get().0, keys.get().1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, nestedCompositeHashKeyIsConsistentOnRecompose
#[test]
fn a_nested_hash_stays_the_same_when_its_scope_recomposes_alone() {
    #[composable]
    fn RestartGroup(content: impl Fn() + 'static) {
        content();
    }
    #[composable]
    fn Content(scope: ScopeHolder, last: Rc<Cell<u64>>) {
        RestartGroup!({});
        key(0, || {
            let (scope, last) = (scope.clone(), last.clone());
            RestartGroup!({
                scope.capture();
                last.set(current_composite_key_hash_code());
            });
        });
    }
    let mut test = composition_test();
    let (scope, last) = (ScopeHolder::new(), Rc::new(Cell::new(0)));
    let (s, l) = (scope.clone(), last.clone());
    test.compose(move || Content(s.clone(), l.clone()));
    test.validate(|_| {});
    let original = last.get();
    last.set(u64::MAX);
    scope.invalidate();
    test.expect_no_changes();
    test.revalidate();
    assert_eq!(last.get(), original);
}
