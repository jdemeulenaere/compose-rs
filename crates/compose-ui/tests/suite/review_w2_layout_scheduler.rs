//! Adversarial review tests for `w2-layout-scheduler` (D2380–D2385, D2392–D2394; reviewed in
//! D2445–D2448).

use std::any::Any;
use std::cell::{Cell, RefCell};
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
struct Probe {
    placed: Rc<Cell<u32>>,
    unplaced: Rc<Cell<u32>>,
    draws: Rc<Cell<u32>>,
    handle: Rc<RefCell<Option<NodeHandle>>>,
    remeasurer: Rc<RefCell<Option<Remeasurement>>>,
}

impl PartialEq for Probe {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.placed, &other.placed)
    }
}

struct ProbeNode(Probe);

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

struct ProbeElement(Probe);

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

fn probe(log: &Probe) -> Modifier {
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

/// An eager layout modifier (`scope.layout(w, h)` without `scope.place`, so it creates no
/// `layout_with` block at layer 0) skipping placement must still skip the inner layout modifier's
/// and the policy's deferred `layout_with` blocks until it places its content (D2445).
#[test]
#[ignore = "review: an eager layout modifier that skips placing its content still runs inner layout_with blocks"]
fn an_eager_unplacing_modifier_skips_inner_deferred_placement_blocks() {
    let show = mutable_state_of(false);
    let show_h = show.handle();
    let inner_blocks = Rc::new(Cell::new(0u32));
    let policy_blocks = Rc::new(Cell::new(0u32));
    let child_log = Probe::default();

    let (inner_in, policy_in, child_in) =
        (inner_blocks.clone(), policy_blocks.clone(), child_log.clone());
    let mut ui = host(move || {
        let (inner_blocks, policy_blocks, child_log) =
            (inner_in.clone(), policy_in.clone(), child_in.clone());
        let modifier = Modifier
            .layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                if show_h.get() {
                    scope.place(&placeable, 0, 0);
                }
                scope.layout(placeable.width, placeable.height)
            })
            .layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let inner_blocks = inner_blocks.clone();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    inner_blocks.set(inner_blocks.get() + 1);
                    placement.place(&placeable, 2, 3);
                })
            });
        let policy = measure_policy(move |scope, measurables, constraints| {
            let child = scope.measure(measurables[0], constraints);
            let policy_blocks = policy_blocks.clone();
            scope.layout_with(child.width, child.height, move |placement| {
                policy_blocks.set(policy_blocks.get() + 1);
                placement.place(&child, 4, 5);
            })
        });
        Layout(
            modifier,
            policy,
            callback(move || {
                Leaf(probe(&child_log).size(10.dp()).test_tag("eager-gated-child"));
            }),
        );
    });

    assert_eq!((inner_blocks.get(), policy_blocks.get()), (0, 0));
    assert_eq!((child_log.placed.get(), child_log.draws.get()), (0, 0));
    assert_eq!(tag_bounds(&ui, "eager-gated-child"), None);

    show.set(true);
    ui.frame();
    assert_eq!((inner_blocks.get(), policy_blocks.get()), (1, 1));
    assert_eq!(child_log.placed.get(), 1);
    assert_eq!(tag_bounds(&ui, "eager-gated-child"), Some(Rect::new(6.0, 8.0, 16.0, 18.0)));
}

