//! Composition contexts and the compositions under them, as the subcomposition tests of Compose's
//! `CompositionTests`, `CompositionLocalTests`, `CompoundHashKeyTests`, `RecomposerTests`,
//! `CompositionReusingTests` and `PausableCompositionTests`, and what the design adds: passes
//! nested in passes, sharing an applier and lending it (composition-context.md §3.2, §3.3).

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use compose_runtime::platform::*;
use compose_runtime::snapshots::Snapshot;
use compose_runtime::*;

use crate::mock::*;

fn empty_applier() -> NodeTree<()> {
    NodeTree::new()
}

/// A composition under this place of its parent, whose content follows `content`, as the file's
/// `TestSubcomposition`: made in an effect, disposed with the place.
#[composable]
fn TestSubcomposition(content: impl Fn() + 'static) {
    let parent = remember_composition_context();
    let current = remember_updated_state(content);
    DisposableEffect(parent_key(parent), move |scope| {
        let subcomposition = Composition::with_parent(empty_applier(), parent);
        // Read in its content, so it follows a new one, as the note in `TestSubcomposition` says.
        subcomposition.set_content(move || (current.get())());
        scope.on_dispose(move || subcomposition.dispose())
    });
}

/// A context as an effect's key: the one place stays the one key.
fn parent_key(_context: CompositionContext) -> i32 {
    0
}

/// `Composition(ViewApplier(container), ref.value).apply { setContent { block() } }`, for later,
/// as the file's `testDeferredSubcomposition`: the composable's reader of what it's composed
/// under, with the place it found.
#[composable(non_restartable)]
fn DeferredSubcomposition(to_compose: Rc<RefCell<Option<CompositionContext>>>) {
    *to_compose.borrow_mut() = Some(remember_composition_context());
}

