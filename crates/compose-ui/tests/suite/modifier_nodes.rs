//! `Modifier.Node` infrastructure as Compose's (docs/parity/design/modifier-nodes.md): the chain
//! diff, auto-invalidation by node kinds, and the nodes' lifecycle.

#![allow(non_snake_case)]

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

// ---- Test elements ------------------------------------------------------------------------------

thread_local! {
    static SERIAL: Cell<u32> = const { Cell::new(0) };
}

/// A new node's serial: its identity across chain diffs.
fn next_serial() -> u32 {
    SERIAL.with(|serial| {
        serial.set(serial.get() + 1);
        serial.get()
    })
}

/// An element of one type for every name, equal only to itself, as Compose's
/// `reusableModifier(name)`: the chain diff updates its nodes in place.
struct Reusable(&'static str);

struct ReusableNode {
    name: &'static str,
    serial: u32,
}

impl ModifierNodeElement for Reusable {
    type Node = ReusableNode;
    fn create(&self) -> ReusableNode {
        ReusableNode { name: self.0, serial: next_serial() }
    }
    fn update(&self, node: &mut ReusableNode) {
        node.name = self.0;
    }
    fn same(&self, _: &Self) -> bool {
        false
    }
}

impl ModifierNode for ReusableNode {}

fn reusable(name: &'static str) -> Modifier {
    Modifier.then_custom(Reusable(name))
}

/// Elements of a type per letter, as Compose's `modifierA()` … `modifierD()`.
macro_rules! typed_elements {
    ($($element:ident, $node:ident, $name:literal, $function:ident;)*) => {$(
        struct $element;
        struct $node(u32);
        impl ModifierNodeElement for $element {
            type Node = $node;
            fn create(&self) -> $node {
                $node(next_serial())
            }
            fn update(&self, _: &mut $node) {}
            fn same(&self, _: &Self) -> bool {
                true
            }
        }
        impl ModifierNode for $node {}
        fn $function() -> Modifier {
            Modifier.then_custom($element)
        }
    )*};
}

typed_elements! {
    ElementA, NodeA, "a", modifier_a;
    ElementB, NodeB, "b", modifier_b;
    ElementC, NodeC, "c", modifier_c;
    ElementD, NodeD, "d", modifier_d;
}

/// What a test's node is: its name and serial.
fn describe(node: &dyn ModifierNode) -> (&'static str, u32) {
    if let Some(node) = node.downcast_ref::<ReusableNode>() {
        (node.name, node.serial)
    } else if let Some(node) = node.downcast_ref::<NodeA>() {
        ("a", node.0)
    } else if let Some(node) = node.downcast_ref::<NodeB>() {
        ("b", node.0)
    } else if let Some(node) = node.downcast_ref::<NodeC>() {
        ("c", node.0)
    } else if let Some(node) = node.downcast_ref::<NodeD>() {
        ("d", node.0)
    } else {
        ("?", 0)
    }
}

/// A leaf whose modifier a test sets, as Compose's `NodeChainTester`.
struct ChainTester {
    host: UiHost,
    modifier: Rc<RefCell<Modifier>>,
    version: OwnedState<u32>,
}

impl ChainTester {
    fn new() -> ChainTester {
        let modifier = Rc::new(RefCell::new(Modifier));
        let version = mutable_state_of(0);
        let (content_modifier, content_version) = (modifier.clone(), version.handle());
        let host = host(move || {
            content_version.get();
            let modifier = content_modifier.borrow().clone();
            Leaf(modifier);
        });
        ChainTester { host, modifier, version }
    }

    /// Sets the leaf's modifier and runs a frame.
    fn with(&mut self, modifier: Modifier) -> &mut ChainTester {
        *self.modifier.borrow_mut() = modifier;
        self.version.update(|version| version + 1);
        self.host.frame();
        self
    }

    fn leaf(&self) -> NodeId {
        self.host.with_tree(|tree| tree.children_of(tree.root_id()).next().expect("the leaf"))
    }

    /// The chain's nodes, by name and serial.
    fn nodes(&self) -> Vec<(&'static str, u32)> {
        let leaf = self.leaf();
        self.host.with_tree(|tree| tree.modifier_nodes(leaf).map(describe).collect())
    }

    /// The chain as Compose's `NodeChain.toString`: `[a,b,c]`.
    fn names(&self) -> String {
        let names: Vec<&str> = self.nodes().into_iter().map(|(name, _)| name).collect();
        format!("[{}]", names.join(","))
    }
}

// ---- The chain diff -----------------------------------------------------------------------------

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testInsertsAndDeletesAtTail
#[test]
fn elements_inserted_and_deleted_at_the_tail_leave_the_head_nodes() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    assert_eq!(tester.with(a.clone().then(b.clone()).then(c.clone())).names(), "[a,b,c]");
    let before = tester.nodes();
    let chain = a.clone().then(b.clone()).then(c.clone()).then(b.clone());
    assert_eq!(tester.with(chain).names(), "[a,b,c,b]");
    assert_eq!(tester.nodes()[..3], before[..]);
    assert_eq!(tester.with(a.then(b).then(c)).names(), "[a,b,c]");
    assert_eq!(tester.nodes(), before);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testInsertsAndDeletesAtHead
#[test]
fn elements_inserted_and_deleted_at_the_head() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    assert_eq!(tester.with(a.clone().then(b.clone()).then(c.clone())).names(), "[a,b,c]");
    let chain = c.clone().then(a.clone()).then(b.clone()).then(c.clone());
    assert_eq!(tester.with(chain).names(), "[c,a,b,c]");
    assert_eq!(tester.with(a.then(b).then(c)).names(), "[a,b,c]");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testInsertsInMiddle
#[test]
fn an_element_inserted_in_the_middle_and_removed() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    assert_eq!(tester.with(a.clone().then(b.clone()).then(c.clone())).names(), "[a,b,c]");
    let chain = a.clone().then(b.clone()).then(a.clone()).then(c.clone());
    assert_eq!(tester.with(chain).names(), "[a,b,a,c]");
    assert_eq!(tester.with(a.then(b).then(c)).names(), "[a,b,c]");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testRemovingEverything
#[test]
fn removing_every_element_empties_the_chain() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    assert_eq!(tester.with(a.then(b).then(c)).names(), "[a,b,c]");
    assert_eq!(tester.with(Modifier).names(), "[]");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testSwapModifierInMiddle
#[test]
fn an_element_swapped_in_the_middle() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    let chain = a.clone().then(b.clone()).then(b.clone()).then(b.clone()).then(c.clone());
    assert_eq!(tester.with(chain).names(), "[a,b,b,b,c]");
    let before = tester.nodes();
    let chain = a.then(b.clone()).then(c.clone()).then(b).then(c);
    assert_eq!(tester.with(chain).names(), "[a,b,c,b,c]");
    // The same kinds in the same order: every node updated in place.
    let serials = |nodes: Vec<(&str, u32)>| nodes.into_iter().map(|(_, serial)| serial).collect();
    let (before, after): (Vec<u32>, Vec<u32>) = (serials(before), serials(tester.nodes()));
    assert_eq!(before, after);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testSwapModifierAtHead
#[test]
fn an_element_swapped_at_the_head() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    let chain = a.then(b.clone()).then(b.clone()).then(b.clone()).then(c.clone());
    assert_eq!(tester.with(chain).names(), "[a,b,b,b,c]");
    let chain = c.clone().then(b.clone()).then(b.clone()).then(b).then(c);
    assert_eq!(tester.with(chain).names(), "[c,b,b,b,c]");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testSwapModifierAtTail
#[test]
fn an_element_swapped_at_the_tail() {
    let (a, b, c) = (reusable("a"), reusable("b"), reusable("c"));
    let mut tester = ChainTester::new();
    let chain = a.clone().then(b.clone()).then(b.clone()).then(b.clone()).then(c);
    assert_eq!(tester.with(chain).names(), "[a,b,b,b,c]");
    let chain = a.clone().then(b.clone()).then(b.clone()).then(b).then(a);
    assert_eq!(tester.with(chain).names(), "[a,b,b,b,a]");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testReused
#[test]
fn new_elements_of_the_same_types_reuse_the_nodes() {
    let mut tester = ChainTester::new();
    let first = reusable("a").then(reusable("b")).then(reusable("c"));
    assert_eq!(tester.with(first).names(), "[a,b,c]");
    let before = tester.nodes();
    let second = reusable("a").then(reusable("b")).then(reusable("c"));
    assert_eq!(tester.with(second).names(), "[a,b,c]");
    assert_eq!(tester.nodes(), before);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testSimple
#[test]
fn a_removed_element_s_node_goes_and_the_others_stay() {
    let mut tester = ChainTester::new();
    assert_eq!(tester.with(modifier_a().then(modifier_b()).then(modifier_c())).names(), "[a,b,c]");
    let before = tester.nodes();
    assert_eq!(tester.with(modifier_a().then(modifier_c())).names(), "[a,c]");
    let after = tester.nodes();
    assert_eq!(after, [before[0], before[2]], "a and c kept, b removed");
    tester.with(modifier_a().then(modifier_d()).then(modifier_b()));
    let last = tester.nodes();
    assert_eq!(last[0], after[0], "a kept");
    assert_ne!(last[2].1, after[1].1, "c's node isn't b's");
    assert_eq!(tester.names(), "[a,d,b]");
}

// ---- Auto-invalidation by kinds -----------------------------------------------------------------

/// A policy of the smallest size its constraints allow, counting its measurements.
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

/// A colour change of a `background` draws its node again and doesn't measure it, as Compose's
/// update of a draw node invalidates its drawing only (`NodeKind.kt`, `autoInvalidateNodeSelf`).
#[test]
fn a_background_colour_change_draws_again_without_measuring() {
    let color = mutable_state_of(Color::RED);
    let measures = Rc::new(Cell::new(0));
    let (handle, policy) = (color.handle(), counted(&measures));
    let mut host = host(move || {
        Layout(Modifier.size(10.dp()).background(handle.get()), policy.clone(), callback(|| {}));
    });
    assert_eq!(measures.get(), 1);
    assert!(host.display_list().dump().contains("#FFFF0000"), "{}", host.display_list().dump());
    color.set(Color::BLUE);
    host.frame();
    assert_eq!(measures.get(), 1, "not measured again");
    assert!(host.display_list().dump().contains("#FF0000FF"), "{}", host.display_list().dump());
}

/// A draw-only custom element.
struct Tint(Color);

struct TintNode(Color);

impl ModifierNodeElement for Tint {
    type Node = TintNode;
    fn create(&self) -> TintNode {
        TintNode(self.0)
    }
    fn update(&self, node: &mut TintNode) {
        node.0 = self.0;
    }
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for TintNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
}

impl DrawModifierNode for TintNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(self.0));
    }
}