/// Reading an alignment line across an inner `Modifier.layout` runs that modifier's `layout_with`
/// block early (`run_block_for_alignment`): when the block skips placing its content, the line
/// becomes `AlignmentLine::UNSPECIFIED`, `unplaced_layers` updates so the node becomes unplaced
/// (`on_unplaced`, not drawn, not hit), and inner placement blocks do not run in `place_pass`
/// (D2445).
#[test]
#[ignore = "review: run_block_for_alignment neither updates unplaced_layers nor hides alignment lines when a modifier skips placing"]
fn an_alignment_line_read_through_an_unplacing_modifier_returns_unspecified_and_marks_unplaced() {
    let line = HorizontalAlignmentLine::new(i32::min);
    let place = mutable_state_of(true);
    let place_h = place.handle();
    let seen_line = Rc::new(Cell::new(0i32));
    let policy_blocks = Rc::new(Cell::new(0u32));
    let node_log = Probe::default();

    let (seen_in, blocks_in, log_in) = (seen_line.clone(), policy_blocks.clone(), node_log.clone());
    let mut ui = host(move || {
        let (seen_line, policy_blocks, node_log) =
            (seen_in.clone(), blocks_in.clone(), log_in.clone());
        let modifier = Modifier
            .layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                seen_line.set(scope.alignment_line(&placeable, line));
                scope.place(&placeable, 0, 0);
                scope.layout(placeable.width, placeable.height)
            })
            .layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let place = place_h.get();
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    if place {
                        placement.place(&placeable, 0, 7);
                    }
                })
            })
            .then(probe(&node_log))
            .test_tag("aligned-node");
        let policy = measure_policy(move |scope, _, _| {
            scope.provide_alignment_lines([(AlignmentLine::from(line), 12)]);
            let policy_blocks = policy_blocks.clone();
            scope.layout_with(20, 20, move |_| {
                policy_blocks.set(policy_blocks.get() + 1);
            })
        });
        Layout(modifier, policy, callback(|| {}));
    });

    assert_eq!(seen_line.get(), 19);
    assert_eq!((node_log.placed.get(), node_log.unplaced.get(), policy_blocks.get()), (1, 0, 1));
    assert_eq!(tag_bounds(&ui, "aligned-node"), Some(Rect::new(0.0, 7.0, 20.0, 27.0)));

    // Inner modifier stops placing: alignment line read during measure becomes UNSPECIFIED,
    // `node` becomes unplaced (`on_unplaced` == 1), and the inner policy block does not run.
    place.set(false);
    ui.frame();
    assert_eq!(seen_line.get(), AlignmentLine::UNSPECIFIED);
    assert_eq!((node_log.placed.get(), node_log.unplaced.get()), (1, 1));
    assert_eq!(policy_blocks.get(), 1, "inner policy block must not run while unplaced");
    assert_eq!(tag_bounds(&ui, "aligned-node"), None);

    // Resuming placement restores the offset alignment line and placed state.
    place.set(true);
    ui.frame();
    assert_eq!(seen_line.get(), 19);
    assert_eq!((node_log.placed.get(), node_log.unplaced.get(), policy_blocks.get()), (2, 1, 2));
    assert_eq!(tag_bounds(&ui, "aligned-node"), Some(Rect::new(0.0, 7.0, 20.0, 27.0)));
}

