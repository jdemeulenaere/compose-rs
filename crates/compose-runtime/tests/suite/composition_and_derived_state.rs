//! Compose's `CompositionAndDerivedStateTests`, adapted: `derived_state_of` read in
//! composition invalidates only on a changed result, by its policy; only the derived states
//! composables read are computed, and not before the pass reaches them (D711).
//!
//! Compose changes two states at once in a `Snapshot.withMutableSnapshot`; writes here are seen
//! at once (D16), but nothing recomposes between two writes of a test, so two writes in a row
//! are seen together by the next pass.

#![allow(non_snake_case)]

use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

#[composable]
fn DisplayItem(name: &'static str, state: State<i32>) {
    Text(&format!("{name} = {}", state.get()));
}

#[composable]
fn DisplayIndirect(name: &'static str, state: State<i32>) {
    DisplayItem(name, state);
}

/// `Display(vararg names)`: a new list each call, as Kotlin's varargs, so it always runs.
#[composable]
fn Display(names: Rc<[(&'static str, State<i32>)]>) {
    for (name, state) in names.iter() {
        DisplayIndirect(name, *state);
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateOfChangesInvalidate
#[test]
fn a_derived_state_change_invalidates_its_readers() {
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(31), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let answer = derived_state_of(move || ah.get() + bh.get());
    let answer_state = answer.as_state();
    test.compose(move || Text(&format!("The answer is {}", answer_state.get())));
    test.validate(move |v| v.text(&format!("The answer is {}", ah.get() + bh.get())));
    a.update(|a| a + 1);
    test.expect_changes();
    b.update(|b| b + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, onlyInvalidatesIfResultIsDifferent
#[test]
fn readers_recompose_only_when_the_result_changes() {
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(32), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let answer = derived_state_of(move || ah.get() + bh.get());
    let answer_state = answer.as_state();
    test.compose(move || Text(&format!("The answer is {}", answer_state.get())));
    test.validate(move |v| v.text(&format!("The answer is {}", ah.get() + bh.get())));
    a.update(|a| a + 1);
    b.update(|b| b - 1);
    test.expect_no_changes();
    test.revalidate();
    a.update(|a| a + 1);
    test.expect_changes();
    test.revalidate();
    b.update(|b| b - 1);
    test.expect_changes();
    test.revalidate();
    a.update(|a| a + 1);
    b.update(|b| b - 1);
    test.expect_no_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, onlyInvalidatesIfResultIsStructurallyDifferent
#[test]
fn readers_recompose_only_when_the_result_is_structurally_different() {
    let mut test = composition_test();
    let a = mutable_state_of_with(Rc::new(vec![32]), referential_equality_policy());
    let b = mutable_state_of_with(Rc::new(vec![10]), referential_equality_policy());
    let (ah, bh) = (a.handle(), b.handle());
    let answer = derived_state_of(move || [ah.get().as_slice(), bh.get().as_slice()].concat());
    let answer_state = answer.as_state();
    test.compose(move || Text(&format!("The answer is {:?}", answer_state.get())));
    test.validate(|v| v.text("The answer is [32, 10]"));
    a.set(Rc::new(vec![32]));
    b.set(Rc::new(vec![10]));
    test.expect_no_changes();
    test.revalidate();
    a.set(Rc::new(vec![32]));
    test.expect_no_changes();
    test.revalidate();
}

const USE_D: i32 = 1;
const USE_E: i32 = 2;
const USE_F: i32 = 4;

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, onlyEvaluateDerivedStatesThatAreLive
#[test]
fn only_the_derived_states_read_are_computed() {
    #[composable]
    fn Content(uses: MutableState<i32>, d: State<i32>, e: State<i32>, f: State<i32>) {
        let uses = uses.get();
        let has = |mask: i32| uses & mask == mask;
        if has(USE_D) {
            Display(Rc::new([("d", d)]));
        }
        if has(USE_E) {
            Display(Rc::new([("e", e)]));
        }
        if has(USE_F) {
            Display(Rc::new([("f", f)]));
        }
        if has(USE_D | USE_E) {
            Display(Rc::new([("d", d), ("e", e)]));
        }
        if has(USE_D | USE_F) {
            Display(Rc::new([("d", d), ("f", f)]));
        }
        if has(USE_E | USE_F) {
            Display(Rc::new([("e", e), ("f", f)]));
        }
        if has(USE_D | USE_E | USE_F) {
            Display(Rc::new([("d", d), ("e", e), ("f", f)]));
        }
    }
    let mut test = composition_test();
    let a = mutable_state_of(11);
    let ah = a.handle();
    let uses = mutable_state_of(USE_D);
    let (d_calculated, e_calculated, f_calculated) =
        (Counter::new(), Counter::new(), Counter::new());
    let derived = |counter: &Counter, offset: i32| {
        let counter = counter.clone();
        derived_state_of(move || {
            counter.increment();
            ah.get() + offset
        })
    };
    let (d, e, f) =
        (derived(&d_calculated, 0), derived(&e_calculated, 100), derived(&f_calculated, 1000));
    let mut expected = (0, 0, 0);
    let mut expect = |modified: i32, context: &str| {
        if modified & USE_D != 0 {
            expected.0 += 1;
        }
        if modified & USE_E != 0 {
            expected.1 += 1;
        }
        if modified & USE_F != 0 {
            expected.2 += 1;
        }
        let actual = (d_calculated.get(), e_calculated.get(), f_calculated.get());
        assert_eq!(actual, expected, "calculations of d, e, f{context}");
    };
    // Nothing is calculated yet.
    expect(0, "");
    let (u, ds, es, fs) = (uses.handle(), d.as_state(), e.as_state(), f.as_state());
    test.compose(move || Content(u, ds, es, fs));
    let (uh, a_value) = (uses.handle(), a.handle());
    test.validate(move |v| {
        let (uses, a) = (uh.get(), a_value.get());
        let has = |mask: i32| uses & mask == mask;
        let (d, e, f) =
            (format!("d = {a}"), format!("e = {}", a + 100), format!("f = {}", a + 1000));
        if has(USE_D) {
            v.text(&d);
        }
        if has(USE_E) {
            v.text(&e);
        }
        if has(USE_F) {
            v.text(&f);
        }
        if has(USE_D | USE_E) {
            v.text(&d);
            v.text(&e);
        }
        if has(USE_D | USE_F) {
            v.text(&d);
            v.text(&f);
        }
        if has(USE_E | USE_F) {
            v.text(&e);
            v.text(&f);
        }
        if has(USE_D | USE_E | USE_F) {
            v.text(&d);
            v.text(&e);
            v.text(&f);
        }
    });
    test.verify_consistent();
    expect(USE_D, "");
    a.update(|a| a + 1);
    test.expect_changes();
    test.revalidate();
    expect(USE_D, "");
    for new_uses in [
        USE_D | USE_E,
        USE_D | USE_F,
        USE_E,
        USE_F,
        USE_D | USE_E,
        USE_D | USE_F,
        USE_D | USE_E | USE_F,
        USE_E | USE_F,
        0,
    ] {
        let previous = uses.get();
        uses.set(new_uses);
        a.update(|a| a + 1);
        test.expect_changes();
        test.verify_consistent();
        test.revalidate();
        expect(new_uses, &format!(" switching from {previous} to {new_uses}"));
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, ensureCalculateIsNotCalledTooSoon
#[test]
fn a_derived_state_is_computed_where_the_pass_reads_it() {
    #[composable]
    fn Content(a: MutableState<i32>, d: State<i32>, calculated: Counter, changed: Counter) {
        Text(&format!("a = {}", a.get()));
        let before = calculated.get();
        Display(Rc::new([("d", d)]));
        changed.set(i64::from(before != calculated.get()));
    }
    let mut test = composition_test();
    let a = mutable_state_of(11);
    let ah = a.handle();
    let calculated = Counter::new();
    let counter = calculated.clone();
    let d = derived_state_of(move || {
        counter.increment();
        ah.get() + 10
    });
    let changed = Counter::new();
    let (ds, c, ch) = (d.as_state(), calculated.clone(), changed.clone());
    test.compose(move || Content(ah, ds, c.clone(), ch.clone()));
    test.validate(move |v| {
        v.text(&format!("a = {}", ah.get()));
        v.text(&format!("d = {}", ah.get() + 10));
    });
    assert_eq!(changed.get(), 1, "expected d to recalculate");
    a.update(|a| a + 1);
    test.expect_changes();
    test.revalidate();
    assert_eq!(changed.get(), 1, "expected d to recalculate where it's read");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, observingBothNormalAndDerivedInSameScope
#[test]
fn a_scope_reading_state_and_derived_state_follows_both() {
    #[composable]
    fn Content(b: State<bool>, c: MutableState<bool>) {
        Linear!({
            if b.get() {
                Text("B is true");
            }
            if c.get() {
                Text("C is true");
            }
        });
    }
    let mut test = composition_test();
    let a = mutable_state_of(0);
    let ah = a.handle();
    let b = derived_state_of(move || ah.get() > 0);
    let c = mutable_state_of(false);
    let (bs, ch) = (b.as_state(), c.handle());
    test.compose(move || Content(bs, ch));
    test.validate(move |v| {
        v.linear(|v| {
            if bs.get() {
                v.text("B is true");
            }
            if ch.get() {
                v.text("C is true");
            }
        })
    });
    a.update(|a| a + 1);
    test.expect_changes();
    test.revalidate();
    a.update(|a| a + 1);
    test.advance();
    test.revalidate();
    a.update(|a| a + 1);
    c.set(true);
    test.advance();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, observingDerivedStateInMultipleScopes
#[test]
fn a_derived_state_read_in_two_scopes_follows_the_one_left() {
    #[composable]
    fn Content(observe_in_first: MutableState<bool>, count: MutableState<i32>) {
        let items =
            remember(move || derived_state_of(move || (0..count.get()).collect::<Vec<_>>()));
        Linear!({
            if observe_in_first.get() {
                Text(&format!("List of size {}", items.with(Vec::len)));
            }
        });
        Linear!({
            Text(&format!("List of size {}", items.with(Vec::len)));
        });
    }
    let mut test = composition_test();
    let observe_in_first = mutable_state_of(true);
    let count = mutable_state_of(0);
    let (o, c) = (observe_in_first.handle(), count.handle());
    test.compose(move || Content(o, c));
    test.validate(|v| {
        v.linear(|v| v.text("List of size 0"));
        v.linear(|v| v.text("List of size 0"));
    });
    observe_in_first.set(false);
    test.advance();
    count.update(|c| c + 1);
    test.advance();
    test.validate(|v| {
        v.empty_linear();
        v.linear(|v| v.text("List of size 1"));
    });
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateOfNestedChangesInvalidate
#[test]
fn nested_derived_states_invalidate_their_readers() {
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(31), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let transient = derived_state_of(move || ah.get() + bh.get());
    let transient_state = transient.as_state();
    let answer = derived_state_of(move || transient_state.get() - 1);
    let answer_state = answer.as_state();
    test.compose(move || Text(&format!("The answer is {}", answer_state.get())));
    test.validate(move |v| v.text(&format!("The answer is {}", ah.get() + bh.get() - 1)));
    a.update(|a| a + 1);
    test.expect_changes();
    b.update(|b| b + 1);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateOfMutationPolicyDoesNotInvalidateNestedStates
#[test]
fn a_nested_derived_state_unchanged_by_its_policy_stops_the_change() {
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(30), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let transient =
        derived_state_of_with(structural_equality_policy(), move || (ah.get() + bh.get()) / 10);
    let transient_state = transient.as_state();
    let calculations = Counter::new();
    let counter = calculations.clone();
    let answer = derived_state_of(move || {
        counter.increment();
        transient_state.get()
    });
    let answer_state = answer.as_state();
    test.compose(move || Text(&format!("The answer is {}", answer_state.get())));
    test.validate(move |v| v.text(&format!("The answer is {}", (ah.get() + bh.get()) / 10)));
    assert_eq!(calculations.get(), 1);
    a.update(|a| a + 10);
    test.expect_changes();
    assert_eq!(calculations.get(), 2);
    b.update(|b| b + 1);
    test.expect_no_changes();
    assert_eq!(calculations.get(), 2);
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateOfStructuralMutationPolicyDoesNotRecompose
#[test]
fn an_equal_result_by_the_structural_policy_recomposes_nothing() {
    #[composable]
    fn Content(answer: State<Vec<bool>>, compositions: Counter) {
        let remembered = remember_updated_state(Rc::new(answer.get()));
        Linear!({
            compositions.increment();
            Text(&format!("The answer is {:?}", remembered.get()));
        });
    }
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(30), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let answer = derived_state_of_with(structural_equality_policy(), move || {
        vec![ah.get() >= 30, bh.get() >= 10]
    });
    let compositions = Counter::new();
    let (s, c) = (answer.as_state(), compositions.clone());
    test.compose(move || Content(s, c.clone()));
    test.validate(|v| v.linear(|v| v.text("The answer is [true, true]")));
    assert_eq!(compositions.get(), 1);
    a.update(|a| a + 1);
    test.expect_no_changes();
    assert_eq!(compositions.get(), 1);
    b.update(|b| b + 1);
    test.expect_no_changes();
    assert_eq!(compositions.get(), 1);
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateOfReferentialMutationPolicyRecomposes
#[test]
fn a_new_result_by_the_referential_policy_recomposes_its_readers() {
    #[composable]
    fn Content(answer: State<Rc<Vec<bool>>>, compositions: Counter) {
        let remembered = remember_updated_state(answer.get());
        Linear!({
            compositions.increment();
            Text(&format!("The answer is {:?}", remembered.get()));
        });
    }
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(30), mutable_state_of(10));
    let (ah, bh) = (a.handle(), b.handle());
    let answer = derived_state_of_with(referential_equality_policy(), move || {
        Rc::new(vec![ah.get() >= 30, bh.get() >= 10])
    });
    let compositions = Counter::new();
    let (s, c) = (answer.as_state(), compositions.clone());
    test.compose(move || Content(s, c.clone()));
    test.validate(|v| v.linear(|v| v.text("The answer is [true, true]")));
    assert_eq!(compositions.get(), 1);
    a.update(|a| a + 1);
    // An equal list, but a new one: its readers recompose, and change no view.
    test.expect_no_changes();
    assert_eq!(compositions.get(), 2);
    b.update(|b| b + 1);
    test.expect_no_changes();
    assert_eq!(compositions.get(), 3);
    test.revalidate();
}

/// `NestedItem`: a number, and whether it's active (state).
struct NestedItem {
    number: i32,
    active: OwnedState<bool>,
}

impl NestedItem {
    fn new(number: i32, active: bool) -> Rc<NestedItem> {
        Rc::new(NestedItem { number, active: mutable_state_of(active) })
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, nestedDerivedStateWithChangingState
#[test]
fn nested_derived_states_follow_state_inside_their_values() {
    let mut test = composition_test();
    let item = mutable_state_of_with(NestedItem::new(10, true), referential_equality_policy());
    let ih = item.handle();
    let even_active_item = derived_state_of_with(PtrOptionPolicy, move || {
        let item = ih.get();
        (item.active.get() && item.number % 2 == 0).then_some(item)
    });
    let even = even_active_item.as_state();
    let even_active_number =
        derived_state_of(move || even.with(|item| item.as_ref().map(|i| i.number)));
    let number = even_active_number.as_state();
    test.compose(move || Text(&format!("evenActiveNumber = {:?}", number.get())));
    test.validate(|v| v.text("evenActiveNumber = Some(10)"));
    item.with(|item| item.active.set(false));
    test.expect_changes();
    test.validate(|v| v.text("evenActiveNumber = None"));
    let copy = |number: i32| NestedItem::new(number, ih.with(|item| item.active.get()));
    item.set(copy(9));
    test.expect_no_changes();
    test.revalidate();
    item.set(copy(8));
    test.expect_no_changes();
    test.revalidate();
    item.with(|item| item.active.set(true));
    test.expect_changes();
    test.validate(|v| v.text("evenActiveNumber = Some(8)"));
}

/// Compose's structural policy on a `NestedItem?`: `equals` of a class without it is
/// identity.
struct PtrOptionPolicy;

impl MutationPolicy<Option<Rc<NestedItem>>> for PtrOptionPolicy {
    fn equivalent(a: &Option<Rc<NestedItem>>, b: &Option<Rc<NestedItem>>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateChangesDependencies
#[test]
fn a_derived_state_follows_the_dependencies_of_its_last_calculation() {
    let mut test = composition_test();
    let (a, b, c) = (mutable_state_of(10), mutable_state_of(20), mutable_state_of(30));
    let condition = mutable_state_of(true);
    let (ah, bh, ch, condh) = (a.handle(), b.handle(), c.handle(), condition.handle());
    let derived =
        derived_state_of(move || if condh.get() { ah.get() + bh.get() } else { ch.get() });
    let compositions = Counter::new();
    let (s, counter) = (derived.as_state(), compositions.clone());
    test.compose(move || {
        counter.increment();
        Text(&format!("Value is {}", s.get()));
    });
    test.validate(|v| v.text("Value is 30"));
    assert_eq!(compositions.get(), 1);
    condition.set(false);
    test.expect_no_changes();
    test.revalidate();
    assert_eq!(compositions.get(), 1);
    a.set(30);
    test.expect_no_changes();
    test.revalidate();
    assert_eq!(compositions.get(), 1);
    c.set(0);
    test.expect_changes();
    test.validate(|v| v.text("Value is 0"));
    assert_eq!(compositions.get(), 2);
    condition.set(true);
    a.set(-20);
    test.expect_no_changes();
    test.revalidate();
    assert_eq!(compositions.get(), 2);
    a.set(0);
    test.expect_changes();
    test.validate(|v| v.text("Value is 20"));
    assert_eq!(compositions.get(), 3);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, derivedStateSkippedScopeInvalidationsAreCleared
#[test]
fn an_unchanged_derived_state_leaves_no_invalidation_behind() {
    // Compose checks its composer's stacks are empty; here, that no scope is left invalid.
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(1), mutable_state_of(1));
    let (ah, bh) = (a.handle(), b.handle());
    let derived = derived_state_of(move || ah.get() + bh.get());
    let s = derived.as_state();
    test.compose(move || Wrap!({ Text(&format!("Derived: {}", s.get())) }));
    test.validate(|v| v.text("Derived: 2"));
    a.update(|a| a + 1);
    b.update(|b| b - 1);
    test.expect_no_changes();
    test.revalidate();
    assert!(!test.composition().has_invalidations());
    test.verify_consistent();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionAndDerivedStateTests.kt, writingToADerivedStateDependencyTriggersAForwardInvalidate
#[test]
fn writing_a_derived_state_dependency_in_composition_invalidates_forward() {
    #[composable]
    fn Content(a: MutableState<i32>, b: MutableState<i32>, d: State<i32>) {
        DisplayIndirect("d", d);
        let c = remember(|| mutable_state_of(0));
        c.set(a.get() + b.get());
        let e = remember(move || derived_state_of(move || a.get() + b.get() + c.get()));
        DisplayIndirect("e", e);
    }
    let mut test = composition_test();
    let (a, b) = (mutable_state_of(12), mutable_state_of(30));
    let (ah, bh) = (a.handle(), b.handle());
    let d = derived_state_of(move || ah.get() + bh.get());
    let ds = d.as_state();
    test.compose(move || Content(ah, bh, ds));
    test.validate(move |v| {
        let (a, b) = (ah.get(), bh.get());
        v.text(&format!("d = {}", a + b));
        v.text(&format!("e = {}", a + b + a + b));
    });
    a.update(|a| a + 1);
    test.expect_changes();
    test.revalidate();
    b.update(|b| b - 1);
    test.expect_changes();
    test.revalidate();
    a.update(|a| a + 1);
    b.update(|b| b - 1);
    test.advance();
    test.revalidate();
}
