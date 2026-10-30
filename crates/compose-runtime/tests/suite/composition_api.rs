//! What a host does to a composition, as Compose's `Composition`, `ReusableComposition` and
//! `PausableComposition`: disposing it, reusing and deactivating its content, composing it in
//! pauses. Against `CompositionReusingTests` and `PausableCompositionTests`, whose workflows run
//! inside a parent's effect there and in the test here until there are contexts (C3). A panic
//! poisons a composition, where Compose's reports it and can go on (behaviour 10); nodes are applied
//! as the pass reaches them outside paused content (D31).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::*;

use crate::mock::*;

fn default_context() -> CompositionContext {
    Recomposer::current_thread().as_context()
}

/// Calls of a remember observer.
#[derive(Default)]
struct Calls {
    remembered: Cell<u32>,
    forgotten: Cell<u32>,
    abandoned: Cell<u32>,
}

struct Observed(Rc<Calls>);

impl RememberObserver for Observed {
    fn on_remembered(&self) {
        self.0.remembered.set(self.0.remembered.get() + 1);
    }
    fn on_forgotten(&self) {
        self.0.forgotten.set(self.0.forgotten.get() + 1);
    }
    fn on_abandoned(&self) {
        self.0.abandoned.set(self.0.abandoned.get() + 1);
    }
}

fn counts(calls: &Calls) -> (u32, u32, u32) {
    (calls.remembered.get(), calls.forgotten.get(), calls.abandoned.get())
}

/// A `Text` under a `Linear`, the `Text` showing a remembered observer's name.
#[composable(non_restartable)]
fn RememberingLinear(calls: Rc<Calls>) {
    Linear!(move || {
        let held = calls.clone();
        remember_observer(move || Observed(held));
        Text!("Some text");
    });
}

