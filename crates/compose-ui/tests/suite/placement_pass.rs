//! The placement pass: `MeasureScope::layout_with`'s blocks (docs/parity/design/
//! placement-and-lookahead.md §3, step PL-1).

use crate::support;

use std::cell::Cell;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::{SemanticsNode, UiHost};
use compose_ui::*;
use support::*;

type Count = Rc<Cell<u32>>;

fn count() -> Count {
    Rc::default()
}

fn bump(count: &Count) {
    count.set(count.get() + 1);
}

/// A leaf of `size` pixels.
fn sized(size: i32) -> Modifier {
    Modifier.size((size as f32).dp())
}

/// A layout placing its only child in a block at what `at` says, counting its measurements and
/// its block's runs.
fn placing(
    measures: &Count,
    blocks: &Count,
    at: impl Fn() -> i32 + 'static,
) -> Rc<dyn MeasurePolicy> {
    let (measures, blocks) = (measures.clone(), blocks.clone());
    let at = Rc::new(at);
    measure_policy(move |scope, measurables, constraints| {
        bump(&measures);
        let placeable = scope.measure(measurables[0], constraints);
        let (blocks, at) = (blocks.clone(), at.clone());
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            bump(&blocks);
            let x = at();
            placement.place(&placeable, x, 0);
        })
    })
}

/// The bounds of the node tagged `tag`, if it's in the semantics (placed).
fn bounds_of(host: &UiHost, tag: &str) -> Option<Rect> {
    fn find(node: &SemanticsNode, tag: &str) -> Option<Rect> {
        let tagged = node.config.get(&SemanticsProperties::TEST_TAG);
        if tagged.is_some_and(|tagged| tagged.as_str() == tag) {
            return Some(node.bounds);
        }
        node.children.iter().find_map(|child| find(child, tag))
    }
    find(&host.semantics_tree(true), tag)
}

fn left_of(host: &UiHost, tag: &str) -> f32 {
    bounds_of(host, tag).unwrap_or_else(|| panic!("no node {tag}")).left
}

// ---- ModelReadsTest ----------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, useDifferentModelsInMeasureAndPosition
#[test]
fn a_block_s_read_places_again_and_a_measure_s_measures_and_places() {
    let (measure_model, position_model) = (mutable_state_of(5), mutable_state_of(5));
    let (measures, blocks) = (count(), count());
    let (mm, pm, m, b) =
        (measure_model.handle(), position_model.handle(), measures.clone(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        bump(&m);
        let b = b.clone();
        scope.layout_with(mm.get(), 10, move |_| {
            pm.get();
            bump(&b);
        })
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| {})));
    assert_eq!((measures.get(), blocks.get()), (1, 1));
    measure_model.set(10);
    host.frame();
    assert_eq!((measures.get(), blocks.get()), (2, 2), "measuring places again");
    position_model.set(15);
    host.frame();
    assert_eq!((measures.get(), blocks.get()), (2, 3), "only placed again");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, layoutReactsOnCorrectModelsChanges
#[test]
fn a_block_reacts_only_to_what_it_read_last() {
    let (enabled, model) = (mutable_state_of(true), mutable_state_of(0));
    let blocks = count();
    let (e, v, b) = (enabled.handle(), model.handle(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        let b = b.clone();
        scope.layout_with(10, 10, move |_| {
            if e.get() {
                v.get();
            }
            bump(&b);
        })
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| {})));
    assert_eq!(blocks.get(), 1);
    model.set(1);
    host.frame();
    assert_eq!(blocks.get(), 2);
    enabled.set(false);
    host.frame();
    assert_eq!(blocks.get(), 3);
    model.set(2);
    host.frame();
    assert_eq!(blocks.get(), 3, "not read any more");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, layoutStopsReactingOnModelsAfterDetaching