// ---- CompositionTests ------------------------------------------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testModificationsPropagateToSubcomposition
#[test]
fn what_a_parent_writes_while_composing_reaches_its_child_in_the_frame() {
    #[composable]
    fn Content(
        value: MutableState<i32>,
        content: MutableState<Callback>,
        occurred: Rc<Cell<bool>>,
    ) {
        let parent_value = value.get();
        let (seen, occurred) = (value, occurred.clone());
        content.set(Callback::new(move || {
            occurred.set(true);
            assert_eq!(parent_value, seen.get());
        }));
        let reader = content;
        TestSubcomposition!(move || (reader.get())());
    }
    let mut test = composition_test();
    let (value, content) = (
        mutable_state_of(0),
        mutable_state_of_with(Callback::new(|| {}), ReferentialEqualityPolicy),
    );
    let occurred = Rc::new(Cell::new(false));
    let (v, c, o) = (value.handle(), content.handle(), occurred.clone());
    test.compose(move || Content!(v, c, o.clone()));
    occurred.set(false);
    value.set(10);
    test.advance();
    assert!(occurred.get());
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testComposableLambdaSubcompositionInvalidation
#[test]
fn a_lambda_with_a_new_capture_recomposes_the_child_in_the_frame() {
    let recomposer = Recomposer::new();
    let composition = Composition::with_parent(empty_applier(), recomposer.as_context());
    let root_state = mutable_state_of(false);
    let results: Rc<RefCell<Vec<bool>>> = Rc::default();
    let (state, seen) = (root_state.handle(), results.clone());
    Snapshot::notify_objects_initialized();
    composition.set_content(move || {
        let captured = state.get();
        let seen = seen.clone();
        TestSubcomposition!(move || seen.borrow_mut().push(captured));
    });
    assert_eq!(*results.borrow(), [false]);
    root_state.set(true);
    Snapshot::send_apply_notifications();
    assert!(perform_frame(recomposer.handle(), 0));
    assert_eq!(*results.borrow(), [false, true], "the same frame (b/320385076)");
    composition.dispose();
    recomposer.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testCompositionContextIsRemembered
#[test]
fn a_place_gives_the_one_context() {
    let composition = Composition::new(empty_applier());
    let scope = ScopeHolder::new();
    let contexts: Rc<RefCell<Vec<CompositionContext>>> = Rc::default();
    let (holder, seen) = (scope.clone(), contexts.clone());
    composition.set_content(move || {
        holder.capture();
        seen.borrow_mut().push(remember_composition_context());
    });
    scope.invalidate();
    composition.recompose();
    let contexts = contexts.borrow();
    assert!(contexts.len() > 1, "composed more than once");
    assert!(contexts.iter().all(|context| *context == contexts[0]));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testParentCompositionRecomposesFirst
#[test]
fn a_parent_recomposes_before_its_child() {
    let recomposer = Recomposer::new();
    let composition = Composition::with_parent(empty_applier(), recomposer.as_context());
    let log = Log::new();
    let (first, second) = (mutable_state_of("firstInitial"), mutable_state_of("secondInitial"));
    let (f, s, l) = (first.handle(), second.handle(), log.clone());
    Snapshot::notify_objects_initialized();
    composition.set_content(move || {
        l.push(f.get());
        let l = l.clone();
        TestSubcomposition!(move || l.push(s.get()));
    });
    second.set("secondSet");
    Snapshot::send_apply_notifications();
    first.set("firstSet");
    Snapshot::send_apply_notifications();
    assert!(perform_frame(recomposer.handle(), 0));
    assert_eq!(log.take(), ["firstInitial", "secondInitial", "firstSet", "secondSet"]);
    composition.dispose();
    recomposer.cancel();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testSubcomposeSingleComposition
#[test]
fn a_parent_and_child_change_in_one_frame() {
    let mut test = composition_test();
    let (flag, flag_copy) = (mutable_state_of(true), mutable_state_of(true));
    let (copy_value, remembered) = (Rc::new(Cell::new(true)), Rc::new(Cell::new(true)));
    let (f, c, cv, r) = (flag.handle(), flag_copy.handle(), copy_value.clone(), remembered.clone());
    test.compose(move || {
        Text(&format!("Parent {}", f.get()));
        c.set(f.get());
        let (cv, r) = (cv.clone(), r.clone());
        TestSubcomposition!(move || {
            cv.set(c.get());
            let copied = cv.get();
            r.set(remember_keyed(c.get(), move || copied));
        });
    });
    flag.set(false);
    let count = test.advance_count();
    assert!(!copy_value.get());
    assert!(!remembered.get());
    assert_eq!(count, 1);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, test_crossinline_differentComposition
#[test]
fn a_branch_of_the_parent_beside_an_empty_child() {
    let mut test = composition_test();
    let branch = mutable_state_of(false);
    let b = branch.handle();
    test.compose(move || {
        if b.get() {
            Text("Content");
        }
        TestSubcomposition!(|| {
            if key_of("never") == 0 {
                remember(|| "Something");
            }
        });
    });
    let b = branch.handle();
    test.validate(move |v| {
        if b.get() {
            v.text("Content");
        }
    });
    branch.set(true);
    test.expect_changes();
    test.revalidate();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionTests.kt, testCompositionAndRecomposerDeadlock
// (no threads, D14: what it runs, in one)
#[test]
fn an_inner_composition_disposed_with_its_parent_under_a_recomposer() {
    for _ in 0..3 {
        let recomposer = Recomposer::new();
        let composition = Composition::with_parent(empty_applier(), recomposer.as_context());
        composition.set_content(|| {
            let parent = remember_composition_context();
            let inner = remember(|| Rc::new(Composition::with_parent(empty_applier(), parent)));
            DisposableEffect((), move |scope| scope.on_dispose(move || inner.dispose()));
        });
        composition.dispose();
        recomposer.close();
    }
}

// ---- CompositionLocalTests -------------------------------------------------------------------

static LOCAL_TEXT: CompositionLocal<String> =
    CompositionLocal::new("subcompose.LocalText", || "Default".into());
static LOCAL_INT: CompositionLocal<i32> = CompositionLocal::new("subcompose.LocalInt", || 1);
static STATIC_TEXT: CompositionLocal<String> =
    CompositionLocal::new_static("subcompose.StaticText", || "Default".into());
static STATIC_INT: CompositionLocal<i32> =
    CompositionLocal::new_static("subcompose.StaticInt", || -1);

/// `subCompose_Dynamic` and `subCompose_Static`.
fn locals_in_a_child(
    text_local: &'static CompositionLocal<String>,
    int_local: &'static CompositionLocal<i32>,
) {
    let mut test = composition_test();
    let text = Rc::new(RefCell::new(String::from("Unmodified")));
    let scope = ScopeHolder::new();
    let (t, s) = (text.clone(), scope.clone());
    test.compose(move || {
        s.capture();
        let provided = t.borrow().clone();
        let t = t.clone();
        CompositionLocalProvider(
            (text_local.provides(provided), int_local.provides(0)),
            move || {
                Text(&text_local.current());
                let expected = t.borrow().clone();
                TestSubcomposition!(move || {
                    assert_eq!(expected, text_local.current());
                    assert_eq!(0, int_local.current());
                    let expected = expected.clone();
                    CompositionLocalProvider(int_local.provides(42), move || {
                        assert_eq!(expected, text_local.current());
                        assert_eq!(42, int_local.current());
                    });
                });
            },
        );
    });
    test.validate(|v| v.text("Unmodified"));
    *text.borrow_mut() = "Modified".into();
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.text("Modified"));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, subCompose_Dynamic
#[test]
fn a_child_reads_its_parents_dynamic_locals() {
    locals_in_a_child(&LOCAL_TEXT, &LOCAL_INT);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, subCompose_Static
#[test]
fn a_child_reads_its_parents_static_locals() {
    locals_in_a_child(&STATIC_TEXT, &STATIC_INT);
}

/// `deferredSubCompose_Dynamic` and `deferredSubCompose_Static`.
fn locals_in_a_child_made_later(
    text_local: &'static CompositionLocal<String>,
    int_local: &'static CompositionLocal<i32>,
) {
    let mut test = composition_test();
    let text = Rc::new(RefCell::new(String::from("Unmodified")));
    let scope = ScopeHolder::new();
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let (t, s, p) = (text.clone(), scope.clone(), place.clone());
    test.compose(move || {
        s.capture();
        let p = p.clone();
        CompositionLocalProvider(
            (text_local.provides(t.borrow().clone()), int_local.provides(0)),
            move || {
                Text(&text_local.current());
                DeferredSubcomposition(p.clone());
            },
        );
    });
    let compose_later = |text: Rc<RefCell<String>>,
                         place: &Rc<RefCell<Option<CompositionContext>>>| {
        let context = place.borrow().expect("composed");
        let composition = Composition::with_parent(empty_applier(), context);
        composition.set_content(move || {
            assert_eq!(*text.borrow(), text_local.current());
            assert_eq!(0, int_local.current());
            let text = text.clone();
            CompositionLocalProvider(int_local.provides(42), move || {
                assert_eq!(*text.borrow(), text_local.current());
                assert_eq!(42, int_local.current());
            });
        });
        composition
    };
    test.validate(|v| v.text("Unmodified"));
    let first = compose_later(text.clone(), &place);
    *text.borrow_mut() = "Modified".into();
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.text("Modified"));
    let second = compose_later(text.clone(), &place);
    drop((first, second));
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, deferredSubCompose_Dynamic
#[test]
fn a_child_made_later_reads_its_parents_dynamic_locals() {
    locals_in_a_child_made_later(&LOCAL_TEXT, &LOCAL_INT);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, deferredSubCompose_Static
#[test]
fn a_child_made_later_reads_its_parents_static_locals() {
    locals_in_a_child_made_later(&STATIC_TEXT, &STATIC_INT);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionLocalTests.kt, deferredSubCompose_Nested_Static
#[test]
fn a_grandchild_made_later_reads_the_static_locals_two_up() {
    let mut test = composition_test();
    let text = Rc::new(RefCell::new(String::from("Unmodified")));
    let scope = ScopeHolder::new();
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let inner_place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let (t, s, p) = (text.clone(), scope.clone(), place.clone());
    test.compose(move || {
        s.capture();
        let p = p.clone();
        CompositionLocalProvider(
            (STATIC_TEXT.provides(t.borrow().clone()), STATIC_INT.provides(0)),
            move || {
                Text(&STATIC_TEXT.current());
                DeferredSubcomposition(p.clone());
            },
        );
    });
    let compose_both = || {
        let middle = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));
        let (t, i) = (text.clone(), inner_place.clone());
        middle.set_content(move || {
            assert_eq!(*t.borrow(), STATIC_TEXT.current());
            assert_eq!(0, STATIC_INT.current());
            DeferredSubcomposition(i.clone());
        });
        let inner =
            Composition::with_parent(empty_applier(), inner_place.borrow().expect("composed"));
        let t = text.clone();
        inner.set_content(move || {
            assert_eq!(*t.borrow(), STATIC_TEXT.current());
            assert_eq!(0, STATIC_INT.current());
        });
        (middle, inner)
    };
    test.validate(|v| v.text("Unmodified"));
    let first = compose_both();
    *text.borrow_mut() = "Modified".into();
    scope.invalidate();
    test.expect_changes();
    test.validate(|v| v.text("Modified"));
    let second = compose_both();
    drop((first, second));
}

/// A dynamic local's new value, written by the parent's pass, reaches what read it in a child in
/// that frame, with nothing else of the child composing. (§3.2)
#[test]
fn a_new_dynamic_value_recomposes_only_the_childs_readers() {
    #[composable]
    fn Reader(reads: Rc<Cell<i32>>) {
        reads.set(LOCAL_INT.current());
    }
    #[composable]
    fn Content(value: MutableState<i32>, reads: Rc<Cell<i32>>, others: Rc<Cell<i32>>) {
        CompositionLocalProvider(LOCAL_INT.provides(value.get()), || {
            TestSubcomposition!(|| {
                others.set(others.get() + 1);
                Reader!(reads.clone());
            });
        });
    }
    let mut test = composition_test();
    let value = mutable_state_of(0);
    let (reads, others) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (v, r, o) = (value.handle(), reads.clone(), others.clone());
    test.compose(move || Content!(v, r.clone(), o.clone()));
    assert_eq!(others.get(), 1);
    value.set(7);
    test.advance();
    assert_eq!(reads.get(), 7);
    assert_eq!(others.get(), 1, "the child's root didn't read it");
}

/// A child of a context whose place went reads the locals of its own: nothing above. (§3.2)
#[test]
fn a_child_whose_place_went_reads_defaults() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let (t, p) = (there.handle(), place.clone());
    test.compose(move || {
        if t.get() {
            let p = p.clone();
            CompositionLocalProvider(LOCAL_INT.provides(5), move || {
                DeferredSubcomposition(p.clone());
            });
        }
    });
    let seen = Rc::new(Cell::new(0));
    let s = seen.clone();
    let child = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));
    child.set_content(move || s.set(LOCAL_INT.current()));
    assert_eq!(seen.get(), 5);
    there.set(false);
    test.advance();
    let s = seen.clone();
    child.set_content(move || s.set(LOCAL_INT.current()));
    assert_eq!(seen.get(), 1, "the default");
}

/// What a map, a context of locals and a provider's `provides_default` see goes across. (§3.2)
#[test]
fn a_childs_maps_and_defaults_see_across_its_root() {
    let mut test = composition_test();
    let (map_value, default_value, anchor_value) =
        (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (m, d, a) = (map_value.clone(), default_value.clone(), anchor_value.clone());
    test.compose(move || {
        let (m, d, a) = (m.clone(), d.clone(), a.clone());
        CompositionLocalProvider(LOCAL_INT.provides(5), move || {
            let (m, d, a) = (m.clone(), d.clone(), a.clone());
            TestSubcomposition!(move || {
                m.set(current_composition_local_map().get(&LOCAL_INT));
                a.set(current_composition_local_anchor().get(&LOCAL_INT));
                let d = d.clone();
                CompositionLocalProvider(LOCAL_INT.provides_default(9), move || {
                    d.set(LOCAL_INT.current());
                });
            });
        });
    });
    assert_eq!(map_value.get(), 5);
    assert_eq!(default_value.get(), 5, "provided above, across the root");
    assert_eq!(anchor_value.get(), 5);
}

// ---- CompoundHashKeyTests --------------------------------------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, testWithSubCompose
#[test]
fn a_childs_hash_goes_on_from_its_place() {
    #[composable]
    fn RecordHashKeys(
        outer: Rc<RefCell<Vec<u64>>>,
        inner: Rc<RefCell<Vec<u64>>>,
        scopes: ScopeHolders,
    ) {
        scopes.borrow_mut().push(current_recompose_scope());
        outer.borrow_mut().push(current_composite_key_hash_code());
        TestSubcomposition!(move || {
            scopes.borrow_mut().push(current_recompose_scope());
            inner.borrow_mut().push(current_composite_key_hash_code());
        });
    }
    type ScopeHolders = Rc<RefCell<Vec<RecomposeScope>>>;
    let mut test = composition_test();
    let outer: Rc<RefCell<Vec<u64>>> = Rc::default();
    let inner: Rc<RefCell<Vec<u64>>> = Rc::default();
    let scopes: ScopeHolders = Default::default();
    let (o, i, s) = (outer.clone(), inner.clone(), scopes.clone());
    test.compose(move || {
        for index in 0..2 {
            let (o, i, s) = (o.clone(), i.clone(), s.clone());
            key(index, move || RecordHashKeys!(o.clone(), i.clone(), s.clone()));
        }
    });
    let first_outer = outer.take();
    let first_inner = inner.take();
    assert_eq!((first_outer.len(), first_inner.len()), (2, 2));
    assert_ne!(first_outer[0], first_outer[1]);
    assert_ne!(first_inner[0], first_inner[1]);
    for scope in scopes.take() {
        scope.invalidate();
    }
    test.expect_no_changes();
    assert_eq!(outer.take(), first_outer);
    assert_eq!(inner.take(), first_inner);
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompoundHashKeyTests.kt, testReusableContentWithSubcompose
#[test]
fn a_reused_childs_hash_follows_its_reused_place() {
    let mut test = composition_test();
    let outer_key = mutable_state_of(0);
    let (outer_hash, inner_hash) = (Rc::new(Cell::new(0u64)), Rc::new(Cell::new(0u64)));
    let inner_scope: Rc<RefCell<Option<RecomposeScope>>> = Rc::default();
    let subcomposition: Rc<RefCell<Option<ReusableComposition>>> = Rc::default();
    let (k, oh, ih, is, sub) = (
        outer_key.handle(),
        outer_hash.clone(),
        inner_hash.clone(),
        inner_scope.clone(),
        subcomposition.clone(),
    );
    test.compose(move || {
        let (oh, ih, is, sub) = (oh.clone(), ih.clone(), is.clone(), sub.clone());
        ReusableContent(k.get(), move || {
            oh.set(current_composite_key_hash_code());
            let parent = remember_composition_context();
            let mut held = sub.borrow_mut();
            let held = held.get_or_insert_with(|| Composition::reusable(empty_applier(), parent));
            let (ih, is) = (ih.clone(), is.clone());
            held.set_content_with_reuse(move || {
                let (ih, is) = (ih.clone(), is.clone());
                ReusableContent(0, move || {
                    *is.borrow_mut() = Some(current_recompose_scope());
                    ih.set(current_composite_key_hash_code());
                });
            });
        });
    });
    let (initial_outer, initial_inner) = (outer_hash.get(), inner_hash.get());
    assert_ne!(initial_outer, 0);
    assert_ne!(initial_inner, 0);

    outer_key.set(1);
    test.advance();
    let (reused_outer, reused_inner) = (outer_hash.get(), inner_hash.get());
    assert_ne!(initial_outer, reused_outer);
    assert_ne!(initial_inner, reused_inner);

    inner_hash.set(0);
    inner_scope.borrow().as_ref().expect("composed").invalidate();
    test.advance();
    assert_eq!(reused_inner, inner_hash.get());

    outer_key.set(0);
    test.advance();
    assert_eq!(initial_outer, outer_hash.get());
    assert_eq!(initial_inner, inner_hash.get());

    subcomposition.borrow().as_ref().expect("composed").dispose();
}

// ---- RecomposerTests -------------------------------------------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testSubcompositionDisposedInParent
#[test]
fn a_child_whose_place_goes_in_the_frame_isnt_recomposed() {
    let mut test = composition_test();
    let state = mutable_state_of(true);
    let s = state.handle();
    test.compose(move || {
        if s.get() {
            TestSubcomposition!(move || {
                assert!(s.get(), "Subcomposition should be disposed if state is false");
            });
        }
    });
    state.set(false);
    test.advance();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testInsertDuringRecomposition
#[test]
fn inserting_content_that_writes_state_in_a_child() {
    #[composable]
    fn B(value: i32) {
        let _ = value;
    }
    #[composable]
    fn A(some_state: MutableState<i32>) {
        B!(some_state.get());
        some_state.update(|state| state + 1);
    }
    let mut test = composition_test();
    let (include_a, some_state, some_other_state) =
        (mutable_state_of(false), mutable_state_of(0), mutable_state_of(1));
    let (i, s, o) = (include_a.handle(), some_state.handle(), some_other_state.handle());
    test.compose(move || {
        TestSubcomposition!(move || {
            // Takes up some slot space (bug 157111271).
            remember_keyed((1, 2, 3, 4, 5, 6, 7, 8), || 1);
            if i.get() {
                Wrap!({
                    B!(0);
                    B!(o.get());
                    B!(2);
                    B!(3);
                    B!(4);
                    A!(s);
                });
            }
        });
    });
    include_a.set(true);
    test.advance_ignoring_pending_work();
    some_other_state.set(10);
    test.advance_ignoring_pending_work();
    test.advance_ignoring_pending_work();
}

// ---- CompositionReusingTests and PausableCompositionTests ------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/CompositionReusingTests.kt, compositionParentContextIsUpdatedAfterReuse
#[test]
fn a_child_reads_what_its_reused_place_provides_now() {
    static LOCAL: CompositionLocal<i32> = CompositionLocal::new("reuse.local", || 0);
    let mut test = composition_test();
    let (reuse_key, active) = (mutable_state_of(0), mutable_state_of(true));
    let child: Rc<RefCell<Option<Composition>>> = Rc::default();
    let (k, a, s) = (reuse_key.handle(), active.handle(), child.clone());
    test.compose(move || {
        let s = s.clone();
        ReusableContentHost(a.get(), move || {
            let s = s.clone();
            let reuse = k.get();
            ReusableContent(reuse, move || {
                let s = s.clone();
                CompositionLocalProvider(LOCAL.provides(reuse), move || {
                    let context = remember_composition_context();
                    let mut held = s.borrow_mut();
                    if held.is_none() {
                        let composition = subcomposition(context, views_root());
                        composition.set_content(|| Text(&LOCAL.current().to_string()));
                        *held = Some(composition);
                    }
                });
            });
        });
    });
    let k = reuse_key.handle();
    test.validate(move |v| v.text(&k.get().to_string()));
    active.set(false);
    test.expect_changes();
    active.set(true);
    reuse_key.update(|key| key + 1);
    test.expect_changes();
    child.borrow().as_ref().expect("composed").set_content(|| Text(&LOCAL.current().to_string()));
    test.revalidate();
}

/// The root of a [`ViewApplier`], the same in each: where a child of the test's views emits.
fn views_root() -> NodeRef {
    ViewApplier::new().root()
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, canCreateARootPausableComposition
#[test]
fn a_pausable_composition_under_a_recomposer() {
    let recomposer = Recomposer::new();
    let pausable = Composition::pausable(empty_applier(), recomposer.as_context());
    pausable.dispose();
    recomposer.cancel();
    recomposer.close();
}

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/PausableCompositionTests.kt, canCreateANestedPausableComposition
#[test]
fn a_pausable_composition_under_a_place() {
    let mut test = composition_test();
    test.compose(|| {
        let parent = remember_composition_context();
        DisposableEffect((), move |scope| {
            let pausable = Composition::pausable(empty_applier(), parent);
            scope.on_dispose(move || pausable.dispose())
        });
    });
}

// ---- Nested passes (§3.2) --------------------------------------------------------------------

/// A child set in a body composes at once, its effects run before the next line of the parent,
/// and the parent goes on where it was, its nodes in their places.
#[test]
fn a_child_composes_at_once_in_a_body_and_the_parent_goes_on() {
    let mut test = composition_test();
    let log = Log::new();
    let l = log.clone();
    test.compose(move || {
        let l = l.clone();
        Linear!(move || {
            Text("before");
            l.push("parent before");
            let parent = remember_composition_context();
            let child = remember(|| Rc::new(Composition::with_parent(empty_applier(), parent)));
            let inner = l.clone();
            child.set_content(move || {
                inner.push("child");
                let inner = inner.clone();
                SideEffect(move || inner.push("child effect"));
            });
            l.push("parent after");
            Text("after");
            let l = l.clone();
            SideEffect(move || l.push("parent effect"));
        });
    });
    assert_eq!(
        log.take(),
        ["parent before", "child", "child effect", "parent after", "parent effect"]
    );
    test.validate(|v| {
        v.linear(|v| {
            v.text("before");
            v.text("after");
        })
    });
}

/// A pass of a composition already composing further up is a misuse.
#[test]
#[should_panic(expected = "already running")]
fn a_composition_cant_set_its_own_content_in_a_pass() {
    let composition = Rc::new(Composition::new(empty_applier()));
    let held = Rc::downgrade(&composition);
    composition.set_content(move || {
        held.upgrade().expect("alive").set_content(|| {});
    });
}

/// Disposing a composition composing further up is a misuse, from a child's content too.
#[test]
#[should_panic(expected = "dropped during its own pass")]
fn a_child_cant_dispose_the_parent_composing_it() {
    let composition = Rc::new(Composition::new(empty_applier()));
    let held = Rc::downgrade(&composition);
    composition.set_content(move || {
        let parent = remember_composition_context();
        let child = Composition::with_parent(empty_applier(), parent);
        let held = held.clone();
        child.set_content(move || held.upgrade().expect("alive").dispose());
    });
}

/// What a child's pass writes is applied to the parent's snapshot, and its readers hear when the
/// parent's is applied: the parent's scope that read it composes in the next frame.
#[test]
fn what_a_child_writes_reaches_the_parent_after_the_pass() {
    let mut test = composition_test();
    let value = mutable_state_of(0);
    let reads = Log::new();
    let (v, r) = (value.handle(), reads.clone());
    test.compose(move || {
        r.push(v.get().to_string());
        let parent = remember_composition_context();
        let child = remember(|| Rc::new(Composition::with_parent(empty_applier(), parent)));
        child.set_content(move || v.set(1));
    });
    assert_eq!(reads.take(), ["0"]);
    test.advance();
    assert_eq!(reads.take(), ["1"]);
}

/// A panic in a nested pass poisons the child; the parent, whose body catches it, goes on where it
/// was, as if the child's content hadn't run.
#[test]
fn a_panic_in_a_nested_pass_leaves_the_outer_pass_as_it_was() {
    let mut test = composition_test();
    let child_slot: Rc<RefCell<Option<Rc<Composition>>>> = Rc::default();
    let s = child_slot.clone();
    test.compose(move || {
        Text("before");
        let parent = remember_composition_context();
        let child = remember(|| Rc::new(Composition::with_parent(empty_applier(), parent)));
        *s.borrow_mut() = Some(child.clone());
        let caught = catch_unwind(AssertUnwindSafe(|| {
            child.set_content(|| {
                Text("child");
                panic!("in the child");
            });
        }));
        assert!(caught.is_err());
        Text("after");
    });
    test.validate(|v| {
        v.text("before");
        v.text("after");
    });
    assert!(child_slot.borrow().as_ref().expect("composed").is_poisoned());
    assert!(!test.composition().is_poisoned());
}

/// A nested pass in a paused one: the paused composition's pauses are its own, the child's content
/// doesn't ask them.
#[test]
fn a_nested_pass_doesnt_ask_the_paused_ones_callback() {
    let asked = Rc::new(Cell::new(0));
    let pausable =
        Composition::pausable(empty_applier(), Recomposer::current_thread().as_context());
    let composed = Rc::new(Cell::new(false));
    let c = composed.clone();
    let paused = pausable.set_pausable_content(move || {
        let parent = remember_composition_context();
        let child = remember(|| Rc::new(Composition::with_parent(empty_applier(), parent)));
        let c = c.clone();
        child.set_content(move || {
            #[composable]
            fn Empty() {}
            Empty!();
            c.set(true);
        });
    });
    let counted = asked.clone();
    while !paused.resume({
        let counted = counted.clone();
        move || {
            counted.set(counted.get() + 1);
            false
        }
    }) {}
    assert!(composed.get());
    let asked_before = asked.get();
    assert!(asked_before <= 2, "only the parent's points: {asked_before}");
    paused.apply();
}

// ---- Parents first, and frames ---------------------------------------------------------------

/// A child's invalidation asks for its root's frame, and `has_invalidations` of the root says.
#[test]
fn a_childs_invalidation_asks_the_roots_frame() {
    let composition = Composition::new(empty_applier());
    let frames = Rc::new(Cell::new(0));
    let f = frames.clone();
    composition.set_frame_requester(move || f.set(f.get() + 1));
    let value = mutable_state_of(0);
    let seen = Rc::new(Cell::new(-1));
    let (v, s) = (value.handle(), seen.clone());
    composition.set_content(move || {
        let s = s.clone();
        TestSubcomposition!(move || s.set(v.get()));
    });
    assert!(!composition.has_invalidations());
    let before = frames.get();
    value.set(3);
    Snapshot::send_apply_notifications();
    assert_eq!(frames.get(), before + 1);
    assert!(composition.has_invalidations());
    assert!(composition.recompose(), "a pass ran, though only below it");
    assert_eq!(seen.get(), 3);
    assert!(!composition.has_invalidations());
    assert!(!composition.recompose());
}

/// All the tree composes before any of its work, parents' work first.
#[test]
fn a_trees_work_runs_after_it_all_composed_parents_first() {
    let composition = Composition::new(empty_applier());
    let log = Log::new();
    let value = mutable_state_of(0);
    let (v, l) = (value.handle(), log.clone());
    composition.set_content(move || {
        let parent_value = v.get();
        l.push(format!("parent {parent_value}"));
        let effect = l.clone();
        SideEffect(move || effect.push(format!("parent effect {parent_value}")));
        let l = l.clone();
        TestSubcomposition!(move || {
            let child_value = v.get();
            l.push(format!("child {child_value}"));
            let effect = l.clone();
            SideEffect(move || effect.push(format!("child effect {child_value}")));
        });
    });
    log.clear();
    value.set(1);
    composition.recompose();
    assert_eq!(log.take(), ["parent 1", "child 1", "parent effect 1", "child effect 1"]);
}

/// A grandchild's invalidation recomposes it from the root, nobody between composing.
#[test]
fn a_grandchild_recomposes_from_the_root() {
    let composition = Composition::new(empty_applier());
    let value = mutable_state_of(0);
    let log = Log::new();
    let (v, l) = (value.handle(), log.clone());
    composition.set_content(move || {
        l.push("root");
        let l = l.clone();
        TestSubcomposition!(move || {
            l.push("child");
            let l = l.clone();
            TestSubcomposition!(move || l.push(format!("grandchild {}", v.get())));
        });
    });
    log.clear();
    value.set(1);
    assert!(composition.has_invalidations(), "two levels down");
    assert!(composition.recompose());
    assert_eq!(log.take(), ["grandchild 1"]);
}

/// A panic composing a child poisons it alone: the work of what's composed runs, it goes on out,
/// and the root recomposes after, without it.
#[test]
fn a_panic_composing_a_child_poisons_it_alone() {
    let composition = Composition::new(empty_applier());
    let value = mutable_state_of(0);
    let log = Log::new();
    let (v, l) = (value.handle(), log.clone());
    composition.set_content(move || {
        let parent_value = v.get();
        let effect = l.clone();
        SideEffect(move || effect.push(format!("parent effect {parent_value}")));
        TestSubcomposition!(move || {
            assert_ne!(v.get(), 1, "in the child");
        });
    });
    log.clear();
    value.set(1);
    let caught = catch_unwind(AssertUnwindSafe(|| composition.recompose()));
    assert!(caught.is_err());
    assert_eq!(log.take(), ["parent effect 1"]);
    assert!(!composition.is_poisoned());
    value.set(2);
    assert!(composition.recompose());
    assert_eq!(log.take(), ["parent effect 2"]);
}

/// A composition whose parent went on without its place, kept by its owner, composes on: Compose
/// holds it back in the frame of the removal only.
#[test]
fn a_child_kept_after_its_place_went_composes_on() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let (t, p) = (there.handle(), place.clone());
    test.compose(move || {
        if t.get() {
            DeferredSubcomposition(p.clone());
        }
    });
    let value = mutable_state_of(0);
    let seen = Rc::new(Cell::new(-1));
    let (v, s) = (value.handle(), seen.clone());
    let child = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));
    child.set_content(move || s.set(v.get()));

    there.set(false);
    value.set(1);
    test.composition().recompose();
    assert_eq!(seen.get(), 0, "not in the frame its place goes");
    value.set(2);
    test.composition().recompose();
    assert_eq!(seen.get(), 2);
    child.dispose();
    value.set(3);
    assert!(!test.composition().has_invalidations());
}

/// A parent disposed leaves its children to their owners, composing under its recomposer.
#[test]
fn a_child_outlives_its_disposed_parent_under_a_recomposer() {
    let recomposer = Recomposer::new();
    let parent = Composition::with_parent(empty_applier(), recomposer.as_context());
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let p = place.clone();
    parent.set_content(move || DeferredSubcomposition(p.clone()));
    let child = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));
    let value = mutable_state_of(0);
    let seen = Rc::new(Cell::new(-1));
    let (v, s) = (value.handle(), seen.clone());
    child.set_content(move || s.set(v.get()));
    parent.dispose();
    value.set(1);
    assert!(perform_frame(recomposer.handle(), 0));
    assert_eq!(seen.get(), 1);
    drop(child);
    recomposer.cancel();
}

/// A child under a recomposer's composition is that recomposer's: its effects' scope, its frames.
#[test]
fn a_child_is_under_its_parents_recomposer() {
    let recomposer = Recomposer::new();
    let parent = Composition::with_parent(empty_applier(), recomposer.as_context());
    let child_context = Rc::new(Cell::new(None));
    let c = child_context.clone();
    parent.set_content(move || c.set(Some(remember_composition_context())));
    let context = child_context.get().expect("composed");
    let ran = Rc::new(Cell::new(false));
    let r = ran.clone();
    context.schedule_frame_end_callback(move || r.set(true));
    assert!(recomposer.has_pending_work(), "frame work, with no loop");
    perform_frame(recomposer.handle(), 0);
    assert!(ran.get());
    let child = Composition::with_parent(empty_applier(), context);
    assert_eq!(child.parent(), context);
    recomposer.cancel();
}

// ---- Sharing and lending an applier (§3.3) ---------------------------------------------------

/// A subcomposition's nodes go in its parent's tree, below the node it's given, and go with what
/// disposes it, also when that node is released first.
#[test]
fn a_subcomposition_emits_in_its_parents_tree_and_goes_with_its_node() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let value = mutable_state_of(0);
    let (t, v) = (there.handle(), value.handle());
    let root = views_root();
    test.compose(move || {
        if t.get() {
            let parent = remember_composition_context();
            DisposableEffect((), move |scope| {
                let child = subcomposition(parent, root);
                child.set_content(move || {
                    Linear!(move || Text(&v.get().to_string()));
                });
                scope.on_dispose(move || child.dispose())
            });
        }
    });
    test.validate(|v| v.linear(|v| v.text("0")));
    value.set(1);
    test.expect_changes();
    test.validate(|v| v.linear(|v| v.text("1")));
    there.set(false);
    test.expect_changes();
    test.validate(|_| {});
}

/// A child below a node of the parent's, the node removed with its group: disposing the child
/// after the node's release finds nothing to do.
#[test]
fn a_subcomposition_disposed_after_its_node_was_released() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let t = there.handle();
    let place: Rc<Cell<Option<CompositionContext>>> = Rc::default();
    let child: Rc<RefCell<Option<Composition>>> = Rc::default();
    let (p, c) = (place.clone(), child.clone());
    test.compose(move || {
        if t.get() {
            p.set(Some(remember_composition_context()));
            let c = c.clone();
            DisposableEffect((), move |scope| {
                scope.on_dispose(move || c.borrow().as_ref().expect("made").dispose())
            });
            Linear!(|| {});
        }
    });
    let host = test.with_views(|views| views.roots()[0]);
    let made = subcomposition(place.get().expect("composed"), host);
    made.set_content(|| Text("in the host"));
    *child.borrow_mut() = Some(made);
    test.validate(|v| v.linear(|v| v.text("in the host")));
    there.set(false);
    test.expect_changes();
    test.validate(|_| {});
}

/// Composing while the holder has the applier borrowed: the pass's changes reach the real tree,
/// the holder finds them when it's back, and a panic brings it back too.
#[test]
fn a_lent_applier_takes_a_passs_changes_and_comes_back() {
    let composition = Composition::new(ViewApplier::new());
    let place: Rc<Cell<Option<CompositionContext>>> = Rc::default();
    let p = place.clone();
    composition.set_content(move || p.set(Some(remember_composition_context())));
    let root = composition.with_applier(|views: &mut ViewApplier| views.root());
    let child = subcomposition(place.get().expect("composed"), root);
    composition.with_applier(|views: &mut ViewApplier| {
        with_lent_applier(&composition, views, || {
            child.set_content(|| {
                Linear!(|| Text("lent"));
            });
        });
        assert_eq!(views.tree.dump().trim(), "<linear>\n  <text text='lent'>");
    });
    // Back, and not lent: a pass goes through the shared applier again.
    child.set_content(|| Text("after"));
    composition.with_applier(|views: &mut ViewApplier| {
        assert_eq!(views.tree.dump().trim(), "<text text='after'>");
    });
    // A panic in what's lent to.
    composition.with_applier(|views: &mut ViewApplier| {
        let unwound = catch_unwind(AssertUnwindSafe(|| {
            with_lent_applier(&composition, views, || panic!("in the lend"));
        }));
        assert!(unwound.is_err());
        assert_eq!(views.tree.dump().trim(), "<text text='after'>");
    });
    // Disposing in a lend, as a measurement dropping a slot.
    composition.with_applier(|views: &mut ViewApplier| {
        with_lent_applier(&composition, views, || child.dispose());
        assert_eq!(views.tree.dump().trim(), "");
    });
}

/// A lend needs the holder to have the applier: otherwise what's passed isn't it.
#[test]
#[should_panic(expected = "with_lent_applier lends the applier")]
fn lending_an_applier_not_borrowed_is_a_misuse() {
    let composition = Composition::new(ViewApplier::new());
    let mut other = ViewApplier::new();
    with_lent_applier(&composition, &mut other, || {});
}

/// A context with no place can't give an applier.
#[test]
#[should_panic(expected = "needs a context in a live composition")]
fn a_subcomposition_needs_a_place() {
    let recomposer = Recomposer::new();
    let _ = subcomposition(recomposer.as_context(), NodeRef::NONE);
}

// ---- More of the frames of a tree ------------------------------------------------------------

// Compose: compose/runtime/runtime/src/nonEmulatorCommonTest/kotlin/androidx/compose/runtime/RecomposerTests.kt, testSubcompositionDisposedInParent
// (under a recomposer: its loop of what's invalid ends with the child that's going)
#[test]
fn a_child_going_in_the_frame_ends_the_recomposers_frame() {
    let recomposer = Recomposer::new();
    let composition = Composition::with_parent(empty_applier(), recomposer.as_context());
    let state = mutable_state_of(true);
    let s = state.handle();
    composition.set_content(move || {
        if s.get() {
            TestSubcomposition!(move || {
                assert!(s.get(), "Subcomposition should be disposed if state is false");
            });
        }
    });
    state.set(false);
    assert!(perform_frame(recomposer.handle(), 0));
    composition.dispose();
    recomposer.cancel();
}

/// A recomposer's frame goes down from what's on top, whatever the arena's order: here the child's
/// index is below its parent's.
#[test]
fn a_recomposers_frame_goes_from_the_top_whatever_the_order_made() {
    let recomposer = Recomposer::new();
    let freed = Composition::new(empty_applier());
    let composition = Composition::with_parent(empty_applier(), recomposer.as_context());
    drop(freed);
    let place: Rc<Cell<Option<CompositionContext>>> = Rc::default();
    let log = Log::new();
    let (first, second) = (mutable_state_of(0), mutable_state_of(0));
    let (f, p, l) = (first.handle(), place.clone(), log.clone());
    composition.set_content(move || {
        l.push(format!("parent {}", f.get()));
        p.set(Some(remember_composition_context()));
    });
    let child = Composition::with_parent(empty_applier(), place.get().expect("composed"));
    let (s, l) = (second.handle(), log.clone());
    child.set_content(move || l.push(format!("child {}", s.get())));
    log.clear();
    first.set(1);
    second.set(1);
    assert!(perform_frame(recomposer.handle(), 0));
    assert_eq!(log.take(), ["parent 1", "child 1"]);
    drop(child);
    recomposer.cancel();
}

/// What's only invalid below a recomposer's composition is a frame's too.
#[test]
fn a_recomposers_frame_composes_what_is_only_invalid_below() {
    let recomposer = Recomposer::new();
    let composition = Composition::with_parent(empty_applier(), recomposer.as_context());
    let value = mutable_state_of(0);
    let seen = Rc::new(Cell::new(-1));
    let (v, s) = (value.handle(), seen.clone());
    composition.set_content(move || {
        let s = s.clone();
        TestSubcomposition!(move || s.set(v.get()));
    });
    value.set(1);
    assert!(perform_frame(recomposer.handle(), 0));
    assert_eq!(seen.get(), 1);
    composition.dispose();
    recomposer.cancel();
}

/// A panic in a child's effect, in a tree's frame: the rest of the work runs, and it goes on out.
#[test]
fn a_panic_in_a_tree_work_goes_out_after_the_rest() {
    let composition = Composition::new(empty_applier());
    let value = mutable_state_of(0);
    let log = Log::new();
    let (v, l) = (value.handle(), log.clone());
    composition.set_content(move || {
        let current = v.get();
        let first = l.clone();
        SideEffect(move || {
            first.push("parent effect");
            assert_eq!(current, 0, "in the parent's effect");
        });
        let second = l.clone();
        TestSubcomposition!(move || {
            let current = v.get();
            let second = second.clone();
            SideEffect(move || second.push(format!("child effect {current}")));
        });
    });
    log.clear();
    value.set(1);
    let caught = catch_unwind(AssertUnwindSafe(|| composition.recompose()));
    assert!(caught.is_err());
    assert_eq!(log.take(), ["parent effect", "child effect 1"]);
}

/// A child's own effect writing what it read: the frame it asks for is its root's.
#[test]
fn a_childs_effect_writing_what_it_read_asks_the_roots_frame() {
    let composition = Composition::new(empty_applier());
    let frames = Rc::new(Cell::new(0));
    let f = frames.clone();
    composition.set_frame_requester(move || f.set(f.get() + 1));
    let (value, go) = (mutable_state_of(0), mutable_state_of(false));
    let (v, g) = (value.handle(), go.handle());
    composition.set_content(move || {
        TestSubcomposition!(move || {
            let read = v.get();
            let going = g.get();
            SideEffect(move || {
                if going && read == 0 {
                    v.set(1);
                }
            });
        });
    });
    go.set(true);
    assert!(composition.recompose());
    let before = frames.get();
    Snapshot::send_apply_notifications();
    assert!(frames.get() > before, "asked of the root's requester");
}

/// A place and the compositions under it are freed when the last of them goes, whichever is last.
#[test]
fn a_context_goes_with_its_place_and_children() {
    let before = live_context_count();
    let composition = Composition::new(empty_applier());
    let there = mutable_state_of(true);
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let (t, p) = (there.handle(), place.clone());
    composition.set_content(move || {
        if t.get() {
            DeferredSubcomposition(p.clone());
        }
    });
    assert_eq!(live_context_count(), before + 1);
    let child = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));

    there.set(false);
    composition.recompose();
    assert_eq!(live_context_count(), before + 1, "its child asks of it still");
    drop(child);
    assert_eq!(live_context_count(), before);

    there.set(true);
    composition.recompose();
    assert_eq!(live_context_count(), before + 1);
    let child = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));
    drop(child);
    assert_eq!(live_context_count(), before + 1, "its place is there still");
    there.set(false);
    composition.recompose();
    assert_eq!(live_context_count(), before);

    // A composition under a stale place is one of its own.
    let stale = place.borrow().expect("composed");
    let alone = Composition::with_parent(empty_applier(), stale);
    alone.set_content(|| assert_eq!(LOCAL_INT.current(), 1));
    assert_eq!(live_context_count(), before);
}