fn top_text(applier: &ViewApplier) -> Vec<String> {
    fn add(applier: &ViewApplier, node: platform::NodeRef, into: &mut Vec<String>) {
        for child in applier.tree.children(node) {
            let view = applier.tree.get(child).expect("a live view");
            into.push(view.attribute("text").unwrap_or(view.name).to_string());
            add(applier, child, into);
        }
    }
    let mut views = Vec::new();
    add(applier, applier.tree.root(), &mut views);
    views
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, disposedCompositionShouldReportAsDisposed
#[test]
fn a_disposed_composition_says_so() {
    let composition = Composition::new(ViewApplier::new());
    assert!(!composition.is_disposed());
    composition.dispose();
    assert!(composition.is_disposed());
}

/// What it holds goes at `dispose`, not at the handle's drop; disposing again does nothing.
#[test]
fn disposing_lets_go_of_what_it_holds() {
    let calls = Rc::new(Calls::default());
    let composition = Composition::new(ViewApplier::new());
    let held = calls.clone();
    composition.set_content(move || RememberingLinear(held.clone()));
    assert_eq!(counts(&calls), (1, 0, 0));
    composition.dispose();
    assert_eq!(counts(&calls), (1, 1, 0));
    assert!(composition.with_applier(|applier: &mut ViewApplier| applier.roots().is_empty()));
    composition.dispose();
    assert_eq!(counts(&calls), (1, 1, 0));
    assert!(!composition.recompose());
    assert!(!composition.has_invalidations());
    drop(composition);
    assert_eq!(counts(&calls), (1, 1, 0));
}

#[test]
#[should_panic(expected = "the composition is disposed")]
fn setting_a_disposed_composition_s_content_panics() {
    let composition = Composition::new(ViewApplier::new());
    composition.dispose();
    composition.set_content(|| {});
}

/// Disposing from its own effect, as a child's owner does, is the drop it was.
#[test]
fn a_composition_can_dispose_itself_from_an_effect() {
    let composition = Rc::new(Composition::new(ViewApplier::new()));
    let (done, calls) = (Rc::new(Cell::new(false)), Rc::new(Calls::default()));
    let (disposing, seen, held) = (composition.clone(), done.clone(), calls.clone());
    composition.set_content(move || {
        RememberingLinear(held.clone());
        let (disposing, seen) = (disposing.clone(), seen.clone());
        SideEffect(move || {
            disposing.dispose();
            seen.set(true);
        });
    });
    assert!(done.get());
    assert!(composition.is_disposed());
    assert_eq!(counts(&calls), (1, 1, 0));
}

/// Every scope composes again in the next pass, though nothing it read changed.
#[test]
fn invalidate_all_composes_every_scope_again() {
    #[composable]
    fn Counted(runs: Rc<Cell<u32>>) {
        runs.set(runs.get() + 1);
    }
    #[composable]
    fn Outer(outer: Rc<Cell<u32>>, inner: Rc<Cell<u32>>) {
        outer.set(outer.get() + 1);
        Counted(inner.clone());
    }
    let (outer, inner) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let composition = Composition::new(ViewApplier::new());
    let (a, b) = (outer.clone(), inner.clone());
    composition.set_content(move || Outer(a.clone(), b.clone()));
    assert_eq!((outer.get(), inner.get()), (1, 1));
    assert!(!composition.recompose());
    composition.invalidate_all();
    assert!(composition.has_invalidations());
    assert!(composition.recompose());
    assert_eq!((outer.get(), inner.get()), (2, 2));
    assert!(!composition.recompose());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, forceReuseForgetsWhenContentDidntChange
#[test]
fn content_set_with_reuse_forgets_though_it_s_the_same() {
    let calls = Rc::new(Calls::default());
    let active = mutable_state_of(true);
    let read = active.handle();
    let composition = Composition::reusable(ViewApplier::new(), default_context());
    let content = {
        let calls = calls.clone();
        move || {
            let calls = calls.clone();
            ReusableContentHost(read.get(), move || RememberingLinear(calls.clone()));
        }
    };
    composition.set_content(content.clone());
    let views = |composition: &ReusableComposition| {
        composition.with_applier(|applier: &mut ViewApplier| top_text(applier))
    };
    assert_eq!(views(&composition), ["linear", "Some text"]);
    assert_eq!(counts(&calls), (1, 0, 0));
    // Out and back before anything looks: nothing to do.
    active.set(false);
    active.set(true);
    let changes = composition.with_applier(|applier: &mut ViewApplier| applier.changes);
    composition.recompose();
    assert_eq!(composition.with_applier(|applier: &mut ViewApplier| applier.changes), changes);
    assert_eq!(views(&composition), ["linear", "Some text"]);
    assert_eq!(counts(&calls), (1, 0, 0));
    composition.set_content_with_reuse(content);
    assert_eq!(views(&composition), ["linear", "Some text"]);
    assert_eq!(counts(&calls), (2, 1, 0));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, deactivatesForgetsWhenContentDidntChange
#[test]
fn deactivating_forgets_and_reuse_remembers_again() {
    let calls = Rc::new(Calls::default());
    let composition = Composition::reusable(ViewApplier::new(), default_context());
    let content = {
        let calls = calls.clone();
        move || RememberingLinear(calls.clone())
    };
    composition.set_content(content.clone());
    assert_eq!(counts(&calls), (1, 0, 0));
    composition.deactivate();
    assert_eq!(counts(&calls), (1, 1, 0));
    composition.set_content_with_reuse(content);
    assert_eq!(counts(&calls), (2, 1, 0));
    let views = composition.with_applier(|applier: &mut ViewApplier| top_text(applier));
    assert_eq!(views, ["linear", "Some text"]);
}

/// Deactivating keeps its nodes for what's next, and tells them; it's a pass, so it's in order.
#[test]
fn deactivated_nodes_are_kept_and_reused() {
    let (deactivated, reused) = (Counter::new(), Counter::new());
    let composition = Composition::reusable(ViewApplier::new(), default_context());
    let content = {
        let (deactivated, reused) = (deactivated.clone(), reused.clone());
        move || {
            let (deactivated, reused) = (deactivated.clone(), reused.clone());
            LinearWithCallbacks(
                Rc::new(move || {
                    reused.increment();
                }),
                Rc::new(move || {
                    deactivated.increment();
                }),
                nothing(),
                nothing(),
                || {},
            );
        }
    };
    composition.set_content(content.clone());
    let first = composition.with_applier(|applier: &mut ViewApplier| applier.roots());
    composition.deactivate();
    assert_eq!((deactivated.get(), reused.get()), (1, 0));
    assert_eq!(composition.with_applier(|applier: &mut ViewApplier| applier.roots()), first);
    composition.set_content(content);
    assert_eq!((deactivated.get(), reused.get()), (1, 1));
    assert_eq!(composition.with_applier(|applier: &mut ViewApplier| applier.roots()), first);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, setContentWithReuse_throwInInitialComposition_preservesOriginalException
#[test]
fn a_panic_setting_content_with_reuse_is_the_content_s() {
    let composition = Composition::reusable(ViewApplier::new(), default_context());
    let panic = catch_unwind(AssertUnwindSafe(|| {
        composition.set_content_with_reuse(|| std::panic::panic_any(7i32));
    }))
    .unwrap_err();
    assert_eq!(panic.downcast_ref::<i32>(), Some(&7));
    composition.dispose();
    assert!(composition.is_disposed());
}

thread_local! {
    static RECORD: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn report(entry: &str) {
    RECORD.with_borrow_mut(|record| record.push(entry.to_string()));
}

fn take_record() -> String {
    RECORD.with_borrow_mut(std::mem::take).join(", ")
}

/// Restartable, a pause point of its own, as Compose's mock `Text` is; this crate's isn't.
#[composable]
fn RestartableText(value: String) {
    Text(&value);
}

#[composable]
fn C() {
    report("+C");
    report("^x");
    let x = remember(|| 3);
    report("*Text");
    RestartableText!(format!("C: {x}"));
    report("-C");
}

#[composable]
fn D() {
    report("+D");
    Linear!(|| {
        report("+D:1");
        for _ in 0..3 {
            report("*C");
            C!();
        }
        report("-D:1");
    });
    report("-D");
}

#[composable]
fn B(content: impl Fn() + 'static) {
    report("+B");
    report("*Linear");
    Linear!(move || content());
    report("-B");
}

#[composable]
fn A() {
    report("+A");
    report("^z");
    let z = remember(|| 0);
    report("^Y");
    let y = remember(|| 1);
    RestartableText!(format!("A: {z} {y}"));
    report("*B");
    B!(|| {
        report("+A:1");
        report("*C");
        C!();
        report("*D");
        D!();
        report("-A:1");
    });
    report("-A");
}

fn check_a(composition: &Composition) {
    composition.with_applier(|applier: &mut ViewApplier| {
        let views = top_text(applier);
        assert_eq!(
            views,
            ["A: 0 1", "linear", "C: 3", "linear", "C: 3", "C: 3", "C: 3"],
            "as composed at once"
        );
    });
}

fn resume_till_complete(
    paused: &PausedComposition,
    should_pause: impl Fn() -> bool + Clone + 'static,
) -> u32 {
    let mut iterations = 0;
    while !paused.is_complete() {
        paused.resume(should_pause.clone());
        iterations += 1;
    }
    iterations
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, canRecordAComposition
#[test]
fn composed_at_once_each_composable_runs_where_it_s_called() {
    let composition = Composition::new(ViewApplier::new());
    composition.set_content(|| A!());
    assert_eq!(
        take_record(),
        "+A, ^z, ^Y, *B, +B, *Linear, +A:1, *C, +C, ^x, *Text, -C, *D, +D, +D:1, *C, +C, ^x, \
         *Text, -C, *C, +C, ^x, *Text, -C, *C, +C, ^x, *Text, -C, -D:1, -D, -A:1, -B, -A"
    );
    check_a(&composition);
}

const PAUSED_AT_EACH: &str = "+A, ^z, ^Y, *B, -A, +B, *Linear, -B, +A:1, *C, *D, -A:1, +C, ^x, \
                              *Text, -C, +D, -D, +D:1, *C, *C, *C, -D:1, +C, ^x, *Text, -C, +C, \
                              ^x, *Text, -C, +C, ^x, *Text, -C";

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, canPauseContent
#[test]
fn paused_at_each_point_a_composable_runs_after_its_caller() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| A!());
    let iterations = resume_till_complete(&paused, || true);
    paused.apply();
    assert_eq!(take_record(), PAUSED_AT_EACH);
    assert_eq!(iterations, 9);
    check_a(&composition);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, canPauseReusableContent
#[test]
fn content_set_with_reuse_for_the_first_time_pauses_as_new_content() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content_with_reuse(|| A!());
    let iterations = resume_till_complete(&paused, || true);
    paused.apply();
    assert_eq!(take_record(), PAUSED_AT_EACH);
    assert_eq!(iterations, 9);
    check_a(&composition);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, canPauseReusingContent
#[test]
fn content_reused_pauses_as_new_content() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content_with_reuse(|| A!());
    resume_till_complete(&paused, || false);
    paused.apply();
    take_record();
    let paused = composition.set_pausable_content_with_reuse(|| A!());
    resume_till_complete(&paused, || true);
    paused.apply();
    assert_eq!(take_record(), PAUSED_AT_EACH);
    check_a(&composition);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, reuseFromRoot_preventsGroupIndexCollisionWithRootKey
#[test]
fn reusing_from_the_root_finds_a_group_of_any_index() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let content = || {
        for index in 0..94 {
            key(index, || {});
        }
        key(94, || ReusableContent(94, || {}));
    };
    for _ in 0..2 {
        let paused = composition.set_pausable_content_with_reuse(content);
        resume_till_complete(&paused, || false);
        paused.apply();
    }
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, applierOnlyCalledInApply
#[test]
fn nothing_is_attached_before_apply() {
    // The pass makes nodes and updates them, in no parent (D591), as they return values: what
    // Compose's recording applier keeps from the tree is what's attached.
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let shown =
        |composition: &Composition| composition.with_applier(|a: &mut ViewApplier| a.roots().len());
    let paused = composition.set_pausable_content(|| A!());
    assert_eq!(shown(&composition), 0, "setting the content");
    resume_till_complete(&paused, || false);
    assert_eq!(shown(&composition), 0, "resuming");
    paused.apply();
    assert_eq!(shown(&composition), 2, "applied");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, rememberOnlyCalledInApply
#[test]
fn observers_are_remembered_only_in_apply_in_order() {
    struct Named(&'static str);
    impl RememberObserver for Named {
        fn on_remembered(&self) {
            report(&format!("+{}", self.0));
        }
        fn on_forgotten(&self) {
            report(&format!("-{}", self.0));
        }
        fn on_abandoned(&self) {
            report(&format!("!{}", self.0));
        }
    }
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| {
        remember_observer(|| Named("a"));
        report("C(a)");
        B!(|| {
            remember_observer(|| Named("b"));
            report("C(b)");
            B!(|| {
                remember_observer(|| Named("c"));
                report("C(c)");
                C!();
                remember_observer(|| Named("d"));
                report("C(d)");
                D!();
            });
        });
    });
    let checked = Rc::new(Cell::new(0));
    let counted = checked.clone();
    resume_till_complete(&paused, move || {
        counted.set(counted.get() + 1);
        assert!(
            !RECORD.with_borrow(|record| record.iter().any(|entry| entry == "+a")),
            "remembered during a resume"
        );
        true
    });
    assert!(checked.get() > 0);
    assert!(!RECORD.with_borrow(|record| record.iter().any(|entry| entry == "+a")));
    paused.apply();
    assert_eq!(
        take_record(),
        "C(a), +B, *Linear, -B, C(b), +B, *Linear, -B, C(c), C(d), +C, ^x, *Text, -C, +D, -D, \
         +D:1, *C, *C, *C, -D:1, +C, ^x, *Text, -C, +C, ^x, *Text, -C, +C, ^x, *Text, -C, +a, +b, \
         +c, +d"
    );
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausable_testRemember_RememberForgetOrder
#[test]
fn observers_are_remembered_in_the_order_composed_at_once() {
    struct Named(&'static str);
    impl RememberObserver for Named {
        fn on_remembered(&self) {
            report(self.0);
        }
    }
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| {
        remember_observer(|| Named("L0B"));
        Linear!(|| {
            remember_observer(|| Named("L1B"));
            Linear!(|| {
                remember_observer(|| Named("L2B"));
                Linear!(|| {
                    remember_observer(|| Named("L3B"));
                    Linear!(|| {
                        remember_observer(|| Named("Leaf"));
                    });
                    remember_observer(|| Named("L3A"));
                });
                remember_observer(|| Named("L2A"));
            });
            remember_observer(|| Named("L1A"));
        });
        remember_observer(|| Named("L0A"));
    });
    resume_till_complete(&paused, || true);
    paused.apply();
    assert_eq!(take_record(), "L0B, L1B, L2B, L3B, Leaf, L3A, L2A, L1A, L0A");
}

fn deactivating_in_a_paused_composition(times: usize) {
    let (active, text) = (mutable_state_of(true), mutable_state_of("Value".to_string()));
    let (is_active, shown) = (active.handle(), text.handle());
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(move || {
        ReusableContentHost(is_active.get(), || Text!(&shown.get()));
    });
    resume_till_complete(&paused, || true);
    for _ in 0..times {
        active.set(false);
        composition.recompose();
        paused.resume(|| true);
        resume_till_complete(&paused, || true);
        active.set(true);
        composition.recompose();
        paused.resume(|| true);
        resume_till_complete(&paused, || true);
    }
    paused.apply();
    text.set("Changed Value".to_string());
    assert!(composition.recompose());
    let views = composition.with_applier(|applier: &mut ViewApplier| top_text(applier));
    assert_eq!(views, ["Changed Value"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_reuseDeactivateOrder_100
#[test]
fn content_deactivated_and_active_a_hundred_times_while_paused_applies() {
    deactivating_in_a_paused_composition(100);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_reuseDeactivateOrder
#[test]
fn content_deactivated_and_active_while_paused_applies() {
    deactivating_in_a_paused_composition(1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_throwInResume
#[test]
#[should_panic(expected = "Test error")]
fn a_panic_in_paused_content_comes_out_of_resume() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| panic!("Test error"));
    paused.resume(|| false);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_throwInResumeWithReuse_preservesOriginalException
#[test]
fn a_panic_in_paused_reused_content_is_the_content_s() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content_with_reuse(|| std::panic::panic_any(7i32));
    let panic = catch_unwind(AssertUnwindSafe(|| paused.resume(|| false))).unwrap_err();
    assert_eq!(panic.downcast_ref::<i32>(), Some(&7));
    composition.dispose();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_throwInApply
#[test]
#[should_panic(expected = "test")]
fn a_panic_in_an_effect_comes_out_of_apply() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| {
        DisposableEffect((), |_| panic!("test"));
    });
    paused.resume(|| false);
    paused.apply();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_throwIfReusedAfterCancel
#[test]
#[should_panic(expected = "paused composition")]
fn content_can_t_be_set_after_a_cancel() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| Text!("Some text"));
    paused.cancel();
    let again = composition.set_pausable_content(|| Text!("Some other text"));
    again.resume(|| false);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_isAppliedReturnsCorrectValue
#[test]
fn a_paused_composition_says_when_it_s_applied() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| {
        DisposableEffect((), |scope| scope.on_dispose(|| {}));
    });
    assert!(!paused.is_applied());
    paused.resume(|| false);
    assert!(!paused.is_applied());
    paused.apply();
    assert!(paused.is_applied());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_isCancelledReturnsCorrectValue
#[test]
fn a_paused_composition_says_when_it_s_cancelled() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| {
        DisposableEffect((), |scope| scope.on_dispose(|| {}));
    });
    assert!(!paused.is_cancelled());
    paused.resume(|| false);
    assert!(!paused.is_cancelled());
    paused.cancel();
    assert!(paused.is_cancelled());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, rememberObserverCount
#[test]
fn a_cancel_abandons_what_a_paused_composition_remembered() {
    let calls = Rc::new(Calls::default());
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let held = calls.clone();
    let paused = composition.set_pausable_content(move || {
        let held = held.clone();
        remember_observer(move || Observed(held));
    });
    resume_till_complete(&paused, || false);
    paused.cancel();
    composition.dispose();
    assert_eq!(counts(&calls), (0, 0, 1));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, deactivateAnotherComposition
#[test]
fn another_composition_s_effect_can_deactivate_it() {
    let state = mutable_state_of(false);
    let read = state.handle();
    let held: Rc<PausableComposition> =
        Rc::new(Composition::pausable(ViewApplier::new(), default_context()));
    let outer = Composition::new(ViewApplier::new());
    let inner = held.clone();
    outer.set_content(move || {
        Text!(&read.get().to_string());
        let inner = inner.clone();
        SideEffect(move || {
            if read.get() {
                inner.deactivate();
            }
        });
    });
    let content = move || Text!(&read.get().to_string());
    let paused = held.set_pausable_content(content);
    resume_till_complete(&paused, || false);
    paused.apply();
    state.set(true);
    assert!(outer.recompose());
    let paused = held.set_pausable_content(content);
    resume_till_complete(&paused, || false);
    paused.apply();
    let views = held.with_applier(|applier: &mut ViewApplier| top_text(applier));
    assert_eq!(views, ["true"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, tryPausingTheSameScopeTwice
#[test]
fn a_scope_paused_twice_composes_once_more() {
    let composed = Rc::new(Cell::new(false));
    let text = mutable_state_of("blah".to_string());
    let read = text.handle();
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let content = {
        let composed = composed.clone();
        move || {
            composed.set(true);
            Text!(&read.get());
        }
    };
    let paused = composition.set_pausable_content(content.clone());
    resume_till_complete(&paused, || false);
    paused.apply();
    composition.deactivate();
    let paused = composition.set_pausable_content(content);
    let seen = composed.clone();
    paused.resume(move || seen.get());
    text.set("text".to_string());
    paused.resume(|| true);
    resume_till_complete(&paused, || false);
    paused.apply();
    let views = composition.with_applier(|applier: &mut ViewApplier| top_text(applier));
    assert_eq!(views, ["text"]);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, rememberObserverThrashing
#[test]
fn what_a_paused_composition_dropped_is_abandoned_and_the_last_remembered() {
    struct Named(String);
    impl RememberObserver for Named {
        fn on_remembered(&self) {
            report(&format!("Remember({})", self.0));
        }
        fn on_forgotten(&self) {
            report(&format!("Forget({})", self.0));
        }
        fn on_abandoned(&self) {
            report(&format!("Abandon({})", self.0));
        }
    }
    let (enable, name) = (mutable_state_of(true), mutable_state_of("A"));
    let (is_enabled, key) = (enable.handle(), name.handle());
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(move || {
        if is_enabled.get() {
            let name = key.get();
            remember_observer_keyed(name, move || Named(name.to_string()));
        }
    });
    let settle = |paused: &PausedComposition| {
        compose_runtime::snapshots::Snapshot::send_apply_notifications();
        resume_till_complete(paused, || false);
    };
    settle(&paused);
    enable.set(false);
    settle(&paused);
    enable.set(true);
    settle(&paused);
    name.set("B");
    settle(&paused);
    enable.set(false);
    settle(&paused);
    enable.set(true);
    settle(&paused);
    paused.apply();
    let record = take_record();
    let events: Vec<&str> = record.split(", ").collect();
    assert_eq!(events.len(), 4, "{record}");
    assert_eq!(events[0], "Remember(B)", "{record}");
    assert_eq!(events.iter().filter(|event| **event == "Abandon(A)").count(), 2, "{record}");
    assert_eq!(events.iter().filter(|event| **event == "Abandon(B)").count(), 1, "{record}");
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, pausableComposition_reusableContent
#[test]
fn reusable_content_in_paused_reused_content_keeps_order() {
    let key = mutable_state_of(0);
    let read = key.handle();
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let content = move || {
        Linear!(move || {
            ReusableContent(read.get(), || {});
            Text!(&format!("After {}", read.get()));
        });
    };
    let paused = composition.set_pausable_content(content);
    resume_till_complete(&paused, || false);
    paused.apply();
    composition.deactivate();
    key.set(1);
    let paused = composition.set_pausable_content_with_reuse(content);
    paused.resume(|| true);
    resume_till_complete(&paused, || false);
    paused.apply();
    let views = composition.with_applier(|applier: &mut ViewApplier| top_text(applier));
    assert_eq!(views, ["linear", "After 1"]);
}

/// Deactivating needs what's paused to be done with, as Compose's says.
#[test]
#[should_panic(expected = "paused composition")]
fn deactivating_while_paused_panics() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let _paused = composition.set_pausable_content(|| Text("Some text"));
    composition.deactivate();
}

/// `set_content_with_reuse` is no key: a plain, a reusable and a pausable composition's content
/// hash alike, as `startReuseFromRoot`.
#[test]
fn a_reusable_compositions_root_is_in_no_hash() {
    let hash = Rc::new(Cell::new(0));
    let seen = hash.clone();
    let content = move || seen.set(current_composite_key_hash_code());

    let plain = Composition::new(ViewApplier::new());
    plain.set_content(content.clone());
    let expected = hash.get();
    assert_ne!(expected, 0);

    let reusable = Composition::reusable(ViewApplier::new(), default_context());
    reusable.set_content(content.clone());
    assert_eq!(hash.get(), expected);
    reusable.set_content_with_reuse(content.clone());
    assert_eq!(hash.get(), expected, "reused");
    reusable.set_content_with_reuse(content.clone());
    assert_eq!(hash.get(), expected, "reused again");

    let pausable = Composition::pausable(ViewApplier::new(), default_context());
    let paused = pausable.set_pausable_content(content);
    resume_till_complete(&paused, || false);
    paused.apply();
    assert_eq!(hash.get(), expected);
}

/// A composition another's effect disposes in the frame doesn't run what it left to do: what it
/// remembered is abandoned, its side effects dropped, as `dispose` clears its changes.
#[test]
fn a_composition_disposed_in_a_frame_runs_none_of_its_work() {
    let owner = Recomposer::new();
    let recomposer = owner.handle();
    let context = recomposer.as_context();
    let first = Composition::with_parent(ViewApplier::new(), context);
    let second = Rc::new(Composition::with_parent(ViewApplier::new(), context));
    let state = mutable_state_of(0);
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    let (read, killed, log_first) = (state.handle(), second.clone(), log.clone());
    first.set_content(move || {
        let value = read.get();
        let (killed, log_first) = (killed.clone(), log_first.clone());
        SideEffect(move || {
            if value == 1 {
                log_first.borrow_mut().push("first".into());
                killed.dispose();
            }
        });
    });
    let (read, log_second) = (state.handle(), log.clone());
    second.set_content(move || {
        let value = read.get();
        let log_second = log_second.clone();
        SideEffect(move || {
            if value == 1 {
                log_second.borrow_mut().push("second".into());
            }
        });
    });
    state.set(1);
    assert!(platform::perform_frame(recomposer, 0));
    assert_eq!(*log.borrow(), ["first"]);
    assert!(second.is_disposed());
}

/// Setting a reusable composition's content while a paused one is in progress is refused, as
/// `clearDeactivated`.
#[test]
#[should_panic(expected = "paused composition")]
fn content_can_t_be_set_while_paused() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let _paused = composition.set_pausable_content(|| Text("Some text"));
    composition.set_content(|| Text("Some text"));
}

#[test]
#[should_panic(expected = "paused composition")]
fn content_can_t_be_set_with_reuse_while_paused() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let _paused = composition.set_pausable_content(|| Text("Some text"));
    composition.set_content_with_reuse(|| Text("Some text"));
}

/// Disposed, what's paused is cancelled: its observers abandoned.
#[test]
fn disposing_cancels_what_s_paused() {
    let calls = Rc::new(Calls::default());
    let held = calls.clone();
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(move || RememberingLinear(held.clone()));
    resume_till_complete(&paused, || false);
    assert_eq!(counts(&calls), (0, 0, 0));
    composition.dispose();
    assert!(paused.is_cancelled());
    assert_eq!(counts(&calls), (0, 0, 1));
}

#[test]
#[should_panic(expected = "dropped during its own pass")]
fn disposing_during_its_pass_panics() {
    let composition = Rc::new(Composition::new(ViewApplier::new()));
    let held = Rc::downgrade(&composition);
    composition.set_content(move || held.upgrade().expect("alive").dispose());
}

/// What a paused composition deactivates is deactivated when it's applied, in its order.
#[test]
fn a_paused_deactivation_happens_at_apply() {
    let deactivated = Counter::new();
    let active = mutable_state_of(true);
    let read = active.handle();
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let content = {
        let deactivated = deactivated.clone();
        move || {
            let deactivated = deactivated.clone();
            ReusableContentHost(read.get(), move || {
                let deactivated = deactivated.clone();
                let on_deactivate: Action = Rc::new(move || {
                    deactivated.increment();
                });
                LinearWithCallbacks!(nothing(), on_deactivate, nothing(), nothing(), || {});
            });
        }
    };
    let paused = composition.set_pausable_content(content.clone());
    resume_till_complete(&paused, || false);
    paused.apply();
    assert_eq!(deactivated.get(), 0);
    active.set(false);
    let paused = composition.set_pausable_content_with_reuse(content);
    resume_till_complete(&paused, || false);
    assert_eq!(deactivated.get(), 0, "not before it's applied");
    paused.apply();
    assert_eq!(deactivated.get(), 1);
}

/// Complete, it's to be applied: resuming it is out of order, as Compose's.
#[test]
#[should_panic(expected = "paused composition")]
fn resuming_what_s_complete_panics() {
    let composition = Composition::pausable(ViewApplier::new(), default_context());
    let paused = composition.set_pausable_content(|| Text("Some text"));
    resume_till_complete(&paused, || false);
    paused.resume(|| false);
}