#[test]
fn a_block_gone_with_its_node_reacts_to_nothing() {
    let (shown, model) = (mutable_state_of(true), mutable_state_of(0));
    let blocks = count();
    let (s, v, b) = (shown.handle(), model.handle(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        let b = b.clone();
        scope.layout_with(10, 10, move |_| {
            v.get();
            bump(&b);
        })
    });
    let mut host = host(move || {
        if s.get() {
            Layout(Modifier, policy.clone(), callback(|| {}));
        }
    });
    assert_eq!(blocks.get(), 1);
    shown.set(false);
    host.frame();
    model.set(1);
    host.frame();
    assert_eq!(blocks.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, remeasureRequestForTheNodeBeingLaidOut
#[test]
fn a_block_asking_for_a_measurement_of_its_node_gets_it_in_the_frame() {
    let (remeasure, relayout) = (mutable_state_of(0), mutable_state_of(0));
    let seen = Rc::new(Cell::new(-1));
    let changed = Rc::new(Cell::new(false));
    let (rm, rl, s, c) = (remeasure.handle(), relayout.handle(), seen.clone(), changed.clone());
    let policy = measure_policy(move |scope, _, _| {
        s.set(rm.get());
        let c = c.clone();
        scope.layout_with(100, 100, move |_| {
            if rl.get() != 0 && !c.replace(true) {
                rm.set(1);
            }
        })
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| {})));
    relayout.set(1);
    host.frame();
    assert_eq!(seen.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, relayoutRequestForTheNodeBeingMeasured
#[test]
fn a_measurement_asking_to_place_its_node_again_gets_it() {
    let (remeasure, relayout) = (mutable_state_of(0), mutable_state_of(0));
    let (measures, blocks) = (count(), count());
    let (rm, rl, m, b) = (remeasure.handle(), relayout.handle(), measures.clone(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        if rm.get() != 0 {
            rl.set(1);
        }
        bump(&m);
        let b = b.clone();
        scope.layout_with(100, 100, move |_| {
            rl.get();
            bump(&b);
        })
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| {})));
    let before = (measures.get(), blocks.get());
    remeasure.set(1);
    host.frame();
    assert!(measures.get() > before.0);
    assert!(blocks.get() > before.1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, relayoutRequestForTheNodeBeingLaidOut
#[test]
fn a_block_asking_to_place_its_node_again_ends() {
    let model = mutable_state_of(0);
    let blocks = count();
    let (v, b) = (model.handle(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        let b = b.clone();
        scope.layout_with(100, 100, move |_| {
            if v.get() == 1 {
                v.set(2);
            }
            bump(&b);
        })
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| {})));
    blocks.set(0);
    model.set(1);
    host.frame();
    assert_eq!(blocks.get(), 2, "for the write, then the write it read");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, layoutModifierReactsOnCorrectModelsChanges
#[test]
fn a_layout_modifier_s_block_reacts_to_what_it_read() {
    let (enabled, model) = (mutable_state_of(true), mutable_state_of(0));
    let (measures, blocks) = (count(), count());
    let (e, v, m, b) = (enabled.handle(), model.handle(), measures.clone(), blocks.clone());
    let mut host = host(move || {
        let (m, b) = (m.clone(), b.clone());
        Leaf(Modifier.layout(move |scope, measurable, constraints| {
            bump(&m);
            let placeable = scope.measure(measurable, constraints);
            let b = b.clone();
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                if e.get() {
                    v.get();
                }
                bump(&b);
                placement.place(&placeable, 0, 0);
            })
        }));
    });
    assert_eq!((measures.get(), blocks.get()), (1, 1));
    model.set(1);
    host.frame();
    assert_eq!((measures.get(), blocks.get()), (1, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModelReadsTest.kt, parentIsNotRelaidOutWhenChildLayoutModifierUsesState
#[test]
fn a_parent_isn_t_placed_again_for_a_child_modifier_s_block() {
    let model = mutable_state_of(0);
    let (parent_measures, parent_blocks, child_blocks) = (count(), count(), count());
    let (v, c) = (model.handle(), child_blocks.clone());
    let parent = placing(&parent_measures, &parent_blocks, || 0);
    let mut host = host(move || {
        let c = c.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || {
                let c = c.clone();
                Leaf(Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let c = c.clone();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        v.get();
                        bump(&c);
                        placement.place(&placeable, 0, 0);
                    })
                }));
            }),
        );
    });
    assert_eq!((parent_blocks.get(), child_blocks.get()), (1, 1));
    model.set(1);
    host.frame();
    assert_eq!((parent_blocks.get(), child_blocks.get()), (1, 2));
    assert_eq!(parent_measures.get(), 1);
}

// ---- MeasureInPlacementTest ---------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureInPlacementTest.kt, measureInModifierPlacement
#[test]
fn a_modifier_s_block_may_measure_what_it_places() {
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let (modifier_measures, measures, blocks) = (count(), count(), count());
    let (s, mm, m, b) = (size.clone(), modifier_measures.clone(), measures.clone(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        bump(&m);
        let b = b.clone();
        scope.layout_with(10, 10, move |_| bump(&b))
    });
    let mut host = host(move || {
        let (s, mm) = (s.clone(), mm.clone());
        let measuring = Modifier.layout(move |scope, measurable, constraints| {
            bump(&mm);
            let s = s.clone();
            scope.layout_with(100, 100, move |placement| {
                let placeable = placement.measure(measurable, constraints);
                s.set(placeable.size());
                placement.place(&placeable, 0, 0);
            })
        });
        Layout(measuring, policy.clone(), callback(|| {}));
    });
    assert_eq!(size.get(), IntSize::new(10, 10));
    assert_eq!((modifier_measures.get(), measures.get(), blocks.get()), (1, 1, 1));
    host.frame();
    host.frame();
    assert_eq!((modifier_measures.get(), measures.get(), blocks.get()), (1, 1, 1), "at rest");
    host.with_tree(|tree| assert!(!tree.layout(IntSize::new(200, 100)), "nothing's waiting"));
}

