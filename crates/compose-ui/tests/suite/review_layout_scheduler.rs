//! Adversarial parity tests for layout invalidation scheduling, per-layer placement,
//! `remeasure_now`, `reset_layout_state`, and cross-tree `LayoutCoordinates` (D2380–D2385).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use crate::support::*;
use compose_runtime::*;
use compose_ui::platform::{SemanticsSnapshot, UiHost};
use compose_ui::subcompose::{
    SlotIdsSet, SubcomposeLayoutState, SubcomposeLayoutWithState, SubcomposeSlotReusePolicy,
    subcompose_policy,
};
use compose_ui::*;

#[derive(Clone, Default)]
struct PlacementLog {
    placed: Rc<Cell<u32>>,
    unplaced: Rc<Cell<u32>>,
    draws: Rc<Cell<u32>>,
    handle: Rc<RefCell<Option<NodeHandle>>>,
    remeasurer: Rc<RefCell<Option<Remeasurement>>>,
}

impl PartialEq for PlacementLog {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.placed, &other.placed)
    }
}

struct ProbeNode(PlacementLog);

impl ModifierNode for ProbeNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.0.handle.borrow_mut() = Some(context.handle());
        *self.0.remeasurer.borrow_mut() = Some(context.remeasurement());
    }

    fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
        Some(self)
    }

    fn as_unplaced_aware(&mut self) -> Option<&mut dyn UnplacedAwareModifierNode> {
        Some(self)
    }

    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl LayoutAwareModifierNode for ProbeNode {
    fn on_placed(&mut self, _: LayoutCoordinates, _: &mut NodeContext<'_>) {
        self.0.placed.set(self.0.placed.get() + 1);
    }
}

impl UnplacedAwareModifierNode for ProbeNode {
    fn on_unplaced(&mut self, _: &mut NodeContext<'_>) {
        self.0.unplaced.set(self.0.unplaced.get() + 1);
    }
}

impl DrawModifierNode for ProbeNode {
    fn draw(&mut self, _: &mut DrawScope<'_>) {
        self.0.draws.set(self.0.draws.get() + 1);
    }
}

struct ProbeElement(PlacementLog);

impl ModifierNodeElement for ProbeElement {
    type Node = ProbeNode;

    fn create(&self) -> ProbeNode {
        ProbeNode(self.0.clone())
    }