/// When an ancestor outside `LookaheadScope` becomes unplaced via a layout modifier (leaving the
/// scope root's `placed_by_parent == true` while `parent_is_placed(root) == false`), neither
/// `remeasure_now` (`force_remeasure`) nor `lookahead_pass` runs placement blocks under the
/// unplaced parent until the parent places again (D2446).
#[test]
#[ignore = "review: remeasure_now and lookahead_pass ignore parent_is_placed when placed_by_parent stays true"]
fn a_lookahead_root_under_an_unplaced_parent_becomes_unplaced_ahead_and_force_remeasure_waits() {
    let place_parent = mutable_state_of(true);
    let place_parent_h = place_parent.handle();
    let scope_probe = Probe::default();
    let ahead_places = Rc::new(Cell::new(0u32));
    let now_places = Rc::new(Cell::new(0u32));
    let measures = Rc::new(Cell::new(0u32));

    let (probe_in, ahead_in, now_in, measures_in) =
        (scope_probe.clone(), ahead_places.clone(), now_places.clone(), measures.clone());
    let mut ui = host(move || {
        let (scope_probe, ahead_places, now_places, measures) =
            (probe_in.clone(), ahead_in.clone(), now_in.clone(), measures_in.clone());
        let gate = Modifier.layout(move |scope, measurable, constraints| {
            let placeable = scope.measure(measurable, constraints);
            let place = place_parent_h.get();
            scope.layout_with(placeable.width, placeable.height, move |placement| {
                if place {
                    placement.place(&placeable, 0, 0);
                }
            })
        });
        Stack(
            gate.size(80.dp()),
            callback(move || {
                let (scope_probe, ahead_places, now_places, measures) = (
                    scope_probe.clone(),
                    ahead_places.clone(),
                    now_places.clone(),
                    measures.clone(),
                );
                LookaheadScope!(|_| {
                    let (ahead_places, now_places, measures) =
                        (ahead_places.clone(), now_places.clone(), measures.clone());
                    let policy = measure_policy(move |scope, measurables, constraints| {
                        measures.set(measures.get() + 1);
                        let child = scope.measure(measurables[0], constraints);
                        let (ahead_places, now_places) = (ahead_places.clone(), now_places.clone());
                        scope.layout_with(30, 30, move |placement| {
                            if placement.is_looking_ahead() {
                                ahead_places.set(ahead_places.get() + 1);
                            } else {
                                now_places.set(now_places.get() + 1);
                            }
                            placement.place(&child, 0, 0);
                        })
                    });
                    Layout(
                        probe(&scope_probe),
                        policy,
                        callback(|| {
                            Leaf(Modifier.size(10.dp()));
                        }),
                    );
                });
            }),
        );
    });

    assert_eq!((measures.get(), ahead_places.get(), now_places.get()), (2, 1, 1));
    assert_eq!((scope_probe.placed.get(), scope_probe.unplaced.get()), (1, 0));

    // Unplace the outer `Stack` via its modifier (`Stack` does NOT re-run its policy, so the
    // LookaheadScope root keeps `placed_by_parent == true` while `Stack.placed == false`).
    place_parent.set(false);
    ui.frame();
    assert_eq!((scope_probe.placed.get(), scope_probe.unplaced.get()), (1, 1));
    assert_eq!((ahead_places.get(), now_places.get()), (1, 1));

    // `force_remeasure()` on the LookaheadScope root while its parent is unplaced measures both
    // passes, does NOT run either pass's placement block, and leaves lookahead unplaced too.
    scope_probe.remeasurer.borrow().as_ref().unwrap().force_remeasure();
    assert_eq!(measures.get(), 4, "lookahead + approach measured");
    assert_eq!(
        (ahead_places.get(), now_places.get()),
        (1, 1),
        "placement blocks must wait while parent is unplaced"
    );

    // Re-placing the parent runs the waiting placement blocks.
    place_parent.set(true);
    ui.frame();
    assert_eq!((ahead_places.get(), now_places.get()), (2, 2));
    assert_eq!((scope_probe.placed.get(), scope_probe.unplaced.get()), (2, 1));
}

/// An overlay node (`Modifier.overlay`) whose own inner layout modifier skips placing its content
/// has `placed == false`: `is_placed_in_root` must report `false` so `on_unplaced` fires on the
/// overlay node itself and `on_placed` does not fire while unplaced (D2446).
#[test]
#[ignore = "review: is_placed_in_root ignores !value.placed on an overlay node itself"]
fn an_overlay_node_with_an_unplacing_modifier_fires_on_unplaced_and_skips_on_placed() {
    let place = mutable_state_of(true);
    let place_h = place.handle();
    let overlay_log = Probe::default();

    let log_in = overlay_log.clone();
    let mut ui = host(move || {
        let overlay_log = log_in.clone();
        Leaf(
            Modifier
                .overlay(Rc::new(WindowPosition), false)
                .layout(move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    let place = place_h.get();
                    scope.layout_with(placeable.width, placeable.height, move |placement| {
                        if place {
                            placement.place(&placeable, 0, 0);
                        }
                    })
                })
                .then(probe(&overlay_log))
                .size(20.dp()),
        );
    });

    assert_eq!((overlay_log.placed.get(), overlay_log.unplaced.get()), (1, 0));

    place.set(false);
    ui.frame();
    assert_eq!((overlay_log.placed.get(), overlay_log.unplaced.get()), (1, 1));

    place.set(true);
    ui.frame();
    assert_eq!((overlay_log.placed.get(), overlay_log.unplaced.get()), (2, 1));
}