/// A block measuring its modifier's content again, running again: it's measured again, as a
/// coordinator's `measure`, which keeps nothing, and the inner block is the new measurement's.
#[test]
fn a_modifier_s_block_run_again_measures_its_content_again() {
    let model = mutable_state_of(0);
    let (measures, blocks) = (count(), count());
    let (v, m, b) = (model.handle(), measures.clone(), blocks.clone());
    let policy = measure_policy(move |scope, _, _| {
        bump(&m);
        let b = b.clone();
        scope.layout_with(10, 10, move |_| bump(&b))
    });
    let mut host = host(move || {
        let measuring = Modifier.layout(move |scope, measurable, constraints| {
            scope.layout_with(100, 100, move |placement| {
                v.get();
                let placeable = placement.measure(measurable, constraints);
                placement.place(&placeable, 0, 0);
            })
        });
        Layout(measuring, policy.clone(), callback(|| {}));
    });
    model.set(1);
    host.frame();
    assert_eq!((measures.get(), blocks.get()), (2, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureInPlacementTest.kt, measureInLayoutPlacement
#[test]
fn a_layout_s_block_may_measure_what_it_places() {
    let size = Rc::new(Cell::new(IntSize::ZERO));
    let s = size.clone();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let (s, child) = (s.clone(), measurables[0]);
        scope.layout_with(100, 100, move |placement| {
            let placeable = placement.measure(child, constraints);
            s.set(placeable.size());
            placement.place(&placeable, 0, 0);
        })
    });
    let _host = host(move || Layout(Modifier, policy.clone(), callback(|| Leaf(sized(10)))));
    assert_eq!(size.get(), IntSize::new(10, 10));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureInPlacementTest.kt, remeasureRequestForANodeWhichIsNotYetPlacedButMeasuredAlready
#[test]
fn a_measurement_asked_of_a_node_measured_and_not_yet_placed_happens() {
    let (need, top) = (mutable_state_of(false), mutable_state_of(0));
    let (n, t) = (need.handle(), top.handle());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let (first, second) = (measurables[0], measurables[1]);
        scope.layout_with(constraints.max_width, constraints.max_height, move |placement| {
            let mut height = 0;
            if n.get() {
                let placeable = placement.measure(
                    first,
                    Constraints::new(0, Constraints::INFINITY, 0, Constraints::INFINITY),
                );
                if compose_runtime::snapshots::Snapshot::without_read_observation(|| t.get()) == 0 {
                    t.set(10);
                }
                placement.place(&placeable, 0, 0);
                height = placeable.height;
            }
            let placeable = placement.measure(
                second,
                Constraints::new(0, Constraints::INFINITY, 0, Constraints::INFINITY),
            );
            placement.place(&placeable, 0, height);
        })
    });
    let state_sized = move || {
        Modifier.layout(move |scope, measurable, _| {
            let size = t.get();
            let placeable = scope.measure(measurable, Constraints::fixed(size, size));
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        })
    };
    let mut host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                Leaf(state_sized().test_tag("top"));
                Leaf(sized(10).test_tag("bottom"));
            }),
        );
    });
    need.set(true);
    host.frame();
    host.frame();
    assert_eq!(bounds_of(&host, "bottom").expect("placed").top, 10.0);
    assert_eq!(bounds_of(&host, "top").expect("placed").top, 0.0);
}

// ---- MeasuringPlacingTwiceIsNotAllowedTest ------------------------------------------------------

