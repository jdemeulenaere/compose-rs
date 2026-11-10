//! Compose's `SubcomposeLayoutTest`, rewritten: slots composed while measuring, kept for reuse and
//! precomposed, their compositions going as they should (composition-context.md §3.4, D1309).

#![allow(non_snake_case)]
// A closure as a `Callback` isn't an `Fn` of its own.
#![allow(clippy::redundant_closure)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_foundation::layout::*;
use compose_foundation::*;
use compose_runtime::*;
use compose_test::*;
use compose_ui::subcompose::*;
use compose_ui::{
    Constraints, FocusRequester, IntSize, Layout, Measurable, ModifierNode, ModifierNodeElement,
    NodeContext, Placeable, Remeasurement, RemeasurementModifier, TraversableNode,
    TraverseDescendantsAction, TraverseKey, UnitExt, measure_policy,
};

fn sized(tag: String, size: i32) -> impl Fn() + 'static {
    move || {
        Box!(modifier = Modifier.size(((size as f32).dp(), (size as f32).dp())).test_tag(&tag));
    }
}

/// Measures and places in a column all that `slots` compose, as the file's simple policies do.
fn stack_policy(
    slots: impl Fn(&mut SubcomposeMeasureScope<'_, '_>) -> Vec<Vec<Measurable>> + 'static,
) -> Rc<dyn SubcomposeMeasurePolicy> {
    subcompose_policy(move |scope, constraints| {
        let measurables = slots(scope);
        let placeables: Vec<Placeable> = measurables
            .into_iter()
            .flatten()
            .map(|measurable| scope.measure(measurable, constraints.loosen()))
            .collect();
        let (mut width, mut height) = (0, 0);
        for placeable in &placeables {
            width = width.max(placeable.width);
            scope.place(placeable, 0, height);
            height += placeable.height;
        }
        scope.layout(width, height)
    })
}

/// Lives while it's remembered.
struct Live(Rc<Cell<i32>>);

impl RememberObserver for Live {
    fn on_remembered(&self) {
        self.0.set(self.0.get() + 1);
    }
    fn on_forgotten(&self) {
        self.0.set(self.0.get() - 1);
    }
}