    fn update(&self, node: &mut ProbeNode) {
        node.0 = self.0.clone();
    }

    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

fn probe(log: &PlacementLog) -> Modifier {
    Modifier.then_custom(ProbeElement(log.clone()))
}

fn tag_bounds(ui: &UiHost, tag: &str) -> Option<Rect> {
    fn walk(node: &SemanticsSnapshot, tag: &str) -> Option<Rect> {
        if node.config.get(&SemanticsProperties::TEST_TAG).is_some_and(|it| it.as_str() == tag) {
            return Some(node.bounds);
        }
        node.children.iter().find_map(|child| walk(child, tag))
    }
    walk(&ui.semantics_tree(false), tag)
}

#[test]
fn an_eager_child_under_an_unplaced_parent_stays_unplaced_until_its_parent_places() {
    let place_parent = mutable_state_of(false);
    let place_parent_h = place_parent.handle();
    let grandchild = PlacementLog::default();
    let clicks = Rc::new(Cell::new(0u32));

    let (grandchild_in, clicks_in) = (grandchild.clone(), clicks.clone());
    let policy = measure_policy(move |scope, measurables, constraints| {
        let child = scope.measure(measurables[0], constraints);
        let place = place_parent_h.get();
        scope.layout_with(child.width, child.height, move |placement| {
            if place {
                placement.place(&child, 0, 0);
            }
        })
    });

    let mut ui = host(move || {
        let (grandchild, clicks) = (grandchild_in.clone(), clicks_in.clone());
        let policy = policy.clone();
        Layout(
            Modifier,
            policy,
            callback(move || {
                let (grandchild, clicks) = (grandchild.clone(), clicks.clone());
                // `Stack` places its children eagerly inside `measure`: when its own parent skips
                // placing it on frame 1, `grandchild` must still stay unplaced (D2383).
                Stack(
                    Modifier.size(40.dp()),
                    callback(move || {
                        let clicks = clicks.clone();
                        Leaf(
                            probe(&grandchild)
                                .size(20.dp())
                                .test_tag("grandchild")
                                .clickable(move || clicks.set(clicks.get() + 1)),
                        );
                    }),
                );
            }),
        );
    });

    assert_eq!(grandchild.placed.get(), 0, "eager parent was never placed on frame 1");
    assert_eq!(grandchild.unplaced.get(), 0);
    assert_eq!(grandchild.draws.get(), 0, "unplaced grandchild does not draw");
    assert_eq!(tag_bounds(&ui, "grandchild"), None, "unplaced grandchild is out of semantics");
    click(&mut ui, 10.0, 10.0);
    assert_eq!(clicks.get(), 0, "unplaced grandchild is not hit");

    place_parent.set(true);
    ui.frame();
    assert_eq!(grandchild.placed.get(), 1);
    assert_eq!(grandchild.unplaced.get(), 0);
    assert!(grandchild.draws.get() >= 1);
    assert_eq!(tag_bounds(&ui, "grandchild"), Some(Rect::new(0.0, 0.0, 20.0, 20.0)));
    click(&mut ui, 10.0, 10.0);
    assert_eq!(clicks.get(), 1);

    place_parent.set(false);
    ui.frame();
    assert_eq!(grandchild.placed.get(), 1);
    assert_eq!(grandchild.unplaced.get(), 1);
    assert_eq!(tag_bounds(&ui, "grandchild"), None);
    click(&mut ui, 10.0, 10.0);
    assert_eq!(clicks.get(), 1, "unplaced again: not hit");
}

#[test]
fn two_layout_modifiers_toggling_placement_gate_is_placed_and_outer_semantics_layers() {
    let outer_places = mutable_state_of(true);
    let inner_places = mutable_state_of(true);
    let (outer_h, inner_h) = (outer_places.handle(), inner_places.handle());
    let node_log = PlacementLog::default();
    let child_log = PlacementLog::default();

    let (node_in, child_in) = (node_log.clone(), child_log.clone());
    let mut ui = host(move || {
        let (node_log, child_log) = (node_in.clone(), child_in.clone());
        let modifier = Modifier
            .test_tag("outer-tag")
            .layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let place = outer_h.get();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    if place {
                        placement.place(&placeable, 5, 6);
                    }
                })
            })
            .test_tag("mid-tag")
            .layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let place = inner_h.get();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    if place {
                        placement.place(&placeable, 7, 8);
                    }
                })
            })
            .test_tag("inner-tag")
            .then(probe(&node_log))
            .size(30.dp());
        Stack(
            modifier,
            callback(move || {
                Leaf(probe(&child_log).size(10.dp()).test_tag("child-tag"));
            }),
        );
    });

    // Both modifiers place: node and child are placed; outermost tag ("outer-tag") owns the node.
    assert_eq!((node_log.placed.get(), node_log.unplaced.get()), (1, 0));
    assert_eq!((child_log.placed.get(), child_log.unplaced.get()), (1, 0));
    assert_eq!(tag_bounds(&ui, "outer-tag"), Some(Rect::new(0.0, 0.0, 30.0, 30.0)));
    assert_eq!(tag_bounds(&ui, "child-tag"), Some(Rect::new(12.0, 14.0, 22.0, 24.0)));

    // Inner modifier stops placing (`unplaced_layers == 0b10`): `isPlaced` turns false, `on_unplaced`
    // fires on both node and child, child leaves semantics, while `outer-tag` in layer 0 stays.
    inner_places.set(false);
    ui.frame();
    assert_eq!((node_log.placed.get(), node_log.unplaced.get()), (1, 1));
    assert_eq!((child_log.placed.get(), child_log.unplaced.get()), (1, 1));
    assert_eq!(tag_bounds(&ui, "outer-tag"), Some(Rect::new(0.0, 0.0, 30.0, 30.0)));
    assert_eq!(tag_bounds(&ui, "child-tag"), None);

    // Outer stops placing too (`unplaced_layers == 0b11`): already unplaced, no duplicate `on_unplaced`.
    outer_places.set(false);
    ui.frame();
    assert_eq!((node_log.placed.get(), node_log.unplaced.get()), (1, 1));
    assert_eq!((child_log.placed.get(), child_log.unplaced.get()), (1, 1));
    assert_eq!(tag_bounds(&ui, "outer-tag"), Some(Rect::new(0.0, 0.0, 30.0, 30.0)));

    // Inner resumes while outer still skips (`unplaced_layers == 0b01`): still unplaced!
    inner_places.set(true);
    ui.frame();
    assert_eq!((node_log.placed.get(), node_log.unplaced.get()), (1, 1));
    assert_eq!((child_log.placed.get(), child_log.unplaced.get()), (1, 1));
    assert_eq!(tag_bounds(&ui, "child-tag"), None);

    // Outer resumes too (`unplaced_layers == 0`): both node and child become placed again.
    outer_places.set(true);
    ui.frame();
    assert_eq!((node_log.placed.get(), node_log.unplaced.get()), (2, 1));
    assert_eq!((child_log.placed.get(), child_log.unplaced.get()), (2, 1));
    assert_eq!(tag_bounds(&ui, "child-tag"), Some(Rect::new(12.0, 14.0, 22.0, 24.0)));

    // And a node whose only semantics tag sits *inside* an unplaced layout modifier is left out.
    let gated = mutable_state_of(false);
    let gated_h = gated.handle();
    let mut inner_only = host(move || {
        Leaf(
            Modifier
                .size(25.dp())
                .layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let place = gated_h.get();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        if place {
                            placement.place(&placeable, 0, 0);
                        }
                    })
                })
                .test_tag("inside-unplaced"),
        );
    });
    assert_eq!(tag_bounds(&inner_only, "inside-unplaced"), None);
    gated.set(true);
    inner_only.frame();
    assert_eq!(tag_bounds(&inner_only, "inside-unplaced"), Some(Rect::new(0.0, 0.0, 25.0, 25.0)));
}