/// A layout over one child doing `in_measure` and leaving `in_block` to a block.
fn over_a_child(
    in_measure: impl Fn(&mut MeasureScope<'_>, Measurable, Constraints) + 'static,
    in_block: impl Fn(&mut PlacementScope<'_>, Measurable, Constraints) + 'static,
) {
    let in_block = Rc::new(in_block);
    let policy = measure_policy(move |scope, measurables, constraints| {
        let child = measurables[0];
        in_measure(scope, child, constraints);
        let in_block = in_block.clone();
        scope.layout_with(100, 100, move |placement| in_block(placement, child, constraints))
    });
    let _host = host(move || Layout(Modifier, policy.clone(), callback(|| Leaf(sized(50)))));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasuringPlacingTwiceIsNotAllowedTest.kt, measureTwiceInLayoutBlock
#[test]
#[should_panic(expected = "twice")]
fn measuring_twice_in_a_block_panics() {
    over_a_child(
        |_, _, _| {},
        |placement, child, constraints| {
            placement.measure(child, constraints);
            placement.measure(child, constraints);
        },
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasuringPlacingTwiceIsNotAllowedTest.kt, measureInBothStages
#[test]
#[should_panic(expected = "twice")]
fn measuring_in_the_measurement_and_its_block_panics() {
    over_a_child(
        |scope, child, constraints| {
            scope.measure(child, constraints);
        },
        |placement, child, constraints| {
            placement.measure(child, constraints);
        },
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasuringPlacingTwiceIsNotAllowedTest.kt, placeTwiceWithTheSamePosition
#[test]
#[should_panic(expected = "placed twice")]
fn placing_twice_in_a_block_at_one_position_panics() {
    over_a_child(
        |_, _, _| {},
        |placement, child, constraints| {
            let placeable = placement.measure(child, constraints);
            placement.place(&placeable, 0, 0);
            placement.place(&placeable, 0, 0);
        },
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasuringPlacingTwiceIsNotAllowedTest.kt, placeTwiceWithDifferentPositions
#[test]
#[should_panic(expected = "placed twice")]
fn placing_twice_in_a_block_at_two_positions_panics() {
    over_a_child(
        |_, _, _| {},
        |placement, child, constraints| {
            let placeable = placement.measure(child, constraints);
            placement.place(&placeable, 0, 0);
            placement.place(&placeable, 10, 10);
        },
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/DrawReorderingTest.kt, placingTheSameItemTwiceIsNotAllowedAsItBreaksTheDrawingOrder
#[test]
#[should_panic(expected = "placed twice")]
fn placing_twice_in_a_measurement_panics() {
    let policy = measure_policy(|scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 0, 0);
        scope.place(&placeable, 0, 0);
        scope.layout(10, 10)
    });
    let _host = host(move || Layout(Modifier, policy.clone(), callback(|| Leaf(sized(5)))));
}

/// A block running again may place what it placed: it's another run.
#[test]
fn a_block_run_again_places_what_it_placed() {
    let at = mutable_state_of(0);
    let (measures, blocks) = (count(), count());
    let a = at.handle();
    let policy = placing(&measures, &blocks, move || a.get());
    let mut host = host(move || {
        Layout(Modifier, policy.clone(), callback(|| Leaf(sized(5).test_tag("child"))));
    });
    assert_eq!(left_of(&host, "child"), 0.0);
    at.set(7);
    host.frame();
    assert_eq!(left_of(&host, "child"), 7.0);
    assert_eq!((measures.get(), blocks.get()), (1, 2));
}

// ---- MeasureAndLayoutDelegateTest ---------------------------------------------------------------

/// A parent measuring its child in a block; what the child's size is changing.
fn measured_in_block(
    parent_measures: &Count,
    parent_blocks: &Count,
    size: MutableState<i32>,
    child_blocks: &Count,
) -> UiHost {
    let (m, b, c) = (parent_measures.clone(), parent_blocks.clone(), child_blocks.clone());
    let policy = measure_policy(move |scope, measurables, constraints| {
        bump(&m);
        let (b, child) = (b.clone(), measurables[0]);
        scope.layout_with(100, 100, move |placement| {
            bump(&b);
            let placeable = placement.measure(child, constraints);
            placement.place(&placeable, 0, 0);
        })
    });
    host(move || {
        let c = c.clone();
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                let c = c.clone();
                Leaf(Modifier.layout(move |scope, measurable, _| {
                    let size = size.get();
                    let placeable = scope.measure(measurable, Constraints::fixed(size, size));
                    let c = c.clone();
                    scope.layout_with(size, size, move |placement| {
                        bump(&c);
                        placement.place(&placeable, 0, 0);
                    })
                }));
            }),
        );
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureInLayoutBlockWithDifferentResult_parentRelaidOut
#[test]
fn a_child_measured_in_a_block_with_a_new_size_places_its_parent_again() {
    let size = mutable_state_of(5);
    let (measures, blocks, child_blocks) = (count(), count(), count());
    let mut host = measured_in_block(&measures, &blocks, size.handle(), &child_blocks);
    assert_eq!((measures.get(), blocks.get(), child_blocks.get()), (1, 1, 1));
    size.set(6);
    host.frame();
    assert_eq!(
        measures.get(),
        1,
        "childRemeasureInLayoutBlockWithDifferentResult_parentNotRemeasured"
    );
    assert_eq!(blocks.get(), 2);
    assert_eq!(child_blocks.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRemeasureInLayoutBlockWithTheSameResult_parentNotRelaidOut
#[test]
fn a_child_measured_in_a_block_with_the_same_size_leaves_its_parent() {
    let size = mutable_state_of(5);
    let extra = mutable_state_of(0);
    let (measures, blocks, child_blocks) = (count(), count(), count());
    let (e, c, m, b) = (extra.handle(), child_blocks.clone(), measures.clone(), blocks.clone());
    let s = size.handle();
    let policy = measure_policy(move |scope, measurables, constraints| {
        bump(&m);
        let (b, child) = (b.clone(), measurables[0]);
        scope.layout_with(100, 100, move |placement| {
            bump(&b);
            let placeable = placement.measure(child, constraints);
            placement.place(&placeable, 0, 0);
        })
    });
    let mut host = host(move || {
        let c = c.clone();
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                let c = c.clone();
                Leaf(Modifier.layout(move |scope, measurable, _| {
                    e.get();
                    let size = s.get();
                    let placeable = scope.measure(measurable, Constraints::fixed(size, size));
                    let c = c.clone();
                    scope.layout_with(size, size, move |placement| {
                        bump(&c);
                        placement.place(&placeable, 0, 0);
                    })
                }));
            }),
        );
    });
    extra.set(1);
    host.frame();
    assert_eq!((measures.get(), blocks.get()), (1, 1));
    assert_eq!(child_blocks.get(), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, childRelayoutRequest_childNotRemeasured
#[test]
fn placing_a_child_again_measures_nothing_and_leaves_its_parent() {
    let model = mutable_state_of(0);
    let (parent_measures, parent_blocks, child_measures, child_blocks) =
        (count(), count(), count(), count());
    let (v, cm, cb) = (model.handle(), child_measures.clone(), child_blocks.clone());
    let parent = placing(&parent_measures, &parent_blocks, || 0);
    let child = measure_policy(move |scope, _, _| {
        bump(&cm);
        let cb = cb.clone();
        scope.layout_with(10, 10, move |_| {
            v.get();
            bump(&cb);
        })
    });
    let mut host = host(move || {
        let child = child.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || Layout(Modifier, child.clone(), callback(|| {}))),
        );
    });
    model.set(1);
    host.frame();
    assert_eq!((parent_measures.get(), parent_blocks.get()), (1, 1));
    assert_eq!((child_measures.get(), child_blocks.get()), (1, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, parentRelayoutRequest_childNotAffected
#[test]
fn placing_a_parent_again_leaves_its_child_s_blocks() {
    let at = mutable_state_of(0);
    let (parent_measures, parent_blocks, child_measures, child_blocks) =
        (count(), count(), count(), count());
    let a = at.handle();
    let parent = placing(&parent_measures, &parent_blocks, move || a.get());
    let (cm, cb) = (child_measures.clone(), child_blocks.clone());
    let child = measure_policy(move |scope, _, _| {
        bump(&cm);
        let cb = cb.clone();
        scope.layout_with(10, 10, move |_| bump(&cb))
    });
    let mut host = host(move || {
        let child = child.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || Layout(Modifier, child.clone(), callback(|| {}))),
        );
    });
    at.set(1);
    host.frame();
    assert_eq!((parent_measures.get(), parent_blocks.get()), (1, 2));
    assert_eq!((child_measures.get(), child_blocks.get()), (1, 1));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, theWholeSubtreeIsNotPlacedWhenParentWasntPlaced
#[test]
fn a_child_a_block_stops_placing_isn_t_placed_nor_what_s_in_it() {
    let place = mutable_state_of(true);
    let p = place.handle();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if p.get() {
                placement.place(&placeable, 0, 0);
            }
        })
    });
    let mut host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(|| {
                Stack(sized(10).test_tag("child"), callback(|| Leaf(sized(5).test_tag("inner"))))
            }),
        );
    });
    assert!(bounds_of(&host, "inner").is_some());
    place.set(false);
    host.frame();
    assert!(bounds_of(&host, "child").is_none());
    assert!(bounds_of(&host, "inner").is_none());
    place.set(true);
    host.frame();
    assert!(bounds_of(&host, "inner").is_some());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasureAndLayoutDelegateTest.kt, requestRelayoutTriggersModifierRelayout
#[test]
fn invalidating_placement_runs_a_modifier_s_block() {
    struct Placing(Count, Rc<Cell<Option<NodeHandle>>>, Count);
    impl ModifierNode for Placing {
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            self.1.set(Some(context.handle()));
        }
    }
    impl LayoutModifierNode for Placing {
        fn measure(
            &mut self,
            scope: &mut MeasureScope<'_>,
            content: Measurable,
            constraints: Constraints,
        ) -> MeasureResult {
            bump(&self.2);
            let placeable = scope.measure(content, constraints);
            let blocks = self.0.clone();
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                bump(&blocks);
                placement.place(&placeable, 0, 0);
            })
        }
    }
    struct PlacingElement(Count, Rc<Cell<Option<NodeHandle>>>, Count);
    impl ModifierNodeElement for PlacingElement {
        type Node = Placing;
        fn create(&self) -> Placing {
            Placing(self.0.clone(), self.1.clone(), self.2.clone())
        }
        fn update(&self, _: &mut Placing) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let (blocks, measures) = (count(), count());
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let (b, h, m) = (blocks.clone(), handle.clone(), measures.clone());
    let mut host = host(move || {
        Leaf(Modifier.then_custom(PlacingElement(b.clone(), h.clone(), m.clone())));
    });
    assert_eq!((blocks.get(), measures.get()), (1, 1));
    handle.get().expect("attached").invalidate_placement();
    host.frame();
    assert_eq!((blocks.get(), measures.get()), (2, 1));
}

/// `invalidate_placement` of a node placing inside its measurement: it has only that to run.
#[test]
fn invalidating_placement_of_a_node_without_a_block_measures_it() {
    struct Fused(Count, Rc<Cell<Option<NodeHandle>>>);
    impl ModifierNode for Fused {
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            self.1.set(Some(context.handle()));
        }
    }
    impl LayoutModifierNode for Fused {
        fn measure(
            &mut self,
            scope: &mut MeasureScope<'_>,
            content: Measurable,
            constraints: Constraints,
        ) -> MeasureResult {
            bump(&self.0);
            let placeable = scope.measure(content, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }
    }
    struct FusedElement(Count, Rc<Cell<Option<NodeHandle>>>);
    impl ModifierNodeElement for FusedElement {
        type Node = Fused;
        fn create(&self) -> Fused {
            Fused(self.0.clone(), self.1.clone())
        }
        fn update(&self, _: &mut Fused) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let measures = count();
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let (m, h) = (measures.clone(), handle.clone());
    let mut host = host(move || Leaf(Modifier.then_custom(FusedElement(m.clone(), h.clone()))));
    handle.get().expect("attached").invalidate_placement();
    host.frame();
    assert_eq!(measures.get(), 2);
}

// ---- LayoutCooperationTest ----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/LayoutCooperationTest.kt, relayoutSkippingModifiersDoesntBreakCooperation
#[test]
fn a_block_run_again_keeps_a_child_centered_in_constraints_it_ignores() {
    let offset = mutable_state_of(0);
    let o = offset.handle();
    let policy = measure_policy(move |scope, measurables, _| {
        let placeable = scope.measure(measurables[0], Constraints::fixed(100, 100));
        scope.layout_with(100, 100, move |placement| {
            placement.place(&placeable, o.get(), o.get());
        })
    });
    let mut host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(|| {
                Stack(
                    Modifier.required_size((50.dp(), 40.dp())),
                    callback(|| Leaf(Modifier.test_tag("child"))),
                );
            }),
        );
    });
    let bounds = |host: &UiHost| bounds_of(host, "child").unwrap();
    assert_eq!((bounds(&host).left, bounds(&host).top), (25.0, 30.0));
    offset.set(10);
    host.frame();
    assert_eq!((bounds(&host).left, bounds(&host).top), (35.0, 40.0));
}