/// A user-defined draw node updated with a new colour draws again, unmeasured.
#[test]
fn a_custom_draw_node_s_update_draws_again_without_measuring() {
    let color = mutable_state_of(Color::RED);
    let measures = Rc::new(Cell::new(0));
    let (handle, policy) = (color.handle(), counted(&measures));
    let mut host = host(move || {
        Layout(
            Modifier.size(10.dp()).then_custom(Tint(handle.get())),
            policy.clone(),
            callback(|| {}),
        );
    });
    assert_eq!(measures.get(), 1);
    color.set(Color::GREEN);
    host.frame();
    assert_eq!(measures.get(), 1);
    assert!(host.display_list().dump().contains("#FF00FF00"), "{}", host.display_list().dump());
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/CustomLayoutAndMeasureTest.kt, updatingModifierIsNotCausingParentsRelayout
#[test]
fn an_updated_layout_modifier_measures_its_node_not_its_parent() {
    let generation = mutable_state_of(0);
    let (parent, child) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let handle = generation.handle();
    let (parent_policy, child_count) = (counted(&parent), child.clone());
    let mut host = host(move || {
        let generation = handle.get();
        let child_count = child_count.clone();
        Layout(
            Modifier,
            parent_policy.clone(),
            callback(move || {
                let child_count = child_count.clone();
                // A new closure each pass: a new element of the same type, which updates the node.
                let modifier = Modifier.layout(move |scope, measurable, constraints| {
                    let _ = generation;
                    child_count.set(child_count.get() + 1);
                    let placeable = scope.measure(measurable, constraints);
                    scope.place(&placeable, 0, 0);
                    scope.layout(placeable.width, placeable.height)
                });
                Layout(
                    modifier,
                    measure_policy(|scope, _, _| scope.layout(10, 10)),
                    callback(|| {}),
                );
            }),
        );
    });
    assert_eq!((parent.get(), child.get()), (1, 1));
    generation.set(1);
    host.frame();
    assert_eq!(child.get(), 2, "the updated layout modifier measures again");
    assert_eq!(parent.get(), 1, "its size held: the parent isn't measured again");
}

/// A node that opts out of auto-invalidation (`should_auto_invalidate`) isn't measured again
/// when its element updates it, as Compose's `shouldAutoInvalidate = false`.
#[test]
fn a_node_opting_out_of_auto_invalidation_is_not_measured_on_update() {
    struct Quiet(u32);
    struct QuietNode;
    impl ModifierNodeElement for Quiet {
        type Node = QuietNode;
        fn create(&self) -> QuietNode {
            QuietNode
        }
        fn update(&self, _: &mut QuietNode) {}
        fn same(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl ModifierNode for QuietNode {
        fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
            Some(self)
        }
        fn should_auto_invalidate(&self) -> bool {
            false
        }
    }
    impl LayoutModifierNode for QuietNode {
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
    let value = mutable_state_of(0);
    let measures = Rc::new(Cell::new(0));
    let (handle, policy) = (value.handle(), counted(&measures));
    let mut host = host(move || {
        Layout(Modifier.then_custom(Quiet(handle.get())), policy.clone(), callback(|| {}));
    });
    assert_eq!(measures.get(), 1);
    value.set(1);
    host.frame();
    assert_eq!(measures.get(), 1, "no measurement for an update that asks for none");
}

// ---- Graphics layer equality --------------------------------------------------------------------

fn layer_with(values: [f32; 7], origin: TransformOrigin, shape: Shape, clip: bool) -> Modifier {
    Modifier.graphics_layer_with(GraphicsLayerScope {
        scale_x: values[0],
        scale_y: values[1],
        alpha: values[2],
        translation_x: values[3],
        translation_y: values[4],
        shadow_elevation: values[5],
        rotation_z: values[6],
        transform_origin: origin,
        ambient_shadow_color: Color::from_argb(0xFF00FF42),
        spot_shadow_color: Color::from_argb(0xFF00FF64),
        shape,
        clip,
        ..GraphicsLayerScope::default()
    })
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/GraphicsLayerModifierTest.kt, testEquals
#[test]
fn graphics_layers_of_equal_properties_are_the_same_element() {
    let values = [1.0, 2.0, 0.75, 3.0, 4.0, 5.0, 8.0];
    let make = || layer_with(values, TransformOrigin::CENTER, Shape::Rectangle, true);
    assert!(make().same(&make()));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/draw/GraphicsLayerModifierTest.kt, testNotEquals
#[test]
fn graphics_layers_differing_in_one_property_are_not_the_same_element() {
    // compose-rs's layers have no rotation about x or y (ledger: layer-3d-and-effects).
    let values = [1.0, 2.0, 0.75, 3.0, 4.0, 5.0, 8.0];
    let regular = layer_with(values, TransformOrigin::CENTER, Shape::Rectangle, true);
    for index in 0..values.len() {
        let mut changed = values;
        changed[index] = 0.5;
        let other = layer_with(changed, TransformOrigin::CENTER, Shape::Rectangle, true);
        assert!(!other.same(&regular), "property {index}");
    }
    let origin = TransformOrigin { pivot_fraction_x: 0.0, pivot_fraction_y: 0.0 };
    assert!(!layer_with(values, origin, Shape::Rectangle, true).same(&regular));
    let circle = Shape::Rounded([CornerSize::Percent(50.0); 4]);
    assert!(!layer_with(values, TransformOrigin::CENTER, circle, true).same(&regular));
    let unclipped = layer_with(values, TransformOrigin::CENTER, Shape::Rectangle, false);
    assert!(!unclipped.same(&regular));
}

// ---- Reuse --------------------------------------------------------------------------------------

/// What a reused test element and its node did: `create`, `update`, `reset`.
type Events = Rc<RefCell<Vec<&'static str>>>;

fn count(events: &Events, event: &str) -> usize {
    events.borrow().iter().filter(|logged| **logged == event).count()
}

/// As the Compose test's `GenericModifierWithLifecycle`: equal to an element of the same key.
struct WithLifecycle {
    key: u32,
    events: Events,
}

struct WithLifecycleNode(Events);

impl ModifierNodeElement for WithLifecycle {
    type Node = WithLifecycleNode;
    fn create(&self) -> WithLifecycleNode {
        self.events.borrow_mut().push("create");
        WithLifecycleNode(self.events.clone())
    }
    fn update(&self, _: &mut WithLifecycleNode) {
        self.events.borrow_mut().push("update");
    }
    fn same(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl ModifierNode for WithLifecycleNode {
    fn on_reset(&mut self, _context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push("reset");
    }
    fn on_attach(&mut self, _context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push("attach");
    }
    fn on_detach(&mut self, _context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push("detach");
    }
}

/// A 100 × 100 layout with a `WithLifecycle` element of `key`, as the Compose test's
/// `TestLayout`.
fn test_layout(key: u32, events: &Events) {
    let modifier = Modifier.then_custom(WithLifecycle { key, events: events.clone() });
    Layout(modifier, measure_policy(|scope, _, _| scope.layout(100, 100)), callback(|| {}));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, reusingCallsResetOnModifier
#[test]
fn reused_content_resets_its_modifier_nodes() {
    let reuse_key = mutable_state_of(0);
    let events = Events::default();
    let (key, content_events) = (reuse_key.handle(), events.clone());
    let mut host = host(move || ReusableContent(key.get(), || test_layout(0, &content_events)));
    assert_eq!(count(&events, "reset"), 0);
    reuse_key.set(1);
    host.frame();
    assert_eq!(count(&events, "reset"), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, nodeIsNotRecreatedWhenReused
#[test]
fn reused_content_keeps_its_modifier_nodes() {
    let reuse_key = mutable_state_of(0);
    let events = Events::default();
    let (key, content_events) = (reuse_key.handle(), events.clone());
    let mut host = host(move || ReusableContent(key.get(), || test_layout(0, &content_events)));
    assert_eq!(count(&events, "create"), 1);
    reuse_key.set(1);
    host.frame();
    assert_eq!(count(&events, "create"), 1);
}

/// Reusable content in a host `active` says.
fn hosted(active: MutableState<bool>, events: &Events) -> impl Fn() + 'static {
    let events = events.clone();
    move || {
        let events = events.clone();
        ReusableContentHost(active.get(), move || {
            ReusableContent(0, || test_layout(0, &events));
        });
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, resetIsCalledWhenContentIsDeactivated
#[test]
fn deactivated_content_resets_its_modifier_nodes() {
    let active = mutable_state_of(true);
    let events = Events::default();
    let mut host = host(hosted(active.handle(), &events));
    assert_eq!(count(&events, "reset"), 0);
    active.set(false);
    host.frame();
    assert_eq!(count(&events, "reset"), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, resetIsCalledAgainWhenContentIsReactivated
#[test]
fn reactivated_content_is_not_reset_again() {
    let active = mutable_state_of(true);
    let events = Events::default();
    let mut host = host(hosted(active.handle(), &events));
    active.set(false);
    host.frame();
    active.set(true);
    host.frame();
    // Reset once, at deactivation: a detached node isn't reset (D1156).
    assert_eq!(count(&events, "reset"), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, updateIsNotCalledWhenReusedWithTheSameParams
#[test]
fn reused_content_with_equal_elements_updates_nothing() {
    let reuse_key = mutable_state_of(0);
    let events = Events::default();
    let (key, content_events) = (reuse_key.handle(), events.clone());
    let mut host = host(move || ReusableContent(key.get(), || test_layout(1, &content_events)));
    assert_eq!(count(&events, "update"), 0);
    reuse_key.set(1);
    host.frame();
    assert_eq!(count(&events, "update"), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, updateIsCalledWhenReusedWithDifferentParam
#[test]
fn reused_content_with_a_new_element_updates_its_node() {
    let reuse_key = mutable_state_of(0);
    let events = Events::default();
    let (key, content_events) = (reuse_key.handle(), events.clone());
    let mut host = host(move || {
        let key = key.get();
        ReusableContent(key, || test_layout(key, &content_events))
    });
    assert_eq!(count(&events, "update"), 0);
    reuse_key.set(1);
    host.frame();
    assert_eq!(count(&events, "update"), 1);
}

/// As the Compose test's `StatelessElement`: measures its content at `size` and counts it.
struct Stateless {
    on_measure: Rc<Cell<u32>>,
    size: i32,
}

struct StatelessNode {
    on_measure: Rc<Cell<u32>>,
    size: i32,
}

impl ModifierNodeElement for Stateless {
    type Node = StatelessNode;
    fn create(&self) -> StatelessNode {
        StatelessNode { on_measure: self.on_measure.clone(), size: self.size }
    }
    fn update(&self, node: &mut StatelessNode) {
        node.size = self.size;
        node.on_measure = self.on_measure.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.on_measure, &other.on_measure) && self.size == other.size
    }
}

impl ModifierNode for StatelessNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for StatelessNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        _: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, Constraints::fixed(self.size, self.size));
        self.on_measure.set(self.on_measure.get() + 1);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

/// Reusable content in an active host, its key `key`, holding a `Stateless` of `size`.
fn stateless_content(
    active: MutableState<bool>,
    key: MutableState<u32>,
    size: MutableState<i32>,
    on_measure: &Rc<Cell<u32>>,
) -> impl Fn() + 'static {
    let on_measure = on_measure.clone();
    // One policy, as the Compose test's `MeasurePolicy` value: a new one would measure again.
    let policy = measure_policy(|scope, _, _| scope.layout(100, 100));
    move || {
        let (on_measure, policy) = (on_measure.clone(), policy.clone());
        ReusableContentHost(active.get(), move || {
            let (on_measure, policy) = (on_measure.clone(), policy.clone());
            ReusableContent(key.get(), move || {
                let modifier = Modifier
                    .then_custom(Stateless { on_measure: on_measure.clone(), size: size.get() });
                Layout(modifier, policy.clone(), callback(|| {}));
            });
        });
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, reusingStatelessModifierWithUpdatedInputCausingInvalidation
#[test]
fn reused_content_with_a_changed_layout_element_measures_again() {
    let (active, key, size) = (mutable_state_of(true), mutable_state_of(0), mutable_state_of(10));
    let invalidations = Rc::new(Cell::new(0));
    let content = stateless_content(active.handle(), key.handle(), size.handle(), &invalidations);
    let mut host = host(content);
    assert_eq!(invalidations.get(), 1);
    active.set(false);
    host.frame();
    active.set(true);
    key.set(1);
    size.set(20);
    host.frame();
    assert_eq!(invalidations.get(), 2);
}

// ---- The lifecycle ------------------------------------------------------------------------------

/// A shared log of lifecycle events, as the Compose tests' `log`.
type Log = Rc<RefCell<Vec<String>>>;

fn take(log: &Log) -> Vec<String> {
    std::mem::take(&mut *log.borrow_mut())
}

/// As `ModifierNodeAttachOrderTest`'s `LoggerElement`: equal to an element of the same name.
#[derive(Clone)]
struct Logger {
    log: Log,
    name: &'static str,
}

struct LoggerNode {
    log: Log,
    name: &'static str,
}

impl ModifierNodeElement for Logger {
    type Node = LoggerNode;
    fn create(&self) -> LoggerNode {
        LoggerNode { log: self.log.clone(), name: self.name }
    }
    fn update(&self, node: &mut LoggerNode) {
        node.log.borrow_mut().push(format!("update({} -> {})", node.name, self.name));
        node.name = self.name;
    }
    fn same(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl ModifierNode for LoggerNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached(), "attached when on_attach runs");
        self.log.borrow_mut().push(format!("attach({})", self.name));
    }
    fn on_detach(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached(), "still attached when on_detach runs");
        self.log.borrow_mut().push(format!("detach({})", self.name));
    }
    fn on_reset(&mut self, _context: &mut NodeContext<'_>) {
        self.log.borrow_mut().push(format!("reset({})", self.name));
    }
}

fn logger(log: &Log, name: &'static str) -> Modifier {
    Modifier.then_custom(Logger { log: log.clone(), name })
}

/// A layout node with `chains[parent]` holding one with `chains[child]`.
fn nested(
    parent: MutableState<usize>,
    child: MutableState<usize>,
    chains: Rc<Vec<Modifier>>,
) -> impl Fn() + 'static {
    move || {
        let inner = chains[child.get()].clone();
        Stack!(modifier = chains[parent.get()].clone(), {
            Stack(inner.clone(), callback(|| {}));
        });
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAttachOrderTest.kt, attachOrderInitialComposition
#[test]
fn modifier_nodes_attach_children_first_head_to_tail() {
    let log = Log::default();
    let chains = Rc::new(vec![
        logger(&log, "a").then(logger(&log, "b")),
        logger(&log, "c").then(logger(&log, "d")),
    ]);
    let (parent, child) = (mutable_state_of(0), mutable_state_of(1));
    let _host = host(nested(parent.handle(), child.handle(), chains));
    // Compose's test checks the set; its order (LayoutNode.attach) is the children's first.
    assert_eq!(take(&log), ["attach(c)", "attach(d)", "attach(a)", "attach(b)"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAttachOrderTest.kt, attachOrderUpdate
#[test]
fn modifier_nodes_added_by_a_chain_change_attach() {
    let log = Log::default();
    let padding = Modifier.padding(0.dp());
    let chains = Rc::new(vec![
        padding.clone(),
        logger(&log, "a").then(padding.clone()).then(logger(&log, "b")),
        logger(&log, "c").then(padding.clone()).then(logger(&log, "d")),
    ]);
    let (parent, child) = (mutable_state_of(0), mutable_state_of(0));
    let mut host = host(nested(parent.handle(), child.handle(), chains));
    assert_eq!(take(&log), Vec::<String>::new());
    parent.set(1);
    child.set(2);
    host.frame();
    assert_eq!(take(&log), ["attach(a)", "attach(b)", "attach(c)", "attach(d)"]);
}

/// A leaf whose chain is `chains[index]`.
fn leaf_of(index: MutableState<usize>, chains: Rc<Vec<Modifier>>) -> impl Fn() + 'static {
    move || Leaf(chains[index.get()].clone())
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAttachOrderTest.kt, attachOrderWhenMiddleIsRemoved
#[test]
fn a_removed_middle_node_updates_the_next_and_detaches_the_last() {
    let log = Log::default();
    let chains = Rc::new(vec![
        logger(&log, "a").then(logger(&log, "b")).then(logger(&log, "c")),
        logger(&log, "a").then(logger(&log, "c")),
    ]);
    let index = mutable_state_of(0);
    let mut host = host(leaf_of(index.handle(), chains));
    assert_eq!(take(&log), ["attach(a)", "attach(b)", "attach(c)"]);
    index.set(1);
    host.frame();
    // Compose's test checks the set: the update runs as the pass goes, the detach at its end.
    assert_eq!(take(&log), ["update(b -> c)", "detach(c)"]);
    index.set(0);
    host.frame();
    assert_eq!(take(&log), ["update(c -> b)", "attach(c)"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAttachOrderTest.kt, addMultipleNodesInMiddle
#[test]
fn nodes_added_in_the_middle_attach() {
    let log = Log::default();
    let padding = || Modifier.padding(10.dp());
    let chains = Rc::new(vec![
        logger(&log, "a").then(padding()).then(padding()).then(logger(&log, "z")),
        logger(&log, "a")
            .then(padding())
            .then(logger(&log, "b"))
            .then(logger(&log, "c"))
            .then(padding())
            .then(logger(&log, "z")),
    ]);
    let index = mutable_state_of(0);
    let mut host = host(leaf_of(index.handle(), chains));
    assert_eq!(take(&log), ["attach(a)", "attach(z)"]);
    index.set(1);
    host.frame();
    assert_eq!(take(&log), ["attach(b)", "attach(c)"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAttachOrderTest.kt, addMultipleNodesInMiddleMultipleLayouts
#[test]
fn nodes_added_in_the_middle_of_two_layouts_attach_parent_first() {
    let log = Log::default();
    let padding = || Modifier.padding(10.dp());
    let around = |first: &'static str, last: &'static str| {
        logger(&log, first).then(padding()).then(padding()).then(logger(&log, last))
    };
    let inside = |first, second, third, last| {
        logger(&log, first)
            .then(padding())
            .then(logger(&log, second))
            .then(logger(&log, third))
            .then(padding())
            .then(logger(&log, last))
    };
    let chains = Rc::new(vec![
        around("a", "d"),
        around("e", "h"),
        inside("a", "b", "c", "d"),
        inside("e", "f", "g", "h"),
    ]);
    let (parent, child) = (mutable_state_of(0), mutable_state_of(1));
    let mut host = host(nested(parent.handle(), child.handle(), chains));
    assert_eq!(take(&log), ["attach(e)", "attach(h)", "attach(a)", "attach(d)"]);
    parent.set(2);
    child.set(3);
    host.frame();
    // The parent's chain changes first, then the child's, as Compose's.
    assert_eq!(take(&log), ["attach(b)", "attach(c)", "attach(f)", "attach(g)"]);
}

/// As `NodeChainTests`' `AttachedStateDebuggerNode`: checks it's attached in its callbacks, and
/// keeps a handle the test checks afterwards.
struct Debugger(Rc<RefCell<Vec<NodeHandle>>>);

struct DebuggerNode(Rc<RefCell<Vec<NodeHandle>>>);

impl ModifierNodeElement for Debugger {
    type Node = DebuggerNode;
    fn create(&self) -> DebuggerNode {
        DebuggerNode(self.0.clone())
    }
    fn update(&self, _: &mut DebuggerNode) {}
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for DebuggerNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached());
        let handle = context.handle();
        self.0.borrow_mut().push(handle);
        // Every node attached so far, with it, is attached.
        assert!(self.0.borrow().iter().all(NodeHandle::is_attached));
    }
    fn on_detach(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached());
        assert!(self.0.borrow().iter().all(NodeHandle::is_attached));
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testAttachDetach
#[test]
fn chained_nodes_are_attached_in_their_callbacks_and_after() {
    let handles = Rc::new(RefCell::new(Vec::new()));
    let chain =
        Modifier.then_custom(Debugger(handles.clone())).then_custom(Debugger(handles.clone()));
    let index = mutable_state_of(0);
    let mut host = host(leaf_of(index.handle(), Rc::new(vec![chain, Modifier])));
    assert_eq!(handles.borrow().len(), 2);
    assert!(handles.borrow().iter().all(NodeHandle::is_attached));
    index.set(1);
    host.frame();
    assert!(!handles.borrow().iter().any(NodeHandle::is_attached), "detached once removed");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/NodeChainTests.kt, testAttachDetach2
#[test]
fn nodes_inserted_around_an_attached_one_are_attached() {
    let handles = Rc::new(RefCell::new(Vec::new()));
    let (a, b, c) = (modifier_a(), modifier_b(), Modifier.then_custom(Debugger(handles.clone())));
    let chains = Rc::new(vec![a.clone(), c.then(a).then(b)]);
    let index = mutable_state_of(0);
    let mut host = host(leaf_of(index.handle(), chains));
    index.set(1);
    host.frame();
    let handles = handles.borrow();
    assert_eq!(handles.len(), 1);
    assert!(handles.iter().all(NodeHandle::is_attached));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, nodesAreDetachedWhenReused
#[test]
fn reused_nodes_are_reset_detached_and_attached_again() {
    let reuse_key = mutable_state_of(0);
    let events = Events::default();
    let (key, content_events) = (reuse_key.handle(), events.clone());
    let mut host = host(move || ReusableContent(key.get(), || test_layout(0, &content_events)));
    assert_eq!(*events.borrow(), ["create", "attach"]);
    events.borrow_mut().clear();
    reuse_key.set(1);
    host.frame();
    // The reset before the detach, as Compose's `resetModifierState`.
    assert_eq!(*events.borrow(), ["reset", "detach", "attach"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, nodesAreDetachedAndAttachedWhenDeactivatedAndReactivated
#[test]
fn deactivated_nodes_are_detached_and_reactivated_ones_attached() {
    let active = mutable_state_of(true);
    let events = Events::default();
    let mut host = host(hosted(active.handle(), &events));
    assert_eq!(count(&events, "attach"), 1);
    events.borrow_mut().clear();
    active.set(false);
    host.frame();
    assert_eq!(*events.borrow(), ["reset", "detach"]);
    events.borrow_mut().clear();
    active.set(true);
    host.frame();
    assert_eq!(*events.borrow(), ["attach"]);
}

/// An element whose node keeps a handle to itself.
struct Handled(Rc<RefCell<Option<NodeHandle>>>);

struct HandledNode(Rc<RefCell<Option<NodeHandle>>>);

impl ModifierNodeElement for Handled {
    type Node = HandledNode;
    fn create(&self) -> HandledNode {
        HandledNode(self.0.clone())
    }
    fn update(&self, _: &mut HandledNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for HandledNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.0.borrow_mut() = Some(context.handle());
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, unchangedNodesAreDetachedAndReattachedWhenReused
#[test]
fn an_unchanged_node_is_detached_while_deactivated() {
    let active = mutable_state_of(true);
    let handle = Rc::new(RefCell::new(None));
    let (is_active, element) = (active.handle(), handle.clone());
    let mut host = host(move || {
        let element = element.clone();
        ReusableContentHost(is_active.get(), move || {
            let modifier = Modifier.then_custom(Handled(element.clone()));
            Layout(modifier, measure_policy(|scope, _, _| scope.layout(1, 1)), callback(|| {}));
        });
    });
    let attached = || handle.borrow().expect("a handle").is_attached();
    assert!(attached(), "attached when first created");
    active.set(false);
    host.frame();
    assert!(!attached(), "detached when its layout node is deactivated");
    active.set(true);
    host.frame();
    assert!(attached(), "attached after being reactivated");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, reusingStatelessModifierNotCausingInvalidation
#[test]
fn reused_content_with_unchanged_elements_measures_nothing_again() {
    let (active, key, size) = (mutable_state_of(true), mutable_state_of(0), mutable_state_of(10));
    let invalidations = Rc::new(Cell::new(0));
    let content = stateless_content(active.handle(), key.handle(), size.handle(), &invalidations);
    let mut host = host(content);
    assert_eq!(invalidations.get(), 1);
    active.set(false);
    host.frame();
    active.set(true);
    key.set(1);
    host.frame();
    assert_eq!(invalidations.get(), 1);
}

/// A layout element running `block` as it measures, as the Compose test's `LayoutElement`.
struct Measuring(Rc<dyn Fn()>);

struct MeasuringNode(Rc<dyn Fn()>);

impl ModifierNodeElement for Measuring {
    type Node = MeasuringNode;
    fn create(&self) -> MeasuringNode {
        MeasuringNode(self.0.clone())
    }
    fn update(&self, node: &mut MeasuringNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for MeasuringNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for MeasuringNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        let placeable = scope.measure(content, constraints);
        (self.0)();
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, reusingModifierReadingStateInMeasureBlock
#[test]
fn a_state_read_while_deactivated_measures_the_node_once_reactivated() {
    let (active, counter) = (mutable_state_of(true), mutable_state_of(0));
    let invalidations = Rc::new(Cell::new(0));
    let (is_active, read, measured) = (active.handle(), counter.handle(), invalidations.clone());
    let block: Rc<dyn Fn()> = Rc::new(move || {
        read.get();
        measured.set(measured.get() + 1);
    });
    let policy = measure_policy(|scope, _, _| scope.layout(100, 100));
    let mut host = host(move || {
        let (block, policy) = (block.clone(), policy.clone());
        ReusableContentHost(is_active.get(), move || {
            let (block, policy) = (block.clone(), policy.clone());
            ReusableContent(0, move || {
                let modifier = Modifier.then_custom(Measuring(block.clone()));
                Layout(modifier, policy.clone(), callback(|| {}));
            });
        });
    });
    assert_eq!(invalidations.get(), 1);
    active.set(false);
    host.frame();
    assert_eq!(invalidations.get(), 1);
    counter.update(|value| value + 1);
    host.frame();
    assert_eq!(invalidations.get(), 1, "not measured while deactivated");
    active.set(true);
    host.frame();
    assert_eq!(invalidations.get(), 2, "measured once reactivated");
    counter.update(|value| value + 1);
    host.frame();
    assert_eq!(invalidations.get(), 3);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, reusingModifierReadingStateInDrawBlock
#[test]
fn a_state_read_while_drawing_deactivated_content_draws_it_once_reactivated() {
    let (active, counter) = (mutable_state_of(true), mutable_state_of(0));
    let draws = Rc::new(Cell::new(0));
    let (is_active, read, drawn) = (active.handle(), counter.handle(), draws.clone());
    let block: Rc<dyn Fn(&mut DrawScope<'_>)> = Rc::new(move |_: &mut DrawScope<'_>| {
        read.get();
        drawn.set(drawn.get() + 1);
    });
    let mut host = host(move || {
        let block = block.clone();
        ReusableContentHost(is_active.get(), move || {
            let block = block.clone();
            ReusableContent(0, move || {
                let block = block.clone();
                let modifier = Modifier.draw_behind(move |scope| block(scope));
                let policy = measure_policy(|scope, _, _| scope.layout(100, 100));
                Layout(modifier, policy, callback(|| {}));
            });
        });
    });
    assert_eq!(draws.get(), 1);
    active.set(false);
    host.frame();
    counter.update(|value| value + 1);
    host.frame();
    assert_eq!(draws.get(), 1, "not drawn while deactivated");
    active.set(true);
    host.frame();
    assert_eq!(draws.get(), 2);
    counter.update(|value| value + 1);
    host.frame();
    assert_eq!(draws.get(), 3);
}

/// A layout element of no state that measures its content as it is, equal to itself only, as
/// the Compose test's `StatelessLayoutElement1` and `2`.
#[derive(Clone, Copy, PartialEq)]
struct PassThrough(u8);

impl ModifierNodeElement for PassThrough {
    type Node = PassThrough;
    fn create(&self) -> PassThrough {
        *self
    }
    fn update(&self, _: &mut PassThrough) {}
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl ModifierNode for PassThrough {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
}

impl LayoutModifierNode for PassThrough {
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

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, placingChildWithReusedUnchangedModifier
#[test]
fn a_reactivated_child_with_a_shorter_chain_is_placed_where_its_parent_says() {
    let (active, two, child_x) =
        (mutable_state_of(true), mutable_state_of(true), mutable_state_of(0));
    let (is_active, both, x) = (active.handle(), two.handle(), child_x.handle());
    let mut host = host(move || {
        ReusableContentHost(is_active.get(), move || {
            ReusableContent(0, move || {
                let parent = measure_policy(move |scope, measurables, constraints| {
                    let placeable = scope.measure(measurables[0], constraints);
                    scope.place(&placeable, x.get(), 0);
                    scope.layout(placeable.width, placeable.height)
                });
                let content = callback(move || {
                    let mut modifier = Modifier.then_custom(PassThrough(1));
                    if both.get() {
                        modifier = modifier.then_custom(PassThrough(2));
                    }
                    let child = measure_policy(|scope, _, _| scope.layout(10, 10));
                    Layout(modifier.test_tag("child"), child, callback(|| {}));
                });
                Layout(Modifier, parent, content);
            });
        });
    });
    active.set(false);
    host.frame();
    active.set(true);
    two.set(false);
    child_x.set(10);
    host.frame();
    let child = host.with_tree(|tree| {
        let parent = tree.children_of(tree.root_id()).next().expect("the parent");
        let child = tree.children_of(parent).next().expect("the child");
        tree.bounds_in_root(child)
    });
    assert_eq!(child.left, 10.0);
}

/// A node logging its density when attached and when it changes.
struct DensityLogger(Rc<RefCell<Vec<f32>>>);

struct DensityLoggerNode(Rc<RefCell<Vec<f32>>>);

impl ModifierNodeElement for DensityLogger {
    type Node = DensityLoggerNode;
    fn create(&self) -> DensityLoggerNode {
        DensityLoggerNode(self.0.clone())
    }
    fn update(&self, _: &mut DensityLoggerNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DensityLoggerNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(context.require_density().density);
    }
    fn on_density_change(&mut self, context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(context.require_density().density);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeOnDensityChangeTest.kt, densityChange_triggersNodeCallback
#[test]
fn a_density_change_is_told_to_attached_nodes() {
    let densities = Rc::new(RefCell::new(Vec::new()));
    let logged = densities.clone();
    let mut host = host(move || Leaf(Modifier.then_custom(DensityLogger(logged.clone()))));
    assert_eq!(*densities.borrow(), [1.0]);
    // compose-rs's density is the host's, for the whole tree (ledger divergence 5.2).
    host.set_density(Density::new(3.0));
    host.frame();
    assert_eq!(*densities.borrow(), [1.0, 3.0]);
}

/// A node logging its layout direction when it changes.
struct DirectionLogger(Rc<RefCell<Vec<LayoutDirection>>>);

struct DirectionLoggerNode(Rc<RefCell<Vec<LayoutDirection>>>);

impl ModifierNodeElement for DirectionLogger {
    type Node = DirectionLoggerNode;
    fn create(&self) -> DirectionLoggerNode {
        DirectionLoggerNode(self.0.clone())
    }
    fn update(&self, _: &mut DirectionLoggerNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for DirectionLoggerNode {
    fn on_layout_direction_change(&mut self, context: &mut NodeContext<'_>) {
        self.0.borrow_mut().push(context.require_layout_direction());
    }
}

/// A layout direction change is told to the nodes of the layout nodes composed in it, as
/// Compose's `onLayoutDirectionChange`.
#[test]
fn a_layout_direction_change_is_told_to_attached_nodes() {
    let directions = Rc::new(RefCell::new(Vec::new()));
    let logged = directions.clone();
    let mut host = host(move || Leaf(Modifier.then_custom(DirectionLogger(logged.clone()))));
    assert!(directions.borrow().is_empty());
    host.set_layout_direction(LayoutDirection::Rtl);
    host.frame();
    assert_eq!(*directions.borrow(), [LayoutDirection::Rtl]);
}

/// A node launching a task in its coroutine scope when attached, the task counting frames.
struct Ticking(Rc<Cell<u32>>);

struct TickingNode(Rc<Cell<u32>>);

impl ModifierNodeElement for Ticking {
    type Node = TickingNode;
    fn create(&self) -> TickingNode {
        TickingNode(self.0.clone())
    }
    fn update(&self, _: &mut TickingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for TickingNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let ticks = self.0.clone();
        context.coroutine_scope().launch(async move {
            loop {
                compose_runtime::with_frame_nanos(|_| ()).await;
                ticks.set(ticks.get() + 1);
            }
        });
    }
}

/// A node's coroutine scope is cancelled when it's detached, as Compose's (`Modifier.kt`,
/// `markAsDetached`): its tasks stop.
#[test]
fn a_detached_node_s_tasks_are_cancelled() {
    let ticks = Rc::new(Cell::new(0));
    let shown = mutable_state_of(true);
    let (is_shown, counted) = (shown.handle(), ticks.clone());
    let mut host = host(move || {
        if is_shown.get() {
            Leaf(Modifier.then_custom(Ticking(counted.clone())));
        }
    });
    host.frame();
    host.frame();
    assert!(ticks.get() > 0, "the task runs while attached");
    shown.set(false);
    host.frame();
    let stopped = ticks.get();
    host.frame();
    host.frame();
    assert_eq!(ticks.get(), stopped, "cancelled once detached");
}

/// A node drawing the colour a shared cell holds, keeping a handle to itself.
struct Painted(Rc<Cell<Color>>, Rc<RefCell<Option<NodeHandle>>>);

struct PaintedNode(Rc<Cell<Color>>, Rc<RefCell<Option<NodeHandle>>>);

impl ModifierNodeElement for Painted {
    type Node = PaintedNode;
    fn create(&self) -> PaintedNode {
        PaintedNode(self.0.clone(), self.1.clone())
    }
    fn update(&self, _: &mut PaintedNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for PaintedNode {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.1.borrow_mut() = Some(context.handle());
    }
}

impl DrawModifierNode for PaintedNode {
    fn draw(&mut self, scope: &mut DrawScope<'_>) {
        let bounds = scope.bounds();
        scope.draw_rect(bounds, Paint::fill(self.0.get()));
    }
}

/// `NodeHandle::invalidate_draw`, asked outside the tree's passes, draws the node again in the
/// next frame without measuring it; `with_node` reaches the node from a task.
#[test]
fn a_handle_invalidates_its_node_and_reaches_it_from_a_task() {
    let color = Rc::new(Cell::new(Color::RED));
    let handle = Rc::new(RefCell::new(None));
    let measures = Rc::new(Cell::new(0));
    let (paint, held, policy) = (color.clone(), handle.clone(), counted(&measures));
    let mut host = host(move || {
        let modifier = Modifier.size(10.dp()).then_custom(Painted(paint.clone(), held.clone()));
        Layout(modifier, policy.clone(), callback(|| {}));
    });
    let node = handle.borrow().expect("a handle");
    assert!(host.display_list().dump().contains("#FFFF0000"));
    color.set(Color::BLUE);
    node.invalidate_draw();
    assert!(host.needs_frame());
    host.frame();
    assert!(host.display_list().dump().contains("#FF0000FF"), "{}", host.display_list().dump());
    assert_eq!(measures.get(), 1, "drawn again, not measured");
    // From a task: the tree is free between frames.
    let reached = Rc::new(Cell::new(false));
    let flag = reached.clone();
    let task = compose_runtime::CoroutineScope::new();
    task.launch(async move {
        let attached = node.with_node(|_: &mut PaintedNode, context| context.is_attached());
        flag.set(attached == Some(true));
    });
    host.frame();
    assert!(reached.get());
}

/// A node's side effect runs once the pass that attached it is over.
#[test]
fn a_side_effect_runs_after_the_pass() {
    struct Effect(Log);
    struct EffectNode(Log);
    impl ModifierNodeElement for Effect {
        type Node = EffectNode;
        fn create(&self) -> EffectNode {
            EffectNode(self.0.clone())
        }
        fn update(&self, _: &mut EffectNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for EffectNode {
        fn on_attach(&mut self, context: &mut NodeContext<'_>) {
            self.0.borrow_mut().push("attach".into());
            let log = self.0.clone();
            context.side_effect(move || log.borrow_mut().push("effect".into()));
        }
    }
    let log = Log::default();
    let logged = log.clone();
    let _host = host(move || Leaf(Modifier.then_custom(Effect(logged.clone()))));
    assert_eq!(take(&log), ["attach", "effect"]);
}

/// A node detached for reuse keeps what it holds (`on_release` isn't called): only a node
/// leaving for good gives it back, after its `on_detach` (D1153).
#[test]
fn only_a_node_dropped_for_good_is_released() {
    struct Holding(Log);
    struct HoldingNode(Log);
    impl ModifierNodeElement for Holding {
        type Node = HoldingNode;
        fn create(&self) -> HoldingNode {
            HoldingNode(self.0.clone())
        }
        fn update(&self, _: &mut HoldingNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for HoldingNode {
        fn on_detach(&mut self, _: &mut NodeContext<'_>) {
            self.0.borrow_mut().push("detach".into());
        }
        fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
            self.0.borrow_mut().push("release".into());
        }
    }
    let log = Log::default();
    let (key, shown) = (mutable_state_of(0), mutable_state_of(true));
    let (reuse, is_shown, logged) = (key.handle(), shown.handle(), log.clone());
    let mut host = host(move || {
        if is_shown.get() {
            let logged = logged.clone();
            ReusableContent(reuse.get(), move || {
                Leaf(Modifier.then_custom(Holding(logged.clone())))
            });
        }
    });
    key.set(1);
    host.frame();
    assert_eq!(take(&log), ["detach"], "reused: detached, kept");
    shown.set(false);
    host.frame();
    assert_eq!(take(&log), ["detach", "release"]);
}

/// As `reusingModifierReadingStateInMeasureBlock`, the reading node inside its reused content
/// rather than its top: the measurement it asked for while deactivated is scheduled from where
/// it is once reactivated.
#[test]
fn a_nested_node_s_state_read_while_deactivated_measures_it_once_reactivated() {
    let (active, counter) = (mutable_state_of(true), mutable_state_of(0));
    let invalidations = Rc::new(Cell::new(0));
    let (is_active, read, measured) = (active.handle(), counter.handle(), invalidations.clone());
    let block: Rc<dyn Fn()> = Rc::new(move || {
        read.get();
        measured.set(measured.get() + 1);
    });
    let policy = measure_policy(|scope, _, _| scope.layout(100, 100));
    let mut host = host(move || {
        let (block, policy) = (block.clone(), policy.clone());
        ReusableContentHost(is_active.get(), move || {
            let (block, policy) = (block.clone(), policy.clone());
            ReusableContent(0, move || {
                let (block, policy) = (block.clone(), policy.clone());
                Stack(
                    Modifier,
                    callback(move || {
                        let modifier = Modifier.then_custom(Measuring(block.clone()));
                        Layout(modifier, policy.clone(), callback(|| {}));
                    }),
                );
            });
        });
    });
    assert_eq!(invalidations.get(), 1);
    active.set(false);
    host.frame();
    counter.update(|value| value + 1);
    host.frame();
    assert_eq!(invalidations.get(), 1, "not measured while deactivated");
    active.set(true);
    host.frame();
    assert_eq!(invalidations.get(), 2, "measured once reactivated");
}

// ---- Review fixes (D1156–D1160) ------------------------------------------------------------

/// A node launching a task in its coroutine scope when reset, as an app node may.
struct ResetLauncher(Log);

struct ResetLauncherNode(Log);

impl ModifierNodeElement for ResetLauncher {
    type Node = ResetLauncherNode;
    fn create(&self) -> ResetLauncherNode {
        ResetLauncherNode(self.0.clone())
    }
    fn update(&self, _: &mut ResetLauncherNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ResetLauncherNode {
    fn on_reset(&mut self, context: &mut NodeContext<'_>) {
        assert!(context.is_attached(), "a node is reset while attached, as Compose's");
        let log = self.0.clone();
        context.coroutine_scope().launch(async move { log.borrow_mut().push("launched".into()) });
        self.0.borrow_mut().push("reset".into());
    }
}

/// A node reused from the kept pool isn't reset again (it was, when deactivated, attached):
/// one launching in its reset works, as in Compose, where a detached node is never reset.
#[test]
fn a_node_reused_from_the_kept_pool_is_reset_only_while_attached() {
    let log = Log::default();
    let active = mutable_state_of(true);
    let (is_active, logged) = (active.handle(), log.clone());
    let mut host = host(move || {
        let logged = logged.clone();
        ReusableContentHost(is_active.get(), move || {
            Leaf(Modifier.then_custom(ResetLauncher(logged.clone())));
        });
    });
    active.set(false);
    host.frame();
    assert_eq!(take(&log), ["reset"], "reset as it's deactivated");
    active.set(true);
    host.frame();
    host.frame();
    assert_eq!(take(&log), Vec::<String>::new(), "not reset again once reused");
}

/// A node keeping a handle, reaching itself from its own side effect.
struct SideEffectReach(Rc<Cell<Option<bool>>>);

struct SideEffectReachNode(Rc<Cell<Option<bool>>>);

impl ModifierNodeElement for SideEffectReach {
    type Node = SideEffectReachNode;
    fn create(&self) -> SideEffectReachNode {
        SideEffectReachNode(self.0.clone())
    }
    fn update(&self, _: &mut SideEffectReachNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for SideEffectReachNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let handle = context.handle();
        let reached = self.0.clone();
        context.side_effect(move || {
            let attached = handle.with_node(|_: &mut SideEffectReachNode, cx| cx.is_attached());
            reached.set(attached);
        });
    }
}

/// A side effect runs once the tree is free, after the host's pass: `with_node` reaches the
/// node from it, as Compose's `sideEffect` runs outside the applier.
#[test]
fn a_side_effect_runs_with_the_tree_free() {
    let reached = Rc::new(Cell::new(None));
    let seen = reached.clone();
    let _host = host(move || Leaf(Modifier.then_custom(SideEffectReach(seen.clone()))));
    assert_eq!(reached.get(), Some(true));
}

/// A node trying another's handle from its own callback, where the tree is busy.
struct Busy(Rc<RefCell<Option<NodeHandle>>>, Rc<Cell<Option<bool>>>);

struct BusyNode(Rc<RefCell<Option<NodeHandle>>>, Rc<Cell<Option<bool>>>);

impl ModifierNodeElement for Busy {
    type Node = BusyNode;
    fn create(&self) -> BusyNode {
        BusyNode(self.0.clone(), self.1.clone())
    }
    fn update(&self, _: &mut BusyNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for BusyNode {
    fn on_attach(&mut self, _: &mut NodeContext<'_>) {
        let other = self.0.borrow().expect("the first node's handle");
        let answer = other.with_node(|_: &mut HandledNode, _| ());
        self.1.set(Some(answer.is_some()));
    }
}

/// `with_node` from inside a pass (another node's callback) answers `None`: the tree is busy.
#[test]
fn with_node_answers_none_while_the_tree_is_busy() {
    let handle = Rc::new(RefCell::new(None));
    let answer = Rc::new(Cell::new(None));
    let second = mutable_state_of(false);
    let (shown, held, answered) = (second.handle(), handle.clone(), answer.clone());
    let mut host = host(move || {
        Leaf(Modifier.then_custom(Handled(held.clone())));
        if shown.get() {
            Leaf(Modifier.then_custom(Busy(held.clone(), answered.clone())));
        }
    });
    second.set(true);
    host.frame();
    assert_eq!(answer.get(), Some(false));
}

/// A node invalidating its semantics through its handle: the frame says the semantics may
/// have changed, so accessibility reads them again (D1158).
#[test]
fn a_semantics_invalidation_is_in_the_frame_s_stats() {
    let handle = Rc::new(RefCell::new(None));
    let held = handle.clone();
    let mut host = host(move || Leaf(Modifier.then_custom(Handled(held.clone()))));
    assert!(!host.frame().semantics_changed, "a quiet frame changes no semantics");
    handle.borrow().expect("a handle").invalidate_semantics();
    let stats = host.frame();
    assert!(stats.semantics_changed && !stats.recomposed && !stats.measured);
    assert!(!host.frame().semantics_changed);
}

/// A semantics element updated without a layout: the frame says the semantics changed.
#[test]
fn an_updated_semantics_element_is_in_the_frame_s_stats() {
    let tag = mutable_state_of("a");
    let shown = tag.handle();
    let mut host = host(move || Leaf(Modifier.size(10.dp()).test_tag(shown.get())));
    tag.set("b");
    let stats = host.frame();
    assert!(stats.semantics_changed && !stats.measured);
}

/// A clickable reused with another interaction source releases its hover on the old source,
/// at the swap, and leaves the new one alone (another node's hover holds it).
#[test]
fn a_clickable_swapping_its_source_on_reuse_releases_the_old_one() {
    let (old, new) = (MutableInteractionSource::new(), MutableInteractionSource::new());
    let key = mutable_state_of(0);
    let (reuse, first, second) = (key.handle(), old.clone(), new.clone());
    let mut host = host(move || {
        let (first, second) = (first.clone(), second.clone());
        Stack(
            Modifier.hoverable(second.clone()).size(20.dp()),
            callback(move || {
                let source = if reuse.get() == 0 { first.clone() } else { second.clone() };
                ReusableContent(reuse.get(), move || {
                    let options = ClickableOptions {
                        interaction_source: Some(source.clone()),
                        ..ClickableOptions::DEFAULT
                    };
                    Leaf(Modifier.clickable_with(options, || {}).size(10.dp()));
                });
            }),
        );
    });
    host.send_pointer(PointerEvent::moved(Offset::new(5.0, 5.0), false));
    host.frame();
    assert!(old.is_hovered() && new.is_hovered());
    key.set(1);
    host.frame();
    host.frame();
    assert!(!old.is_hovered(), "the old source's hover is over");
    assert!(new.is_hovered(), "the new source keeps the hover the stack gave it");
}

/// A pointer node added under a resting mouse, with no layout, hears its `Enter`: the hover is
/// refreshed after a chain change of pointer nodes, as it was after the layout every modifier
/// change used to cause.
#[test]
fn a_pointer_node_added_under_a_resting_mouse_is_hovered() {
    let source = MutableInteractionSource::new();
    let hoverable = mutable_state_of(false);
    let (with, shown) = (hoverable.handle(), source.clone());
    let mut host = host(move || {
        let modifier = Modifier.size(10.dp());
        let modifier = if with.get() { modifier.hoverable(shown.clone()) } else { modifier };
        Leaf(modifier);
    });
    host.send_pointer(PointerEvent::moved(Offset::new(5.0, 5.0), false));
    host.frame();
    hoverable.set(true);
    let stats = host.frame();
    assert!(!stats.measured, "nothing laid out");
    host.frame();
    assert!(source.is_hovered(), "the new node heard the mouse's Enter");
}

/// A chain change removing an attached node, then the layout node's release, in one pass: the
/// removed node is detached and released while its layout node lives (its context reaches it).
#[test]
fn a_node_removed_from_a_chain_released_in_the_same_pass_is_detached_first() {
    use compose_runtime::platform::Applier;
    struct Reaching(Log);
    struct ReachingNode(Log);
    impl ModifierNodeElement for Reaching {
        type Node = ReachingNode;
        fn create(&self) -> ReachingNode {
            ReachingNode(self.0.clone())
        }
        fn update(&self, _: &mut ReachingNode) {}
        fn same(&self, _: &Self) -> bool {
            true
        }
    }
    impl ModifierNode for ReachingNode {
        fn on_detach(&mut self, context: &mut NodeContext<'_>) {
            let _ = context.require_layout_direction();
            self.0.borrow_mut().push("detach".into());
        }
        fn on_release(&mut self, _: &mut ReleaseContext<'_>) {
            self.0.borrow_mut().push("release".into());
        }
    }
    let log = Log::default();
    let mut tree = UiTree::new(Density::new(1.0), Box::new(FixedTextMeasurer::new()));
    tree.on_begin_changes();
    let root = Applier::root(&tree);
    let node = tree.insert(root, 0, LayoutNode::new(empty_measure_policy()));
    let id = tree.id_of(node);
    tree.set_modifier(id, Modifier.then_custom(Reaching(log.clone())));
    tree.on_end_changes();
    tree.on_begin_changes();
    tree.set_modifier(id, Modifier);
    tree.remove(root, 0, 1);
    tree.release(node);
    tree.on_end_changes();
    assert_eq!(take(&log), ["detach", "release"]);
}