fn live_content(live: &Rc<Cell<i32>>) -> Callback {
    let live = live.clone();
    Callback::new(move || {
        let live = live.clone();
        remember_observer(move || Live(live));
        Box!(modifier = Modifier.size((10.dp(), 10.dp())));
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, useSizeOfTheFirstItemInSecondSubcomposition
#[test]
fn a_second_slot_is_sized_by_what_the_first_measured() {
    let mut ui = compose_test(|| {
        SubcomposeLayout!(
            measure_policy = subcompose_policy(|scope, constraints| {
                let first = scope.subcompose("first", Callback::new(sized("first".into(), 50)));
                let placeable = scope.measure(first[0], constraints);
                let size = placeable.width;
                let second =
                    scope.subcompose("second", Callback::new(sized("second".into(), size / 2)));
                let second = scope.measure(second[0], constraints);
                scope.place(&placeable, 0, 0);
                scope.place(&second, 0, placeable.height);
                scope.layout(placeable.width, placeable.height + second.height)
            })
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("first").assert_exists();
    ui.on_node_with_tag("second").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, subcomposeMultipleLayoutsInOneSlot
#[test]
fn a_slot_can_emit_several_layouts() {
    let count = Rc::new(Cell::new(0));
    let c = count.clone();
    let mut ui = compose_test(move || {
        let c = c.clone();
        SubcomposeLayout!(
            measure_policy = subcompose_policy(move |scope, constraints| {
                let items = scope.subcompose(
                    (),
                    Callback::new(|| {
                        sized("one".into(), 10)();
                        sized("two".into(), 10)();
                    }),
                );
                c.set(items.len());
                let placeables: Vec<Placeable> =
                    items.iter().map(|item| scope.measure(*item, constraints)).collect();
                let mut y = 0;
                for placeable in &placeables {
                    scope.place(placeable, 0, y);
                    y += placeable.height;
                }
                scope.layout(10, y)
            })
        );
    });
    ui.wait_for_idle();
    assert_eq!(count.get(), 2);
    ui.on_node_with_tag("one").assert_exists();
    ui.on_node_with_tag("two").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, recompositionOfTheFirstSlotDoestAffectTheSecond
#[test]
fn a_slot_recomposing_leaves_the_other_alone() {
    let (value, first_runs, second_runs) =
        (mutable_state_of(0), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (v, f, s) = (value.handle(), first_runs.clone(), second_runs.clone());
    let first = Callback::new(move || {
        v.get();
        f.set(f.get() + 1);
    });
    let second = Callback::new(move || s.set(s.get() + 1));
    let policy = stack_policy(move |scope| {
        vec![scope.subcompose(1, first.clone()), scope.subcompose(2, second.clone())]
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    assert_eq!((first_runs.get(), second_runs.get()), (1, 1));
    value.set(1);
    ui.wait_for_idle();
    assert_eq!((first_runs.get(), second_runs.get()), (2, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, addLayoutOnlyAfterRecomposition
#[test]
fn a_slot_adding_a_layout_measures_its_parent_again() {
    let added = mutable_state_of(false);
    let a = added.handle();
    let content = Callback::new(move || {
        if a.get() {
            sized("added".into(), 10)();
        }
    });
    let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    ui.on_node_with_tag("added").assert_does_not_exist();
    added.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("added").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, providingNewLambdaCausingRecomposition
#[test]
fn a_new_lambda_composes_the_slot_again() {
    let content =
        mutable_state_of_with(Callback::new(sized("first".into(), 10)), ReferentialEqualityPolicy);
    let c = content.handle();
    let policy = stack_policy(move |scope| vec![scope.subcompose((), c.get())]);
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    ui.on_node_with_tag("first").assert_exists();
    content.set(Callback::new(sized("second".into(), 10)));
    ui.wait_for_idle();
    ui.on_node_with_tag("first").assert_does_not_exist();
    ui.on_node_with_tag("second").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, notSubcomposedSlotIsDisposed
#[test]
fn a_slot_no_longer_composed_is_disposed() {
    let (shown, live) = (mutable_state_of(true), Rc::new(Cell::new(0)));
    let (s, content) = (shown.handle(), live_content(&live));
    let policy = stack_policy(move |scope| {
        if s.get() { vec![scope.subcompose((), content.clone())] } else { Vec::new() }
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(live.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, slotsCouldBeReordered
#[test]
fn slots_can_be_reordered() {
    let first_up = mutable_state_of(true);
    let f = first_up.handle();
    let (a, b) = (Callback::new(sized("a".into(), 10)), Callback::new(sized("b".into(), 10)));
    let policy = stack_policy(move |scope| {
        if f.get() {
            vec![scope.subcompose("a", a.clone()), scope.subcompose("b", b.clone())]
        } else {
            vec![scope.subcompose("b", b.clone()), scope.subcompose("a", a.clone())]
        }
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    let height = ui.on_node_with_tag("a").get_bounds_in_root().height();
    ui.on_node_with_tag("b").assert_top_position_in_root_is_equal_to(height);
    first_up.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("a").assert_exists();
    ui.on_node_with_tag("b").assert_top_position_in_root_is_equal_to(0.dp());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, slotsAreDisposedWhenLayoutIsDisposed
#[test]
fn slots_go_with_the_layout() {
    let (shown, live) = (mutable_state_of(true), Rc::new(Cell::new(0)));
    let (s, content) = (shown.handle(), live_content(&live));
    let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
    let mut ui = compose_test(move || {
        if s.get() {
            SubcomposeLayout!(measure_policy = policy.clone());
        }
    });
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(live.get(), 0);
}

fn composes_the_keys(
    keys: MutableState<Vec<i32>>,
    live: &Rc<Cell<i32>>,
) -> Rc<dyn SubcomposeMeasurePolicy> {
    let live = live.clone();
    stack_policy(move |scope| {
        keys.get().into_iter().map(|key| scope.subcompose(key, live_content(&live))).collect()
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precompose
#[test]
fn a_precomposed_slot_is_there_before_its_measurement() {
    let (state, live) = (SubcomposeLayoutState::new(), Rc::new(Cell::new(0)));
    let keys = mutable_state_of(Vec::<i32>::new());
    let policy = composes_the_keys(keys.handle(), &live);
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!(live.get(), 0);
    let handle = state.precompose(1, live_content(&live));
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
    assert_eq!(handle.placeables_count(), 1);
    keys.set(vec![1]);
    ui.wait_for_idle();
    assert_eq!(live.get(), 1, "taken by the measurement");
    assert_eq!(handle.placeables_count(), 0, "the measurement's now");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, disposePrecomposedItem
#[test]
fn a_precomposed_slot_can_be_disposed() {
    let (state, live) = (SubcomposeLayoutState::new(), Rc::new(Cell::new(0)));
    let keys = mutable_state_of(Vec::<i32>::new());
    let policy = composes_the_keys(keys.handle(), &live);
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let handle = state.precompose(1, live_content(&live));
    assert_eq!(live.get(), 1);
    handle.dispose();
    ui.wait_for_idle();
    assert_eq!(live.get(), 0);
    // Compose: composeItemRegularlyAfterDisposingPrecomposedItem
    keys.set(vec![1]);
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposedItemDisposedWhenSubcomposeLayoutIsDisposed
#[test]
fn a_precomposed_slot_goes_with_the_layout() {
    let (state, live, shown) =
        (SubcomposeLayoutState::new(), Rc::new(Cell::new(0)), mutable_state_of(true));
    let keys = mutable_state_of(Vec::<i32>::new());
    let policy = composes_the_keys(keys.handle(), &live);
    let (st, s) = (state.clone(), shown.handle());
    let mut ui = compose_test(move || {
        if s.get() {
            SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone());
        }
    });
    ui.wait_for_idle();
    let _handle = state.precompose(1, live_content(&live));
    assert_eq!(live.get(), 1);
    shown.set(false);
    ui.wait_for_idle();
    assert_eq!(live.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposeIsNotTriggeringParentRemeasure
#[test]
fn precomposing_measures_nothing_again() {
    let (state, measures) = (SubcomposeLayoutState::new(), Rc::new(Cell::new(0)));
    let (st, m) = (state.clone(), measures.clone());
    let mut ui = compose_test(move || {
        let m = m.clone();
        SubcomposeLayoutWithState!(
            st.clone(),
            measure_policy = subcompose_policy(move |scope, _| {
                m.set(m.get() + 1);
                scope.layout(10, 10)
            })
        );
    });
    ui.wait_for_idle();
    let measured = measures.get();
    let handle = state.precompose(1, Callback::new(sized("item".into(), 10)));
    ui.wait_for_idle();
    assert_eq!(measures.get(), measured);
    handle.dispose();
    ui.wait_for_idle();
    assert_eq!(measures.get(), measured, "precomposedItemDisposalIsNotTriggeringParentRemeasure");
}

/// A composition kept for reuse is what a new slot takes: what's remembered goes and comes.
type ReuseScene = (ComposeTestRule, Rc<SubcomposeLayoutState>, OwnedState<Vec<i32>>, Rc<Cell<i32>>);

fn reuse_scene(max: usize) -> ReuseScene {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(max));
    let (keys, live) = (mutable_state_of(vec![0, 1, 2]), Rc::new(Cell::new(0)));
    let policy = composes_the_keys(keys.handle(), &live);
    let st = state.clone();
    let ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    (ui, state, keys, live)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, slotsKeptForReuse
#[test]
fn slots_are_kept_for_reuse_as_the_policy_says() {
    let (mut ui, state, keys, live) = reuse_scene(2);
    ui.wait_for_idle();
    assert_eq!(live.get(), 3);
    keys.set(Vec::new());
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 2, "of three");
    assert_eq!(live.get(), 0, "what's remembered is forgotten when it's deactivated");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, newSlotIsUsingReusedSlot
#[test]
fn a_new_slot_uses_what_was_kept() {
    let (mut ui, state, keys, _) = reuse_scene(2);
    ui.wait_for_idle();
    keys.set(vec![0]);
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 2);
    keys.set(vec![0, 5]);
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 1, "one's in use again");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, nothingIsReusedWhenMaxSlotsAre0
#[test]
fn nothing_is_kept_with_a_policy_of_none() {
    let (mut ui, state, keys, live) = reuse_scene(0);
    ui.wait_for_idle();
    keys.set(Vec::new());
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 0);
    assert_eq!(live.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, prefetchIsUsingReusableNodes
#[test]
fn precomposing_uses_what_was_kept() {
    let (mut ui, state, keys, live) = reuse_scene(1);
    ui.wait_for_idle();
    keys.set(vec![0, 1]);
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 1);
    let handle = state.precompose(7, live_content(&live));
    assert_eq!(state.reusable_count(), 0);
    assert_eq!(handle.placeables_count(), 1);
    assert_eq!(live.get(), 3);
    handle.dispose();
    assert_eq!(state.reusable_count(), 1);
    assert_eq!(live.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reusedCompositionResetsRememberedObject
#[test]
fn a_reused_composition_remembers_anew() {
    let keys = mutable_state_of(vec![0]);
    let made = Rc::new(Cell::new(0));
    let m = made.clone();
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let k = keys.handle();
    let policy = stack_policy(move |scope| {
        let m = m.clone();
        k.get()
            .into_iter()
            .map(|key| {
                let m = m.clone();
                scope.subcompose(
                    key,
                    Callback::new(move || {
                        let m = m.clone();
                        remember(move || m.set(m.get() + 1));
                    }),
                )
            })
            .collect()
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!(made.get(), 1);
    keys.set(Vec::new());
    ui.wait_for_idle();
    keys.set(vec![1]);
    ui.wait_for_idle();
    assert_eq!(made.get(), 2);
}

static DYNAMIC: CompositionLocal<i32> = CompositionLocal::new("subcompose.dynamic", || 0);
static STATIC: CompositionLocal<i32> = CompositionLocal::new_static("subcompose.static", || 0);

fn local_change_reaches(local: &'static CompositionLocal<i32>) {
    let (value, seen) = (mutable_state_of(1), Rc::new(Cell::new(0)));
    let s = seen.clone();
    let content = Callback::new(move || s.set(local.current()));
    let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
    let v = value.handle();
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        CompositionLocalProvider(local.provides(v.get()), move || {
            SubcomposeLayout!(measure_policy = policy.clone());
        });
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), 1);
    value.set(2);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, compositionLocalChangeInMainCompositionRecomposesSubcomposition
#[test]
fn a_dynamic_local_above_reaches_a_slot() {
    local_change_reaches(&DYNAMIC);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, staticCompositionLocalChangeInMainCompositionRecomposesSubcomposition
#[test]
fn a_static_local_above_reaches_a_slot() {
    local_change_reaches(&STATIC);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, premeasuringAllowsToSkipMeasureOnceTheSlotIsComposed
#[test]
fn a_premeasured_slot_is_measured_when_its_measurement_takes_it() {
    let (state, keys, measures) =
        (SubcomposeLayoutState::new(), mutable_state_of(Vec::<i32>::new()), Rc::new(Cell::new(0)));
    let m = measures.clone();
    let content = Callback::new(move || {
        let m = m.clone();
        Layout!(
            measure_policy = measure_policy(move |scope, _, _| {
                m.set(m.get() + 1);
                scope.layout(10, 10)
            }),
            || {}
        );
    });
    let (k, c) = (keys.handle(), content.clone());
    let policy = stack_policy(move |scope| {
        k.get().into_iter().map(|key| scope.subcompose(key, c.clone())).collect()
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let handle = state.precompose(1, content);
    let constraints = Constraints::fixed(10, 10);
    handle.premeasure(0, constraints);
    assert_eq!(measures.get(), 1);
    assert_eq!(handle.get_size(0), IntSize::new(10, 10));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, premeasuringIncorrectIndexesCrashes
#[test]
#[should_panic(expected = "out of bound")]
fn premeasuring_an_index_it_doesnt_have_panics() {
    let state = SubcomposeLayoutState::new();
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(
            st.clone(),
            measure_policy = subcompose_policy(|scope, _| scope.layout(10, 10))
        );
    });
    ui.wait_for_idle();
    let handle = state.precompose(1, Callback::new(sized("item".into(), 10)));
    handle.premeasure(1, Constraints::fixed(10, 10));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, composingTheSameKeyTwiceIsNotAllowed
#[test]
#[should_panic(expected = "subcompose the same slot id twice")]
fn a_key_composed_twice_in_a_measurement_panics() {
    let mut ui = compose_test(|| {
        SubcomposeLayout!(
            measure_policy = subcompose_policy(|scope, _| {
                scope.subcompose(1, Callback::new(|| {}));
                scope.subcompose(1, Callback::new(|| {}));
                scope.layout(10, 10)
            })
        );
    });
    ui.wait_for_idle();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reusingWithNestedSubcomposeLayoutInside
#[test]
fn a_slot_can_hold_a_subcompose_layout() {
    let (value, seen) = (mutable_state_of(1), Rc::new(Cell::new(0)));
    let (v, s) = (value.handle(), seen.clone());
    let inner = Callback::new(move || s.set(v.get()));
    let outer = Callback::new(move || {
        let inner = inner.clone();
        SubcomposeLayout!(
            measure_policy = stack_policy(move |scope| vec![scope.subcompose((), inner.clone())])
        );
    });
    let policy = stack_policy(move |scope| vec![scope.subcompose((), outer.clone())]);
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    assert_eq!(seen.get(), 1);
    value.set(2);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 2);
}

/// An effect in a slot's pass runs in the measurement that lent the tree: a coordinates query there
/// finds it busy, with no panic (D1211); the frame after, it answers.
#[test]
fn a_query_in_a_slots_effect_finds_the_tree_busy() {
    let coordinates: Rc<RefCell<Option<compose_ui::LayoutCoordinates>>> = Rc::default();
    let busy_seen = Rc::new(Cell::new(None));
    let (c, b) = (coordinates.clone(), busy_seen.clone());
    let content = Callback::new(move || {
        let (c, b) = (c.clone(), b.clone());
        let holder = c.clone();
        Box!(
            modifier = Modifier.size((10.dp(), 10.dp())).on_placed(
                move |coordinates: compose_ui::LayoutCoordinates| {
                    *holder.borrow_mut() = Some(coordinates)
                }
            ),
            {}
        );
        SideEffect(move || {
            if let Some(coordinates) = *c.borrow() {
                b.set(Some(coordinates.is_busy()));
            }
        });
    });
    let tick = mutable_state_of(0);
    let t = tick.handle();
    let policy = stack_policy(move |scope| {
        t.get();
        // A new one each time, for its pass to be the measurement's.
        let content = content.clone();
        vec![scope.subcompose((), Callback::new(move || content()))]
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    tick.set(1);
    ui.wait_for_idle();
    assert_eq!(busy_seen.get(), Some(true), "its effect runs in the measurement's lend");
    let coordinates = (*coordinates.borrow()).expect("placed");
    assert!(coordinates.is_attached());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, subcomposeCoordinates
#[test]
fn its_placement_block_has_coordinates() {
    let attached = Rc::new(Cell::new(None));
    let a = attached.clone();
    let mut ui = compose_test(move || {
        let a = a.clone();
        SubcomposeLayout!(
            measure_policy = subcompose_policy(move |scope, constraints| {
                let a = a.clone();
                scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
                    a.set(placement.coordinates().map(|coordinates| coordinates.is_attached()));
                })
            })
        );
    });
    ui.wait_for_idle();
    assert_eq!(attached.get(), Some(true));
}

/// Not a test of Compose's: what's deactivated goes from the semantics, and comes back.
#[test]
fn what_s_kept_for_reuse_isnt_in_the_semantics() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let shown = mutable_state_of(true);
    let s = shown.handle();
    let content = Callback::new(sized("kept".into(), 10));
    let policy = stack_policy(move |scope| {
        if s.get() { vec![scope.subcompose((), content.clone())] } else { Vec::new() }
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("kept").assert_exists();
    shown.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("kept").assert_does_not_exist();
    assert_eq!(state.reusable_count(), 1);
    shown.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("kept").assert_exists();
}

// ---- BoxWithConstraints -----------------------------------------------------------------------

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/BoxWithConstraintsTest.kt, withConstraintCallbackIsNotExecutedWithInnerRecompositions
#[test]
fn its_content_doesnt_run_for_what_recomposes_inside_it() {
    #[composable]
    fn Inner(model: MutableState<i32>, runs: Rc<Cell<i32>>) {
        model.get();
        runs.set(runs.get() + 1);
    }
    let (model, outer, inner) = (mutable_state_of(0), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (m, o, i) = (model.handle(), outer.clone(), inner.clone());
    let mut ui = compose_test(move || {
        let (o, i) = (o.clone(), i.clone());
        BoxWithConstraints!(move |_| {
            o.set(o.get() + 1);
            Inner!(m, i.clone());
        });
    });
    ui.wait_for_idle();
    model.set(1);
    ui.wait_for_idle();
    assert_eq!((outer.get(), inner.get()), (1, 2));
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/BoxWithConstraintsTest.kt, updateConstraintsRecomposingWithConstraints
#[test]
fn new_constraints_compose_its_content() {
    let (size, seen, runs) =
        (mutable_state_of(50), Rc::new(Cell::new(None)), Rc::new(Cell::new(0)));
    let (sz, s, r) = (size.handle(), seen.clone(), runs.clone());
    let mut ui = compose_test(move || {
        let (s, r) = (s.clone(), r.clone());
        let side = (sz.get() as f32).dp();
        Box!(modifier = Modifier.size((side, side)), {
            let (s, r) = (s.clone(), r.clone());
            BoxWithConstraints!(move |scope: BoxWithConstraintsScope| {
                s.set(Some(scope.constraints));
                r.set(r.get() + 1);
            });
        });
    });
    ui.wait_for_idle();
    let first = seen.get().expect("composed");
    assert_eq!(runs.get(), 1, "composed once for its constraints");
    size.set(100);
    ui.wait_for_idle();
    assert_ne!(seen.get(), Some(first));
    assert_eq!(runs.get(), 2);
}

// Compose: compose/foundation/foundation-layout/src/androidDeviceTest/kotlin/androidx/compose/foundation/layout/BoxWithConstraintsTest.kt, allTheStepsCalledExactlyOnce
#[test]
fn each_step_runs_exactly_once() {
    let (composed, measured) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (c, m) = (composed.clone(), measured.clone());
    let mut ui = compose_test(move || {
        let (c, m) = (c.clone(), m.clone());
        BoxWithConstraints!(move |_| {
            c.set(c.get() + 1);
            let m = m.clone();
            Layout!(
                measure_policy = measure_policy(move |scope, _, _| {
                    m.set(m.get() + 1);
                    scope.layout(100, 100)
                }),
                || {}
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!((composed.get(), measured.get()), (1, 1));
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidMovableContentTests.kt, testMovableContentParameterInBoxWithConstraints
#[test]
fn movable_content_moves_in_and_out_of_a_box_with_constraints() {
    let state = mutable_state_of(false);
    let (last_seen, made) = (Rc::new(Cell::new(None)), Rc::new(Cell::new(0)));
    let (l, m) = (last_seen.clone(), made.clone());
    let content = movable_content_of_with(move |parameter: bool| {
        let m = m.clone();
        remember(move || m.set(m.get() + 1));
        l.set(Some(parameter));
        Box!(modifier = Modifier.size((10.dp(), 10.dp())).test_tag("moved"));
    });
    let s = state.handle();
    let mut ui = compose_test(move || {
        let content = content.clone();
        key(s.get(), move || {
            if s.get() {
                content(true);
            } else {
                let content = content.clone();
                BoxWithConstraints!(move |_| content(false));
            }
        });
    });
    ui.wait_for_idle();
    for round in 0..4 {
        assert_eq!(last_seen.get(), Some(state.get()), "round {round}");
        ui.on_node_with_tag("moved").assert_exists();
        state.update(|state| !state);
        ui.wait_for_idle();
    }
    assert_eq!(made.get(), 1, "the one state");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, subcomposeLayoutInsideMovableContent_compositionIsNotDisposed
#[test]
fn a_subcompose_layout_in_movable_content_keeps_its_slots() {
    let (state, live) = (mutable_state_of(false), Rc::new(Cell::new(0)));
    let slot = live_content(&live);
    let content = movable_content_of(move || {
        let slot = slot.clone();
        SubcomposeLayout!(
            measure_policy = stack_policy(move |scope| vec![scope.subcompose((), slot.clone())])
        );
    });
    let s = state.handle();
    let mut ui = compose_test(move || {
        let content = content.clone();
        Box!({
            let (first, second) = (content.clone(), content.clone());
            key(s.get(), move || {
                if s.get() {
                    first();
                }
            });
            Box!({
                let second = second.clone();
                key(s.get(), move || {
                    if !s.get() {
                        second();
                    }
                });
            });
        });
    });
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
    state.set(true);
    ui.wait_for_idle();
    assert_eq!(live.get(), 1, "its slot's composition stays");
}

/// The layouts of the slots, in the tree, in their order: what's added to the first comes before
/// the second's, and a reorder moves them.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, slotsAreDrawnInTheOrderTheyComposed
#[test]
fn slots_are_in_the_tree_in_the_order_they_compose() {
    let (more, first_up) = (mutable_state_of(false), mutable_state_of(true));
    let (m, f) = (more.handle(), first_up.handle());
    let a = Callback::new(move || {
        sized("a".into(), 10)();
        if m.get() {
            sized("more".into(), 10)();
        }
    });
    let b = Callback::new(sized("b".into(), 10));
    let policy = stack_policy(move |scope| {
        if f.get() {
            vec![scope.subcompose("a", a.clone()), scope.subcompose("b", b.clone())]
        } else {
            vec![scope.subcompose("b", b.clone()), scope.subcompose("a", a.clone())]
        }
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    // Placed down in the order they compose: the tree's children go down too.
    let tops = |ui: &ComposeTestRule| {
        let dump = ui.dump_tree();
        let children =
            dump.lines().filter(|line| line.starts_with("    ") && !line.starts_with("     "));
        children
            .map(|line| {
                let position = &line[line.find('[').expect("a position") + 1..];
                let (_, y) = position.split_once(',').expect("x,y");
                y.split_whitespace().next().expect("y").parse::<i32>().expect("a number")
            })
            .collect::<Vec<_>>()
    };
    let in_order = |tops: Vec<i32>, count: usize| {
        assert_eq!(tops.len(), count, "{tops:?}");
        assert!(tops.is_sorted(), "{tops:?}");
    };
    in_order(tops(&ui), 2);
    more.set(true);
    ui.wait_for_idle();
    in_order(tops(&ui), 3);
    first_up.set(false);
    ui.wait_for_idle();
    in_order(tops(&ui), 3);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposeOverReusedNodeWithUpdatedModifierIsNotCausingEarlyRemeasureForIt
#[test]
fn precomposing_over_what_was_kept_measures_nothing_again() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(2));
    let (keys, measures, live) =
        (mutable_state_of(vec![0, 1, 2]), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (k, m, l) = (keys.handle(), measures.clone(), live.clone());
    let policy = stack_policy(move |scope| {
        m.set(m.get() + 1);
        k.get().into_iter().map(|key| scope.subcompose(key, live_content(&l))).collect()
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    keys.set(vec![0]);
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 2);
    let measured = measures.get();
    let handle = state.precompose(7, live_content(&live));
    ui.wait_for_idle();
    assert_eq!(measures.get(), measured);
    assert_eq!(handle.placeables_count(), 1);
}

// ---- What's deactivated, and policies ---------------------------------------------------------

/// A slot's first layout, measured, as the file's `measure` helper.
fn measure_slot(
    scope: &mut SubcomposeMeasureScope<'_, '_>,
    slot: impl PartialEq + 'static,
    constraints: Constraints,
    content: Callback,
) -> Placeable {
    let first = scope.subcompose(slot, content).into_iter().next().expect("a layout");
    scope.measure(first, constraints)
}

fn add_counter(counter: &Rc<Cell<i32>>) -> Modifier {
    let counter = counter.clone();
    Modifier.layout(move |scope, measurable, constraints| {
        counter.set(counter.get() + 1);
        let placeable = scope.measure(measurable, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    })
}

/// A box holding a nested layout whose slot is a sized box counting its measurements and an effect
/// saying its slot of the outer layout is active, till it's disposed.
fn nested_counted_content(
    counting: &Modifier,
    active: &Rc<RefCell<Vec<i32>>>,
    slot: impl Fn() -> i32 + 'static,
) -> Callback {
    let modifier = Modifier.size((10.dp(), 10.dp())).then(counting.clone());
    let (active, slot) = (active.clone(), Rc::new(slot));
    let inner = Callback::new(move || {
        Box!(modifier = modifier.clone());
        let (active, slot) = (active.clone(), slot.clone());
        DisposableEffect((), move |scope| {
            let captured = slot();
            active.borrow_mut().push(captured);
            let active = active.clone();
            scope.on_dispose(move || active.borrow_mut().retain(|it| *it != captured))
        });
    });
    Callback::new(move || {
        let inner = inner.clone();
        Box!(|_| {
            let inner = inner.clone();
            SubcomposeLayout!(
                measure_policy = subcompose_policy(move |scope, constraints| {
                    let placeable = measure_slot(scope, (), constraints, inner.clone());
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                })
            );
        });
    })
}

/// What a test lets go of when it goes: `on_dispose` runs `f` with what it was given.
fn on_dispose(f: impl Fn() + 'static) {
    let f = Rc::new(f);
    DisposableEffect((), move |scope| scope.on_dispose(move || f()));
}

/// The items composed and measured, each placed so that the semantics hold it, where Compose's
/// hold what's not placed too.
fn items_scene(
    items: &[i32],
    policy: impl SubcomposeSlotReusePolicy,
) -> (ComposeTestRule, Rc<SubcomposeLayoutState>, OwnedState<Vec<i32>>) {
    let items = mutable_state_of(items.to_vec());
    let state = SubcomposeLayoutState::with_policy(policy);
    let i = items.handle();
    let measure = subcompose_policy(move |scope, constraints| {
        for item in i.get() {
            let content = Callback::new(move || {
                Box!(modifier = Modifier.fill_max_size().test_tag(item.to_string()));
            });
            for measurable in scope.subcompose(item, content) {
                let placeable = scope.measure(measurable, constraints);
                scope.place(&placeable, 0, 0);
            }
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = measure.clone())
    });
    (ui, state, items)
}

fn assert_nodes(ui: &mut ComposeTestRule, active: &[i32], disposed: &[i32]) {
    for item in active {
        ui.on_node_with_tag(&item.to_string()).assert_exists();
    }
    for item in disposed {
        ui.on_node_with_tag(&item.to_string()).assert_does_not_exist();
    }
}

fn ids(slots: &SlotIdsSet) -> Vec<i32> {
    slots.iter().map(|id| *id.downcast_ref::<i32>().expect("an item")).collect()
}

fn is_odd(id: &dyn std::any::Any) -> bool {
    id.downcast_ref::<i32>().expect("an item") % 2 == 1
}

/// A policy of two functions, as the tests' object expressions.
struct PolicyOf<R, C>(R, C);

impl<R, C> SubcomposeSlotReusePolicy for PolicyOf<R, C>
where
    R: Fn(&mut SlotIdsSet) + 'static,
    C: Fn(&dyn std::any::Any, &dyn std::any::Any) -> bool + 'static,
{
    fn get_slots_to_retain(&self, slots: &mut SlotIdsSet) {
        (self.0)(slots)
    }

    fn are_compatible(&self, slot: &dyn std::any::Any, reusable: &dyn std::any::Any) -> bool {
        (self.1)(slot, reusable)
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, customPolicy_retainingExactItem
#[test]
fn a_policy_can_keep_one_exact_slot() {
    let policy = PolicyOf(
        |slots: &mut SlotIdsSet| {
            assert_eq!(ids(slots), [1, 2, 4]);
            slots.remove(1);
            slots.remove(4);
        },
        |_: &dyn std::any::Any, reusable: &dyn std::any::Any| {
            assert_eq!(reusable.downcast_ref::<i32>(), Some(&2));
            true
        },
    );
    let (mut ui, _, items) = items_scene(&[0, 1, 2, 3, 4], policy);
    ui.wait_for_idle();
    items.set(vec![0, 3]);
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[0, 3], &[1, 2, 4]);
    items.set(vec![0, 3, 5]);
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[0, 3, 5], &[1, 2, 4]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, customPolicy_lastUsedItemsAreFirstInSet
#[test]
fn what_was_last_used_is_first_for_a_policy() {
    let expected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let e = expected.clone();
    let policy = PolicyOf(
        move |slots: &mut SlotIdsSet| assert_eq!(ids(slots), *e.borrow()),
        |_: &dyn std::any::Any, _: &dyn std::any::Any| true,
    );
    let (mut ui, _, items) = items_scene(&[0, 1, 2, 3, 4], policy);
    ui.wait_for_idle();
    *expected.borrow_mut() = vec![1, 2, 3, 4];
    items.set(vec![0]);
    ui.wait_for_idle();
    *expected.borrow_mut() = vec![1, 2, 3];
    items.set(vec![0, 4]);
    ui.wait_for_idle();
    *expected.borrow_mut() = vec![4, 1, 2, 3];
    items.set(vec![0]);
    ui.wait_for_idle();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, customPolicy_disposedPrefetchedItemIsFirstInSet
#[test]
fn a_precomposed_slot_disposed_is_first_for_a_policy() {
    let expected: Rc<RefCell<Vec<i32>>> = Rc::default();
    let called = Rc::new(Cell::new(false));
    let compatible: Rc<Cell<(i32, i32)>> = Rc::default();
    let (e, c, k) = (expected.clone(), called.clone(), compatible.clone());
    let policy = PolicyOf(
        move |slots: &mut SlotIdsSet| {
            c.set(true);
            assert_eq!(ids(slots), *e.borrow());
        },
        move |slot: &dyn std::any::Any, reusable: &dyn std::any::Any| {
            let pair = (
                *slot.downcast_ref::<i32>().expect("an item"),
                *reusable.downcast_ref::<i32>().expect("an item"),
            );
            assert_eq!(pair, k.get());
            true
        },
    );
    let (mut ui, state, items) = items_scene(&[0, 1, 2, 3, 4], policy);
    ui.wait_for_idle();
    *expected.borrow_mut() = vec![1, 2, 3, 4];
    items.set(vec![0]);
    ui.wait_for_idle();
    assert!(called.replace(false));
    compatible.set((5, 4));
    let handle = state.precompose(5, Callback::new(|| {})); // Slot 4 is what's reused.
    *expected.borrow_mut() = vec![5, 1, 2, 3];
    handle.dispose();
    assert!(called.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, customPolicy_retainingOddNumbers
#[test]
fn a_policy_can_keep_odd_slots_for_odd_slots() {
    let policy = PolicyOf(
        |slots: &mut SlotIdsSet| slots.remove_all(|id| !is_odd(id)),
        |slot: &dyn std::any::Any, reusable: &dyn std::any::Any| is_odd(slot) && is_odd(reusable),
    );
    let (mut ui, state, items) = items_scene(&[0, 1, 2, 3, 4, 5, 6], policy);
    ui.wait_for_idle();
    items.set(Vec::new());
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[], &[0, 2, 4, 6]);
    assert_eq!(state.reusable_count(), 3);
    // New slots for 8 and 10, and 5 is reused for 9.
    items.set(vec![8, 9, 10]);
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[8, 9, 10], &[5]);
    assert_eq!(state.reusable_count(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, customPolicy_reusingSecondSlotFromTheEnd
#[test]
fn a_new_slot_can_take_the_second_from_the_end() {
    let retained: Rc<RefCell<Vec<i32>>> = Rc::default();
    let r = retained.clone();
    let policy = PolicyOf(
        move |slots: &mut SlotIdsSet| *r.borrow_mut() = ids(slots),
        |slot: &dyn std::any::Any, reusable: &dyn std::any::Any| is_odd(slot) == is_odd(reusable),
    );
    let (mut ui, state, items) = items_scene(&[0, 1, 2, 3], policy);
    ui.wait_for_idle();
    items.set(Vec::new());
    ui.wait_for_idle();
    let kept = state.reusable_count();
    // Slot 2 is reused, the others staying.
    items.set(vec![10]);
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[10], &[2]);
    assert_eq!(state.reusable_count(), kept - 1);
    assert_eq!(*retained.borrow(), [0, 1, 3]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, deactivatingOnDetachedView
/// There's no view to detach here: what's left is a measurement forced outside of a frame, finding
/// its slot's composition deactivated when it returns.
#[test]
fn a_slot_out_of_a_forced_measurement_is_deactivated_at_once() {
    let (emit, composed) = (Rc::new(Cell::new(true)), Rc::new(Cell::new(false)));
    let remeasurer: Rc<Cell<Option<Remeasurement>>> = Rc::default();
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (e, c) = (emit.clone(), composed.clone());
    let content = Callback::new(move || {
        let c = c.clone();
        DisposableEffect((), move |scope| {
            c.set(true);
            scope.on_dispose(move || c.set(false))
        });
    });
    let policy = subcompose_policy(move |scope, _| {
        if e.get() {
            scope.subcompose((), content.clone());
        }
        scope.layout(10, 10)
    });
    let r = remeasurer.clone();
    let modifier: Rc<dyn RemeasurementModifier> = Rc::new(move |it| r.set(Some(it)));
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(
            state.clone(),
            modifier = Modifier.remeasurement_modifier(modifier.clone()),
            measure_policy = policy.clone()
        )
    });
    ui.wait_for_idle();
    assert!(composed.get());
    emit.set(false);
    remeasurer.get().expect("attached").force_remeasure();
    ui.wait_for_idle();
    assert!(!composed.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, deactivatingDeeplyNestedLayoutDoesNotCauseRemeasure
#[test]
fn what_a_deactivated_slot_nests_isnt_measured_again() {
    let shown = mutable_state_of(true);
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let content = Callback::new(move || {
        Box!(|_| {
            let disposed = remember(|| mutable_state_of(false));
            on_dispose(move || disposed.set(true));
            Box!(
                modifier = Modifier.layout(move |scope, measurable, constraints| {
                    assert!(!disposed.get());
                    let placeable = scope.measure(measurable, constraints);
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                })
            );
        });
    });
    let s = shown.handle();
    let policy =
        stack_policy(
            move |scope| {
                if s.get() { vec![scope.subcompose(0, content.clone())] } else { Vec::new() }
            },
        );
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(
            state.clone(),
            modifier = Modifier.fill_max_size(),
            measure_policy = policy.clone()
        )
    });
    ui.wait_for_idle();
    shown.set(false);
    ui.wait_for_idle();
    shown.set(true);
    ui.wait_for_idle();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reusingNestedSubcompose_nestedChildrenAreResetAndReused
#[test]
fn a_nested_layout_s_slots_are_reset_and_reused() {
    let slot_state = mutable_state_of(0);
    let active: Rc<RefCell<Vec<i32>>> = Rc::default();
    let measures = Rc::new(Cell::new(0));
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (s, a, m) = (slot_state.handle(), active.clone(), add_counter(&measures));
    // A content per slot, as the test's lambda capturing it.
    let contents: Rc<RefCell<Vec<Callback>>> = Rc::default();
    let policy = subcompose_policy(move |scope, constraints| {
        let slot = s.get();
        let mut contents = contents.borrow_mut();
        while contents.len() <= slot as usize {
            let at = contents.len() as i32;
            contents.push(nested_counted_content(&m, &a, move || at));
        }
        let child = measure_slot(scope, slot, constraints, contents[slot as usize].clone());
        scope.place(&child, 0, 0);
        scope.layout(child.width, child.height)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!(*active.borrow(), [0]);
    slot_state.set(1);
    ui.wait_for_idle();
    assert_eq!(*active.borrow(), [1]);
    assert_eq!(measures.get(), 2);
    measures.set(0);
    slot_state.set(2);
    ui.wait_for_idle();
    assert_eq!(*active.borrow(), [2]);
    assert_eq!(measures.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reusingNestedSubcompose_nestedContentIsResetWhenReusedOnNextFrame
#[test]
fn a_nested_layout_s_content_is_reset_when_it_s_reused_a_frame_later() {
    let (content_active, slot_id) = (mutable_state_of(true), mutable_state_of(0));
    let active: Rc<RefCell<Vec<i32>>> = Rc::default();
    let measures = Rc::new(Cell::new(0));
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (c, s) = (content_active.handle(), slot_id.handle());
    let nested = nested_counted_content(&add_counter(&measures), &active, move || s.get());
    let content = Callback::new(move || {
        nested();
        // When the child's reset, a measurement and a composition are asked for.
        on_dispose(move || {
            c.set(true);
            s.set(s.get() + 1);
        });
    });
    let (c, s) = (content_active.handle(), slot_id.handle());
    let policy = subcompose_policy(move |scope, constraints| {
        if c.get() {
            let child = measure_slot(scope, s.get(), constraints, content.clone());
            scope.place(&child, 0, 0);
            scope.layout(child.width, child.height)
        } else {
            scope.layout(0, 0)
        }
    });
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(state.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!(*active.borrow(), [0]);
    content_active.set(false);
    ui.wait_for_idle();
    assert_eq!(*active.borrow(), [1]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, slotIsProperlyDeactivatedAfterUpdatingReusePolicy
#[test]
fn a_slot_is_deactivated_when_the_policy_changes() {
    let states = [
        SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)),
        SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)),
    ];
    let state = mutable_state_of(0);
    let (should_compose, disposed) = (mutable_state_of(true), Rc::new(Cell::new(false)));
    let d = disposed.clone();
    let content = Callback::new(move || {
        let d = d.clone();
        on_dispose(move || d.set(true));
    });
    let c = should_compose.handle();
    let policy = stack_policy(move |scope| {
        if c.get() { vec![scope.subcompose((), content.clone())] } else { Vec::new() }
    });
    let st = state.handle();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(states[st.get()].clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    state.set(1);
    should_compose.set(false);
    ui.wait_for_idle();
    assert!(disposed.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, updatingModifierOnDetachedSubcomposition
#[test]
fn a_deactivated_slot_s_modifier_can_change() {
    let (active, tall) = (mutable_state_of(true), mutable_state_of(false));
    let t = tall.handle();
    let content = Callback::new(move || {
        let modifier = if t.get() { Modifier.test_tag("drawn") } else { Modifier };
        Layout!(
            modifier = modifier,
            measure_policy = measure_policy(|scope, _, _| scope.layout(10, 10)),
            || {}
        );
    });
    let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
    let a = active.handle();
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        ReusableContentHost(a.get(), move || {
            SubcomposeLayout!(measure_policy = policy.clone());
        });
    });
    ui.wait_for_idle();
    active.set(false);
    tall.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("drawn").assert_does_not_exist();
    active.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("drawn").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposeOnDetachedStateIsNoOp
#[test]
fn precomposing_with_no_layout_there_does_nothing() {
    let need = mutable_state_of(true);
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (n, st) = (need.handle(), state.clone());
    let policy = subcompose_policy(|scope, _| scope.layout(10, 10));
    let mut ui = compose_test(move || {
        if n.get() {
            SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone());
        }
    });
    ui.wait_for_idle();
    need.set(false);
    ui.wait_for_idle();
    let handle = state.precompose((), Callback::new(sized("never".into(), 10)));
    assert_eq!(handle.placeables_count(), 0);
    ui.on_node_with_tag("never").assert_does_not_exist();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, deactivatedNodesAreNotPartOfChildrenSemantics
#[test]
fn what_s_deactivated_isnt_a_child_in_the_semantics() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let need = mutable_state_of(true);
    let content = Callback::new(|| {
        Box!(modifier = Modifier.test_tag("child"));
    });
    let n = need.handle();
    let policy = stack_policy(move |scope| {
        if n.get() { vec![scope.subcompose((), content.clone())] } else { Vec::new() }
    });
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(
            state.clone(),
            modifier = Modifier.test_tag("layout"),
            measure_policy = policy.clone()
        )
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("layout").on_children().assert_count_equals(1);
    need.set(false);
    ui.wait_for_idle();
    ui.on_node_with_tag("layout").on_children().assert_count_equals(0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, schedulingRecompositionOnDeactivatingChildIsNotCausingRecomposition
#[test]
fn a_change_read_by_a_slot_being_deactivated_doesnt_compose_it() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (counter, add_slot) = (mutable_state_of(0), mutable_state_of(true));
    let in_slot = Rc::new(Cell::new(0));
    let (c, i) = (counter.handle(), in_slot.clone());
    let content = Callback::new(move || i.set(c.get()));
    let (c, a) = (counter.handle(), add_slot.handle());
    let policy = subcompose_policy(move |scope, _| {
        if a.get() {
            scope.subcompose((), content.clone());
        } else {
            c.set(1);
            compose_runtime::snapshots::Snapshot::send_apply_notifications();
        }
        scope.layout(10, 10)
    });
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(state.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    add_slot.set(false);
    ui.wait_for_idle();
    assert_eq!(in_slot.get(), 0);
}

/// Not a test of Compose's: a layout deactivated around its slots gives them back when it's shown
/// again, and they go with it when it's removed while deactivated.
#[test]
fn a_deactivated_layout_gets_its_slots_back_or_they_go_with_it() {
    let (active, there, live) =
        (mutable_state_of(true), mutable_state_of(true), Rc::new(Cell::new(0)));
    let content = live_content(&live);
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let st = state.clone();
    let policy = stack_policy(move |scope| vec![scope.subcompose(1, content.clone())]);
    let (a, t) = (active.handle(), there.handle());
    let mut ui = compose_test(move || {
        if t.get() {
            let (policy, st) = (policy.clone(), st.clone());
            ReusableContentHost(a.get(), move || {
                SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone());
            });
        }
    });
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
    active.set(false);
    ui.wait_for_idle();
    assert_eq!(live.get(), 0, "deactivated with it");
    assert_eq!(state.reusable_count(), 1, "and kept");
    active.set(true);
    ui.wait_for_idle();
    assert_eq!(live.get(), 1, "taken back");
    assert_eq!(state.reusable_count(), 0);
    active.set(false);
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 1);
    there.set(false);
    ui.wait_for_idle();
    assert_eq!(live.get(), 0);
    let handle = state.precompose(2, Callback::new(|| {}));
    assert_eq!(handle.placeables_count(), 0, "no layout there");
}

static FOUND: TraverseKey = TraverseKey::new("found");

#[derive(Clone, Copy, PartialEq)]
struct FoundElement(i32);

struct FoundNode(i32);

impl ModifierNodeElement for FoundElement {
    type Node = FoundNode;
    fn create(&self) -> FoundNode {
        FoundNode(self.0)
    }
    fn update(&self, _: &mut FoundNode) {}
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ModifierNode for FoundNode {
    fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
        Some(self)
    }
}

impl TraversableNode for FoundNode {
    fn traverse_key(&self) -> &'static TraverseKey {
        &FOUND
    }
}

/// Not a test of Compose's: `LazyLayoutPrefetchState`'s use of `traverseDescendants`, what's found
/// and what its answer skips and cancels. A slot it doesn't hold finds nothing.
#[test]
fn a_precomposed_slot_finds_what_s_under_it() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let st = state.clone();
    let policy = subcompose_policy(|scope, _| scope.layout(10, 10));
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let content = Callback::new(|| {
        Box!(modifier = Modifier.then_custom(FoundElement(1)), |_| {
            Box!(modifier = Modifier.then_custom(FoundElement(2)));
        });
        Box!(modifier = Modifier.then_custom(FoundElement(3)));
    });
    let handle = state.precompose(1, content);
    let found = |answer: fn(i32) -> TraverseDescendantsAction| {
        let mut found = Vec::new();
        handle.traverse_descendants(&FOUND, |node, _| {
            let at = node.downcast_ref::<FoundNode>().expect("a found node").0;
            found.push(at);
            answer(at)
        });
        found
    };
    assert_eq!(found(|_| TraverseDescendantsAction::ContinueTraversal), [1, 2, 3]);
    assert_eq!(found(|_| TraverseDescendantsAction::SkipSubtreeAndContinueTraversal), [1, 3]);
    assert_eq!(found(|_| TraverseDescendantsAction::CancelTraversal), [1]);
    handle.dispose();
    assert_eq!(found(|_| TraverseDescendantsAction::ContinueTraversal), Vec::<i32>::new());
}

// ---- Paused precomposition --------------------------------------------------------------------

/// A layout whose slot is measured when `add_slot` says, `content` counting its compositions and
/// their applications.
type PausedScene = (
    ComposeTestRule,
    Rc<SubcomposeLayoutState>,
    OwnedState<bool>,
    Callback,
    Rc<Cell<i32>>,
    Rc<Cell<i32>>,
);

fn paused_scene() -> PausedScene {
    let (add_slot, composing, applied) =
        (mutable_state_of(false), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (c, a) = (composing.clone(), applied.clone());
    let content = Callback::new(move || {
        c.set(c.get() + 1);
        let a = a.clone();
        SideEffect(move || a.set(a.get() + 1));
    });
    let state = SubcomposeLayoutState::new();
    let (add, slot, st) = (add_slot.handle(), content.clone(), state.clone());
    let policy = subcompose_policy(move |scope, _| {
        if add.get() {
            scope.subcompose((), slot.clone());
        }
        scope.layout(10, 10)
    });
    let ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    (ui, state, add_slot, content, composing, applied)
}

fn resume_until_completed(precomposition: &PausedPrecomposition) {
    while !precomposition.is_complete() {
        precomposition.resume(|| false);
    }
}

/// A sized box with an effect saying it's composed. Keyed by its size, as the tests' lambdas are by
/// where they're written: each size is other content.
fn composed_box(composed: &Rc<Cell<bool>>, size: f32) -> Callback {
    let composed = composed.clone();
    Callback::new(move || {
        let composed = composed.clone();
        key(size as i32, move || {
            Box!(modifier = Modifier.size((size.dp(), size.dp())));
            let composed = composed.clone();
            DisposableEffect((), move |scope| {
                composed.set(true);
                scope.on_dispose(move || composed.set(false))
            });
        });
    })
}

/// A layout composing no slot of its own, for what's precomposed.
fn empty_layout(state: &Rc<SubcomposeLayoutState>) -> ComposeTestRule {
    let st = state.clone();
    let policy = subcompose_policy(|scope, _| scope.layout(10, 10));
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    ui
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposePaused_composeAndApply
#[test]
fn a_paused_slot_composes_when_resumed_and_applies_when_applied() {
    let (mut ui, state, add_slot, content, composing, applied) = paused_scene();
    ui.wait_for_idle();
    assert_eq!(composing.get(), 0);
    let precomposition = state.create_paused_precomposition((), content);
    ui.wait_for_idle();
    assert_eq!(composing.get(), 0);
    resume_until_completed(&precomposition);
    assert_eq!((composing.get(), applied.get()), (1, 0));
    precomposition.apply();
    assert_eq!((composing.get(), applied.get()), (1, 1));
    ui.wait_for_idle();
    add_slot.set(true);
    ui.wait_for_idle();
    assert_eq!((composing.get(), applied.get()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposePaused_composeOnly_applyDuringRegularPhase
#[test]
fn a_composed_paused_slot_is_applied_by_its_measurement() {
    let (mut ui, state, add_slot, content, composing, applied) = paused_scene();
    ui.wait_for_idle();
    let precomposition = state.create_paused_precomposition((), content);
    resume_until_completed(&precomposition);
    ui.wait_for_idle();
    assert_eq!((composing.get(), applied.get()), (1, 0));
    add_slot.set(true);
    ui.wait_for_idle();
    assert_eq!((composing.get(), applied.get()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposePaused_pauseStraightAway_doTheRestDuringRegularPhase
#[test]
fn a_slot_paused_straight_away_is_finished_by_its_measurement() {
    let (mut ui, state, add_slot, content, composing, applied) = paused_scene();
    ui.wait_for_idle();
    let precomposition = state.create_paused_precomposition((), content);
    ui.wait_for_idle();
    assert_eq!(composing.get(), 0);
    precomposition.resume(|| true);
    ui.wait_for_idle();
    add_slot.set(true);
    ui.wait_for_idle();
    assert_eq!((composing.get(), applied.get()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, disposePrecomposedPausedItem
#[test]
fn a_cancelled_paused_slot_composes_again_in_its_measurement() {
    let (mut ui, state, add_slot, content, composing, applied) = paused_scene();
    ui.wait_for_idle();
    assert_eq!(composing.get(), 0);
    let precomposition = state.create_paused_precomposition((), content);
    resume_until_completed(&precomposition);
    assert_eq!(composing.get(), 1);
    precomposition.cancel();
    ui.wait_for_idle();
    add_slot.set(true);
    ui.wait_for_idle();
    assert_eq!((composing.get(), applied.get()), (2, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposePaused_isComplete
#[test]
fn a_paused_slot_says_it_s_complete_as_resuming_does() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let precomposition =
        state.create_paused_precomposition((), Callback::new(sized("box".into(), 100)));
    assert!(!precomposition.is_complete());
    while !precomposition.is_complete() {
        let result = precomposition.resume(|| true);
        assert_eq!(result, precomposition.is_complete());
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposePaused_applyOnNotCompletedPrecompositionThrows
#[test]
#[should_panic(expected = "a paused composition was used out of order")]
fn applying_what_s_not_complete_panics() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let precomposition =
        state.create_paused_precomposition((), Callback::new(sized("box".into(), 100)));
    assert!(!precomposition.is_complete());
    precomposition.apply();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, premeasuringAfterPrecomposePaused
#[test]
fn a_paused_slot_applied_can_be_premeasured() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let measures = Rc::new(Cell::new(0));
    let modifier = add_counter(&measures).fill_max_size();
    let content = Callback::new(move || {
        Box!(modifier = modifier.clone());
    });
    assert_eq!(measures.get(), 0);
    let precomposition = state.create_paused_precomposition((), content);
    resume_until_completed(&precomposition);
    let handle = precomposition.apply();
    assert_eq!(measures.get(), 0);
    assert_eq!(handle.placeables_count(), 1);
    handle.premeasure(0, Constraints::new(100, 100, 0, i32::MAX));
    assert_eq!(measures.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposeOnTopOfCancelledPrecomposition
#[test]
fn precomposing_over_a_cancelled_one_composes_the_new_content() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let _ui = empty_layout(&state);
    let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let precomposition = state.create_paused_precomposition((), composed_box(&first, 100.0));
    resume_until_completed(&precomposition);
    precomposition.cancel();
    let precomposition2 = state.create_paused_precomposition((), composed_box(&second, 5.0));
    resume_until_completed(&precomposition2);
    precomposition2.apply();
    assert!(!first.get());
    assert!(second.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingDifferentContentOnTop
#[test]
fn precomposing_other_content_replaces_what_was_precomposed() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    state.precompose((), composed_box(&first, 100.0));
    assert!(first.get());
    let handle = state.precompose((), composed_box(&second, 5.0));
    assert!(!first.get());
    assert!(second.get());
    handle.dispose();
    assert!(!first.get());
    assert!(!second.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingDifferentContentOnTop_paused
#[test]
fn a_paused_one_with_other_content_replaces_what_was_applied() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let precomposition = state.create_paused_precomposition((), composed_box(&first, 100.0));
    resume_until_completed(&precomposition);
    precomposition.apply();
    assert!(first.get());
    let precomposition2 = state.create_paused_precomposition((), composed_box(&second, 5.0));
    resume_until_completed(&precomposition2);
    let handle = precomposition2.apply();
    assert!(!first.get());
    assert!(second.get());
    handle.dispose();
    assert!(!first.get());
    assert!(!second.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingNotPausedOnTopOfPaused
#[test]
fn precomposing_over_an_applied_paused_one_replaces_it() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let precomposition = state.create_paused_precomposition((), composed_box(&first, 100.0));
    resume_until_completed(&precomposition);
    precomposition.apply();
    assert!(first.get());
    let handle = state.precompose((), composed_box(&second, 5.0));
    assert!(!first.get());
    assert!(second.get());
    handle.dispose();
    assert!(!first.get());
    assert!(!second.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingNotPausedOnTopOfNotAppliedPaused
#[test]
fn precomposing_the_same_content_applies_what_s_paused() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let composed = Rc::new(Cell::new(false));
    let content = composed_box(&composed, 100.0);
    let precomposition = state.create_paused_precomposition((), content.clone());
    resume_until_completed(&precomposition); // But not applying.
    assert!(!composed.get());
    let handle = state.precompose((), content);
    assert!(composed.get());
    // Nothing: another's composed over it.
    precomposition.cancel();
    assert!(composed.get());
    handle.dispose();
    assert!(!composed.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingNotPausedOnTopOfNotAppliedPaused_differentContent
#[test]
fn precomposing_other_content_cancels_what_s_paused() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let precomposition = state.create_paused_precomposition((), composed_box(&first, 100.0));
    resume_until_completed(&precomposition); // But not applying.
    let handle = state.precompose((), composed_box(&second, 5.0));
    assert!(!first.get());
    assert!(second.get());
    // Nothing: another's composed over it.
    precomposition.cancel();
    assert!(second.get());
    handle.dispose();
    assert!(!second.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingPausedOnTopOfNotAppliedPaused
#[test]
fn a_second_paused_one_with_the_same_content_is_the_first() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let composed = Rc::new(Cell::new(false));
    let content = composed_box(&composed, 100.0);
    let precomposition = state.create_paused_precomposition((), content.clone());
    resume_until_completed(&precomposition); // But not applying.
    assert!(!composed.get());
    let precomposition2 = state.create_paused_precomposition((), content);
    assert!(!composed.get());
    resume_until_completed(&precomposition2);
    let handle = precomposition2.apply();
    assert!(composed.get());
    // Both nothing: it's applied.
    precomposition.cancel();
    precomposition2.cancel();
    assert!(composed.get());
    handle.dispose();
    assert!(!composed.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposingPausedOnTopOfNotAppliedPaused_differentContent
#[test]
fn a_second_paused_one_with_other_content_cancels_the_first() {
    let state = SubcomposeLayoutState::new();
    let _ui = empty_layout(&state);
    let (first, second) = (Rc::new(Cell::new(false)), Rc::new(Cell::new(false)));
    let precomposition = state.create_paused_precomposition((), composed_box(&first, 100.0));
    resume_until_completed(&precomposition); // But not applying.
    let precomposition2 = state.create_paused_precomposition((), composed_box(&second, 5.0));
    assert!(!first.get());
    assert!(!second.get());
    resume_until_completed(&precomposition2);
    precomposition2.apply();
    assert!(!first.get());
    assert!(second.get());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, nestedDeactivateWithPausableComposition
#[test]
fn a_nested_layout_deactivated_with_a_paused_slot_comes_back_able_to_pause() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let nested_state: Rc<RefCell<Option<Rc<SubcomposeLayoutState>>>> = Rc::default();
    let has_content = mutable_state_of(true);
    let nested_content = Callback::new(|| {
        Box!(modifier = Modifier.size((10.dp(), 10.dp())));
    });
    let n = nested_state.clone();
    let content = Callback::new(move || {
        let nested =
            remember(|| SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)));
        *n.borrow_mut() = Some(nested.clone());
        SubcomposeLayoutWithState!(
            nested,
            measure_policy = subcompose_policy(|scope, _| scope.layout(10, 10))
        );
    });
    let (has, c) = (has_content.handle(), content.clone());
    let policy =
        stack_policy(
            move |scope| {
                if has.get() { vec![scope.subcompose((), c.clone())] } else { Vec::new() }
            },
        );
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    // Nested content being prefetched: composed, not applied.
    let nested = nested_state.borrow_mut().take().expect("composed");
    let paused = nested.create_paused_precomposition((), nested_content.clone());
    resume_until_completed(&paused);
    // The first layer goes, and everything in the nested content deactivates.
    has_content.set(false);
    ui.wait_for_idle();
    // The main content's prefetched, then the nested's.
    state.precompose((), content);
    let nested = nested_state.borrow_mut().take().expect("composed");
    let paused = nested.create_paused_precomposition((), nested_content);
    assert!(!paused.is_complete());
    resume_until_completed(&paused);
    paused.apply();
}

/// Not a test of Compose's: slots of a deactivated layout, one paused, come back with it, the
/// paused one's composition new.
#[test]
fn a_deactivated_layout_s_paused_slot_is_cancelled() {
    let (active, composed) = (mutable_state_of(true), Rc::new(Cell::new(false)));
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let st = state.clone();
    let policy = subcompose_policy(|scope, _| scope.layout(10, 10));
    let a = active.handle();
    let mut ui = compose_test(move || {
        let (policy, st) = (policy.clone(), st.clone());
        ReusableContentHost(a.get(), move || {
            SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone());
        });
    });
    ui.wait_for_idle();
    let content = composed_box(&composed, 10.0);
    let paused = state.create_paused_precomposition((), content.clone());
    resume_until_completed(&paused);
    active.set(false);
    ui.wait_for_idle();
    assert!(paused.is_complete(), "nothing's left to it");
    assert!(!composed.get());
    assert_eq!(state.reusable_count(), 1);
    active.set(true);
    ui.wait_for_idle();
    let handle = state.precompose(1, content);
    assert!(composed.get());
    handle.dispose();
    assert!(!composed.get());
}

// ---- A layout reused, a new state, a panic, a reorder ------------------------------------------

/// A `BoxWithConstraints` holding a tagged box.
fn box_with_constraints(tag: String) {
    BoxWithConstraints!(move |_| {
        Box!(modifier = Modifier.size((10.dp(), 10.dp())).test_tag(tag.clone()));
    });
}

/// Not a test of Compose's: an item reused on a scroll holds a `BoxWithConstraints`.
#[test]
fn a_lazy_item_s_box_with_constraints_is_reused_on_a_scroll() {
    let state = LazyListState::new(0, 0);
    let handle = *state;
    let mut ui = compose_test(move || {
        LazyColumn!(
            modifier = Modifier.size((100.dp(), 100.dp())),
            state = handle,
            |scope: LazyListScope| {
                scope.items(100, |index| box_with_constraints(index.to_string()));
            }
        );
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("0").assert_exists();
    handle.request_scroll_to_item(20, 0);
    ui.wait_for_idle();
    ui.on_node_with_tag("20").assert_exists();
    ui.on_node_with_tag("0").assert_does_not_exist();
    handle.request_scroll_to_item(0, 0);
    ui.wait_for_idle();
    ui.on_node_with_tag("0").assert_exists();
}

/// Not a test of Compose's: `ReusableContent` with its key changing around one.
#[test]
fn a_box_with_constraints_is_reused_by_a_new_key() {
    let key = mutable_state_of(0);
    let k = key.handle();
    let mut ui = compose_test(move || {
        let at = k.get();
        ReusableContent(at, move || box_with_constraints(at.to_string()));
    });
    ui.wait_for_idle();
    for at in 1..4 {
        key.set(at);
        ui.wait_for_idle();
        ui.on_node_with_tag(&at.to_string()).assert_exists();
        ui.on_node_with_tag(&(at - 1).to_string()).assert_does_not_exist();
    }
}

/// Not a test of Compose's: `ReusableContentHost` toggled around one.
#[test]
fn a_box_with_constraints_comes_back_with_its_host() {
    let active = mutable_state_of(true);
    let a = active.handle();
    let mut ui = compose_test(move || {
        ReusableContentHost(a.get(), || box_with_constraints("it".into()));
    });
    ui.wait_for_idle();
    for _ in 0..3 {
        ui.on_node_with_tag("it").assert_exists();
        active.set(false);
        ui.wait_for_idle();
        ui.on_node_with_tag("it").assert_does_not_exist();
        active.set(true);
        ui.wait_for_idle();
    }
    ui.on_node_with_tag("it").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reusingWithNestedSubcomposeLayoutInside
#[test]
fn a_nested_layout_in_a_retained_slot_is_reused() {
    let slot_state = mutable_state_of(0);
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let s = slot_state.handle();
    let inner = Callback::new(|| {
        Box!(modifier = Modifier.size((10.dp(), 10.dp())));
    });
    let policy = subcompose_policy(move |scope, constraints| {
        let slot = s.get();
        let inner = inner.clone();
        let content = Callback::new(move || {
            let inner = inner.clone();
            ReusableContent(slot, move || {
                let inner = inner.clone();
                Box!(|_| {
                    let inner = inner.clone();
                    SubcomposeLayout!(
                        modifier = Modifier.test_tag(slot.to_string()),
                        measure_policy = subcompose_policy(move |scope, constraints| {
                            let placeable = measure_slot(scope, 0, constraints, inner.clone());
                            scope.place(&placeable, 0, 0);
                            scope.layout(placeable.width, placeable.height)
                        })
                    );
                });
            });
        });
        let child = measure_slot(scope, slot, constraints, content);
        scope.place(&child, 0, 0);
        scope.layout(child.width, child.height)
    });
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(state.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    slot_state.set(1);
    ui.wait_for_idle();
    slot_state.set(2);
    ui.wait_for_idle();
    ui.on_node_with_tag("2").assert_exists();
    ui.on_node_with_tag("1").assert_does_not_exist();
    ui.on_node_with_tag("0").assert_does_not_exist();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, updatingStateWorks
#[test]
fn a_new_state_object_takes_over_the_layout() {
    let tag_state = mutable_state_of("box1".to_string());
    let t = tag_state.handle();
    let mut ui = compose_test(move || {
        let tag = t.get();
        let state = remember_keyed(tag.clone(), SubcomposeLayoutState::new);
        let content = Callback::new(sized(tag, 10));
        let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
        SubcomposeLayoutWithState!(state, measure_policy = policy);
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("box1").assert_exists();
    tag_state.set("box2".to_string());
    ui.wait_for_idle();
    ui.on_node_with_tag("box2").assert_exists();
    ui.on_node_with_tag("box1").assert_does_not_exist();
}

/// The scene of the two tests that change the state object: a slot of `slot_state`, its one box
/// counting its measurements, under the state `layout_state` picks.
fn state_change_scene(
    measures: &Rc<Cell<i32>>,
    slot_state: MutableState<i32>,
    layout_state: MutableState<usize>,
) -> ComposeTestRule {
    let states = [
        SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)),
        SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)),
    ];
    let m = measures.clone();
    let counting = Modifier.layout(move |scope, _, _| {
        m.set(m.get() + 1);
        scope.layout(10, 10)
    });
    compose_test(move || {
        let slot = slot_state.get();
        let counting = counting.clone();
        let policy = subcompose_policy(move |scope, _| {
            let counting = counting.clone();
            let content = Callback::new(move || {
                let counting = counting.clone();
                ReusableContent(slot, move || {
                    Box!(modifier = counting.clone());
                });
            });
            let placeable = measure_slot(scope, slot, Constraints::new(0, 100, 0, 100), content);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        });
        SubcomposeLayoutWithState!(states[layout_state.get()].clone(), measure_policy = policy);
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, nodesKeptAsReusableAreReusedWhenTheStateObjectChanges
#[test]
fn what_was_kept_is_reused_under_a_new_state_object() {
    let (slot_state, layout_state) = (mutable_state_of(0), mutable_state_of(0));
    let measures = Rc::new(Cell::new(0));
    let mut ui = state_change_scene(&measures, slot_state.handle(), layout_state.handle());
    ui.wait_for_idle();
    slot_state.set(1); // Slot 0 is kept for reuse.
    ui.wait_for_idle();
    assert_eq!(measures.get(), 2);
    measures.set(0);
    slot_state.set(2); // Slot 0 should be reused.
    layout_state.set(1);
    ui.wait_for_idle();
    assert_eq!(measures.get(), 0, "reused, its modifier the same");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, previouslyActiveNodesAreReusedWhenTheStateObjectChanges
#[test]
fn what_was_active_is_reused_under_a_new_state_object() {
    let (slot_state, layout_state) = (mutable_state_of(0), mutable_state_of(0));
    let measures = Rc::new(Cell::new(0));
    let mut ui = state_change_scene(&measures, slot_state.handle(), layout_state.handle());
    ui.wait_for_idle();
    assert_eq!(measures.get(), 1);
    measures.set(0);
    slot_state.set(1);
    layout_state.set(1);
    ui.wait_for_idle();
    assert_eq!(measures.get(), 0, "reused, its modifier the same");
}

/// A child wanted by `need`, under the state `which` picks of two with the policies given.
fn child_scene(
    policies: [usize; 2],
    need: MutableState<bool>,
    which: MutableState<usize>,
) -> (ComposeTestRule, [Rc<SubcomposeLayoutState>; 2]) {
    let states =
        policies.map(|max| SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(max)));
    let content = Callback::new(|| {
        Box!(modifier = Modifier.test_tag("child"));
    });
    let policy = stack_policy(move |scope| {
        if need.get() { vec![scope.subcompose((), content.clone())] } else { Vec::new() }
    });
    let given = states.clone();
    let ui = compose_test(move || {
        SubcomposeLayoutWithState!(given[which.get()].clone(), measure_policy = policy.clone())
    });
    (ui, states)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reusableNodeIsKeptAsReusableAfterStateUpdate
#[test]
fn what_s_kept_stays_kept_under_a_new_state_object() {
    let (need, which) = (mutable_state_of(true), mutable_state_of(0));
    let (mut ui, states) = child_scene([1, 1], need.handle(), which.handle());
    ui.wait_for_idle();
    ui.on_node_with_tag("child").assert_exists();
    need.set(false);
    ui.wait_for_idle();
    assert_eq!(states[0].reusable_count(), 1);
    which.set(1);
    ui.wait_for_idle();
    assert_eq!(states[1].reusable_count(), 1);
    ui.on_node_with_tag("child").assert_does_not_exist();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, passingSmallerMaxSlotsToRetainForReuse
#[test]
fn a_new_state_object_keeping_less_disposes_what_s_kept() {
    let (need, which) = (mutable_state_of(true), mutable_state_of(0));
    let (mut ui, states) = child_scene([1, 0], need.handle(), which.handle());
    ui.wait_for_idle();
    need.set(false);
    ui.wait_for_idle();
    assert_eq!(states[0].reusable_count(), 1);
    which.set(1);
    ui.wait_for_idle();
    assert_eq!(states[1].reusable_count(), 0);
}

/// Not a test of Compose's: a panic in a slot's pass leaves the layout hearing what asks it next.
#[test]
fn the_layout_responds_after_a_slot_panicked() {
    let tag = mutable_state_of(0);
    let t = tag.handle();
    let policy = stack_policy(move |scope| {
        let tag = t.get();
        let content = Callback::new(move || {
            assert_ne!(tag, 1, "the panicking slot");
            sized(tag.to_string(), 10)();
        });
        vec![scope.subcompose(tag, content)]
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    ui.on_node_with_tag("0").assert_exists();
    tag.set(1);
    let panicked =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ui.wait_for_idle())).is_err();
    assert!(panicked);
    tag.set(2);
    ui.wait_for_idle();
    ui.on_node_with_tag("2").assert_exists();
}

/// Not a test of Compose's: a reorder in a measurement leaves nothing to measure again, as its
/// `move` ignores what it asks.
#[test]
fn a_reorder_leaves_the_layout_clean() {
    let (first_up, size) = (mutable_state_of(true), mutable_state_of(10));
    let runs = Rc::new(Cell::new(0));
    let (f, r) = (first_up.handle(), runs.clone());
    let (a, b) = (Callback::new(sized("a".into(), 10)), Callback::new(sized("b".into(), 10)));
    let policy = stack_policy(move |scope| {
        r.set(r.get() + 1);
        if f.get() {
            vec![scope.subcompose("a", a.clone()), scope.subcompose("b", b.clone())]
        } else {
            vec![scope.subcompose("b", b.clone()), scope.subcompose("a", a.clone())]
        }
    });
    let s = size.handle();
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        Column!(|_| {
            SubcomposeLayout!(measure_policy = policy.clone());
            sized("sibling".into(), s.get())();
        });
    });
    ui.wait_for_idle();
    first_up.set(false);
    ui.wait_for_idle();
    assert_eq!(runs.get(), 2);
    size.set(20);
    ui.wait_for_idle();
    assert_eq!(runs.get(), 2, "a sibling's size is nothing to it");
}

/// The tags of the tree's layouts under the layout, top to bottom as placed, read by the order of
/// `dump_tree`: their tops go down with the tree's order.
fn children_tops(ui: &ComposeTestRule) -> Vec<i32> {
    ui.dump_tree()
        .lines()
        .filter(|line| line.starts_with("    ") && !line.starts_with("     "))
        .map(|line| {
            let position = &line[line.find('[').expect("a position") + 1..];
            let (_, y) = position.split_once(',').expect("x,y");
            y.split_whitespace().next().expect("y").parse::<i32>().expect("a number")
        })
        .collect()
}

/// Not a test of Compose's: `move_nodes` in the second slot moves its nodes, past the first's.
#[test]
fn a_move_in_a_later_slot_stays_in_its_slot() {
    let swapped = mutable_state_of(false);
    let s = swapped.handle();
    let first = Callback::new(|| {
        sized("a".into(), 1)();
        sized("b".into(), 2)();
    });
    let second = Callback::new(move || {
        let order = if s.get() { [4, 3] } else { [3, 4] };
        for size in order {
            key(size, move || sized(size.to_string(), size)());
        }
    });
    // Each measured and placed in the tree's order, so the tops say what the order is.
    let policy = stack_policy(move |scope| {
        vec![scope.subcompose(1, first.clone()), scope.subcompose(2, second.clone())]
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    assert_eq!(children_tops(&ui), [0, 1, 3, 6]);
    swapped.set(true);
    ui.wait_for_idle();
    assert_eq!(children_tops(&ui), [0, 1, 3, 7], "the fourth's size comes third");
}

/// Not a test of Compose's: a paused slot's nodes attach under its virtual node, past the slot
/// before it, and a measurement finds them in order.
#[test]
fn a_paused_slot_attaches_past_those_before_it() {
    let state = SubcomposeLayoutState::new();
    let take = mutable_state_of(false);
    let first = Callback::new(|| {
        sized("a".into(), 1)();
        sized("b".into(), 2)();
    });
    let second = Callback::new(|| {
        sized("c".into(), 3)();
        sized("d".into(), 4)();
    });
    let (t, s, st) = (take.handle(), second.clone(), state.clone());
    let policy = stack_policy(move |scope| {
        let mut slots = vec![scope.subcompose(1, first.clone())];
        if t.get() {
            slots.push(scope.subcompose(2, s.clone()));
        }
        slots
    });
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let paused = state.create_paused_precomposition(2, second);
    resume_until_completed(&paused);
    let handle = paused.apply();
    assert_eq!(handle.placeables_count(), 2);
    take.set(true);
    ui.wait_for_idle();
    assert_eq!(children_tops(&ui), [0, 1, 3, 6]);
}

/// Not a test of Compose's: among those kept, the slot's own is taken first, though others are
/// compatible and nearer the end.
#[test]
fn a_slot_takes_its_own_composition_first() {
    let retained: Rc<RefCell<Vec<i32>>> = Rc::default();
    let r = retained.clone();
    let policy = PolicyOf(
        move |slots: &mut SlotIdsSet| *r.borrow_mut() = ids(slots),
        |_: &dyn std::any::Any, _: &dyn std::any::Any| true,
    );
    let (mut ui, _, items) = items_scene(&[0, 1, 2], policy);
    ui.wait_for_idle();
    items.set(Vec::new());
    ui.wait_for_idle();
    assert_eq!(*retained.borrow(), [0, 1, 2]);
    items.set(vec![0]);
    ui.wait_for_idle();
    assert_eq!(*retained.borrow(), [1, 2], "0's its own, not 2's");
}

/// Not a test of Compose's: precomposing what a measurement holds composes nothing, and its handle
/// has nothing of it.
#[test]
fn precomposing_an_active_slot_does_nothing() {
    let (mut ui, state, _) = items_scene(&[0], subcompose_slot_reuse_policy(1));
    ui.wait_for_idle();
    let composed = Rc::new(Cell::new(false));
    let handle = state.precompose(0, composed_box(&composed, 10.0));
    assert!(!composed.get());
    assert_eq!(handle.placeables_count(), 0);
    handle.dispose();
    ui.wait_for_idle();
    ui.on_node_with_tag("0").assert_exists();
    assert_eq!(state.reusable_count(), 0);
}

// ---- In a lend ---------------------------------------------------------------------------------

/// Its `on_attach` asks where it is.
#[derive(Clone, PartialEq)]
struct AskingElement(Rc<Cell<Option<bool>>>);

struct AskingNode(Rc<Cell<Option<bool>>>);

impl ModifierNodeElement for AskingElement {
    type Node = AskingNode;
    fn create(&self) -> AskingNode {
        AskingNode(self.0.clone())
    }
    fn update(&self, _: &mut AskingNode) {}
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ModifierNode for AskingNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let coordinates = context.require_layout_coordinates();
        // Whatever it answers it answers: a read through the cell the lend swapped.
        let _ = coordinates.size();
        self.0.set(Some(coordinates.is_busy()));
    }
}

/// `on_attach` runs in `on_end_changes`, in the slot's pass in the lend: the window's opened again
/// at the cell's address, with real answers (D1309).
#[test]
fn a_node_attached_in_a_lend_asks_the_tree() {
    let asked = Rc::new(Cell::new(None));
    let a = asked.clone();
    let content = Callback::new(move || {
        Box!(modifier = Modifier.then_custom(AskingElement(a.clone())));
    });
    let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    assert_eq!(asked.get(), Some(false), "it answers");
}

/// A focused node leaving in a lend: the callback runs there and asks where something is.
#[test]
fn a_focus_callback_in_a_lend_asks_the_tree() {
    let (shown, told) = (mutable_state_of(true), Rc::new(Cell::new(None)));
    let requester = FocusRequester::new();
    let coordinates: Rc<Cell<Option<compose_ui::LayoutCoordinates>>> = Rc::default();
    let (c, t) = (coordinates.clone(), told.clone());
    let content = Callback::new(move || {
        let (c, c2, t) = (c.clone(), c.clone(), t.clone());
        Box!(
            modifier = Modifier
                .size((10.dp(), 10.dp()))
                .on_placed(move |coordinates: compose_ui::LayoutCoordinates| {
                    c.set(Some(coordinates))
                })
                .focus_requester(requester)
                .on_focus_changed(move |state: compose_ui::FocusState| {
                    if !state.is_focused()
                        && let Some(coordinates) = c2.get()
                    {
                        let _ = coordinates.size();
                        t.set(Some(coordinates.is_busy()));
                    }
                })
                .focusable(),
            {}
        );
    });
    let s = shown.handle();
    let policy = stack_policy(move |scope| {
        if s.get() { vec![scope.subcompose((), content.clone())] } else { Vec::new() }
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    requester.request_focus();
    ui.wait_for_idle();
    shown.set(false);
    ui.wait_for_idle();
    assert!(told.get().is_some(), "told in the slot's going");
}

/// A `LaunchedEffect`'s body runs after the frame, the tree free.
#[test]
fn a_launched_effect_in_a_slot_runs_after_the_frame() {
    let asked = Rc::new(Cell::new(None));
    let coordinates: Rc<Cell<Option<compose_ui::LayoutCoordinates>>> = Rc::default();
    let (c, a) = (coordinates.clone(), asked.clone());
    let content = Callback::new(move || {
        let holder = c.clone();
        Box!(
            modifier =
                Modifier
                    .size((10.dp(), 10.dp()))
                    .on_placed(move |coordinates: compose_ui::LayoutCoordinates| holder
                        .set(Some(coordinates))),
            {}
        );
        let (c, a) = (c.clone(), a.clone());
        LaunchedEffect((), async move {
            a.set(c.get().map(|coordinates| coordinates.is_busy()));
        });
    });
    let policy = stack_policy(move |scope| vec![scope.subcompose((), content.clone())]);
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    assert_eq!(asked.get(), Some(false), "placed by then, and the tree's free");
}

/// Not a test of Compose's: a slot added in a measurement leaves nothing to measure again, as its
/// `createNodeAt` ignores what it asks.
#[test]
fn a_new_slot_leaves_the_layout_clean() {
    let (slots, size) = (mutable_state_of(1), mutable_state_of(10));
    let runs = Rc::new(Cell::new(0));
    let (n, r) = (slots.handle(), runs.clone());
    let policy = stack_policy(move |scope| {
        r.set(r.get() + 1);
        (0..n.get())
            .map(|at| scope.subcompose(at, Callback::new(sized(at.to_string(), 10))))
            .collect()
    });
    let s = size.handle();
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        Column!(|_| {
            SubcomposeLayout!(measure_policy = policy.clone());
            sized("sibling".into(), s.get())();
        });
    });
    ui.wait_for_idle();
    slots.set(2);
    ui.wait_for_idle();
    assert_eq!(runs.get(), 2);
    size.set(20);
    ui.wait_for_idle();
    assert_eq!(runs.get(), 2, "a sibling's size is nothing to it");
}

// ---- Movable content across a subcomposition's edge -------------------------------------------

/// What moved content counts: its state made, and its body run.
#[derive(Clone, Default)]
struct Counts {
    made: Rc<Cell<u32>>,
    bodies: Rc<Cell<u32>>,
}

impl ComposeArg for Counts {
    fn same(&self, other: &Counts) -> bool {
        Rc::ptr_eq(&self.made, &other.made)
    }
}

impl Counts {
    /// Counts a body run, and the state made the first time.
    fn body(&self) {
        self.bodies.set(self.bodies.get() + 1);
        let made = self.made.clone();
        remember(move || made.set(made.get() + 1));
    }
}

/// What the content of `boxWithConstraintsAndIndirectContent` calls, set where it's first called.
type Indirect = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidInstrumentedMovableContentTests.kt, boxWithConstraintsAndIndirectContent
#[test]
fn content_set_inside_a_box_with_constraints_moves_out_and_back() {
    let extract = mutable_state_of(false);
    let counts = Counts::default();
    let (x, c) = (extract.handle(), counts.clone());
    let mut ui = compose_test(move || {
        // The content the movable content calls, set where it's first called.
        let content: Indirect = remember(Rc::default);
        let into = content.clone();
        let movable = remember(move || {
            movable_content_of(move || {
                let called = into.borrow().clone();
                if let Some(called) = called {
                    called();
                }
            })
        });
        let (inside, set, c) = (movable.clone(), content.clone(), c.clone());
        BoxWithConstraints!(move |_| {
            if !x.get() {
                let c = c.clone();
                *set.borrow_mut() = Some(Rc::new(move || {
                    c.body();
                    BasicText!("Movable text");
                }));
                inside();
            }
        });
        if x.get() {
            movable();
        }
    });
    ui.wait_for_idle();
    ui.on_node_with_text("Movable text").assert_exists();
    for extracted in [true, false, true] {
        extract.set(extracted);
        ui.wait_for_idle();
        ui.on_node_with_text("Movable text").assert_exists();
    }
    // Its state stayed through the moves across the subcomposition's edge; its body ran again at
    // each, as `BoxWithConstraints` provides a local of its own there (D1511: under the same
    // locals, it wouldn't).
    assert_eq!(counts.made.get(), 1);
    assert_eq!(counts.bodies.get(), 4);
}

/// `InvalidationLocation.entries`: contents, the same by identity.
#[derive(Clone, Default)]
struct Entries(Vec<Callback>);

impl PartialEq for Entries {
    fn eq(&self, other: &Entries) -> bool {
        self.0.len() == other.0.len() && self.0.iter().zip(&other.0).all(|(a, b)| a.same(b))
    }
}

/// `InvalidationLocation.Entry`: adds its content to `entries` while it's there.
#[composable]
fn Entry(entries: MutableState<Entries>, content: Callback) {
    let added = content.clone();
    DisposableEffect(content.clone(), move |scope| {
        entries.update(|list| Entries(list.0.iter().cloned().chain([added.clone()]).collect()));
        scope.on_dispose(move || {
            entries.update(|list| {
                Entries(list.0.iter().filter(|other| !other.same(&content)).cloned().collect())
            })
        })
    });
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidInstrumentedMovableContentTests.kt, correctInvalidationLocation
#[test]
fn content_moved_with_an_entry_called_elsewhere_recomposes_in_place() {
    let entries = mutable_state_of(Entries::default());
    let e = entries.handle();
    let wrapped = movable_content_of_with(move |number: i32| {
        let shown = 1;
        Entry(
            e,
            Callback::new(move || {
                if shown == 0 {
                    BasicText!("Some text");
                }
            }),
        );
        BasicText!("{number}");
    });
    let page = mutable_state_of(0);
    let p = page.handle();
    let mut ui = compose_test(move || {
        let wrapped = wrapped.clone();
        Column!(|_| {
            match p.get() {
                0 => wrapped(0),
                _ => {
                    let wrapped = wrapped.clone();
                    Box!({ wrapped(1) });
                }
            }
            Column!(|_| BasicText!("Page: {}", p.get()));
        });
        for entry in e.get().0 {
            entry();
        }
    });
    ui.wait_for_idle();
    for round in 1..=3 {
        page.set(round % 2);
        ui.wait_for_idle();
        ui.on_node_with_text(&format!("Page: {}", round % 2)).assert_exists();
        ui.on_node_with_text(&(round % 2).to_string()).assert_exists();
    }
}

/// `RememberObserverAfterReplaceGroupInElidedGroup`: a value remembered in a branch, then an
/// observer.
#[composable]
fn ObserverAfterABranch(condition: bool, counts: Counts) {
    if condition {
        remember(|| 1);
    }
    counts.body();
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidInstrumentedMovableContentTests.kt, moveToDialog_elidedRememberObserver
#[test]
fn content_moves_out_of_a_dialog_with_its_observer() {
    let in_dialog = mutable_state_of(true);
    let counts = Counts::default();
    let c = counts.clone();
    let movable = movable_content_of(move || ObserverAfterABranch(true, c.clone()));
    let d = in_dialog.handle();
    let mut ui = compose_test(move || {
        let movable = movable.clone();
        if !d.get() {
            movable();
        } else {
            use compose_ui::Dialog;
            Dialog!(on_dismiss_request = || {}, { movable() });
        }
    });
    ui.wait_for_idle();
    in_dialog.set(false);
    ui.wait_for_idle();
    in_dialog.set(true);
    ui.wait_for_idle();
    // A dialog here is in its caller's composition: a move within it, under the dialog's locals.
    assert_eq!(counts.made.get(), 1, "its state stayed");
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/AndroidInstrumentedMovableContentTests.kt, moveOutOfSubcompositionWithLargeSlotTable
#[test]
fn content_moves_out_of_a_slot_beside_a_thousand_boxes() {
    let moved = mutable_state_of(false);
    let counts = Counts::default();
    let (m, c) = (moved.handle(), counts.clone());
    let movable = movable_content_of(move || {
        c.body();
        let color = if m.get() { Color::RED } else { Color::BLACK };
        Box!(modifier = Modifier.size((32.dp(), 32.dp())).background(color).test_tag("moved"));
    });
    let mut ui = compose_test(move || {
        let movable = movable.clone();
        Box!(modifier = Modifier.fill_max_size(), {
            if m.get() {
                movable();
            }
            let slot = movable.clone();
            SubcomposeLayout!(
                measure_policy = stack_policy(move |scope| {
                    let slot = slot.clone();
                    vec![scope.subcompose(
                        (),
                        Callback::new(move || {
                            let slot = slot.clone();
                            Box!({
                                if !m.get() {
                                    slot();
                                }
                            });
                        }),
                    )]
                })
            );
            for _ in 0..1000 {
                Box!({});
            }
        });
    });
    ui.wait_for_idle();
    for move_out in [true, false] {
        moved.set(move_out);
        ui.wait_for_idle();
        ui.on_node_with_tag("moved").assert_exists();
    }
    assert_eq!(counts.made.get(), 1, "its state stayed");
}

// ---- PausableCompositionInstrumentedTests ------------------------------------------------------

/// A layout of `state` composing `content` in a slot `"slot"` while `add_slot`, 10 by 10.
fn layout_adding(
    state: &Rc<SubcomposeLayoutState>,
    add_slot: MutableState<bool>,
    content: Callback,
) -> ComposeTestRule {
    let st = state.clone();
    let policy = subcompose_policy(move |scope, _| {
        if add_slot.get() {
            scope.subcompose("slot", content.clone());
        }
        scope.layout(10, 10)
    });
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    ui
}

/// A slot kept for reuse: composed in a measurement, then out of it.
fn with_a_slot_for_reuse(content: Callback) -> (Rc<SubcomposeLayoutState>, ComposeTestRule) {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let add_slot = mutable_state_of(true);
    let mut ui = layout_adding(&state, add_slot.handle(), content);
    add_slot.set(false);
    ui.wait_for_idle();
    (state, ui)
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, changeTheKeyUsedInPrecomposition
#[test]
fn a_precomposition_whose_saveable_key_changes_applies() {
    let state = SubcomposeLayoutState::new();
    let (key, add_slot) = (mutable_state_of("1"), mutable_state_of(false));
    let holder: Rc<Cell<Option<SaveableStateHolder>>> = Rc::default();
    let (h, add, st) = (holder.clone(), add_slot.handle(), state.clone());
    let in_measure = Callback::new({
        let h = h.clone();
        move || h.get().expect("composed").SaveableStateProvider("1", || {})
    });
    let policy = subcompose_policy(move |scope, _| {
        if add.get() {
            scope.subcompose("measurepass", in_measure.clone());
        }
        scope.layout(10, 10)
    });
    let mut ui = compose_test(move || {
        h.set(Some(remember_saveable_state_holder()));
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let (k, h) = (key.handle(), holder.clone());
    let precomposition = state.create_paused_precomposition(
        "precomposition",
        Callback::new(move || h.get().expect("composed").SaveableStateProvider(k.get(), || {})),
    );
    resume_until_completed(&precomposition);
    key.set("2");
    add_slot.set(true);
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    precomposition.apply();
    ui.wait_for_idle();
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, pausedPrecompositionIsNotRecomposedOnItsOwn
#[test]
fn a_paused_precomposition_isnt_recomposed_on_its_own() {
    let state = SubcomposeLayoutState::new();
    let mut ui = empty_layout(&state);
    let key = mutable_state_of("1");
    let composed: Rc<RefCell<Vec<&str>>> = Rc::default();
    let (k, c) = (key.handle(), composed.clone());
    let precomposition =
        state.create_paused_precomposition((), Callback::new(move || c.borrow_mut().push(k.get())));
    resume_until_completed(&precomposition);
    assert_eq!(composed.take(), ["1"]);
    key.set("2");
    ui.wait_for_idle();
    assert!(composed.borrow().is_empty(), "not composed again while it isn't applied");
    resume_until_completed(&precomposition);
    precomposition.apply();
    assert_eq!(composed.take(), ["2"]);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, test_for_404058957
#[test]
fn a_precomposition_deactivated_and_reactivated_while_paused_applies() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let mut ui = empty_layout(&state);
    let active = mutable_state_of(true);
    let drawn = mutable_state_of(false);
    let (a, d) = (active.handle(), drawn.handle());
    let precomposition = state.create_paused_precomposition(
        (),
        Callback::new(move || {
            ReusableContentHost(a.get(), move || {
                let modifier = if d.get() { Modifier.draw_behind(|_| {}) } else { Modifier };
                Layout!(
                    modifier = modifier,
                    measure_policy = measure_policy(|scope, _, _| scope.layout(10, 10)),
                    || {}
                );
            });
        }),
    );
    precomposition.resume(|| false);
    active.set(false);
    ui.wait_for_idle();
    precomposition.resume(|| false);
    active.set(true);
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    precomposition.apply();
    // The modifier of what was just applied, updated.
    drawn.set(true);
    ui.wait_for_idle();
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, movableContent_virtual_node
#[test]
fn movable_content_moving_while_its_precomposition_is_paused_applies() {
    let state = SubcomposeLayoutState::new();
    let mut ui = empty_layout(&state);
    let emit = mutable_state_of(true);
    let movable = movable_content_of_with(|flag: bool| {
        let modifier = if flag { Modifier.background(Color::RED) } else { Modifier };
        Box!(modifier = modifier);
    });
    let e = emit.handle();
    let precomposition = state.create_paused_precomposition(
        (),
        Callback::new(move || {
            Box!({});
            let movable = movable.clone();
            if e.get() {
                Box!({});
                movable(true);
            } else {
                Row!(|_| {
                    Box!({});
                    movable(false);
                });
            }
        }),
    );
    precomposition.resume(|| false);
    emit.set(false);
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    precomposition.apply();
    ui.wait_for_idle();
}

/// Composes `content`, pausing as soon as `outer` says its body ran, then on to the end.
fn pause_after_the_outer(
    state: &SubcomposeLayoutState,
    outer: Rc<Cell<bool>>,
    content: Callback,
) -> PausedPrecomposition {
    let o = outer.clone();
    let precomposition = state.create_paused_precomposition(
        (),
        Callback::new(move || {
            o.set(true);
            content();
        }),
    );
    let o = outer.clone();
    precomposition.resume(move || o.get());
    precomposition.resume(|| false);
    precomposition
}

/// `DisposableEffectWrapper`: runs `on_composed`, then an effect calling `on_applied`.
#[composable]
fn EffectWrapper(on_composed: Rc<dyn Fn()>, on_applied: Rc<dyn Fn()>) {
    on_composed();
    DisposableEffect((), move |scope| {
        on_applied();
        scope.on_dispose(|| {})
    });
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, precomposingWithPauseAndExtraRecomposition_effectAppliedOnlyOnce
#[test]
fn an_effect_recomposed_while_paused_is_applied_once() {
    let (state, mut ui) = with_a_slot_for_reuse(Callback::new(|| {}));
    let trigger = mutable_state_of_with((), never_equal_policy());
    let applied = Rc::new(Cell::new(0));
    let (t, a) = (trigger.handle(), applied.clone());
    let content = Callback::new(move || {
        let a = a.clone();
        EffectWrapper(Rc::new(move || t.get()), Rc::new(move || a.set(a.get() + 1)));
    });
    let precomposition = pause_after_the_outer(&state, Rc::default(), content);
    trigger.set(());
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    precomposition.apply();
    assert_eq!(applied.get(), 1);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, precomposingWithPauseAndExtraRecomposition_effectAppliedOnlyOnce2
#[test]
fn an_effect_in_reusable_content_whose_key_changes_while_paused_is_applied_once() {
    let state = SubcomposeLayoutState::new();
    let mut ui = empty_layout(&state);
    let key = mutable_state_of("A");
    let applied = Rc::new(Cell::new(0));
    let (k, a) = (key.handle(), applied.clone());
    let content = Callback::new(move || {
        let a = a.clone();
        ReusableContent(k.get(), move || {
            let a = a.clone();
            EffectWrapper(Rc::new(|| {}), Rc::new(move || a.set(a.get() + 1)));
        });
    });
    let precomposition = pause_after_the_outer(&state, Rc::default(), content);
    key.set("B");
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    precomposition.apply();
    assert_eq!(applied.get(), 1);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, precomposingWithPauseAndExtraRecomposition_rememberedValueNotRecreated
#[test]
fn a_value_remembered_before_a_recomposition_while_paused_is_kept() {
    let (state, mut ui) = with_a_slot_for_reuse(Callback::new(|| {}));
    let trigger = mutable_state_of_with((), never_equal_policy());
    let remembered = Rc::new(Cell::new(0));
    let (t, r) = (trigger.handle(), remembered.clone());
    let content = Callback::new(move || {
        t.get();
        let r = r.clone();
        remember(move || r.set(r.get() + 1));
    });
    let precomposition = pause_after_the_outer(&state, Rc::default(), content);
    trigger.set(());
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    assert_eq!(remembered.get(), 1);
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, pausedPrecompositionIsNotCrashingWhenRecompositionIsTriggeredAndIgnoredMultipleTimes
#[test]
fn a_paused_precomposition_invalidated_twice_and_left_is_fine() {
    let state = SubcomposeLayoutState::new();
    let mut ui = empty_layout(&state);
    let key = mutable_state_of("1");
    let scope: Rc<Cell<Option<RecomposeScope>>> = Rc::default();
    let (k, s) = (key.handle(), scope.clone());
    let precomposition = state.create_paused_precomposition(
        (),
        Callback::new(move || {
            k.get();
            s.set(Some(current_recompose_scope()));
        }),
    );
    resume_until_completed(&precomposition);
    key.set("2");
    ui.wait_for_idle();
    scope.get().expect("composed").invalidate();
    ui.wait_for_idle();
}

// Compose: compose/runtime/runtime/integration-tests/src/androidTest/kotlin/androidx/compose/runtime/PausableCompositionInstrumentedTests.kt, precomposingWithPauseAndExtraRecomposition_updateModifierDuringRecompositionNotCrashing
#[test]
fn a_reused_slots_modifier_changed_while_paused_applies() {
    let drawn = mutable_state_of(true);
    let d = drawn.handle();
    let content = Callback::new(move || {
        let modifier = if d.get() { Modifier.draw_behind(|_| {}) } else { Modifier };
        Layout!(
            modifier = modifier,
            measure_policy = measure_policy(|scope, _, _| scope.layout(10, 10)),
            || {}
        );
    });
    let (state, mut ui) = with_a_slot_for_reuse(content.clone());
    let precomposition = state.create_paused_precomposition((), content);
    precomposition.resume(|| true);
    drawn.set(false);
    ui.wait_for_idle();
    resume_until_completed(&precomposition);
    precomposition.apply();
    ui.wait_for_idle();
}

/// A small deterministic generator (SplitMix64): a failing run fails the same way again.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// One of `0..n`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }
}

/// The tags of the placed nodes `dump_tree` shows, in its order: `slot{key}.{layout}`.
fn placed_slot_tags(ui: &ComposeTestRule) -> Vec<String> {
    ui.dump_tree()
        .lines()
        .filter(|line| !line.contains("(not placed)"))
        .filter_map(|line| {
            let start = line.find("slot")?;
            let tag: String = line[start..]
                .chars()
                .take_while(|it| it.is_ascii_alphanumeric() || *it == '.')
                .collect();
            Some(tag)
        })
        .collect()
}

/// Slots of 0 to 2 layouts each added (some precomposed first), moved, removed several at once
/// and reversed at random, with 2 kept for reuse and a rolling 3 precomposed, against a model:
/// the placed nodes stay in the order the measurement composed their slots (D1477's moves and
/// removals in one pass, on a real `SubcomposeLayout`).
#[test]
fn random_slots_keep_the_tree_in_composition_order() {
    let mut random = Random(1477);
    let layouts: Rc<RefCell<std::collections::HashMap<i32, (usize, Callback)>>> = Rc::default();
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(2));
    let keys = mutable_state_of(Vec::<i32>::new());
    let (k, l) = (keys.handle(), layouts.clone());
    let policy = stack_policy(move |scope| {
        let layouts = l.borrow();
        k.get().into_iter().map(|key| scope.subcompose(key, layouts[&key].1.clone())).collect()
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let mut model: Vec<i32> = Vec::new();
    let mut precomposed: std::collections::VecDeque<(i32, PrecomposedSlotHandle)> =
        Default::default();
    let mut next_key = 0;
    let mut new_key = |random: &mut Random| {
        let key = next_key;
        next_key += 1;
        let count = random.below(3);
        let content = Callback::new(move || {
            for layout in 0..count {
                Box!(
                    modifier =
                        Modifier.size((10.dp(), 10.dp())).test_tag(format!("slot{key}.{layout}"))
                );
            }
        });
        layouts.borrow_mut().insert(key, (count, content));
        key
    };
    let mut shown = 0;
    for step in 0..300 {
        let len = model.len();
        let kind = if len < 2 { 0 } else { random.below(6) };
        let what = match kind {
            0 | 1 => {
                // A key precomposed and not taken yet, or a new one.
                let waiting: Vec<i32> = (precomposed.iter())
                    .map(|(key, _)| *key)
                    .filter(|key| !model.contains(key))
                    .collect();
                let key = if !waiting.is_empty() && random.chance(2) {
                    waiting[random.below(waiting.len())]
                } else {
                    new_key(&mut random)
                };
                let at = random.below(len + 1);
                model.insert(at, key);
                format!("add {key} at {at}")
            }
            2 => {
                let (from, to) = (random.below(len), random.below(len));
                let key = model.remove(from);
                model.insert(to, key);
                format!("move {from} to {to}")
            }
            3 => {
                let gone: Vec<i32> = model.iter().copied().filter(|_| random.chance(2)).collect();
                model.retain(|key| !gone.contains(key));
                format!("remove {gone:?}")
            }
            4 => {
                model.reverse();
                "reverse".to_string()
            }
            _ => {
                // The rolling set: one more precomposed, the oldest disposed past 3.
                let key = new_key(&mut random);
                let content = layouts.borrow()[&key].1.clone();
                precomposed.push_back((key, state.precompose(key, content)));
                if precomposed.len() > 3 {
                    precomposed.pop_front().expect("past 3").1.dispose();
                }
                format!("precompose {key}")
            }
        };
        keys.set(model.clone());
        ui.wait_for_idle();
        let expected: Vec<String> = (model.iter())
            .flat_map(|key| {
                let count = layouts.borrow()[key].0;
                (0..count).map(move |layout| format!("slot{key}.{layout}"))
            })
            .collect();
        assert_eq!(
            placed_slot_tags(&ui),
            expected,
            "step {step}: {what}, {model:?}\n{}",
            ui.dump_tree()
        );
        shown += expected.len();
    }
    assert!(shown > 1_000, "{shown} nodes checked");
}

// ---- The rest of SubcomposeLayoutTest ---------------------------------------------------------

/// A box with `content`, not inlined: its own restart scope, as the test's `NonInlineBox`.
#[composable]
fn NonInlineBox(modifier: Modifier, content: Callback) {
    Box!(modifier = modifier, move |_| content());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, recompositionDeepInsideTheSlotDoesntRecomposeUnaffectedLayerOrRemeasure
#[test]
fn a_recomposition_deep_in_a_slot_leaves_the_slot_and_the_layout_alone() {
    let (model, measures, slot_runs, inner_runs) =
        (mutable_state_of(0), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (m, s, i) = (model.handle(), slot_runs.clone(), inner_runs.clone());
    let inner = Callback::new(move || {
        m.get();
        i.set(i.get() + 1);
    });
    let slot = Callback::new(move || {
        s.set(s.get() + 1);
        NonInlineBox!(Modifier.required_size((20.dp(), 20.dp())), inner.clone());
    });
    let me = measures.clone();
    let policy = subcompose_policy(move |scope, constraints| {
        me.set(me.get() + 1);
        let placeable = measure_slot(scope, (), constraints, slot.clone());
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    });
    let mut ui = compose_test(move || SubcomposeLayout!(measure_policy = policy.clone()));
    ui.wait_for_idle();
    model.set(1);
    ui.wait_for_idle();
    assert_eq!((measures.get(), slot_runs.get(), inner_runs.get()), (1, 1, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, propagatesDensity
#[test]
fn it_measures_in_the_density_it_s_given() {
    let size = Rc::new(Cell::new(None));
    let s = size.clone();
    let density = compose_ui::Density::new(3.0);
    let side = density.round_to_px(50.dp());
    let mut ui = compose_test(move || {
        let s = s.clone();
        CompositionLocalProvider(compose_ui::LOCAL_DENSITY.provides(density), move || {
            let s = s.clone();
            SubcomposeLayout!(
                modifier = Modifier.required_size((50.dp(), 50.dp())).on_globally_positioned(
                    move |coordinates: compose_ui::LayoutCoordinates| {
                        s.set(Some(coordinates.size()))
                    }
                ),
                measure_policy = subcompose_policy(|scope, constraints| {
                    scope.layout(constraints.max_width, constraints.max_height)
                })
            );
        });
    });
    ui.wait_for_idle();
    assert_eq!(size.get(), Some(IntSize::new(side, side)));
    assert_eq!(side, 150);
}

/// Counts its compositions, and its effect's entries and exits.
fn counted_content(
    composed: &Rc<Cell<i32>>,
    entered: &Rc<Cell<i32>>,
    exited: &Rc<Cell<i32>>,
) -> Callback {
    let (c, en, ex) = (composed.clone(), entered.clone(), exited.clone());
    Callback::new(move || {
        c.set(c.get() + 1);
        let (en, ex) = (en.clone(), ex.clone());
        DisposableEffect((), move |scope| {
            en.set(en.get() + 1);
            let ex = ex.clone();
            scope.on_dispose(move || ex.set(ex.get() + 1))
        });
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, composeItemRegularlyAfterDisposingPrecomposedItem
#[test]
fn a_slot_disposed_after_its_precomposition_composes_anew_in_its_measurement() {
    let (state, add) = (SubcomposeLayoutState::new(), mutable_state_of(false));
    let (composed, entered, exited) =
        (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let content = counted_content(&composed, &entered, &exited);
    let (a, c) = (add.handle(), content.clone());
    let policy = subcompose_policy(move |scope, _| {
        if a.get() {
            scope.subcompose((), c.clone());
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let slot = state.precompose((), content);
    ui.wait_for_idle();
    slot.dispose();
    ui.wait_for_idle();
    assert_eq!((composed.get(), entered.get(), exited.get()), (1, 1, 1));
    add.set(true);
    ui.wait_for_idle();
    assert_eq!((composed.get(), entered.get(), exited.get()), (2, 2, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposeTwoItems
#[test]
fn two_precomposed_slots_are_taken_by_the_measurement_as_they_are() {
    let (state, add) = (SubcomposeLayoutState::new(), mutable_state_of(false));
    let (first_runs, second_runs) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (f, s) = (first_runs.clone(), second_runs.clone());
    let first = Callback::new(move || f.set(f.get() + 1));
    let second = Callback::new(move || s.set(s.get() + 1));
    let (a, f, s) = (add.handle(), first.clone(), second.clone());
    let (empty0, empty3) = (Callback::new(|| {}), Callback::new(|| {}));
    let policy = subcompose_policy(move |scope, _| {
        scope.subcompose(0, empty0.clone());
        if a.get() {
            scope.subcompose(1, f.clone());
            scope.subcompose(2, s.clone());
        }
        scope.subcompose(3, empty3.clone());
        scope.layout(10, 10)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!((first_runs.get(), second_runs.get()), (0, 0));
    let _first = state.precompose(1, first);
    let _second = state.precompose(2, second);
    ui.wait_for_idle();
    assert_eq!((first_runs.get(), second_runs.get()), (1, 1));
    add.set(true);
    ui.wait_for_idle();
    assert_eq!((first_runs.get(), second_runs.get()), (1, 1), "not composed again");
}

/// A layout composing nothing, counting its measurements and placements.
fn counting_empty_layout(
    state: &Rc<SubcomposeLayoutState>,
    measures: &Rc<Cell<i32>>,
    layouts: &Rc<Cell<i32>>,
) -> ComposeTestRule {
    let (st, m, l) = (state.clone(), measures.clone(), layouts.clone());
    compose_test(move || {
        let (m, l) = (m.clone(), l.clone());
        SubcomposeLayoutWithState!(
            st.clone(),
            measure_policy = subcompose_policy(move |scope, _| {
                m.set(m.get() + 1);
                let l = l.clone();
                scope.layout_with(10, 10, move |_| l.set(l.get() + 1))
            })
        );
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposedItemDisposalIsNotTriggeringParentRemeasure
#[test]
fn disposing_a_precomposed_slot_measures_and_places_nothing_again() {
    let (state, measures, layouts) =
        (SubcomposeLayoutState::new(), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut ui = counting_empty_layout(&state, &measures, &layouts);
    ui.wait_for_idle();
    assert_eq!((measures.get(), layouts.get()), (1, 1));
    let handle = state.precompose(
        (),
        Callback::new(|| {
            Box!(modifier = Modifier.fill_max_size());
        }),
    );
    handle.dispose();
    ui.wait_for_idle();
    assert_eq!((measures.get(), layouts.get()), (1, 1));
}

/// Kept for reuse: out of the semantics, but among the deactivated nodes, as the tests'
/// `assertLayoutDeactivatedById`.
fn assert_kept(ui: &mut ComposeTestRule, kept: &[i32]) {
    for item in kept {
        ui.on_node_with_tag(&item.to_string()).assert_does_not_exist().assert_is_deactivated();
    }
}

/// Gone: not even among the deactivated nodes (Compose's `assertIsDetached`).
fn assert_gone(ui: &mut ComposeTestRule, gone: &[i32]) {
    for item in gone {
        ui.on_node_with_tag(&item.to_string()).assert_does_not_exist();
        let deactivated = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ui.on_node_with_tag(&item.to_string()).assert_is_deactivated();
        }))
        .is_ok();
        assert!(!deactivated, "{item} is kept for reuse, not gone");
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, theSameSlotIsUsedWhileItIsInReusableList
#[test]
fn a_slot_kept_for_reuse_comes_back_for_its_own_id() {
    let (mut ui, state, items) = items_scene(&[0, 1, 2, 3, 4], subcompose_slot_reuse_policy(2));
    ui.wait_for_idle();
    items.set(vec![2, 3]); // 0 and 1 are kept for reuse.
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 2);
    assert_kept(&mut ui, &[0, 1]);
    assert_gone(&mut ui, &[4]);
    items.set(vec![2, 3, 1]); // 1 is taken back.
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[2, 3, 1], &[]);
    assert_kept(&mut ui, &[0]);
    assert_eq!(state.reusable_count(), 1, "0 still kept");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, prefetchSlotWhichIsInReusableList
#[test]
fn precomposing_a_kept_slot_s_id_takes_it_back() {
    let (mut ui, state, items) = items_scene(&[0, 1, 2, 3, 4], subcompose_slot_reuse_policy(3));
    ui.wait_for_idle();
    items.set(vec![2]); // 0, 1 and 3 are kept for reuse, 4 disposed.
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 3);
    let live = Rc::new(Cell::new(0));
    // The test's `ItemContent(3)`, its composition counted too.
    let l = live.clone();
    let content = Callback::new(move || {
        let l = l.clone();
        remember_observer(move || Live(l));
        Box!(modifier = Modifier.fill_max_size().test_tag("3"));
    });
    let handle = state.precompose(3, content);
    assert_eq!(state.reusable_count(), 2, "3 taken back");
    assert_eq!(handle.placeables_count(), 1);
    ui.wait_for_idle();
    assert_eq!(live.get(), 1);
    // 3 is active again, precomposed, but not placed, so not in the semantics here (Compose's hold
    // it): at least it's no longer among the deactivated nodes.
    assert_nodes(&mut ui, &[2], &[]);
    assert_kept(&mut ui, &[0, 1]);
    assert_gone(&mut ui, &[3, 4]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, reuse1Node
#[test]
fn a_policy_of_one_keeps_one_of_what_goes() {
    let (mut ui, state, items) = items_scene(&[0, 1, 2, 3], subcompose_slot_reuse_policy(1));
    ui.wait_for_idle();
    items.set(vec![0, 1]);
    ui.wait_for_idle();
    assert_nodes(&mut ui, &[0, 1], &[]);
    assert_kept(&mut ui, &[2]);
    assert_gone(&mut ui, &[3]);
    assert_eq!(state.reusable_count(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, disposeSecondPrecomposedItem
#[test]
fn the_second_precomposed_slot_can_be_disposed() {
    let (mut ui, state, _items) = items_scene(&[], subcompose_slot_reuse_policy(0));
    ui.wait_for_idle();
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let _kept = state.precompose(0, live_content(&first));
    let handle = state.precompose(1, live_content(&second));
    handle.dispose();
    ui.wait_for_idle();
    assert_eq!((first.get(), second.get()), (1, 0));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, compositionKeptForReuseIsDisposed
#[test]
fn a_composition_kept_for_reuse_is_disposed() {
    let (state, need, disposed) = (
        SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)),
        mutable_state_of(true),
        Rc::new(Cell::new(false)),
    );
    let d = disposed.clone();
    let content = Callback::new(move || {
        let d = d.clone();
        on_dispose(move || d.set(true));
    });
    let n = need.handle();
    let policy = subcompose_policy(move |scope, _| {
        if n.get() {
            scope.subcompose((), content.clone());
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    need.set(false);
    ui.wait_for_idle();
    assert_eq!(state.reusable_count(), 1);
    assert!(disposed.get(), "what's kept for reuse has its composition's effects disposed");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, subcomposeLayoutInsideLayoutUsingAlignmentsIsNotCrashing
#[test]
fn a_subcompose_layout_added_under_a_layout_reading_alignments_is_fine() {
    let emit = mutable_state_of(false);
    let e = emit.handle();
    let mut ui = compose_test(move || {
        // The test's `LayoutUsingAlignments`: its child's first baseline read as it's measured.
        Layout!(
            measure_policy = measure_policy(|scope, measurables, constraints| {
                let placeable = scope.measure(measurables[0], constraints);
                scope.alignment_line(&placeable, compose_ui::FIRST_BASELINE);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            }),
            move || {
                Box!(move |_| {
                    if e.get() {
                        SubcomposeLayout!(
                            measure_policy = subcompose_policy(|scope, _| {
                                scope.subcompose((), Callback::new(|| {}));
                                scope.layout(10, 10)
                            })
                        );
                    }
                });
            }
        );
    });
    ui.wait_for_idle();
    emit.set(true);
    ui.wait_for_idle();
}

/// A slot reading `local`, its composition counted, under a layout whose policy stays the same (the
/// tests' `_noRemeasure`: no new policy asks for a measurement): a change of the local composes the
/// slot again, once.
fn local_change_composes_the_slot_once(local: &'static CompositionLocal<i32>) {
    let (value, seen, runs) = (mutable_state_of(1), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (s, r) = (seen.clone(), runs.clone());
    let content = Callback::new(move || {
        assert_ne!(local.current(), s.get(), "composed once for each value");
        s.set(local.current());
        r.set(r.get() + 1);
    });
    let policy = subcompose_policy(move |scope, _| {
        scope.subcompose((), content.clone());
        scope.layout(100, 100)
    });
    let v = value.handle();
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        CompositionLocalProvider(local.provides(v.get()), move || {
            SubcomposeLayout!(measure_policy = policy.clone());
        });
    });
    ui.wait_for_idle();
    assert_eq!((seen.get(), runs.get()), (1, 1));
    value.set(2);
    ui.wait_for_idle();
    assert_eq!((seen.get(), runs.get()), (2, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, compositionLocalChangeInMainCompositionRecomposesSubcomposition_noRemeasure
#[test]
fn a_dynamic_local_above_reaches_a_slot_of_the_same_policy() {
    local_change_composes_the_slot_once(&DYNAMIC);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, staticCompositionLocalChangeInMainCompositionRecomposesSubcomposition_noRemeasure
#[test]
fn a_static_local_above_reaches_a_slot_of_the_same_policy() {
    local_change_composes_the_slot_once(&STATIC);
}

static DARK: CompositionLocal<bool> = CompositionLocal::new_static("subcompose.dark", || false);
static DARK_TOO: CompositionLocal<bool> = CompositionLocal::new("subcompose.dark_too", || false);
static INNER: CompositionLocal<i32> = CompositionLocal::new_static("subcompose.inner", || 0);

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, staticCompositionLocalChangeInMainComposition_withNonStaticLocal_invalidatesComposition
#[test]
fn a_static_local_and_a_dynamic_one_from_it_both_reach_a_slot() {
    let dark = mutable_state_of(false);
    let content = Callback::new(|| {
        CompositionLocalProvider(INNER.provides(1), || {
            let (value1, value2) = (DARK.current(), DARK_TOO.current());
            let side = if value2 { 50.dp() } else { 100.dp() };
            Box!(
                modifier = Modifier
                    .test_tag(if value1 { "dark" } else { "light" })
                    .required_size((side, side))
            );
        });
    });
    let policy = subcompose_policy(move |scope, constraints| {
        let measurables = scope.subcompose((), content.clone());
        let placeables: Vec<Placeable> =
            measurables.into_iter().map(|it| scope.measure(it, constraints)).collect();
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(100, 100)
    });
    let d = dark.handle();
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        CompositionLocalProvider(DARK.provides(d.get()), move || {
            let policy = policy.clone();
            CompositionLocalProvider(DARK_TOO.provides(DARK.current()), move || {
                SubcomposeLayout!(measure_policy = policy.clone());
            });
        });
    });
    ui.wait_for_idle();
    ui.on_node_with_tag("light").assert_width_is_equal_to(100.dp());
    dark.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("dark").assert_width_is_equal_to(50.dp());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, derivedStateChangeInMainCompositionRecomposesSubcomposition
#[test]
fn a_derived_state_of_the_composition_above_reaches_a_slot() {
    let flag = mutable_state_of(true);
    let (seen, inconsistent) = (Rc::new(Cell::new(None)), Rc::new(Cell::new(false)));
    let (f, s, i) = (flag.handle(), seen.clone(), inconsistent.clone());
    let mut ui = compose_test(move || {
        let updated = remember_updated_state(f.get());
        let derived = remember(move || derived_state_of(move || updated.get()));
        let s2 = s.clone();
        let content = remember(move || {
            let s = s2.clone();
            Callback::new(move || {
                assert_ne!(Some(derived.get()), s.get(), "composed once for each value");
                s.set(Some(derived.get()));
            })
        });
        let (s, i) = (s.clone(), i.clone());
        SubcomposeLayout!(
            modifier = Modifier.draw_behind(move |_| {
                // Never drawn with the slot behind.
                if s.get() != Some(updated.get()) {
                    i.set(true);
                }
            }),
            measure_policy = subcompose_policy(move |scope, _| {
                scope.subcompose((), content.clone());
                scope.layout(100, 100)
            })
        );
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), Some(true));
    flag.set(false);
    ui.wait_for_idle();
    assert_eq!(seen.get(), Some(false));
    assert!(!inconsistent.get());
}

/// Two boxes filling what they're given, counting their measurements, as the premeasuring tests'
/// content.
fn two_counted_boxes(measures: &Rc<Cell<i32>>) -> Callback {
    let modifier = add_counter(measures).fill_max_size();
    Callback::new(move || {
        Box!(modifier = modifier.clone());
        Box!(modifier = modifier.clone());
    })
}

fn premeasuring_scene(
    content: &Callback,
) -> (ComposeTestRule, Rc<SubcomposeLayoutState>, OwnedState<bool>) {
    let (state, need) = (SubcomposeLayoutState::new(), mutable_state_of(false));
    let (n, c) = (need.handle(), content.clone());
    let policy = subcompose_policy(move |scope, _| {
        let placeables: Vec<Placeable> = if n.get() {
            let measurables = scope.subcompose((), c.clone());
            assert_eq!(measurables.len(), 2);
            (measurables.into_iter().enumerate())
                .map(|(index, it)| scope.measure(it, premeasure_constraints(index)))
                .collect()
        } else {
            Vec::new()
        };
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    (ui, state, need)
}

/// 100 wide for the first, 200 for the second.
fn premeasure_constraints(index: usize) -> Constraints {
    let width = if index == 0 { 100 } else { 200 };
    Constraints::new(width, width, 0, Constraints::INFINITY)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, premeasuringTwoPlaceables
#[test]
fn two_premeasured_layouts_aren_t_measured_again_by_the_measurement() {
    let measures = Rc::new(Cell::new(0));
    let content = two_counted_boxes(&measures);
    let (mut ui, state, need) = premeasuring_scene(&content);
    ui.wait_for_idle();
    assert_eq!(measures.get(), 0);
    let handle = state.precompose((), content);
    assert_eq!(measures.get(), 0);
    assert_eq!(handle.placeables_count(), 2);
    handle.premeasure(0, premeasure_constraints(0));
    assert_eq!(measures.get(), 1);
    handle.premeasure(1, premeasure_constraints(1));
    assert_eq!(measures.get(), 2);
    measures.set(0);
    need.set(true);
    ui.wait_for_idle();
    assert_eq!(measures.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, premeasuringTwoPlaceables_allowsQueryingSizeAfter
#[test]
fn a_premeasured_layout_s_size_can_be_asked() {
    let measures = Rc::new(Cell::new(0));
    let content = two_counted_boxes(&measures);
    let (mut ui, state, _need) = premeasuring_scene(&content);
    ui.wait_for_idle();
    let handle = state.precompose((), content);
    assert_eq!(measures.get(), 0);
    assert_eq!(handle.placeables_count(), 2);
    assert_eq!(handle.get_size(0), IntSize::new(0, 0));
    handle.premeasure(0, premeasure_constraints(0));
    assert_eq!(handle.get_size(0), IntSize::new(100, 0));
    assert_eq!(measures.get(), 1);
    assert_eq!(handle.get_size(1), IntSize::new(0, 0));
    handle.premeasure(1, premeasure_constraints(1));
    assert_eq!(handle.get_size(1), IntSize::new(200, 0));
    assert_eq!(measures.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, getMeasuredSizes_IncorrectIndexesCrashes
/// Only the index past the end: an index is a `usize`, so Compose's -1 can't be asked.
#[test]
#[should_panic(expected = "out of bound")]
fn the_size_of_an_index_it_doesnt_have_panics() {
    let state = SubcomposeLayoutState::new();
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(
            st.clone(),
            measure_policy = subcompose_policy(|scope, _| scope.layout(10, 10))
        );
    });
    ui.wait_for_idle();
    let handle = state.precompose(
        (),
        Callback::new(|| {
            sized("a".into(), 10)();
            sized("b".into(), 10)();
        }),
    );
    handle.premeasure(0, Constraints::default());
    handle.premeasure(1, Constraints::default());
    handle.get_size(2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, ifSlotWasUsedDuringMeasurePassHandleHasZeroPlaceables
#[test]
fn a_precomposed_slot_taken_by_the_measurement_has_nothing_left_in_its_handle() {
    let (state, need) = (SubcomposeLayoutState::new(), mutable_state_of(false));
    let content = Callback::new(sized("item".into(), 10));
    let (n, c) = (need.handle(), content.clone());
    let policy = subcompose_policy(move |scope, _| {
        if n.get() {
            let placeable = measure_slot(scope, (), Constraints::default(), c.clone());
            scope.place(&placeable, 0, 0);
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    let handle = state.precompose((), content);
    handle.premeasure(0, Constraints::default());
    need.set(true);
    ui.wait_for_idle();
    assert_eq!(handle.placeables_count(), 0);
    handle.dispose(); // Safe.
    ui.wait_for_idle();
    ui.on_node_with_tag("item").assert_exists();
}

/// A layout of `width` by `height` asked in its measurement panics, caught there as the test's
/// `try`: whether it did.
fn a_layout_of(width: i32, height: i32) -> bool {
    let caught = Rc::new(Cell::new(false));
    let c = caught.clone();
    let mut ui = compose_test(move || {
        let c = c.clone();
        SubcomposeLayout!(
            measure_policy = subcompose_policy(move |scope, _| {
                let asked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    scope.layout(width, height)
                }));
                asked.unwrap_or_else(|_| {
                    c.set(true);
                    scope.layout(0, 0)
                })
            })
        );
    });
    ui.wait_for_idle();
    caught.get()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, stateIsRestoredWhenGoBackToScreen1WithSubcomposition
#[test]
fn a_slot_s_saved_state_comes_back_with_its_screen() {
    let (screen, increment, seen) =
        (mutable_state_of(1), Rc::new(Cell::new(0)), Rc::new(Cell::new(-1)));
    let (i, s) = (increment.clone(), seen.clone());
    let content = Callback::new(move || {
        let i = i.clone();
        let number = remember_saveable(move || {
            let next = i.get();
            i.set(next + 1);
            mutable_state_of(next)
        });
        s.set(number.get());
    });
    let policy = subcompose_policy(move |scope, _| {
        scope.subcompose((), content.clone());
        scope.layout(10, 10)
    });
    let sc = screen.handle();
    let mut ui = compose_test(move || {
        let holder = remember_saveable_state_holder();
        let current = sc.get();
        let policy = policy.clone();
        holder.SaveableStateProvider(current, move || {
            if current == 1 {
                SubcomposeLayout!(measure_policy = policy.clone());
            }
        });
    });
    ui.wait_for_idle();
    assert_eq!(seen.get(), 0);
    screen.set(2);
    ui.wait_for_idle();
    seen.set(-1);
    screen.set(1);
    ui.wait_for_idle();
    assert_eq!(seen.get(), 0, "restored, not made again");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, disposingPrecomposedItemInTheNestedSubcomposeLayout
#[test]
fn a_precomposed_slot_of_a_nested_layout_deactivated_can_be_disposed() {
    let need = mutable_state_of(true);
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let n = need.handle();
    let inner_policy = subcompose_policy(move |scope, constraints| {
        if n.get() {
            let content = Callback::new(|| {
                Box!(modifier = Modifier.size((10.dp(), 10.dp())));
            });
            let placeable = measure_slot(scope, 0, constraints, content);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        } else {
            scope.layout(100, 100)
        }
    });
    let st = state.clone();
    let outer_content = Callback::new(move || {
        let (st, inner_policy) = (st.clone(), inner_policy.clone());
        Box!(move |_| {
            SubcomposeLayoutWithState!(
                st.clone(),
                modifier = Modifier.test_tag("0"),
                measure_policy = inner_policy.clone()
            );
        });
    });
    let n = need.handle();
    let outer_policy = subcompose_policy(move |scope, constraints| {
        if n.get() {
            let child = measure_slot(scope, 0, constraints, outer_content.clone());
            scope.place(&child, 0, 0);
        }
        scope.layout(100, 100)
    });
    let mut ui = compose_test(move || {
        let outer =
            remember(|| SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)));
        SubcomposeLayoutWithState!(outer, measure_policy = outer_policy.clone());
    });
    ui.wait_for_idle();
    let active = Rc::new(Cell::new(false));
    let a = active.clone();
    let handle = state.precompose(
        1,
        Callback::new(move || {
            Box!(modifier = Modifier.size((10.dp(), 10.dp())).test_tag("1"));
            let a = a.clone();
            DisposableEffect((), move |scope| {
                a.set(true);
                let a = a.clone();
                scope.on_dispose(move || a.set(false))
            });
        }),
    );
    ui.wait_for_idle();
    assert!(active.get());
    need.set(false);
    ui.wait_for_idle();
    handle.dispose();
    ui.wait_for_idle();
    assert!(!active.get());
    // Both kept for reuse, as the layout holding them was deactivated with them active.
    ui.on_node_with_tag("1").assert_does_not_exist().assert_is_deactivated();
    ui.on_node_with_tag("0").assert_does_not_exist().assert_is_deactivated();
}

/// A layout node counting its measurements and placements, its update asking for both, its
/// elements equal by `identity` only, as the test's `RemeasureAndRelayoutOnChangeModifierElement`:
/// not auto-invalidated, its update invalidating its measurement and placement itself. With `auto`,
/// the update's default invalidation asks for both instead.
#[derive(Clone)]
struct RemeasureAndRelayoutOnChange {
    measured: Rc<Cell<i32>>,
    placed: Rc<Cell<i32>>,
    identity: i32,
    auto: bool,
}

struct RemeasureAndRelayoutOnChangeNode {
    measured: Rc<Cell<i32>>,
    placed: Rc<Cell<i32>>,
    handle: Option<compose_ui::NodeHandle>,
    auto: bool,
}

impl ModifierNodeElement for RemeasureAndRelayoutOnChange {
    type Node = RemeasureAndRelayoutOnChangeNode;
    fn create(&self) -> RemeasureAndRelayoutOnChangeNode {
        RemeasureAndRelayoutOnChangeNode {
            measured: self.measured.clone(),
            placed: self.placed.clone(),
            handle: None,
            auto: self.auto,
        }
    }
    fn update(&self, node: &mut RemeasureAndRelayoutOnChangeNode) {
        node.measured = self.measured.clone();
        node.placed = self.placed.clone();
        if let Some(handle) = node.handle.filter(|_| !self.auto) {
            handle.invalidate_measurement();
            handle.invalidate_placement();
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl ModifierNode for RemeasureAndRelayoutOnChangeNode {
    fn should_auto_invalidate(&self) -> bool {
        self.auto
    }
    fn on_attach(&mut self, context: &mut compose_ui::NodeContext<'_>) {
        self.handle = Some(context.handle());
    }
    fn as_layout(&mut self) -> Option<&mut dyn compose_ui::LayoutModifierNode> {
        Some(self)
    }
}

impl compose_ui::LayoutModifierNode for RemeasureAndRelayoutOnChangeNode {
    fn measure(
        &mut self,
        scope: &mut compose_ui::MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> compose_ui::MeasureResult {
        let placeable = scope.measure(content, constraints);
        self.measured.set(self.measured.get() + 1);
        let placed = self.placed.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            placed.set(placed.get() + 1);
            placement.place(&placeable, 0, 0);
        })
    }
}

/// The test's scene: a slot kept for reuse, precomposed again with a new element, premeasured, then
/// taken by the measurement: measured once, by the premeasure, and placed once, by the placement.
fn precompose_over_what_was_kept_with_a_new_element(auto: bool) {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (measured, placed) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let element = |identity| RemeasureAndRelayoutOnChange {
        measured: measured.clone(),
        placed: placed.clone(),
        identity,
        auto,
    };
    let (add, modifier) = (
        mutable_state_of(true),
        mutable_state_of_with(Rc::new(element(0)), ReferentialEqualityPolicy),
    );
    let m = modifier.handle();
    let content = Callback::new(move || {
        Box!(move |_| {
            Box!(modifier = Modifier.then_custom((*m.get()).clone()));
        });
    });
    let last_constraints = Rc::new(Cell::new(Constraints::default()));
    let (a, c, l) = (add.handle(), content.clone(), last_constraints.clone());
    let policy = subcompose_policy(move |scope, constraints| {
        l.set(constraints);
        let items: Vec<Placeable> = if a.get() {
            let measurables = scope.subcompose((), c.clone());
            measurables.into_iter().map(|it| scope.measure(it, constraints)).collect()
        } else {
            Vec::new()
        };
        for item in &items {
            scope.place(item, 0, 0);
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    add.set(false);
    ui.wait_for_idle();
    measured.set(0);
    placed.set(0);
    modifier.set(Rc::new(element(1)));
    let handle = state.precompose((), content);
    ui.wait_for_idle();
    assert_eq!((measured.get(), placed.get()), (0, 0));
    handle.premeasure(0, last_constraints.get());
    ui.wait_for_idle();
    assert_eq!((measured.get(), placed.get()), (1, 0));
    add.set(true);
    ui.wait_for_idle();
    assert_eq!((measured.get(), placed.get()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, precomposeOverReusedNodeWithUpdatedModifierIsNotCausingEarlyRemeasureForIt2
#[test]
fn precomposing_over_what_was_kept_with_a_new_element_measures_and_places_it_once() {
    precompose_over_what_was_kept_with_a_new_element(false);
}

/// The same, the update's default invalidation asking for the measurement and placement.
#[test]
fn precomposing_over_what_was_kept_with_an_auto_invalidated_element_measures_and_places_it_once() {
    precompose_over_what_was_kept_with_a_new_element(true);
}

/// Review: a brand-new precomposed slot (never measured yet, `constraints == None`) whose modifier
/// updates and invalidates measurement before `premeasure` must neither measure itself early nor
/// trigger a parent `SubcomposeLayout` remeasure.
#[test]
fn a_new_precomposed_slot_invalidated_before_premeasure_waits_for_it() {
    let state = SubcomposeLayoutState::new();
    let (measured, placed, layout_measures) =
        (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let element = |identity| RemeasureAndRelayoutOnChange {
        measured: measured.clone(),
        placed: placed.clone(),
        identity,
        auto: false,
    };
    let (add, modifier) = (
        mutable_state_of(false),
        mutable_state_of_with(Rc::new(element(0)), ReferentialEqualityPolicy),
    );
    let m = modifier.handle();
    let content = Callback::new(move || {
        Box!(move |_| {
            Box!(modifier = Modifier.then_custom((*m.get()).clone()));
        });
    });
    let last_constraints = Rc::new(Cell::new(Constraints::default()));
    let (a, c, l, lm) =
        (add.handle(), content.clone(), last_constraints.clone(), layout_measures.clone());
    let policy = subcompose_policy(move |scope, constraints| {
        lm.set(lm.get() + 1);
        l.set(constraints);
        let items: Vec<Placeable> = if a.get() {
            scope
                .subcompose((), c.clone())
                .into_iter()
                .map(|it| scope.measure(it, constraints))
                .collect()
        } else {
            Vec::new()
        };
        for item in &items {
            scope.place(item, 0, 0);
        }
        scope.layout(10, 10)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!(layout_measures.get(), 1);
    let handle = state.precompose((), content);
    ui.wait_for_idle();
    assert_eq!(layout_measures.get(), 1, "precomposing measures nothing");
    modifier.set(Rc::new(element(1)));
    ui.wait_for_idle();
    assert_eq!(layout_measures.get(), 1, "SubcomposeLayout not remeasured by unmeasured slot");
    assert_eq!((measured.get(), placed.get()), (0, 0));
    handle.premeasure(0, last_constraints.get());
    ui.wait_for_idle();
    assert_eq!((measured.get(), placed.get()), (1, 0));
    add.set(true);
    ui.wait_for_idle();
    assert_eq!((measured.get(), placed.get()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, measureWidthTooLarge
#[test]
fn a_width_too_large_panics() {
    assert!(a_layout_of(1 << 24, 100));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, measureHeightTooLarge
#[test]
fn a_height_too_large_panics() {
    assert!(a_layout_of(100, 1 << 24));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, nestedDisposeIsCalledInOrder
#[test]
fn nested_box_with_constraints_are_disposed_last_first() {
    let (order, active): (Rc<RefCell<Vec<&str>>>, _) = (Rc::default(), mutable_state_of(true));
    let (o, a) = (order.clone(), active.handle());
    let mut ui = compose_test(move || {
        if a.get() {
            let o = o.clone();
            BoxWithConstraints!(move |_| {
                let disposed = |name: &'static str| {
                    let o = o.clone();
                    move || o.borrow_mut().push(name)
                };
                let inner1 = disposed("inner 1");
                BoxWithConstraints!(move |_| on_dispose(inner1.clone()));
                on_dispose(disposed("outer"));
                let inner2 = disposed("inner 2");
                BoxWithConstraints!(move |_| on_dispose(inner2.clone()));
            });
        }
    });
    ui.wait_for_idle();
    active.set(false);
    ui.wait_for_idle();
    assert_eq!(*order.borrow(), ["inner 2", "outer", "inner 1"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, getAlignmentsOnSubcomposeLayoutContent
#[test]
fn a_slot_s_alignment_line_reaches_above_the_layout() {
    let baseline = Rc::new(Cell::new(None));
    let b = baseline.clone();
    let mut ui = compose_test(move || {
        let b = b.clone();
        Layout!(
            measure_policy = measure_policy(move |scope, measurables, constraints| {
                let constraints = Constraints::new(
                    constraints.min_width,
                    constraints.max_width,
                    0,
                    constraints.max_height,
                );
                let title = scope.measure(measurables[0], constraints);
                b.set(Some(scope.alignment_line(&title, compose_ui::FIRST_BASELINE)));
                scope.place(&title, 0, 0);
                scope.layout(title.width, title.height)
            }),
            || {
                let content = Callback::new(|| {
                    Layout!(
                        measure_policy = measure_policy(|scope, _, _| {
                            scope.provide_alignment_lines([(compose_ui::FIRST_BASELINE, 100)]);
                            scope.layout(10, 10)
                        }),
                        || {}
                    );
                });
                SubcomposeLayout!(
                    measure_policy = subcompose_policy(move |scope, constraints| {
                        let placeable = measure_slot(scope, "0", constraints, content.clone());
                        scope.place(&placeable, 0, 0);
                        scope.layout(placeable.width, placeable.height)
                    })
                );
            }
        );
    });
    ui.wait_for_idle();
    assert_eq!(baseline.get(), Some(100));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, noNotMeasuredWrappersInTheHierarchy
#[test]
fn every_coordinates_above_a_slot_s_layout_is_attached_and_measured() {
    let coordinates: Rc<Cell<Option<compose_ui::LayoutCoordinates>>> = Rc::default();
    let size = Rc::new(Cell::new(None));
    let (c, s) = (coordinates.clone(), size.clone());
    let content = Callback::new(move || {
        let (c, s) = (c.clone(), s.clone());
        Box!(
            modifier = Modifier
                .fill_max_size()
                .on_globally_positioned(move |it: compose_ui::LayoutCoordinates| c.set(Some(it)))
                .on_size_changed(move |it: IntSize| s.set(Some(it)))
        );
    });
    let mut ui = compose_test(move || {
        let content = content.clone();
        Box!(move |_| {
            let content = content.clone();
            SubcomposeLayout!(
                measure_policy = subcompose_policy(move |scope, constraints| {
                    let placeable = measure_slot(scope, "0", constraints, content.clone());
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                })
            );
        });
    });
    ui.wait_for_idle();
    let mut current = coordinates.get();
    let mut seen = 0;
    while let Some(it) = current {
        assert!(it.is_attached());
        assert_eq!(Some(it.size()), size.get());
        current = it.parent_coordinates();
        seen += 1;
    }
    assert!(seen > 2, "{seen} coordinates up to the root");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, subcomposeLayout_movedToDifferentGroup
#[test]
fn a_box_with_constraints_in_movable_content_moves_into_a_box() {
    let wrapped = mutable_state_of(false);
    let w = wrapped.handle();
    let mut ui = compose_test(move || {
        let content = remember(move || {
            movable_content_of(move || {
                BoxWithConstraints!(move |_| {
                    Box!(modifier = Modifier.test_tag(w.get().to_string()));
                });
            })
        });
        if w.get() {
            Box!(move |_| content());
        } else {
            content();
        }
    });
    ui.wait_for_idle();
    wrapped.set(true);
    ui.wait_for_idle();
    ui.on_node_with_tag("true").assert_exists();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, forceMeasureOfInactiveElementFromLaunchedEffect
#[test]
fn a_forced_measurement_from_an_effect_brings_a_slot_back() {
    let (active, force) = (mutable_state_of(true), mutable_state_of(false));
    let remeasurer: Rc<Cell<Option<Remeasurement>>> = Rc::default();
    let r = remeasurer.clone();
    let modifier: Rc<dyn RemeasurementModifier> = Rc::new(move |it| r.set(Some(it)));
    let (a, f) = (active.handle(), force.handle());
    let content = Callback::new(|| {
        Box!(modifier = Modifier);
    });
    let policy = subcompose_policy(move |scope, constraints| {
        let placeables: Vec<Placeable> = if a.get() {
            let measurables = scope.subcompose((), content.clone());
            measurables.into_iter().map(|it| scope.measure(it, constraints)).collect()
        } else {
            f.set(true);
            Vec::new()
        };
        for placeable in &placeables {
            scope.place(placeable, 0, 0);
        }
        scope.layout(0, 0)
    });
    let (a, f, r) = (active.handle(), force.handle(), remeasurer.clone());
    let mut ui = compose_test(move || {
        let state =
            remember(|| SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1)));
        SubcomposeLayoutWithState!(
            state,
            modifier = Modifier.remeasurement_modifier(modifier.clone()),
            measure_policy = policy.clone()
        );
        if f.get() {
            let r = r.clone();
            LaunchedEffect((), async move {
                a.set(true);
                r.get().expect("attached").force_remeasure();
            });
        }
    });
    ui.wait_for_idle();
    active.set(false);
    ui.wait_until(|_| active.get());
    ui.wait_for_idle();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, notSubcomposedAnymoreNodesAreNotRecomposed
#[test]
fn a_slot_no_longer_composed_isnt_recomposed_for_what_its_layout_wrote() {
    let state = SubcomposeLayoutState::with_policy(subcompose_slot_reuse_policy(1));
    let (flag, updated) = (mutable_state_of(true), mutable_state_of(true));
    let runs = Rc::new(Cell::new(0));
    let (u, r) = (updated.handle(), runs.clone());
    let content = Callback::new(move || {
        r.set(r.get() + 1);
        if u.get() {
            Box!(modifier = Modifier.test_tag("tag"));
        }
    });
    let (f, u) = (flag.handle(), updated.handle());
    let policy = subcompose_policy(move |scope, _| {
        u.set(f.get());
        compose_runtime::snapshots::Snapshot::send_apply_notifications();
        if f.get() {
            // Placed, for the semantics to hold it, where Compose's hold what's not placed too.
            for measurable in scope.subcompose(0, content.clone()) {
                let placeable = scope.measure(measurable, Constraints::default());
                scope.place(&placeable, 0, 0);
            }
        }
        scope.layout(100, 100)
    });
    let st = state.clone();
    let mut ui = compose_test(move || {
        SubcomposeLayoutWithState!(st.clone(), measure_policy = policy.clone())
    });
    ui.wait_for_idle();
    assert_eq!(runs.get(), 1);
    ui.on_node_with_tag("tag").assert_exists();
    flag.set(false);
    ui.wait_for_idle();
    // Deactivated before its content could recompose: kept, its box still there, not shown.
    assert_eq!(runs.get(), 1, "deactivated slot did not recompose");
    assert_eq!(state.reusable_count(), 1);
    ui.on_node_with_tag("tag").assert_does_not_exist().assert_is_deactivated();
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/SubcomposeLayoutTest.kt, remeasureRequestDuringSubcompositionIsNotSkippedForNotPlacedChild
#[test]
fn a_new_slot_content_measures_its_layout_again_though_it_isnt_placed() {
    let size = mutable_state_of(100);
    let s = size.handle();
    let policy = subcompose_policy(move |scope, constraints| {
        // The child doesn't read the state: a new content each time has the size.
        let child = s.get();
        let content = Callback::new(move || {
            Box!(move |_| {
                Box!(
                    modifier = Modifier.layout(move |scope, measurable, _| {
                        let placeable = scope.measure(measurable, Constraints::fixed(child, child));
                        scope.place(&placeable, 0, 0);
                        scope.layout(placeable.width, placeable.height)
                    })
                );
            });
        });
        let fab = measure_slot(scope, (), constraints, content);
        scope.layout(fab.width, fab.height)
    });
    let mut ui = compose_test(move || {
        let policy = policy.clone();
        CompositionLocalProvider(
            compose_ui::LOCAL_DENSITY.provides(compose_ui::Density::new(1.0)),
            move || {
                SubcomposeLayout!(
                    modifier = Modifier.test_tag("node"),
                    measure_policy = policy.clone()
                );
            },
        );
    });
    ui.wait_for_idle();
    size.set(150);
    ui.wait_for_idle();
    ui.on_node_with_tag("node").assert_width_is_equal_to(150.dp());
}