#[test]
fn a_remeasure_request_during_placement_runs_next_while_relayout_is_swallowed() {
    let target = PlacementLog::default();
    let sibling = PlacementLog::default();
    let measures = Rc::new(Cell::new(0u32));
    let blocks = Rc::new(Cell::new(0u32));
    let sibling_blocks = Rc::new(Cell::new(0u32));
    let mode = Rc::new(Cell::new(0u8)); // 1 = self relayout + sibling relayout, 2 = self remeasure

    let (target_in, sibling_in) = (target.clone(), sibling.clone());
    let (measures_in, blocks_in, sibling_blocks_in, mode_in) =
        (measures.clone(), blocks.clone(), sibling_blocks.clone(), mode.clone());
    let mut ui = host(move || {
        let (target, sibling) = (target_in.clone(), sibling_in.clone());
        let (measures, blocks, sibling_blocks, mode) =
            (measures_in.clone(), blocks_in.clone(), sibling_blocks_in.clone(), mode_in.clone());
        Row(
            Modifier,
            callback(move || {
                let (target_h, sibling_h) = (target.handle.clone(), sibling.handle.clone());
                let (measures, blocks, mode) = (measures.clone(), blocks.clone(), mode.clone());
                Leaf(probe(&target).size(20.dp()).layout(move |scope, measurable, constraints| {
                    measures.set(measures.get() + 1);
                    let placeable = scope.measure(measurable, constraints);
                    let (target_h, sibling_h, blocks, mode) =
                        (target_h.clone(), sibling_h.clone(), blocks.clone(), mode.clone());
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        blocks.set(blocks.get() + 1);
                        match mode.replace(0) {
                            1 => {
                                target_h.borrow().as_ref().unwrap().invalidate_placement();
                                sibling_h.borrow().as_ref().unwrap().invalidate_placement();
                            }
                            2 => {
                                target_h.borrow().as_ref().unwrap().invalidate_measurement();
                            }
                            _ => {}
                        }
                        placement.place(&placeable, 0, 0);
                    })
                }));
                let sibling_blocks = sibling_blocks.clone();
                Leaf(probe(&sibling).size(20.dp()).layout(
                    move |scope, measurable, constraints| {
                        let placeable = scope.measure(measurable, constraints);
                        let sibling_blocks = sibling_blocks.clone();
                        scope.layout_with(placeable.width, placeable.height, move |placement| {
                            sibling_blocks.set(sibling_blocks.get() + 1);
                            placement.place(&placeable, 0, 0);
                        })
                    },
                ));
            }),
        );
    });

    assert_eq!((measures.get(), blocks.get(), sibling_blocks.get()), (1, 1, 1));
    assert!(!ui.has_pending_work());

    // Self-relayout during placement is swallowed; sibling's relayout request is preserved!
    mode.set(1);
    target.handle.borrow().as_ref().unwrap().invalidate_placement();
    ui.frame();
    assert_eq!((measures.get(), blocks.get()), (1, 2), "self relayout swallowed");
    if ui.has_pending_work() {
        ui.frame();
    }
    assert_eq!(blocks.get(), 2, "target never placed a third time");
    assert_eq!(sibling_blocks.get(), 2, "sibling's request was kept");
    assert!(!ui.has_pending_work());

    // Self-remeasure during placement is postponed (NOT swallowed), as Compose's `LayingOut`.
    mode.set(2);
    target.handle.borrow().as_ref().unwrap().invalidate_placement();
    ui.frame();
    if ui.has_pending_work() {
        ui.frame();
    }
    assert_eq!(measures.get(), 2, "postponed remeasure from placement block ran");
    assert_eq!(blocks.get(), 4);
    assert!(!ui.has_pending_work());
}