/// The outer pass's moves go first: the nested pass has an applier of its own, and moves of its
/// own to coalesce.
#[test]
fn a_nested_pass_finds_the_outer_passs_moves_applied() {
    #[composable]
    fn Inner(order: MutableState<Vec<i32>>) {
        for item in order.get() {
            key(item, || Text(&item.to_string()));
        }
    }
    let composition = Composition::new(ViewApplier::new());
    let order = mutable_state_of(vec![1, 2, 3]);
    let inner_order = mutable_state_of(vec![7, 8, 9]);
    let child: Rc<RefCell<Option<Rc<Composition>>>> = Rc::default();
    let (o, i, c) = (order.handle(), inner_order.handle(), child.clone());
    composition.set_content(move || {
        let parent = remember_composition_context();
        let held = remember(|| Rc::new(Composition::with_parent(ViewApplier::new(), parent)));
        *c.borrow_mut() = Some(held.clone());
        for (at, item) in o.get().into_iter().enumerate() {
            key(item, || {
                // Right after its group's move, still to apply.
                if at == 0 {
                    let held = held.clone();
                    held.set_content(move || Inner!(i));
                }
                Text(&item.to_string());
            });
        }
    });
    order.set(vec![3, 2, 1]);
    inner_order.set(vec![9, 8, 7]);
    composition.recompose();
    let dump = |composition: &Composition| {
        composition.with_applier(|views: &mut ViewApplier| views.tree.dump())
    };
    assert_eq!(dump(&composition).trim(), "<text text='3'>\n<text text='2'>\n<text text='1'>");
    let child = child.borrow().clone().expect("composed");
    assert_eq!(dump(&child).trim(), "<text text='9'>\n<text text='8'>\n<text text='7'>");
}