/// 1) When an earlier sibling queues a handle request before a `SubcomposeLayout` measures, and
///    `scope.subcompose` drains `node_invalidations` mid-measurement (`D2392`), a self-targeted
///    `invalidate_measurement()` or `force_remeasure()` queued by the `SubcomposeLayout` after
///    `subcompose` is still dropped so it measures once (`D2447`).
/// 2) In-layout `NodeContext::invalidate_parent_data()` and busy `Remeasurement::force_remeasure()`
///    satisfied within `UiHost::lay_out` leave `has_pending_work()` false (`D2447`).
#[test]
#[ignore = "review: drain_node_requests inside subcompose breaks drop_own_requests's slice offset and busy force_remeasure / parent_data wake extra frames"]
fn self_requests_after_subcompose_drain_and_in_layout_parent_data_or_force_remeasure_leave_no_pending_work()
 {
    let first = Probe::default();
    let host_probe = Probe::default();
    let target = Probe::default();
    let host_measures = Rc::new(Cell::new(0u32));
    let target_measures = Rc::new(Cell::new(0u32));
    let trigger = Rc::new(Cell::new(false));
    let slot_version = mutable_state_of(0u32);
    let slot_version_h = slot_version.handle();

    let (f_in, h_in, t_in) = (first.clone(), host_probe.clone(), target.clone());
    let (hm_in, tm_in, trig_in) = (host_measures.clone(), target_measures.clone(), trigger.clone());
    let mut ui = host(move || {
        let (first, host_probe, target) = (f_in.clone(), h_in.clone(), t_in.clone());
        let (host_measures, target_measures, trigger) =
            (hm_in.clone(), tm_in.clone(), trig_in.clone());
        Row(
            Modifier,
            callback(move || {
                // Sibling 1: when triggered, queues an invalidation for Sibling 3 (`target`) so
                // `node_invalidations.len() == 1` when Sibling 2 (`SubcomposeLayout`) starts.
                let (target_for_first, trigger_for_first) = (target.clone(), trigger.clone());
                Leaf(probe(&first).size(10.dp()).layout(move |scope, measurable, constraints| {
                    if trigger_for_first.get() {
                        target_for_first.handle.borrow().as_ref().unwrap().invalidate_measurement();
                    }
                    let p = scope.measure(measurable, constraints);
                    scope.place(&p, 0, 0);
                    scope.layout(p.width, p.height)
                }));

                // Sibling 2: `SubcomposeLayout` whose `subcompose` drains `node_invalidations` at
                // `on_end_changes`, and then requests remeasure + `force_remeasure` on itself.
                let (host_for_sub, hm, trig_sub) =
                    (host_probe.clone(), host_measures.clone(), trigger.clone());
                let sub_policy = subcompose_policy(move |scope, constraints| {
                    hm.set(hm.get() + 1);
                    let v = slot_version_h.get();
                    let measurables = scope.subcompose(
                        "slot",
                        callback(move || {
                            Leaf(Modifier.size((10 + v as i32).dp()));
                        }),
                    );
                    if trig_sub.replace(false) {
                        host_for_sub.handle.borrow().as_ref().unwrap().invalidate_measurement();
                        host_for_sub.remeasurer.borrow().as_ref().unwrap().force_remeasure();
                    }
                    let p = scope.measure(measurables[0], constraints);
                    scope.place(&p, 0, 0);
                    scope.layout(p.width, p.height)
                });
                SubcomposeLayoutWithState(
                    SubcomposeLayoutState::new(),
                    probe(&host_probe),
                    sub_policy,
                );

                // Sibling 3: target of Sibling 1's request (and busy `force_remeasure` check).
                let tm = target_measures.clone();
                Leaf(probe(&target).size(10.dp()).layout(move |scope, measurable, constraints| {
                    tm.set(tm.get() + 1);
                    let p = scope.measure(measurable, constraints);
                    scope.place(&p, 0, 0);
                    scope.layout(p.width, p.height)
                }));
            }),
        );
    });

    assert_eq!((host_measures.get(), target_measures.get()), (1, 1));
    assert!(!ui.has_pending_work());

    trigger.set(true);
    slot_version.set(1);
    first.handle.borrow().as_ref().unwrap().invalidate_measurement();
    ui.frame();
    assert_eq!(host_measures.get(), 2, "SubcomposeLayout self-requests after subcompose swallowed");
    assert_eq!(target_measures.get(), 2, "Sibling 3's request still ran");
    assert!(!ui.has_pending_work(), "satisfied in-frame requests leave no pending work");

    // Part 2: `context.invalidate_parent_data()` inside `on_remeasured` and busy
    // `remeasurer.force_remeasure()` on another node during layout also leave `has_pending_work`
    // false once `lay_out_rounds` finishes them.
    struct ParentDataInRemeasure {
        once: Rc<Cell<bool>>,
        sibling: Rc<RefCell<Option<Remeasurement>>>,
    }
    impl ModifierNode for ParentDataInRemeasure {
        fn as_layout_aware(&mut self) -> Option<&mut dyn LayoutAwareModifierNode> {
            Some(self)
        }
        fn as_parent_data(&self) -> Option<&dyn ParentDataModifierNode> {
            Some(self)
        }
    }
    impl ParentDataModifierNode for ParentDataInRemeasure {
        fn modify_parent_data(&self, _: Density, data: Option<Rc<dyn Any>>) -> Option<Rc<dyn Any>> {
            data
        }
    }
    impl LayoutAwareModifierNode for ParentDataInRemeasure {
        fn on_remeasured(&mut self, _: IntSize, context: &mut NodeContext<'_>) {
            if !self.once.replace(true) {
                context.invalidate_parent_data();
                if let Some(rem) = *self.sibling.borrow() {
                    rem.force_remeasure();
                }
            }
        }
    }
    #[derive(Clone)]
    struct ParentDataInRemeasureEl(Rc<Cell<bool>>, Rc<RefCell<Option<Remeasurement>>>);
    impl ModifierNodeElement for ParentDataInRemeasureEl {
        type Node = ParentDataInRemeasure;
        fn create(&self) -> ParentDataInRemeasure {
            ParentDataInRemeasure { once: self.0.clone(), sibling: self.1.clone() }
        }
        fn update(&self, _: &mut ParentDataInRemeasure) {}
        fn same(&self, other: &Self) -> bool {
            Rc::ptr_eq(&self.0, &other.0)
        }
    }

    let other_probe = Probe::default();
    let other_measures = Rc::new(Cell::new(0u32));
    let once = Rc::new(Cell::new(false));
    let resize = mutable_state_of(10i32);
    let resize_h = resize.handle();
    let (op_in, om_in, once_in) = (other_probe.clone(), other_measures.clone(), once.clone());
    let mut ui2 = host(move || {
        let (other_probe, other_measures, once) = (op_in.clone(), om_in.clone(), once_in.clone());
        let sib_rem = other_probe.remeasurer.clone();
        Row(
            Modifier,
            callback(move || {
                let om = other_measures.clone();
                Leaf(probe(&other_probe).size(10.dp()).layout(
                    move |scope, measurable, constraints| {
                        om.set(om.get() + 1);
                        let p = scope.measure(measurable, constraints);
                        scope.place(&p, 0, 0);
                        scope.layout(p.width, p.height)
                    },
                ));
                Leaf(
                    Modifier
                        .then_custom(ParentDataInRemeasureEl(once.clone(), sib_rem.clone()))
                        .size(Dp(resize_h.get() as f32)),
                );
            }),
        );
    });
    assert!(!ui2.has_pending_work());
    once.set(false);
    resize.set(20);
    ui2.frame();
    assert_eq!(other_measures.get(), 2, "busy force_remeasure ran within the frame");
    assert!(
        !ui2.has_pending_work(),
        "no leftover frame_requested after in-frame parent_data/remeasure"
    );
}