#[test]
fn force_remeasure_runs_lookahead_and_now_placement_blocks_and_leaves_no_pending_work() {
    let target = PlacementLog::default();
    let order: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let shift = Rc::new(Cell::new(0i32));

    let (target_in, order_in, shift_in) = (target.clone(), order.clone(), shift.clone());
    let ui = host(move || {
        let (target, order, shift) = (target_in.clone(), order_in.clone(), shift_in.clone());
        Stack(
            Modifier.size(100.dp()),
            callback(move || {
                let (target, order, shift) = (target.clone(), order.clone(), shift.clone());
                LookaheadScope!(|_| {
                    let (order, shift) = (order.clone(), shift.clone());
                    let policy = measure_policy(move |scope, measurables, constraints| {
                        let ahead = scope.is_looking_ahead();
                        order.borrow_mut().push(if ahead {
                            "measure-ahead"
                        } else {
                            "measure-now"
                        });
                        let child = scope.measure(measurables[0], constraints);
                        let (order, x) = (order.clone(), shift.get());
                        scope.layout_with(40, 40, move |placement| {
                            order.borrow_mut().push(if placement.is_looking_ahead() {
                                "place-ahead"
                            } else {
                                "place-now"
                            });
                            placement.place(&child, x, 0);
                        })
                    });
                    Layout(
                        probe(&target),
                        policy,
                        callback(|| {
                            Leaf(Modifier.size(10.dp()));
                        }),
                    );
                });
            }),
        );
    });

    order.borrow_mut().clear();
    target.placed.set(0);
    assert!(!ui.has_pending_work());

    // Synchronously `force_remeasure` at the same 40x40 outer size: lookahead and main-pass
    // `layout_with` blocks must both run before `force_remeasure` returns, `on_placed` dispatches
    // immediately (ancestor subtree flags cleared), and no pending work stays.
    shift.set(12);
    target.remeasurer.borrow().as_ref().unwrap().force_remeasure();

    assert_eq!(&*order.borrow(), &["measure-ahead", "measure-now", "place-ahead", "place-now"]);
    assert_eq!(target.placed.get(), 1, "on_placed dispatched before force_remeasure returned");
    order.borrow_mut().clear();
    let mut ui = ui;
    ui.frame();
    assert!(order.borrow().is_empty(), "subsequent frame runs no measure or placement");
    assert!(!ui.has_pending_work());
}