/// Rule R2's "the next pass" is the next outermost one: what was retired before a pass is there
/// until its own end, nested passes ending or not.
#[test]
fn a_nested_pass_frees_nothing_of_the_outer_passs() {
    let composition = Composition::new(empty_applier());
    let (there, run) = (mutable_state_of(true), mutable_state_of(0));
    let retired: Rc<Cell<Option<MutableState<i32>>>> = Rc::default();
    let seen = Rc::new(Cell::new(0));
    let (t, r, h, s) = (there.handle(), run.handle(), retired.clone(), seen.clone());
    composition.set_content(move || {
        r.get();
        if t.get() {
            let state = remember(|| mutable_state_of(5));
            h.set(Some(state));
        }
        if !t.get() {
            let parent = remember_composition_context();
            let child = Composition::with_parent(empty_applier(), parent);
            child.set_content(|| {});
            s.set(h.get().expect("composed").get());
        }
    });
    there.set(false);
    composition.recompose();
    // It was forgotten by the pass above. It's retired, for this next one to free at its end.
    run.set(1);
    composition.recompose();
    assert_eq!(seen.get(), 5);
}

// ---- What's read above a child's root ---------------------------------------------------------

/// A static local changing above a child doesn't invalidate it: its next pass composes it all, as
/// `startRoot`'s `changed(parentProvider)`, and a pass with no such change only what's invalid.
#[test]
fn a_static_change_reaches_a_child_at_its_next_pass_whole() {
    #[composable]
    fn Inner(log: Log, scope: ScopeHolder) {
        scope.capture();
        log.push(format!("inner {}", STATIC_INT.current()));
    }
    #[composable]
    fn Sibling(log: Log) {
        log.push(format!("sibling {}", STATIC_INT.current()));
    }
    let composition = Composition::new(empty_applier());
    let value = mutable_state_of(1);
    let log = Log::new();
    let scope = ScopeHolder::new();
    let child: Rc<RefCell<Option<Composition>>> = Rc::default();
    let (v, l, s, c) = (value.handle(), log.clone(), scope.clone(), child.clone());
    composition.set_content(move || {
        let (l, s, c) = (l.clone(), s.clone(), c.clone());
        CompositionLocalProvider(STATIC_INT.provides(v.get()), move || {
            let parent = remember_composition_context();
            let (l, s) = (l.clone(), s.clone());
            c.borrow_mut().get_or_insert_with(move || {
                let child = Composition::with_parent(empty_applier(), parent);
                child.set_content(move || {
                    l.push(format!("root {}", STATIC_INT.current()));
                    Inner!(l.clone(), s.clone());
                    Sibling!(l.clone());
                });
                child
            });
        });
    });
    assert_eq!(log.take(), ["root 1", "inner 1", "sibling 1"]);
    // With no change above, a pass composes only what's invalid.
    scope.invalidate();
    composition.recompose();
    assert_eq!(log.take(), ["inner 1"]);
    // It changes: no pass for the child, which nothing invalidated.
    value.set(2);
    composition.recompose();
    assert_eq!(log.take(), Vec::<String>::new());
    // What brings its next pass composes all of it, this once.
    scope.invalidate();
    composition.recompose();
    assert_eq!(log.take(), ["root 2", "inner 2", "sibling 2"]);
    scope.invalidate();
    composition.recompose();
    assert_eq!(log.take(), ["inner 2"]);
}