// ---- InvalidatingNotPlacedChildTest -------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/InvalidatingNotPlacedChildTest.kt, childRedrawAndRelayoutRequestedWhileNotPlaced
#[test]
fn a_child_changed_while_not_placed_shows_it_when_it_s_placed() {
    let place = mutable_state_of(true);
    let color = mutable_state_of(Color::GREEN);
    let (p, c) = (place.handle(), color.handle());
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if p.get() {
                placement.place(&placeable, 0, 0);
            }
        })
    });
    let mut host = host(move || {
        Layout(
            Modifier,
            parent.clone(),
            callback(move || {
                // Its block reads the color too, as the test's `useDuringLayout`.
                let layout = Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        c.get();
                        placement.place(&placeable, 0, 0);
                    })
                });
                Leaf(sized(10).then(layout).draw_behind(move |scope| {
                    scope.draw_rect(scope.bounds(), Paint::fill(c.get()));
                }));
            }),
        );
    });
    let drawn = |host: &UiHost| host.display_list().dump();
    let green = drawn(&host);
    place.set(false);
    host.frame();
    assert!(!drawn(&host).contains("rect "));
    color.set(Color::RED);
    host.frame();
    assert!(!drawn(&host).contains("rect "));
    place.set(true);
    host.frame();
    assert!(drawn(&host).contains("rect "));
    assert_ne!(drawn(&host), green, "the new color");
}

