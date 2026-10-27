//! Modifier nodes invalidating their tree (docs/parity/design/modifier-nodes.md §3.4, step
//! MN-4): the subtree invalidations, `Remeasurement` and `remeasure_sync`, and
//! `DrawModifierNode::on_measure_result_changed`.

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

// ---- Test elements ------------------------------------------------------------------------------

/// How often a counter's node measured and drew.
#[derive(Default)]
struct Counts {
    measures: Cell<u32>,
    draws: Cell<u32>,
}

impl Counts {
    /// (measures, draws).
    fn get(&self) -> (u32, u32) {
        (self.measures.get(), self.draws.get())
    }
}

/// A node that lays out and draws, counting both, as InvalidateSubtreeTest's
/// `LayoutAndDrawCounter` (compose-rs places as it measures: a measurement is a placement too).
struct Counter(Rc<Counts>);

struct CounterNode(Rc<Counts>);

impl ModifierNodeElement for Counter {
    type Node = CounterNode;
    fn create(&self) -> CounterNode {
        CounterNode(self.0.clone())
    }
    fn update(&self, node: &mut CounterNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for CounterNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for CounterNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        self.0.measures.set(self.0.measures.get() + 1);
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

impl DrawModifierNode for CounterNode {
    fn draw(&mut self, _: &mut DrawScope<'_>) {
        self.0.draws.set(self.0.draws.get() + 1);
    }
}

fn counter(counts: &Rc<Counts>) -> Modifier {
    Modifier.then_custom(Counter(counts.clone()))
}

type Held = Rc<RefCell<Option<NodeHandle>>>;

/// A node of no kind keeping its handle, as InvalidateSubtreeTest's
/// `CaptureInvalidateCounterElement`.
struct Capture(Held);

struct CaptureNode(Held);

impl ModifierNodeElement for Capture {
    type Node = CaptureNode;
    fn create(&self) -> CaptureNode {
        CaptureNode(self.0.clone())
    }
    fn update(&self, _: &mut CaptureNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for CaptureNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.0.borrow_mut() = Some(context.handle());
    }
}

/// A host drawing as Compose does (no optimizations), its first frame run.
fn parity_host(content: impl Fn() + 'static) -> UiHost {
    take_log();
    let mut host = UiHost::headless(200, 100);
    host.set_optimizations(Optimizations::NONE);
    host.set_content(content);
    host.frame();
    host
}

/// A `parity_host` whose content is in a graphics layer: Compose's root has one (the root
/// graphics layer of `AndroidComposeView`), compose-rs's root draws what's outside every repaint
/// boundary on each draw (D1164), so the test gives its content the layer Compose's has.
fn layered_host(content: impl Fn() + 'static) -> UiHost {
    let content = Rc::new(content);
    parity_host(move || {
        let content = content.clone();
        stack(Modifier.graphics_layer(|_| {}), move || content());
    })
}

/// A `Box`: a stack of `content`.
fn stack(modifier: Modifier, content: impl Fn() + 'static) {
    Stack(modifier, callback(content));
}

fn held(handle: &Held) -> NodeHandle {
    handle.borrow().expect("the node's handle")
}

// ---- Subtree invalidations ----------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/InvalidateSubtreeTest.kt, invalidateSubtreeNoLayers
#[test]
fn invalidating_a_subtree_without_layers_measures_it_and_draws_everything() {
    let counts: [Rc<Counts>; 3] = Default::default();
    let handle = Held::default();
    let (c, h) = (counts.clone(), handle.clone());
    let mut host = layered_host(move || {
        let (c, h) = (c.clone(), h.clone());
        stack(counter(&c[0]).draw_behind(|_| {}), move || {
            let c = c.clone();
            stack(counter(&c[1]).then_custom(Capture(h.clone())), move || {
                Leaf(counter(&c[2]).size(10.dp()));
            });
        });
    });
    for counts in &counts {
        assert_eq!(counts.get(), (1, 1));
    }
    held(&handle).invalidate_subtree();
    assert!(host.needs_frame());
    host.frame();
    // No layer to draw again apart: the whole window draws again, as Compose's.
    assert_eq!(counts[0].get(), (1, 2));
    assert_eq!(counts[1].get(), (2, 2));
    assert_eq!(counts[2].get(), (2, 2));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/InvalidateSubtreeTest.kt, invalidateSubtreeWithLayers
#[test]
fn invalidating_a_subtree_with_layers_draws_only_the_subtree_s_layers() {
    let counts: [Rc<Counts>; 4] = Default::default();
    let handle = Held::default();
    let (c, h) = (counts.clone(), handle.clone());
    let mut host = layered_host(move || {
        let (c, h) = (c.clone(), h.clone());
        let outer = Modifier.graphics_layer(|_| {}).then(counter(&c[0])).graphics_layer(|_| {});
        stack(outer, move || {
            let c = c.clone();
            let inner = Modifier
                .graphics_layer(|_| {})
                .then(counter(&c[1]))
                .then_custom(Capture(h.clone()))
                .graphics_layer(|_| {})
                .then(counter(&c[2]));
            stack(inner, move || Leaf(counter(&c[3]).size(10.dp())));
        });
    });
    for counts in &counts {
        assert_eq!(counts.get().1, 1);
    }
    held(&handle).invalidate_subtree();
    host.frame();
    assert_eq!(counts[0].get().1, 1, "outside the subtree's layers");
    assert_eq!(counts[1].get().1, 2);
    assert_eq!(counts[2].get().1, 2);
    assert_eq!(counts[3].get().1, 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/InvalidateSubtreeTest.kt, invalidateMeasurementForSubtreeTest
#[test]
fn invalidating_a_subtree_s_measurement_measures_it_alone() {
    let counts: [Rc<Counts>; 3] = Default::default();
    let measured = Rc::new(Cell::new(0));
    let handle = Held::default();
    let (c, h, m) = (counts.clone(), handle.clone(), measured.clone());
    let mut host = layered_host(move || {
        let (c, h, m) = (c.clone(), h.clone(), m.clone());
        stack(counter(&c[0]), move || {
            let c = c.clone();
            let m = m.clone();
            let modifier = counter(&c[1]).then_custom(Capture(h.clone())).layout(
                move |scope, measurable, constraints| {
                    let placeable = scope.measure(measurable, constraints);
                    m.set(m.get() + 1);
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                },
            );
            stack(modifier, move || Leaf(counter(&c[2]).size(10.dp())));
        });
    });
    assert_eq!(measured.get(), 1);
    for counts in &counts {
        assert_eq!(counts.get(), (1, 1));
    }
    held(&handle).invalidate_measurement_for_subtree();
    host.frame();
    assert_eq!(measured.get(), 2);
    assert_eq!(counts[0].get().0, 1, "its parent isn't measured: its size holds");
    assert_eq!(counts[1].get().0, 2);
    assert_eq!(counts[2].get().0, 2);
    // Known difference (D1164): a node measured again draws its repaint boundary again in
    // compose-rs (here the root's stand-in layer, the parent's drawing included), where
    // Compose's draws again only what moved or changed size: nothing here.
    assert_eq!(counts[0].get().1, 2);
    assert_eq!(counts[1].get().1, 2);
    assert_eq!(counts[2].get().1, 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/InvalidateSubtreeTest.kt, invalidateDrawForSubtreeTest
#[test]
fn invalidating_a_subtree_s_drawing_draws_its_layer_again_without_measuring() {
    let counts: [Rc<Counts>; 4] = Default::default();
    let handle = Held::default();
    let (c, h) = (counts.clone(), handle.clone());
    let mut host = layered_host(move || {
        let (c, h) = (c.clone(), h.clone());
        let outer = counter(&c[0]).graphics_layer(|_| {}).then(counter(&c[1])).draw_behind(|_| {});
        stack(outer, move || {
            let c = c.clone();
            stack(counter(&c[2]).then_custom(Capture(h.clone())), move || {
                Leaf(counter(&c[3]).size(10.dp()));
            });
        });
    });
    for counts in &counts {
        assert_eq!(counts.get(), (1, 1));
    }
    held(&handle).invalidate_draw_for_subtree();
    host.frame();
    assert_eq!(counts[0].get(), (1, 1), "outside the layer");
    assert_eq!(counts[1].get(), (1, 2));
    assert_eq!(counts[2].get(), (1, 2));
    assert_eq!(counts[3].get(), (1, 2));
}

// ---- Remeasurement ------------------------------------------------------------------------------

/// A policy of its constraints' smallest size, counting its measurements.
fn counted(count: &Rc<Cell<u32>>) -> Rc<dyn MeasurePolicy> {
    let count = count.clone();
    measure_policy(move |scope, measurables, constraints| {
        count.set(count.get() + 1);
        for &measurable in measurables {
            let placeable = scope.measure(measurable, constraints);
            scope.place(&placeable, 0, 0);
        }
        scope.layout(constraints.min_width, constraints.min_height)
    })
}

type HeldRemeasurement = Rc<Cell<Option<Remeasurement>>>;

fn keeping(held: &HeldRemeasurement) -> Modifier {
    let held = held.clone();
    let modifier: Rc<dyn RemeasurementModifier> =
        Rc::new(move |remeasurement: Remeasurement| held.set(Some(remeasurement)));
    Modifier.remeasurement_modifier(modifier)
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasurementModifierTest.kt, nodeIsRemeasuredAfterForceRemeasureBlocking
#[test]
fn force_remeasure_measures_a_clean_node_at_once() {
    let held = HeldRemeasurement::default();
    let measures = Rc::new(Cell::new(0));
    let (modifier, policy) = (keeping(&held), counted(&measures));
    let host = parity_host(move || {
        Layout(modifier.clone().size(10.dp()), policy.clone(), callback(|| {}));
    });
    let remeasurement = held.get().expect("given when attached");
    assert_eq!(measures.get(), 1);
    assert!(!host.needs_frame(), "the node is clean");
    remeasurement.force_remeasure();
    assert_eq!(measures.get(), 2, "measured (and placed) now, synchronously");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/layout/RemeasurementModifierTest.kt, otherNodesAreNotRemeasured
#[test]
fn force_remeasure_measures_no_other_node() {
    let held = HeldRemeasurement::default();
    let (first, second) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (modifier, first_policy, second_policy) =
        (keeping(&held), counted(&first), counted(&second));
    let host = parity_host(move || {
        Layout(modifier.clone().size(10.dp()), first_policy.clone(), callback(|| {}));
        Layout(Modifier.size(10.dp()), second_policy.clone(), callback(|| {}));
    });
    let remeasurement = held.get().expect("given when attached");
    let second_node = host.with_tree(|tree| tree.children_of(tree.root_id()).nth(1).expect("two"));
    host.with_tree(|tree| tree.invalidate_measure(second_node));
    remeasurement.force_remeasure();
    assert_eq!(first.get(), 2);
    assert_eq!(second.get(), 1, "the other node, though dirty, waits for the layout");
}

/// `force_remeasure` while the tree is busy (from inside a pass or a handler) measures the node
/// in the next layout instead, which comes before anything is drawn.
#[test]
fn force_remeasure_on_a_busy_tree_measures_in_the_next_layout() {
    let held = HeldRemeasurement::default();
    let measures = Rc::new(Cell::new(0));
    let (modifier, policy) = (keeping(&held), counted(&measures));
    let mut host = parity_host(move || {
        Layout(modifier.clone().size(10.dp()), policy.clone(), callback(|| {}));
    });
    let remeasurement = held.get().expect("given when attached");
    host.with_tree(|_| remeasurement.force_remeasure());
    assert_eq!(measures.get(), 1, "not measured inside the borrow");
    assert!(host.needs_frame());
    host.frame();
    assert_eq!(measures.get(), 2);
}

/// A node measured again by `force_remeasure` at a new size measures its parent again, as
/// Compose's `doRemeasure` asks its parent for a measurement.
#[test]
fn force_remeasure_at_a_new_size_measures_the_parent_again() {
    let held = HeldRemeasurement::default();
    let width = Rc::new(Cell::new(10));
    let parent_measures = Rc::new(Cell::new(0));
    let (modifier, w, parent_policy) = (keeping(&held), width.clone(), counted(&parent_measures));
    let mut host = parity_host(move || {
        let (modifier, w) = (modifier.clone(), w.clone());
        Layout(
            Modifier,
            parent_policy.clone(),
            callback(move || {
                let w = w.clone();
                let policy = measure_policy(move |scope, _, _| scope.layout(w.get(), 10));
                Layout(modifier.clone(), policy, callback(|| {}));
            }),
        );
    });
    let remeasurement = held.get().expect("given when attached");
    assert_eq!(parent_measures.get(), 1);
    width.set(20);
    remeasurement.force_remeasure();
    assert!(host.needs_frame(), "its parent measures again");
    host.frame();
    assert_eq!(parent_measures.get(), 2);
}

/// A layout node whose `on_attach` asks `remeasure_sync` from a task: measured when the call
/// returns, its own entry back in its chain.
#[test]
fn remeasure_sync_from_a_task_measures_the_node_and_its_own_layout_entry() {
    struct Syncing(Held, Rc<Counts>);
    struct SyncingNode(Held, Rc<Counts>);
    impl ModifierNodeElement for Syncing {
        type Node = SyncingNode;
        fn create(&self) -> SyncingNode {
            SyncingNode(self.0.clone(), self.1.clone())
        }
        fn update(&self, _: &mut SyncingNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for SyncingNode {
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            *self.0.borrow_mut() = Some(context.handle());
            // In the composition pass's end: measured in the frame's layout, which follows.
            context.remeasure_sync();
        }
    }
    impl LayoutModifierNode for SyncingNode {
        fn measure(
            &mut self,
            scope: &mut MeasureScope<'_>,
            content: Measurable,
            constraints: Constraints,
        ) -> MeasureResult {
            self.1.measures.set(self.1.measures.get() + 1);
            let placeable = scope.measure(content, constraints);
            scope.place(&placeable, 0, 0);
            scope.layout(placeable.width, placeable.height)
        }
    }
    let (handle, counts) = (Held::default(), Rc::new(Counts::default()));
    let (h, c) = (handle.clone(), counts.clone());
    let mut host =
        parity_host(move || Leaf(Modifier.then_custom(Syncing(h.clone(), c.clone())).size(5.dp())));
    assert_eq!(counts.get().0, 1, "measured once, by the first frame's layout");
    let node = held(&handle);
    let done = Rc::new(Cell::new(None));
    let seen = done.clone();
    let counted = counts.clone();
    compose_runtime::CoroutineScope::new().launch(async move {
        let ran = node.with_node(|_: &mut SyncingNode, context| {
            context.remeasure_sync();
            counted.get().0
        });
        seen.set(ran.map(|during| (during, counted.get().0)));
    });
    host.frame();
    assert_eq!(done.get(), Some((1, 2)), "measured once the callback returned");
}

// ---- on_measure_result_changed ------------------------------------------------------------------

/// A draw node logging `on_measure_result_changed`.
struct Sized(Rc<Cell<u32>>);

struct SizedNode(Rc<Cell<u32>>);

impl ModifierNodeElement for Sized {
    type Node = SizedNode;
    fn create(&self) -> SizedNode {
        SizedNode(self.0.clone())
    }
    fn update(&self, _: &mut SizedNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for SizedNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for SizedNode {
    fn on_measure_result_changed(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached());
        self.0.set(self.0.get() + 1);
    }
}

/// `DrawModifierNode::on_measure_result_changed` is called when the measured size of the layer
/// the node draws in changes, as Compose's coordinator calls it (`onMeasureResultChanged`), not
/// when it's measured again at the same size.
#[test]
fn a_draw_node_hears_its_layer_s_size_change() {
    let (outer, inner) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let size = mutable_state_of(10);
    let (o, i, s) = (outer.clone(), inner.clone(), size.handle());
    let mut host = parity_host(move || {
        // The outer node draws in the padding's layer (20 + its size), the inner one in the
        // size's layer.
        let modifier = Modifier
            .then_custom(Sized(o.clone()))
            .padding(10.dp())
            .then_custom(Sized(i.clone()))
            .size(s.get().dp());
        Leaf(modifier);
    });
    assert_eq!((outer.get(), inner.get()), (1, 1), "a first size");
    let leaf = host.with_tree(|tree| tree.children_of(tree.root_id()).next().expect("the leaf"));
    host.with_tree(|tree| tree.invalidate_measure(leaf));
    host.frame();
    assert_eq!((outer.get(), inner.get()), (1, 1), "measured again at the same sizes");
    size.set(20);
    host.frame();
    assert_eq!((outer.get(), inner.get()), (2, 2));
}