/// The one context through deactivation and reuse, with its children; a group given to another
/// place after it's gone isn't taken for it.
#[test]
fn a_context_lives_through_deactivation_and_reuse_not_removal() {
    let composition = Composition::new(empty_applier());
    let (active, reuse_key, there) =
        (mutable_state_of(true), mutable_state_of(0), mutable_state_of(true));
    let seen: Rc<RefCell<Vec<CompositionContext>>> = Rc::default();
    let (a, k, t, s) = (active.handle(), reuse_key.handle(), there.handle(), seen.clone());
    composition.set_content(move || {
        if !t.get() {
            // At what may be the same group's index, another place.
            s.borrow_mut().push(remember_composition_context());
            return;
        }
        let s = s.clone();
        let k = k;
        ReusableContentHost(a.get(), move || {
            let s = s.clone();
            ReusableContent(k.get(), move || {
                s.borrow_mut().push(remember_composition_context());
            });
        });
    });
    let first = seen.borrow()[0];
    let child = Composition::with_parent(empty_applier(), first);
    let local_seen = Rc::new(Cell::new(0));
    let l = local_seen.clone();
    child.set_content(move || l.set(LOCAL_INT.current()));

    active.set(false);
    composition.recompose();
    assert_eq!(live_context_count(), 1, "kept, deactivated");
    active.set(true);
    composition.recompose();
    assert_eq!(*seen.borrow().last().expect("composed"), first, "the same, active again");
    reuse_key.set(1);
    composition.recompose();
    assert_eq!(*seen.borrow().last().expect("composed"), first, "the same, reused");

    there.set(false);
    composition.recompose();
    let other = *seen.borrow().last().expect("composed");
    assert_ne!(other, first, "another");
    drop(child);
}