/// A block's child invalidated while it's not placed waits, and runs its block when it's placed.
#[test]
fn a_block_of_a_child_invalidated_while_not_placed_runs_when_it_s_placed() {
    let (place, model) = (mutable_state_of(true), mutable_state_of(0));
    let (p, v) = (place.handle(), model.handle());
    let seen = Rc::new(Cell::new(-1));
    let s = seen.clone();
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if p.get() {
                placement.place(&placeable, 0, 0);
            }
        })
    });
    let child = measure_policy(move |scope, _, _| {
        let s = s.clone();
        scope.layout_with(10, 10, move |_| s.set(v.get()))
    });
    let mut host = host(move || {
        let child = child.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || Layout(Modifier, child.clone(), callback(|| {}))),
        );
    });
    assert_eq!(seen.get(), 0);
    place.set(false);
    host.frame();
    model.set(1);
    let stats = host.frame();
    assert!(!stats.drew, "it only waits, as `requestRelayout` marks it pending");
    assert_eq!(seen.get(), 0, "not placed");
    place.set(true);
    host.frame();
    assert_eq!(seen.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/InvalidatingNotPlacedChildTest.kt, childIsNotDisplayedWhenIsNotPlacedAnymore
#[test]
fn a_child_a_block_stops_placing_isn_t_drawn_and_comes_back() {
    let place = mutable_state_of(true);
    let p = place.handle();
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            if p.get() {
                placement.place(&placeable, 0, 0);
            }
        })
    });
    let mut host = host(move || {
        Layout(
            Modifier,
            parent.clone(),
            callback(|| {
                Leaf(sized(10).background(Color::RED));
            }),
        );
    });
    let shows = |host: &UiHost| host.display_list().dump().contains("rect ");
    assert!(shows(&host));
    place.set(false);
    host.frame();
    assert!(!shows(&host));
    place.set(true);
    host.frame();
    assert!(shows(&host));
}

// ---- Right to left, and what a measurement's fast path keeps ------------------------------------

#[test]
fn a_block_mirrors_what_it_places_relatively_in_the_layout_s_width() {
    let policy = measure_policy(|scope, measurables, _| {
        let placeable = scope.measure(measurables[0], Constraints::fixed(10, 10));
        scope.layout_with(100, 20, move |placement| {
            placement.place_relative(&placeable, 5, 0);
        })
    });
    let _host = {
        let mut host = host(move || {
            let policy = policy.clone();
            CompositionLocalProvider(
                LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
                move || {
                    Layout(Modifier, policy.clone(), callback(|| Leaf(Modifier.test_tag("child"))));
                },
            );
        });
        host.frame();
        assert_eq!(left_of(&host, "child"), 85.0);
        host
    };
}

/// A measurement that stops using a block keeps none: the state it read places nothing.
#[test]
fn a_measurement_placing_itself_after_a_block_drops_it() {
    let (use_block, model) = (mutable_state_of(true), mutable_state_of(0));
    let (blocks, measures) = (count(), count());
    let (u, v, b, m) = (use_block.handle(), model.handle(), blocks.clone(), measures.clone());
    let policy = measure_policy(move |scope, _, _| {
        bump(&m);
        if u.get() {
            let b = b.clone();
            scope.layout_with(10, 10, move |_| {
                v.get();
                bump(&b);
            })
        } else {
            scope.layout(10, 10)
        }
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| {})));
    assert_eq!(blocks.get(), 1);
    use_block.set(false);
    host.frame();
    let measured = measures.get();
    model.set(1);
    host.frame();
    assert_eq!(blocks.get(), 1);
    assert_eq!(measures.get(), measured, "the read of a block that's gone measures nothing");
}