/// `draw_ahead` on a premeasured repaint boundary records its placed-by-parent children while
/// skipping subtrees inside it whose layout modifier did not place its content (D2383).
#[test]
fn draw_ahead_skips_modifier_unplaced_subtrees_inside_a_prefetched_boundary() {
    let state = SubcomposeLayoutState::new();
    let drawn_placed = Rc::new(Cell::new(0u32));
    let drawn_unplaced = Rc::new(Cell::new(0u32));

    let state_in = state.clone();
    let ui = host(move || {
        let policy = subcompose_policy(|scope, _| scope.layout(50, 50));
        SubcomposeLayoutWithState(state_in.clone(), Modifier, policy);
    });

    let (dp, du) = (drawn_placed.clone(), drawn_unplaced.clone());
    let handle = state.precompose(
        "prefetched",
        callback(move || {
            let (dp, du) = (dp.clone(), du.clone());
            Stack(
                Modifier.repaint_boundary().size(30.dp()),
                callback(move || {
                    let dp = dp.clone();
                    Leaf(Modifier.size(10.dp()).draw_behind(move |_| dp.set(dp.get() + 1)));
                    let du = du.clone();
                    Stack(
                        Modifier.size(10.dp()).layout(|scope, measurable, constraints| {
                            let p = scope.measure(measurable, constraints);
                            // Eagerly skip placing `p` inside the prefetched boundary.
                            scope.layout(p.width, p.height)
                        }),
                        callback(move || {
                            let du = du.clone();
                            Leaf(Modifier.size(10.dp()).draw_behind(move |_| du.set(du.get() + 1)));
                        }),
                    );
                }),
            );
        }),
    );
    handle.premeasure(0, Constraints::fixed(30, 30));
    let groups = ui.with_tree(|tree| tree.draw_ahead());
    assert!(groups.is_some(), "prefetched boundary drawn ahead");
    assert_eq!(drawn_placed.get(), 1);
    assert_eq!(drawn_unplaced.get(), 0, "modifier-unplaced subtree skipped in draw_ahead");
}