#[test]
fn cross_tree_bounding_box_and_transform_panic_while_detached_and_overlay_succeed() {
    let coords_a: Rc<RefCell<Option<LayoutCoordinates>>> = Rc::default();
    let overlay_a: Rc<RefCell<Option<LayoutCoordinates>>> = Rc::default();
    let detached_a: Rc<RefCell<Option<LayoutCoordinates>>> = Rc::default();
    let show_detached = mutable_state_of(true);
    let show_detached_h = show_detached.handle();

    let (ca, oa, da) = (coords_a.clone(), overlay_a.clone(), detached_a.clone());
    let mut host_a = host(move || {
        let (ca, oa, da) = (ca.clone(), oa.clone(), da.clone());
        Stack(
            Modifier.size(100.dp()),
            callback(move || {
                let ca = ca.clone();
                Leaf(Modifier.size(30.dp()).on_placed(move |c| *ca.borrow_mut() = Some(c)));
                let oa = oa.clone();
                Leaf(
                    Modifier
                        .overlay(Rc::new(WindowPosition), false)
                        .size(20.dp())
                        .on_placed(move |c| *oa.borrow_mut() = Some(c)),
                );
                if show_detached_h.get() {
                    let da = da.clone();
                    Leaf(Modifier.size(15.dp()).on_placed(move |c| *da.borrow_mut() = Some(c)));
                }
            }),
        );
    });

    let coords_b: Rc<RefCell<Option<LayoutCoordinates>>> = Rc::default();
    let cb = coords_b.clone();
    let _host_b = host(move || {
        let cb = cb.clone();
        Leaf(Modifier.size(25.dp()).on_placed(move |c| *cb.borrow_mut() = Some(c)));
    });

    let a = coords_a.borrow().unwrap();
    let over = overlay_a.borrow().unwrap();
    let b = coords_b.borrow().unwrap();

    // Same-tree overlay conversions succeed without panicking.
    assert_eq!(a.local_position_of(&over, Offset::new(3.0, 4.0)), Offset::new(3.0, 4.0));
    assert_eq!(a.local_bounding_box_of(&over, false), Rect::new(0.0, 0.0, 20.0, 20.0));
    let mut m = Matrix::new();
    a.transform_from(&over, &mut m);
    assert_eq!(m, Matrix::new());

    // Detaching a node in the same tree returns empty/identity instead of panicking.
    show_detached.set(false);
    host_a.frame();
    let gone = detached_a.borrow().unwrap();
    assert!(!gone.is_attached());
    assert_eq!(a.local_position_of(&gone, Offset::new(5.0, 5.0)), Offset::ZERO);
    assert_eq!(a.local_bounding_box_of(&gone, true), Rect::ZERO);
    let mut m = Matrix::new();
    a.transform_from(&gone, &mut m);
    assert_eq!(m, Matrix::new());

    // Another host's coordinates panic on all cross-coordinate methods.
    for check in [
        catch_unwind(AssertUnwindSafe(|| {
            let _ = a.local_bounding_box_of(&b, true);
        })),
        catch_unwind(AssertUnwindSafe(|| {
            let mut matrix = Matrix::new();
            a.transform_from(&b, &mut matrix);
        })),
    ] {
        let err = check.expect_err("cross-tree query must panic");
        let msg = err
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| err.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("");
        assert!(msg.contains("not part of the same hierarchy"), "got: {msg}");
    }
}