/// A block's child's block runs after its parent's, top down.
#[test]
fn blocks_run_top_down() {
    let order = Rc::new(std::cell::RefCell::new(Vec::new()));
    let (a, b) = (order.clone(), order.clone());
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        let a = a.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            a.borrow_mut().push("parent");
            placement.place(&placeable, 0, 0);
        })
    });
    let child = measure_policy(move |scope, _, _| {
        let b = b.clone();
        scope.layout_with(10, 10, move |_| b.borrow_mut().push("child"))
    });
    let _host = host(move || {
        let child = child.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || Layout(Modifier, child.clone(), callback(|| {}))),
        );
    });
    assert_eq!(*order.borrow(), ["parent", "child"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/CustomLayoutAndMeasureTest.kt, placeableMeasuredSize
#[test]
fn a_placeable_knows_its_size_before_its_constraints() {
    let policy = measure_policy(|scope, measurables, _| {
        let placeable = scope.measure(measurables[0], Constraints::fixed(50, 50));
        assert_eq!((placeable.measured_width(), placeable.measured_height()), (100, 100));
        assert_eq!((placeable.width, placeable.height), (50, 50));
        scope.layout_with(1, 1, |_| {})
    });
    let _host = host(move || {
        Layout(Modifier, policy.clone(), callback(|| Leaf(Modifier.required_size(100.dp()))));
    });
}

// ---- Layers, and what mixes ----------------------------------------------------------------------

/// A chain losing a layout modifier: the block of a layer that's gone is gone.
#[test]
fn a_chain_losing_a_layout_modifier_keeps_its_policy_s_block() {
    let on = mutable_state_of(true);
    let (blocks, o) = (count(), on.handle());
    let b = blocks.clone();
    let policy = measure_policy(move |scope, _, _| {
        let b = b.clone();
        scope.layout_with(10, 10, move |_| bump(&b))
    });
    let mut host = host(move || {
        let modifier = if o.get() {
            Modifier.layout(|scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            })
        } else {
            Modifier
        };
        Layout(modifier, policy.clone(), callback(|| {}));
    });
    assert_eq!(blocks.get(), 1);
    on.set(false);
    host.frame();
    assert_eq!(blocks.get(), 2);
    on.set(true);
    host.frame();
    assert_eq!(blocks.get(), 3);
}

/// Measured in the modifier's measurement and its block: what a first run may do is the layer's.
// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/MeasuringPlacingTwiceIsNotAllowedTest.kt, measureInBothStages
#[test]
#[should_panic(expected = "twice")]
fn a_modifier_measuring_in_its_measurement_and_its_block_panics() {
    let policy = measure_policy(|scope, _, _| scope.layout_with(10, 10, |_| {}));
    let _host = host(move || {
        let modifier = Modifier.layout(|scope, measurable, constraints| {
            scope.measure(measurable, constraints);
            scope.layout_with(10, 10, move |placement| {
                placement.measure(measurable, constraints);
            })
        });
        Layout(modifier, policy.clone(), callback(|| {}));
    });
}

/// A block running again measuring what its measurement measured, as `trackMeasurementByParent`.
#[test]
#[should_panic(expected = "twice")]
fn a_block_run_again_measuring_what_its_measurement_measured_panics() {
    let model = mutable_state_of(false);
    let v = model.handle();
    let policy = measure_policy(move |scope, measurables, constraints| {
        let child = measurables[0];
        scope.measure(child, constraints);
        scope.layout_with(10, 10, move |placement| {
            if v.get() {
                placement.measure(child, constraints);
            }
        })
    });
    let mut host = host(move || Layout(Modifier, policy.clone(), callback(|| Leaf(sized(1)))));
    model.set(true);
    host.frame();
}

/// `place` in a measurement beside a block: one place to place from.
#[test]
#[should_panic(expected = "left its placing to a block too")]
fn placing_in_a_measurement_and_leaving_it_to_a_block_panics() {
    let policy = measure_policy(|scope, measurables, constraints| {
        let (first, second) = (measurables[0], measurables[1]);
        let placeable = scope.measure(first, constraints);
        scope.place(&placeable, 0, 0);
        let other = scope.measure(second, constraints);
        scope.layout_with(10, 10, move |placement| placement.place(&other, 0, 0))
    });
    let _host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(|| {
                Leaf(sized(1));
                Leaf(sized(1));
            }),
        );
    });
}

/// A modifier placing its content twice: not the parent's, a layer's.
#[test]
#[should_panic(expected = "placed twice")]
fn a_modifier_placing_its_content_twice_panics() {
    let _host = host(|| {
        Leaf(Modifier.layout(|scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
            scope.place(&placeable, 1, 1);
            scope.layout(placeable.width, placeable.height)
        }));
    });
}

/// Blocks run outermost first, on the first frame and running again; the parent's before its
/// child's.
#[test]
fn blocks_run_from_the_outermost_layer_and_the_parent_first() {
    let order = Rc::new(std::cell::RefCell::new(Vec::new()));
    let model = mutable_state_of(0);
    let v = model.handle();
    let logged = {
        let order = order.clone();
        move |name: &'static str| {
            let order = order.clone();
            move || order.borrow_mut().push(name)
        }
    };
    let (parent_log, modifier_log, policy_log) =
        (logged("parent"), logged("modifier"), logged("policy"));
    let parent = measure_policy(move |scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        let log = parent_log.clone();
        scope.layout_with(placeable.width, placeable.height, move |placement| {
            v.get();
            log();
            placement.place(&placeable, 0, 0);
        })
    });
    let policy = measure_policy(move |scope, _, _| {
        let log = policy_log.clone();
        scope.layout_with(10, 10, move |_| {
            v.get();
            log();
        })
    });
    let mut host = host(move || {
        let (policy, modifier_log) = (policy.clone(), modifier_log.clone());
        Layout(
            Modifier,
            parent.clone(),
            callback(move || {
                let log = modifier_log.clone();
                let modifier = Modifier.layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let log = log.clone();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        v.get();
                        log();
                        placement.place(&placeable, 0, 0);
                    })
                });
                Layout(modifier, policy.clone(), callback(|| {}));
            }),
        );
    });
    assert_eq!(*order.borrow(), ["parent", "modifier", "policy"]);
    order.borrow_mut().clear();
    model.set(1);
    host.frame();
    assert_eq!(*order.borrow(), ["parent", "modifier", "policy"]);
}

/// A block running again under a repaint boundary: what it stops placing, or places in another
/// order, is drawn so, nothing having moved.
#[test]
fn a_block_run_again_draws_what_it_placed_differently() {
    let (reversed, only_first) = (mutable_state_of(false), mutable_state_of(false));
    let (r, o) = (reversed.handle(), only_first.handle());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let first = scope.measure(measurables[0], constraints);
        let second = scope.measure(measurables[1], constraints);
        scope.layout_with(10, 10, move |placement| {
            let reversed = r.get();
            if reversed {
                placement.place(&second, 0, 0);
            }
            placement.place(&first, 0, 0);
            if !reversed && !o.get() {
                placement.place(&second, 0, 0);
            }
        })
    });
    let mut host = host(move || {
        Layout(
            Modifier.repaint_boundary(),
            policy.clone(),
            callback(|| {
                Leaf(sized(5).background(Color::RED));
                Leaf(sized(5).background(Color::BLUE));
            }),
        );
    });
    let before = host.display_list().dump();
    reversed.set(true);
    host.frame();
    let swapped = host.display_list().dump();
    assert_ne!(swapped, before, "the other order");
    reversed.set(false);
    only_first.set(true);
    host.frame();
    assert!(host.display_list().dump().len() < before.len(), "one left");
}