/// Bit 31 (`1u32 << 31`, the 32nd layout modifier) still gates `placed`, and a 33rd layout
/// modifier (`layer == 32 == MOTION_LAYERS`) does not panic or shift out of range (D2383).
#[test]
fn thirty_two_layout_modifiers_track_bit_31_and_thirty_third_does_not_panic() {
    let place_31 = mutable_state_of(true);
    let place_32 = mutable_state_of(true);
    let (p31, p32) = (place_31.handle(), place_32.handle());
    let leaf_log = Probe::default();

    let log_in = leaf_log.clone();
    let mut ui = host(move || {
        let mut modifier = Modifier;
        for index in 0..33usize {
            modifier = modifier.layout(move |scope, measurable, constraints| {
                let placeable = scope.measure(measurable, constraints);
                let place = match index {
                    31 => p31.get(),
                    32 => p32.get(),
                    _ => true,
                };
                scope.layout_with(placeable.width, placeable.height, move |placement| {
                    if place {
                        placement.place(&placeable, 0, 0);
                    }
                })
            });
        }
        Leaf(modifier.then(probe(&log_in)).size(12.dp()).test_tag("deep-layers"));
    });

    assert_eq!((leaf_log.placed.get(), leaf_log.unplaced.get()), (1, 0));
    assert_eq!(tag_bounds(&ui, "deep-layers"), Some(Rect::new(0.0, 0.0, 12.0, 12.0)));

    // Bit 31 (`layer == 31`) unplaces the node cleanly.
    place_31.set(false);
    ui.frame();
    assert_eq!((leaf_log.placed.get(), leaf_log.unplaced.get()), (1, 1));
    assert_eq!(tag_bounds(&ui, "deep-layers"), None);

    place_31.set(true);
    ui.frame();
    assert_eq!((leaf_log.placed.get(), leaf_log.unplaced.get()), (2, 1));

    // Layer 32 (`>= MOTION_LAYERS`) does not panic on shift or bounds read.
    place_32.set(false);
    ui.frame();
    place_32.set(true);
    ui.frame();
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

/// `SlotContent` skips an unchanged `Callback` on `force_recompose_children`, yet re-runs when a
/// `CompositionLocal` above the `SubcomposeLayout` changes or when a kept slot is reused for new
/// content (D2393).
#[test]
fn subcompose_slot_skips_clean_content_and_reruns_on_local_change_or_reuse() {
    static LOCAL_NUM: CompositionLocal<i32> = CompositionLocal::new("LOCAL_NUM", || 1);

    let state = SubcomposeLayoutState::with_policy(KeepOne);
    let parent_tick = mutable_state_of(0u32);
    let local_val = mutable_state_of(10i32);
    let show_slot = mutable_state_of(true);
    let (pt, lv, ss) = (parent_tick.handle(), local_val.handle(), show_slot.handle());
    let slot_runs = Rc::new(Cell::new(0u32));
    let seen_local = Rc::new(Cell::new(0i32));

    let sr = slot_runs.clone();
    let sl = seen_local.clone();
    let slot_cb = callback(move || {
        sr.set(sr.get() + 1);
        sl.set(LOCAL_NUM.current());
        Leaf(Modifier.size(15.dp()));
    });

    let state_in = state.clone();
    let mut ui = host(move || {
        let state = state_in.clone();
        let slot_cb = slot_cb.clone();
        CompositionLocalProvider(LOCAL_NUM.provides(lv.get()), move || {
            let _ = pt.get();
            let slot_cb = slot_cb.clone();
            let policy = subcompose_policy(move |scope, constraints| {
                if ss.get() {
                    let m = scope.subcompose("k1", slot_cb.clone());
                    let p = scope.measure(m[0], constraints);
                    scope.place(&p, 0, 0);
                }
                scope.layout(30, 30)
            });
            SubcomposeLayoutWithState(state, Modifier, policy);
        });
    });

    assert_eq!((slot_runs.get(), seen_local.get()), (1, 10));

    // Parent recomposes (`force_recompose_children`) with identical `slot_cb` and unchanged local:
    // `SlotContent` skips.
    parent_tick.set(1);
    ui.frame();
    assert_eq!(slot_runs.get(), 1, "SlotContent skips clean forced recomposition");

    // Changing a `CompositionLocal` read inside the slot forces `SlotContent` to re-run.
    local_val.set(25);
    ui.frame();
    assert_eq!((slot_runs.get(), seen_local.get()), (2, 25));

    // Deactivating into `KeepOne` and reusing with another `Callback` does NOT skip.
    show_slot.set(false);
    ui.frame();
    let reused_runs = Rc::new(Cell::new(0u32));
    let rr = reused_runs.clone();
    let handle = state.precompose(
        "k2",
        callback(move || {
            rr.set(rr.get() + 1);
            Leaf(Modifier.size(20.dp()));
        }),
    );
    assert_eq!(reused_runs.get(), 1);
    handle.dispose();
}

/// Paused twin of `SubcomposeLayoutTest.precomposeOverReusedNodeWithUpdatedModifierIsNotCausingEarlyRemeasureForIt2`:
/// a `should_auto_invalidate() == false` modifier calling `handle.invalidate_measurement()` in
/// `Element::update` during `create_paused_precomposition` runs `Element::update` inside `resume`
/// while `reuse_paused` leaves the kept-pool node detached until `apply` (`D1162`), so the manual
/// invalidation is dropped and `premeasure` skips measuring (D2448).
#[test]
#[ignore = "review: a paused precomposition reusing a kept slot updates its modifier during resume while its entries are still detached until apply"]
fn a_paused_precomposition_over_a_kept_slot_updating_its_modifier_invalidates_measurement() {
    struct ManualSizeNode {
        size: i32,
        measures: Rc<Cell<u32>>,
        handle: Option<NodeHandle>,
    }
    impl ModifierNode for ManualSizeNode {
        fn should_auto_invalidate(&self) -> bool {
            false
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            self.handle = Some(context.handle());
        }
        fn on_detach(&mut self, _: &mut NodeContext<'_>) {
            self.handle = None;
        }
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
    }
    impl LayoutModifierNode for ManualSizeNode {
        fn measure(
            &mut self,
            scope: &mut MeasureScope<'_>,
            measurable: Measurable,
            _: Constraints,
        ) -> MeasureResult {
            self.measures.set(self.measures.get() + 1);
            let p = scope.measure(measurable, Constraints::fixed(self.size, self.size));
            scope.place(&p, 0, 0);
            scope.layout(self.size, self.size)
        }
    }
    struct ManualSizeEl(i32, Rc<Cell<u32>>);
    impl ModifierNodeElement for ManualSizeEl {
        type Node = ManualSizeNode;
        fn create(&self) -> ManualSizeNode {
            ManualSizeNode { size: self.0, measures: self.1.clone(), handle: None }
        }
        fn update(&self, node: &mut ManualSizeNode) {
            if node.size != self.0 {
                node.size = self.0;
                if let Some(handle) = &node.handle {
                    handle.invalidate_measurement();
                }
            }
            node.measures = self.1.clone();
        }
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0 && Rc::ptr_eq(&self.1, &other.1)
        }
    }

    let state = SubcomposeLayoutState::with_policy(KeepOne);
    let show = mutable_state_of(true);
    let show_h = show.handle();
    let measures = Rc::new(Cell::new(0u32));

    let (state_in, m_in) = (state.clone(), measures.clone());
    let mut ui = host(move || {
        let (state, measures) = (state_in.clone(), m_in.clone());
        let policy = subcompose_policy(move |scope, _| {
            if show_h.get() {
                let m = measures.clone();
                let children = scope.subcompose(
                    "a",
                    callback(move || {
                        Leaf(Modifier.then_custom(ManualSizeEl(20, m.clone())));
                    }),
                );
                let p = scope.measure(children[0], Constraints::new(0, 50, 0, 50));
                scope.place(&p, 0, 0);
            }
            scope.layout(50, 50)
        });
        SubcomposeLayoutWithState(state, Modifier, policy);
    });
    assert_eq!(measures.get(), 1);

    // Move `"a"` into the kept pool.
    show.set(false);
    ui.frame();
    assert_eq!(measures.get(), 1);

    // Paused precomposition of `"b"` reuses `"a"`'s node with a new manual size `30`:
    let m2 = measures.clone();
    let paused = state.create_paused_precomposition(
        "b",
        callback(move || {
            Leaf(Modifier.then_custom(ManualSizeEl(30, m2.clone())));
        }),
    );
    while !paused.is_complete() {
        paused.resume(|| false);
    }
    let handle = paused.apply();
    handle.premeasure(0, Constraints::new(0, 50, 0, 50));
    assert_eq!(measures.get(), 2);
    assert_eq!(handle.get_size(0), IntSize::new(30, 30));
}