/// A child outliving its place keeps the hash it had there, as Compose's context keeps it.
#[test]
fn a_child_keeps_its_hash_when_its_place_goes() {
    let composition = Composition::new(empty_applier());
    let there = mutable_state_of(true);
    let place: Rc<RefCell<Option<CompositionContext>>> = Rc::default();
    let (t, p) = (there.handle(), place.clone());
    composition.set_content(move || {
        if t.get() {
            DeferredSubcomposition(p.clone());
        }
    });
    let child = Composition::with_parent(empty_applier(), place.borrow().expect("composed"));
    let hash = Rc::new(Cell::new(0));
    let h = hash.clone();
    let content = move || h.set(current_composite_key_hash_code());
    child.set_content(content.clone());
    let before = hash.get();
    there.set(false);
    composition.recompose();
    hash.set(0);
    child.set_content(content);
    assert_eq!(hash.get(), before);
}

// ---- A shared applier's changes ----------------------------------------------------------------

/// A nested pass on the tree of the pass around it doesn't end its changes while that's composing,
/// with its nodes in the tree (D31); nor does a frame's first composition before the last.
#[test]
fn a_shared_applier_begins_and_ends_once_around_its_passes() {
    let composition = Composition::new(ViewApplier::new());
    let counts = Rc::new(Cell::new((0, 0)));
    let value = mutable_state_of(0);
    let (c, v) = (counts.clone(), value.handle());
    let child_slot: Rc<RefCell<Option<Rc<Composition>>>> = Rc::default();
    let slot = child_slot.clone();
    composition.set_content(move || {
        let parent = remember_composition_context();
        let child = remember(|| Rc::new(subcomposition(parent, views_root())));
        *slot.borrow_mut() = Some(child.clone());
        child.set_content(move || Text(&v.get().to_string()));
        c.set(
            child.with_applier(|views: &mut ViewApplier| (views.begin_changes, views.end_changes)),
        );
    });
    assert_eq!(counts.get().1, 0, "the tree's changes aren't over in the parent's body");
    let total = composition
        .with_applier(|views: &mut ViewApplier| (views.begin_changes, views.end_changes));
    assert_eq!(total, (1, 1));
    value.set(1);
    composition.recompose();
    let total = composition
        .with_applier(|views: &mut ViewApplier| (views.begin_changes, views.end_changes));
    assert_eq!(total, (2, 2), "the child's frame, alone");
}