/// Its parent measured it in a block, then in its measurement: a new size measures the parent.
#[test]
fn a_child_no_longer_measured_in_a_block_measures_its_parent_again() {
    let (in_block, size) = (mutable_state_of(true), mutable_state_of(5));
    let measures = count();
    let (i, s, m) = (in_block.handle(), size.handle(), measures.clone());
    let policy = measure_policy(move |scope, measurables, constraints| {
        bump(&m);
        let child = measurables[0];
        if i.get() {
            scope.layout_with(100, 100, move |placement| {
                let placeable = placement.measure(child, constraints);
                placement.place(&placeable, 0, 0);
            })
        } else {
            let placeable = scope.measure(child, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(100, 100)
        }
    });
    let mut host = host(move || {
        Layout(
            Modifier,
            policy.clone(),
            callback(move || {
                Leaf(Modifier.layout(move |scope, measurable, _| {
                    let size = s.get();
                    let placeable = scope.measure(measurable, Constraints::fixed(size, size));
                    scope.place(&placeable, 0, 0);
                    scope.layout(size, size)
                }));
            }),
        );
    });
    in_block.set(false);
    host.frame();
    let measured = measures.get();
    size.set(6);
    host.frame();
    assert_eq!(measures.get(), measured + 1);
}

/// `on_placed` hears a block running again, as a coordinator placed again.
#[test]
fn a_block_run_again_tells_on_placed() {
    let model = mutable_state_of(0);
    let told = count();
    let (v, t) = (model.handle(), told.clone());
    let policy = measure_policy(move |scope, _, _| {
        scope.layout_with(10, 10, move |_| {
            v.get();
        })
    });
    let mut host = host(move || {
        let t = t.clone();
        Layout(
            Modifier.on_placed(move |_: LayoutCoordinates| bump(&t)),
            policy.clone(),
            callback(|| {}),
        );
    });
    assert_eq!(told.get(), 1);
    model.set(1);
    host.frame();
    assert_eq!(told.get(), 2);
}

/// A block's panic leaves its work to do, for a later layout.
#[test]
fn a_block_that_panicked_runs_again() {
    let model = mutable_state_of(false);
    let (v, runs) = (model.handle(), count());
    let r = runs.clone();
    let child = measure_policy(move |scope, _, _| {
        let r = r.clone();
        scope.layout_with(10, 10, move |_| {
            bump(&r);
            assert!(!v.get() || r.get() > 2, "once");
        })
    });
    let parent = measure_policy(|scope, measurables, constraints| {
        let placeable = scope.measure(measurables[0], constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    });
    let mut host = host(move || {
        let child = child.clone();
        Layout(
            Modifier,
            parent.clone(),
            callback(move || Layout(Modifier, child.clone(), callback(|| {}))),
        );
    });
    model.set(true);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.frame();
    }));
    assert!(panicked.is_err());
    assert_eq!(runs.get(), 2);
    host.frame();
    assert_eq!(runs.get(), 3, "it wasn't lost");
}

/// A layout of no width mirrors nothing, as `placeAutoMirrored`, in a block and in a measurement.
#[test]
fn a_layout_of_no_width_mirrors_nothing() {
    let in_block = measure_policy(|scope, measurables, _| {
        let placeable = scope.measure(measurables[0], Constraints::fixed(10, 10));
        scope.layout_with(0, 20, move |placement| placement.place_relative(&placeable, 5, 0))
    });
    let in_measure = measure_policy(|scope, measurables, _| {
        let placeable = scope.measure(measurables[0], Constraints::fixed(10, 10));
        scope.place_relative(&placeable, 5, 0);
        scope.layout(0, 20)
    });
    for (policy, tag) in [(in_block, "block"), (in_measure, "measure")] {
        let host = host(move || {
            let policy = policy.clone();
            CompositionLocalProvider(
                LOCAL_LAYOUT_DIRECTION.provides(LayoutDirection::Rtl),
                move || {
                    Layout(
                        Modifier,
                        policy.clone(),
                        callback(move || Leaf(Modifier.test_tag(tag))),
                    );
                },
            );
        });
        assert_eq!(left_of(&host, tag), 5.0, "{tag}");
    }
}

/// `invalidate_placement` of a node placing inside its measurement is a read of its measurement:
/// what's before its inner layer isn't drawn for it, as D522's rule for such a read.
#[test]
fn invalidating_placement_of_a_fused_node_draws_only_what_its_layout_changed() {
    struct Fused(Rc<Cell<Option<NodeHandle>>>);
    impl ModifierNode for Fused {
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            self.0.set(Some(context.handle()));
        }
    }
    impl LayoutModifierNode for Fused {
        fn measure(
            &mut self,
            scope: &mut MeasureScope<'_>,
            content: Measurable,
            constraints: Constraints,
        ) -> MeasureResult {
            let placeable = scope.measure(content, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }
    }
    struct FusedElement(Rc<Cell<Option<NodeHandle>>>);
    impl ModifierNodeElement for FusedElement {
        type Node = Fused;
        fn create(&self) -> Fused {
            Fused(self.0.clone())
        }
        fn update(&self, _: &mut Fused) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    let drawn = count();
    let handle: Rc<Cell<Option<NodeHandle>>> = Rc::default();
    let (d, h) = (drawn.clone(), handle.clone());
    let mut host = host(move || {
        let d = d.clone();
        Leaf(
            sized(10)
                .repaint_boundary()
                .draw_behind(move |_| bump(&d))
                .graphics_layer(|layer| layer.alpha = 0.5)
                .then_custom(FusedElement(h.clone())),
        );
    });
    let before = drawn.get();
    handle.get().expect("attached").invalidate_placement();
    host.frame();
    assert_eq!(drawn.get(), before);
}