struct KeepOne;

impl SubcomposeSlotReusePolicy for KeepOne {
    fn get_slots_to_retain(&self, slots: &mut SlotIdsSet) {
        let mut kept = 0usize;
        slots.retain(|_| {
            kept += 1;
            kept <= 1
        });
    }

    fn are_compatible(&self, _: &dyn Any, _: &dyn Any) -> bool {
        true
    }
}

#[test]
fn a_reused_precomposed_slot_invalidated_before_premeasure_waits_for_it() {
    let state = SubcomposeLayoutState::with_policy(KeepOne);
    let show_first = mutable_state_of(true);
    let show_first_h = show_first.handle();
    let host_measures = Rc::new(Cell::new(0u32));
    let slot_measures = Rc::new(Cell::new(0u32));
    let child_log = PlacementLog::default();

    let (state_in, host_m, slot_m, child_in) =
        (state.clone(), host_measures.clone(), slot_measures.clone(), child_log.clone());
    let mut ui = host(move || {
        let (state, host_m, slot_m, child_log) =
            (state_in.clone(), host_m.clone(), slot_m.clone(), child_in.clone());
        let policy = subcompose_policy(move |scope, constraints| {
            host_m.set(host_m.get() + 1);
            if show_first_h.get() {
                let (slot_m, child_log) = (slot_m.clone(), child_log.clone());
                let measurables = scope.subcompose(
                    "first",
                    callback(move || {
                        let slot_m = slot_m.clone();
                        Leaf(probe(&child_log).size(20.dp()).layout(
                            move |scope, measurable, constraints| {
                                slot_m.set(slot_m.get() + 1);
                                let placeable = scope.measure(measurable, constraints);
                                scope.layout_with(
                                    placeable.width,
                                    placeable.height,
                                    move |placement| placement.place(&placeable, 0, 0),
                                )
                            },
                        ));
                    }),
                );
                let p = scope.measure(measurables[0], constraints);
                scope.layout_with(50, 50, move |placement| placement.place(&p, 0, 0))
            } else {
                scope.layout(50, 50)
            }
        });
        SubcomposeLayoutWithState(state, Modifier, policy);
    });

    assert_eq!((host_measures.get(), slot_measures.get()), (1, 1));

    // Deactivate `"first"` into the reuse pool (`deactivate` calls `reset_layout_state`).
    show_first.set(false);
    ui.frame();
    assert_eq!((host_measures.get(), slot_measures.get()), (2, 1));

    // Precompose `"second"`, reusing `"first"`'s kept node, then invalidate its child *after*
    // precompose and *before* premeasure: host must NOT remeasure until `premeasure` runs.
    let (slot_m, child_in) = (slot_measures.clone(), child_log.clone());
    let handle = state.precompose(
        "second",
        callback(move || {
            let slot_m = slot_m.clone();
            Leaf(probe(&child_in).size(30.dp()).layout(move |scope, measurable, constraints| {
                slot_m.set(slot_m.get() + 1);
                let placeable = scope.measure(measurable, constraints);
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    placement.place(&placeable, 0, 0)
                })
            }));
        }),
    );
    assert_eq!((host_measures.get(), slot_measures.get()), (2, 1));

    child_log.handle.borrow().as_ref().unwrap().invalidate_measurement();
    ui.frame();
    assert_eq!(
        (host_measures.get(), slot_measures.get()),
        (2, 1),
        "post-precompose invalidation on reused slot waits for premeasure"
    );

    handle.premeasure(0, Constraints::new(0, 50, 0, 50));
    assert_eq!((host_measures.get(), slot_measures.get()), (2, 2));
}