/// A panic in a nested pass on the shared tree leaves its count to what's around it.
#[test]
fn a_panic_in_a_nested_pass_leaves_the_shared_appliers_count() {
    let composition = Composition::new(ViewApplier::new());
    composition.set_content(move || {
        let parent = remember_composition_context();
        let child = subcomposition(parent, views_root());
        let caught = catch_unwind(AssertUnwindSafe(|| child.set_content(|| panic!("nested"))));
        assert!(caught.is_err());
        std::mem::forget(child);
    });
    let total = composition
        .with_applier(|views: &mut ViewApplier| (views.begin_changes, views.end_changes));
    assert_eq!(total, (1, 1));
}

/// A place in the tree given to another node isn't the node it was: a child disposed after its node
/// went leaves what's there now alone.
#[test]
fn a_subcomposition_disposed_later_leaves_a_node_at_its_nodes_old_index() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let t = there.handle();
    let place: Rc<Cell<Option<CompositionContext>>> = Rc::default();
    let p = place.clone();
    test.compose(move || {
        p.set(Some(remember_composition_context()));
        Linear!(move || {
            if t.get() {
                Text("host");
            }
        });
    });
    let host = test.with_views(|views| {
        let linear = views.roots()[0];
        views.tree.children(linear).next().expect("composed")
    });
    let child = subcomposition(place.get().expect("composed"), host);
    child.set_content(|| Text("in the host"));
    there.set(false);
    test.advance();
    // The index of the node it was below goes to a new one.
    there.set(true);
    test.advance();
    test.validate(|v| v.linear(|v| v.text("host")));
    child.dispose();
    test.validate(|v| v.linear(|v| v.text("host")));
}

/// Two levels down, through a composition with no provider of its own, a static change reaches its
/// next pass the same.
#[test]
fn a_static_change_reaches_a_grandchild_through_a_child_with_no_provider() {
    #[composable]
    fn Inner(log: Log, scope: ScopeHolder) {
        scope.capture();
        log.push(format!("inner {}", STATIC_INT.current()));
    }
    let composition = Composition::new(empty_applier());
    let value = mutable_state_of(1);
    let log = Log::new();
    let scope = ScopeHolder::new();
    let (v, l, s) = (value.handle(), log.clone(), scope.clone());
    let kept: Rc<RefCell<Vec<Composition>>> = Rc::default();
    let k = kept.clone();
    composition.set_content(move || {
        let (l, s, k) = (l.clone(), s.clone(), k.clone());
        CompositionLocalProvider(STATIC_INT.provides(v.get()), move || {
            let parent = remember_composition_context();
            let first = remember(|| Rc::new(Cell::new(true)));
            if !first.replace(false) {
                return;
            }
            let child = Composition::with_parent(empty_applier(), parent);
            let (l, s, k_inner) = (l.clone(), s.clone(), k.clone());
            child.set_content(move || {
                let parent = remember_composition_context();
                let first = remember(|| Rc::new(Cell::new(true)));
                if !first.replace(false) {
                    return;
                }
                let grandchild = Composition::with_parent(empty_applier(), parent);
                let (l, s) = (l.clone(), s.clone());
                grandchild.set_content(move || {
                    l.push(format!("root {}", STATIC_INT.current()));
                    Inner!(l.clone(), s.clone());
                });
                k_inner.borrow_mut().push(grandchild);
            });
            k.borrow_mut().push(child);
        });
    });
    assert_eq!(log.take(), ["root 1", "inner 1"]);
    value.set(2);
    composition.recompose();
    assert_eq!(log.take(), Vec::<String>::new());
    scope.invalidate();
    composition.recompose();
    assert_eq!(log.take(), ["root 2", "inner 2"]);
}

/// A reusable child below a node of its parent's, deactivated after the node went: nothing of its
/// is there to tell.
#[test]
fn a_subcomposition_deactivated_after_its_node_was_released() {
    let mut test = composition_test();
    let there = mutable_state_of(true);
    let t = there.handle();
    let place: Rc<Cell<Option<CompositionContext>>> = Rc::default();
    let p = place.clone();
    test.compose(move || {
        p.set(Some(remember_composition_context()));
        if t.get() {
            Linear!(|| {});
        }
    });
    let host = test.with_views(|views| views.roots()[0]);
    let child = reusable_subcomposition(place.get().expect("composed"), host);
    child.set_content(|| Text("in the host"));
    there.set(false);
    test.advance();
    child.deactivate();
    test.validate(|_| {});
}
