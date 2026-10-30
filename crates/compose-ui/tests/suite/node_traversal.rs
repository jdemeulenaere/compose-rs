//! Node traversal: `TraversableNode` and its functions, and the internal visitors under them
//! (docs/parity/design/modifier-nodes.md §3.8, step MN-6).

use crate::support;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::ControlFlow;
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::platform::*;
use compose_ui::*;
use support::*;

use TraverseDescendantsAction::{
    CancelTraversal, ContinueTraversal, SkipSubtreeAndContinueTraversal,
};

thread_local! {
    /// The handles of the nodes attached, by label, as the tests' `val`s.
    static HANDLES: RefCell<HashMap<&'static str, NodeHandle>> = RefCell::new(HashMap::new());
}

fn remember_handle(label: &'static str, context: &mut NodeContext<'_>) {
    let handle = context.handle();
    HANDLES.with(|handles| handles.borrow_mut().insert(label, handle));
}

fn handle(label: &str) -> NodeHandle {
    HANDLES.with(|handles| *handles.borrow().get(label).expect("the node attached"))
}

/// A headless host showing `content`, its frame run.
fn show(content: impl Fn() + 'static) -> UiHost {
    HANDLES.with(|handles| handles.borrow_mut().clear());
    host(content)
}

// ---- The visitors: Compose's internal functions ------------------------------------------------

/// As the tests' `object : Modifier.Node() {}`: a node with a label, which can draw, be a focus
/// target or delegate.
struct Tracked {
    label: &'static str,
    draw: bool,
    focus: bool,
    delegates: Delegates,
}

impl ModifierNode for Tracked {
    fn as_draw(&mut self) -> Option<&mut dyn DrawModifierNode> {
        if self.draw { Some(self) } else { None }
    }
    fn as_focus_target(&mut self) -> Option<&mut dyn FocusTargetModifierNode> {
        if self.focus { Some(self) } else { None }
    }
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        remember_handle(self.label, context);
    }
}

impl DrawModifierNode for Tracked {}
impl FocusTargetModifierNode for Tracked {}

/// What makes a `Tracked`.
#[derive(Clone, Copy, PartialEq)]
struct Element {
    label: &'static str,
    draw: bool,
    focus: bool,
    delegate: Option<&'static str>,
    delegate_draws: bool,
}

const PLAIN: Element =
    Element { label: "", draw: false, focus: false, delegate: None, delegate_draws: false };

impl ModifierNodeElement for Element {
    type Node = Tracked;
    fn create(&self) -> Tracked {
        let mut delegates = Delegates::new();
        if let Some(label) = self.delegate {
            delegates.delegate(Tracked {
                label,
                draw: self.delegate_draws,
                focus: false,
                delegates: Delegates::new(),
            });
        }
        Tracked { label: self.label, draw: self.draw, focus: self.focus, delegates }
    }
    fn update(&self, _: &mut Tracked) {}
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

/// `Modifier.elementOf(a).elementOf(b)…`.
fn nodes(labels: &[&'static str]) -> Modifier {
    let mut modifier = Modifier;
    for &label in labels {
        modifier = modifier.then_custom(Element { label, ..PLAIN });
    }
    modifier
}

fn draw_node(label: &'static str) -> Modifier {
    Modifier.then_custom(Element { label, draw: true, ..PLAIN })
}

/// As the test's `DelegatedWrapper { DrawMod() }`.
fn wrapped_draw_node(label: &'static str, wrapped: &'static str) -> Modifier {
    Modifier.then_custom(Element { label, delegate: Some(wrapped), delegate_draws: true, ..PLAIN })
}

fn focus_nodes(labels: &[&'static str]) -> Modifier {
    let mut modifier = Modifier;
    for &label in labels {
        modifier = modifier.then_custom(Element { label, focus: true, ..PLAIN });
    }
    modifier
}

/// Runs `f` with the context of the `Tracked` node `label`.
fn from<R>(label: &str, f: impl FnOnce(&mut NodeContext<'_>) -> R) -> R {
    handle(label)
        .with_node(|_: &mut Tracked, context| f(context))
        .expect("the tree is free between frames")
}

/// The label of a visited node. Others (those a test doesn't make: the root's) have none, as
/// Compose's `trimRootModifierNodes`.
fn label_of(node: &mut dyn ModifierNode) -> Option<&'static str> {
    node.downcast_ref::<Tracked>().map(|node| node.label)
}

fn visit_ancestors(label: &str, kinds: NodeKinds, include_delegates: bool) -> Vec<&'static str> {
    let mut visited = Vec::new();
    from(label, |context| {
        context.visit_ancestors(kinds, include_delegates, &mut |node, _| {
            visited.extend(label_of(node));
            ControlFlow::Continue(())
        });
    });
    visited
}

fn nearest_ancestor(label: &str, kinds: NodeKinds) -> Option<&'static str> {
    let mut found = None;
    from(label, |context| {
        context.visit_ancestors(kinds, false, &mut |node, _| match label_of(node) {
            Some(label) => {
                found = Some(label);
                ControlFlow::Break(())
            }
            None => ControlFlow::Continue(()),
        });
    });
    found
}

fn visit_children(label: &str, kinds: NodeKinds, z_order: bool) -> Vec<&'static str> {
    let mut visited = Vec::new();
    from(label, |context| {
        context.visit_children(kinds, z_order, &mut |node, _| {
            visited.extend(label_of(node));
            ControlFlow::Continue(())
        });
    });
    visited
}

/// As `visitSelfAndChildren`: itself (out of its chain in its own code), then its children.
fn visit_self_and_children(label: &'static str) -> Vec<&'static str> {
    let mut visited = vec![label];
    visited.extend(visit_children(label, NodeKinds::ANY, false));
    visited
}

fn visit_local_ancestors(label: &str) -> Vec<&'static str> {
    let mut visited = Vec::new();
    from(label, |context| {
        context.visit_local_ancestors(NodeKinds::ANY, &mut |node, _| {
            visited.extend(label_of(node));
            ControlFlow::Continue(())
        });
    });
    visited
}

fn visit_local_descendants(label: &str) -> Vec<&'static str> {
    let mut visited = Vec::new();
    from(label, |context| {
        context.visit_local_descendants(NodeKinds::ANY, &mut |node, _| {
            visited.extend(label_of(node));
            ControlFlow::Continue(())
        });
    });
    visited
}

/// As `node.child`: the next node in its chain.
fn child(label: &str) -> Option<&'static str> {
    visit_local_descendants(label).first().copied()
}

/// As `visitSubtree`, or `visitSubtreeIf` with `dive`.
fn visit_subtree(
    label: &str,
    kinds: NodeKinds,
    z_order: bool,
    dive: impl Fn(&str) -> bool,
) -> Vec<&'static str> {
    let mut visited = Vec::new();
    from(label, |context| {
        context.visit_subtree_if(kinds, z_order, &mut |node, _| {
            let Some(label) = label_of(node) else { return ContinueTraversal };
            visited.push(label);
            if dive(label) { ContinueTraversal } else { SkipSubtreeAndContinueTraversal }
        });
    });
    visited
}

const NONE: &[&str] = &[];

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAncestorsTest.kt, noAncestors
#[test]
fn a_lone_node_has_no_ancestors() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_ancestors("node", NodeKinds::ANY, false), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAncestorsTest.kt, noMatchingAncestors
#[test]
fn no_ancestor_of_a_kind_nothing_has() {
    let _host = show(|| Leaf(nodes(&["node"])));
    let mut count = 0;
    from("node", |context| {
        context.visit_ancestors(NodeKinds::GLOBAL_POSITION_AWARE, false, &mut |_, _| {
            count += 1;
            ControlFlow::Continue(())
        });
    });
    assert_eq!(count, 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAncestorsTest.kt, returnsLocalAncestors
#[test]
fn ancestors_are_first_those_before_it_in_its_chain() {
    let _host = show(|| Leaf(nodes(&["local_2", "local_1", "node"])));
    assert_eq!(visit_ancestors("node", NodeKinds::ANY, false), ["local_1", "local_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAncestorsTest.kt, returnsAncestors
#[test]
fn ancestors_go_on_through_the_ancestors_chains_and_no_sibling_s() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["ancestor_2", "ancestor_1"]), {
            Stack!({
                Leaf(nodes(&["local_2", "local_1", "node"]));
            });
        });
        Leaf(nodes(&["other"]));
    });
    assert_eq!(
        visit_ancestors("node", NodeKinds::ANY, false),
        ["local_1", "local_2", "ancestor_1", "ancestor_2"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeAncestorsTest.kt, findAncestorsOfType
// Compose's root has a focus target of its own, which its test drops; compose-rs's hasn't.
#[test]
fn ancestors_of_a_kind_are_only_those() {
    let _host = show(|| {
        Stack!(modifier = focus_nodes(&["ancestor_4", "ancestor_3"]), {
            Stack!({
                Leaf(focus_nodes(&["other_1"]));
                Stack!(modifier = focus_nodes(&["ancestor_2", "ancestor_1"]), {
                    Leaf(focus_nodes(&["node", "other_3"]));
                });
                Leaf(focus_nodes(&["other_2"]));
            });
        });
    });
    assert_eq!(
        visit_ancestors("node", NodeKinds::FOCUS_TARGET, false),
        ["ancestor_1", "ancestor_2", "ancestor_3", "ancestor_4"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeChildTest.kt, noChildren
#[test]
fn the_last_node_of_a_chain_has_no_child() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(child("node"), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeChildTest.kt, localChild
#[test]
fn a_node_s_child_is_the_next_in_its_chain() {
    let _host = show(|| Leaf(nodes(&["node", "child_1", "child_2"])));
    assert_eq!(child("node"), Some("child_1"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeChildTest.kt, nonContiguousChild
#[test]
fn a_node_s_child_passes_over_modifiers_without_nodes() {
    let _host = show(|| Leaf(nodes(&["node"]).then(Modifier).then(nodes(&["child"]))));
    assert_eq!(child("node"), Some("child"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeChildTest.kt, doesNotReturnChildInDifferentLayoutNode
#[test]
fn a_node_s_child_is_never_in_another_layout_node() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child_1", "child_2"]));
        });
    });
    assert_eq!(child("node"), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeNearestAncestorTest.kt, noAncestors
#[test]
fn nearest_ancestor_of_a_kind_nothing_has_is_none() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(nearest_ancestor("node", NodeKinds::GLOBAL_POSITION_AWARE), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeNearestAncestorTest.kt, nearestAncestorInDifferentLayoutNode_nonContiguousParentLayoutNode
#[test]
fn nearest_ancestor_passes_over_a_layout_node_without_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["ancestor"]), {
            Stack!({
                Leaf(nodes(&["node"]));
            });
        });
    });
    assert_eq!(nearest_ancestor("node", NodeKinds::ANY), Some("ancestor"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeNearestAncestorTest.kt, nearestAncestorWithinCurrentLayoutNode_immediateParent
#[test]
fn nearest_ancestor_in_the_chain() {
    let _host = show(|| Leaf(nodes(&["ancestor", "node"])));
    assert_eq!(nearest_ancestor("node", NodeKinds::ANY), Some("ancestor"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeNearestAncestorTest.kt, nearestAncestorWithinCurrentLayoutNode_nonContiguousAncestor
#[test]
fn nearest_ancestor_in_the_chain_passes_over_modifiers_without_nodes() {
    let _host = show(|| Leaf(nodes(&["ancestor"]).then(Modifier).then(nodes(&["node"]))));
    assert_eq!(nearest_ancestor("node", NodeKinds::ANY), Some("ancestor"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeNearestAncestorTest.kt, nearestAncestorInDifferentLayoutNode_immediateParentLayoutNode
#[test]
fn nearest_ancestor_in_the_parent() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["ancestor"]), {
            Leaf(nodes(&["node"]));
        });
    });
    assert_eq!(nearest_ancestor("node", NodeKinds::ANY), Some("ancestor"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, noAncestors
#[test]
fn visiting_the_ancestors_of_a_lone_node_visits_nothing() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_ancestors("node", NodeKinds::ANY, false), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, localAncestors
#[test]
fn visiting_ancestors_visits_those_in_the_chain() {
    let _host = show(|| Leaf(nodes(&["parent_2", "parent_1", "node"])));
    assert_eq!(visit_ancestors("node", NodeKinds::ANY, false), ["parent_1", "parent_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, nonContiguousLocalAncestors
#[test]
fn visiting_ancestors_passes_over_modifiers_without_nodes() {
    let _host = show(|| {
        Leaf(
            nodes(&["parent_2"])
                .then(Modifier)
                .then(nodes(&["parent_1"]))
                .then(Modifier)
                .then(nodes(&["node"])),
        )
    });
    assert_eq!(visit_ancestors("node", NodeKinds::ANY, false), ["parent_1", "parent_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, ancestorsInOtherLayoutNodes
#[test]
fn visiting_ancestors_goes_through_other_layout_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["ancestor_3"]), {
            Stack!(modifier = nodes(&["ancestor_2", "ancestor_1"]), {
                Stack!({
                    Leaf(nodes(&["local", "node"]));
                });
            });
        });
    });
    assert_eq!(
        visit_ancestors("node", NodeKinds::ANY, false),
        ["local", "ancestor_1", "ancestor_2", "ancestor_3"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, shallowVisitAncestorsWithDelegate
#[test]
fn visiting_ancestors_with_delegates_visits_a_delegate_after_its_owner() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["ancestor_3"]), {
            let first = Element { label: "ancestor_1", delegate: Some("delegated"), ..PLAIN };
            Stack!(modifier = nodes(&["ancestor_2"]).then_custom(first), {
                Stack!({
                    Leaf(nodes(&["child"]));
                });
            });
        });
    });
    assert_eq!(
        visit_ancestors("child", NodeKinds::ANY, true),
        ["ancestor_1", "delegated", "ancestor_2", "ancestor_3"]
    );
    // Without them, the chain's nodes alone: a node of the kind stops the dispatch.
    assert_eq!(
        visit_ancestors("child", NodeKinds::ANY, false),
        ["ancestor_1", "ancestor_2", "ancestor_3"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, shallowVisitAncestorsWithoutDelegate
#[test]
fn visiting_ancestors_with_delegates_with_none_visits_the_chain_s_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["ancestor_3"]), {
            Stack!(modifier = nodes(&["ancestor_2", "ancestor_1"]), {
                Stack!({
                    Leaf(nodes(&["child"]));
                });
            });
        });
    });
    assert_eq!(
        visit_ancestors("child", NodeKinds::ANY, true),
        ["ancestor_1", "ancestor_2", "ancestor_3"]
    );
}

/// As the test's `DetachableNode`: visits its ancestors in its `on_detach`.
struct Detachable(Rc<RefCell<Vec<&'static str>>>);
struct DetachableNode(Rc<RefCell<Vec<&'static str>>>);

impl ModifierNodeElement for Detachable {
    type Node = DetachableNode;
    fn create(&self) -> DetachableNode {
        DetachableNode(self.0.clone())
    }
    fn update(&self, _: &mut DetachableNode) {}
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ModifierNode for DetachableNode {
    fn on_detach(&mut self, context: &mut NodeContext<'_>) {
        let visited = self.0.clone();
        context.visit_ancestors(NodeKinds::ANY, false, &mut |node, _| {
            visited.borrow_mut().extend(label_of(node));
            ControlFlow::Continue(())
        });
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, localAncestorsAreAvailableDuringOnDetach
#[test]
fn the_ancestors_in_its_chain_are_there_in_a_node_s_on_detach() {
    let visited = Rc::new(RefCell::new(Vec::new()));
    let remove = mutable_state_of(false);
    let (v, r) = (visited.clone(), remove.handle());
    let mut host = show(move || {
        let modifier = nodes(&["parent_2", "parent_1"]);
        Leaf(if r.get() { modifier } else { modifier.then_custom(Detachable(v.clone())) });
    });
    remove.set(true);
    host.frame();
    assert_eq!(&*visited.borrow(), &["parent_1", "parent_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitAncestorsTest.kt, ancestorsAcrossMultipleLayoutNodesAreAvailableDuringOnDetach
#[test]
fn the_ancestors_in_other_layout_nodes_are_there_in_a_node_s_on_detach() {
    let visited = Rc::new(RefCell::new(Vec::new()));
    let remove = mutable_state_of(false);
    let (v, r) = (visited.clone(), remove.handle());
    let mut host = show(move || {
        let (v, r) = (v.clone(), r);
        Stack!(modifier = nodes(&["ancestor_4"]), {
            let (v, r) = (v.clone(), r);
            Stack!(modifier = nodes(&["ancestor_3", "ancestor_2"]), {
                let modifier = nodes(&["ancestor_1"]);
                Leaf(if r.get() { modifier } else { modifier.then_custom(Detachable(v.clone())) });
            });
        });
    });
    remove.set(true);
    host.frame();
    assert_eq!(&*visited.borrow(), &["ancestor_1", "ancestor_2", "ancestor_3", "ancestor_4"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutNode.kt, onChildRemoved
#[test]
fn what_s_above_a_removed_layout_node_is_there_in_its_nodes_on_detach() {
    let visited = Rc::new(RefCell::new(Vec::new()));
    let shown = mutable_state_of(true);
    let (v, s) = (visited.clone(), shown.handle());
    let mut host = show(move || {
        let (v, s) = (v.clone(), s);
        Stack!(modifier = nodes(&["above_2"]), {
            let (v, s) = (v.clone(), s);
            Stack!(modifier = nodes(&["above_1"]), {
                if s.get() {
                    let v = v.clone();
                    Stack!(modifier = nodes(&["removed"]), {
                        Leaf(Modifier.then_custom(Detachable(v.clone())));
                    });
                }
            });
        });
    });
    shown.set(false);
    host.frame();
    assert_eq!(&*visited.borrow(), &["removed", "above_1", "above_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitChildrenTest.kt, noChildren
#[test]
fn visiting_children_of_a_lone_node_visits_nothing() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_children("node", NodeKinds::ANY, false), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitChildrenTest.kt, localChildren
#[test]
fn visiting_children_stops_at_the_next_in_the_chain() {
    let _host = show(|| Leaf(nodes(&["node", "child_1", "child_2"])));
    assert_eq!(visit_children("node", NodeKinds::ANY, false), ["child_1"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitChildrenTest.kt, nonContiguousLocalChild
#[test]
fn visiting_children_passes_over_modifiers_without_nodes() {
    let _host = show(|| {
        Leaf(
            nodes(&["node"])
                .then(Modifier)
                .then(nodes(&["child_1"]))
                .then(Modifier)
                .then(nodes(&["child_2"])),
        )
    });
    assert_eq!(visit_children("node", NodeKinds::ANY, false), ["child_1"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitChildrenTest.kt, visitChildrenInOtherLayoutNodes
#[test]
fn visiting_children_visits_the_first_in_each_child_not_below() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Stack!(modifier = nodes(&["child_1"]), {
                Leaf(nodes(&["grandchild_1"]));
            });
            Stack!(modifier = nodes(&["child_2"]), {
                Leaf(nodes(&["grandchild_2"]));
            });
            Leaf(nodes(&["child_3"]));
        });
    });
    assert_eq!(visit_children("node", NodeKinds::ANY, false), ["child_1", "child_2", "child_3"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitChildrenTest.kt, visitChildrenInOtherLayoutNodesInDrawOrder_zIndex
#[test]
fn visiting_children_in_draw_order_goes_by_z_index() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child_1"]).z_index(10.0));
            Leaf(nodes(&["child_2"]).z_index(-10.0));
            Leaf(nodes(&["child_3"]));
        });
    });
    assert_eq!(visit_children("node", NodeKinds::ANY, true), ["child_2", "child_3", "child_1"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalAncestorsTest.kt, noParents
#[test]
fn local_ancestors_of_a_lone_node_are_none() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_local_ancestors("node"), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalAncestorsTest.kt, doesNotVisitOtherLayoutNodes
#[test]
fn local_ancestors_stay_in_the_chain() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["parent"]), {
            Leaf(nodes(&["node"]));
        });
    });
    assert_eq!(visit_local_ancestors("node"), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalAncestorsTest.kt, multipleAncestors
#[test]
fn local_ancestors_go_from_the_nearest() {
    let _host = show(|| Leaf(nodes(&["ancestor_2", "ancestor_1", "node"])));
    assert_eq!(visit_local_ancestors("node"), ["ancestor_1", "ancestor_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalAncestorsTest.kt, doesNotVisitChild
#[test]
fn local_ancestors_don_t_include_what_follows() {
    let _host = show(|| Leaf(nodes(&["parent", "node", "child"])));
    assert_eq!(visit_local_ancestors("node"), ["parent"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalAncestorsTest.kt, nonContiguousAncestors
#[test]
fn local_ancestors_pass_over_modifiers_without_nodes() {
    let _host = show(|| {
        Leaf(
            nodes(&["ancestor_2"])
                .then(Modifier)
                .then(nodes(&["ancestor_1"]))
                .then(Modifier)
                .then(nodes(&["node"])),
        )
    });
    assert_eq!(visit_local_ancestors("node"), ["ancestor_1", "ancestor_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalDescendantsTest.kt, noChildren
#[test]
fn local_descendants_of_a_lone_node_are_none() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_local_descendants("node"), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalDescendantsTest.kt, doesNotVisitOtherLayoutNodes
#[test]
fn local_descendants_stay_in_the_chain() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child"]));
        });
    });
    assert_eq!(visit_local_descendants("node"), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalDescendantsTest.kt, multipleDescendants
#[test]
fn local_descendants_go_in_order() {
    let _host = show(|| Leaf(nodes(&["node", "child_1", "child_2"])));
    assert_eq!(visit_local_descendants("node"), ["child_1", "child_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalDescendantsTest.kt, doesNotVisitParent
#[test]
fn local_descendants_don_t_include_what_precedes() {
    let _host = show(|| Leaf(nodes(&["parent", "node", "child"])));
    assert_eq!(visit_local_descendants("node"), ["child"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitLocalDescendantsTest.kt, nonContiguousChildren
#[test]
fn local_descendants_pass_over_modifiers_without_nodes() {
    let _host = show(|| {
        Leaf(
            nodes(&["node"])
                .then(Modifier)
                .then(nodes(&["child_1"]))
                .then(Modifier)
                .then(nodes(&["child_2"])),
        )
    });
    assert_eq!(visit_local_descendants("node"), ["child_1", "child_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, noChildren
#[test]
fn self_and_children_of_a_lone_node_is_itself() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_self_and_children("node"), ["node"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, oneChild
#[test]
fn self_and_children_with_one_child() {
    let _host = show(|| Leaf(nodes(&["node", "child"])));
    assert_eq!(visit_self_and_children("node"), ["node", "child"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, multipleNodesInModifierChain
#[test]
fn self_and_children_stop_at_the_next_in_the_chain() {
    let _host = show(|| Leaf(nodes(&["node", "child_1", "child_2"])));
    assert_eq!(visit_self_and_children("node"), ["node", "child_1"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, nonContiguousChild
#[test]
fn self_and_children_pass_over_modifiers_without_nodes() {
    let _host = show(|| Leaf(nodes(&["node"]).then(Modifier).then(nodes(&["child"]))));
    assert_eq!(visit_self_and_children("node"), ["node", "child"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, childInDifferentLayoutNode
#[test]
fn self_and_children_in_a_child_layout_node() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child"]));
        });
    });
    assert_eq!(visit_self_and_children("node"), ["node", "child"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, childrenInDifferentLayoutNode
#[test]
fn self_and_children_in_several_child_layout_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child_1"]));
            Leaf(nodes(&["child_2"]));
        });
    });
    assert_eq!(visit_self_and_children("node"), ["node", "child_1", "child_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, childInDifferentLayoutNodeNonContiguous
#[test]
fn self_and_children_pass_over_layout_nodes_without_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(Modifier);
            Stack!(modifier = Modifier.then(Modifier), {
                Leaf(nodes(&["child"]));
            });
        });
    });
    assert_eq!(visit_self_and_children("node"), ["node", "child"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSelfAndChildrenTest.kt, childrenInDifferentLayoutNodeNonContiguous
#[test]
fn self_and_children_pass_over_layout_nodes_without_nodes_in_each_branch() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Stack!({
                Leaf(nodes(&["child_1"]));
            });
            Stack!(modifier = Modifier.then(Modifier), {
                Leaf(nodes(&["child_2"]));
            });
            Leaf(nodes(&["child_3"]));
        });
    });
    assert_eq!(visit_self_and_children("node"), ["node", "child_1", "child_2", "child_3"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeIfTest.kt, noChildren
#[test]
fn a_conditional_subtree_visit_of_a_lone_node_visits_nothing() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_subtree("node", NodeKinds::ANY, false, |_| true), NONE);
}

fn subtree_if_content() {
    Stack!(modifier = nodes(&["node_1"]), {
        Stack!(modifier = nodes(&["node_2"]), {
            Leaf(nodes(&["node_3"]));
            Stack!(modifier = nodes(&["node_4"]), {
                Leaf(nodes(&["node_6"]));
            });
            Leaf(nodes(&["node_5"]));
        });
    });
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeIfTest.kt, stopsIfWeReturnFalse
#[test]
fn a_conditional_subtree_visit_doesn_t_go_below_what_says_no() {
    let _host = show(subtree_if_content);
    assert_eq!(
        visit_subtree("node_1", NodeKinds::ANY, false, |label| label != "node_4"),
        ["node_2", "node_3", "node_4", "node_5"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeIfTest.kt, continuesIfWeReturnTrue
#[test]
fn a_conditional_subtree_visit_goes_on_while_told_to() {
    let _host = show(subtree_if_content);
    assert_eq!(
        visit_subtree("node_1", NodeKinds::ANY, false, |_| true),
        ["node_2", "node_3", "node_4", "node_6", "node_5"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeIfTest.kt, visitsItemsAcrossLayoutNodes
#[test]
fn a_conditional_subtree_visit_goes_across_layout_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node_1", "node_2"]), {
            Stack!(modifier = nodes(&["node_3", "node_4"]), {
                Leaf(nodes(&["node_7", "node_8"]));
            });
            Stack!(modifier = nodes(&["node_5", "node_6"]), {
                Leaf(nodes(&["node_9", "node_10"]));
            });
        });
    });
    assert_eq!(
        visit_subtree("node_1", NodeKinds::ANY, false, |_| true),
        ["node_2", "node_3", "node_4", "node_7", "node_8", "node_5", "node_6", "node_9", "node_10"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeIfTest.kt, visitsItemsAcrossLayoutNodesInDrawOrder_zIndex
#[test]
fn a_conditional_subtree_visit_in_draw_order_goes_by_z_index() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child_1"]).z_index(10.0));
            Leaf(nodes(&["child_2"]).z_index(-10.0));
            Leaf(nodes(&["child_3"]));
        });
    });
    assert_eq!(
        visit_subtree("node", NodeKinds::ANY, true, |_| true),
        ["child_2", "child_3", "child_1"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeTest.kt, noChildren
#[test]
fn a_subtree_visit_of_a_lone_node_visits_nothing() {
    let _host = show(|| Leaf(nodes(&["node"])));
    assert_eq!(visit_subtree("node", NodeKinds::ANY, false, |_| true), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeTest.kt, localChildren
#[test]
fn a_subtree_visit_visits_what_follows_in_the_chain() {
    let _host = show(|| Leaf(nodes(&["parent", "node", "child_1", "child_2"])));
    assert_eq!(visit_subtree("node", NodeKinds::ANY, false, |_| true), ["child_1", "child_2"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeTest.kt, differentLayoutNodes
#[test]
fn a_subtree_visit_goes_across_layout_nodes() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node", "child_1", "child_2"]), {
            Stack!(modifier = nodes(&["child_3", "child_4"]), {
                Leaf(nodes(&["child_5", "child_6"]));
            });
            Stack!({
                Leaf(nodes(&["child_7", "child_8"]));
            });
        });
    });
    assert_eq!(
        visit_subtree("node", NodeKinds::ANY, false, |_| true),
        ["child_1", "child_2", "child_3", "child_4", "child_5", "child_6", "child_7", "child_8"]
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ModifierNodeVisitSubtreeTest.kt, differentLayoutNodesInDrawOrder_zIndex
#[test]
fn a_subtree_visit_in_draw_order_goes_by_z_index() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["node"]), {
            Leaf(nodes(&["child_1"]));
            Stack!(modifier = nodes(&["child_2"]).z_index(10.0), {
                Leaf(nodes(&["child_3"]).z_index(-10.0));
            });
            Stack!({
                Leaf(nodes(&["child_4"]));
            });
        });
    });
    assert_eq!(
        visit_subtree("node", NodeKinds::ANY, true, |_| true),
        ["child_1", "child_4", "child_2", "child_3"]
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testVisitSubtreeWithDelegates
#[test]
fn a_subtree_visit_of_a_kind_visits_the_delegates_of_it() {
    let _host = show(|| {
        let chain = draw_node("x").then(wrapped_draw_node("a", "a.wrapped")).then(draw_node("b"));
        Stack!(modifier = chain, {
            Leaf(wrapped_draw_node("c", "c.wrapped"));
            Leaf(draw_node("d"));
        });
    });
    assert_eq!(
        visit_subtree("x", NodeKinds::DRAW, false, |_| true),
        ["a.wrapped", "b", "c.wrapped", "d"]
    );
}

// Compose: compose/ui/ui/src/androidHostTest/kotlin/androidx/compose/ui/node/DelegatingNodeTest.kt, testVisitAncestorsWithDelegates
#[test]
fn an_ancestors_visit_of_a_kind_visits_the_delegates_of_it() {
    let _host = show(|| {
        Stack!(modifier = wrapped_draw_node("a", "a.wrapped"), {
            Stack!(modifier = draw_node("b"), {
                Stack!(modifier = wrapped_draw_node("c", "c.wrapped"), {
                    Leaf(draw_node("d").then(draw_node("x")));
                });
            });
        });
    });
    assert_eq!(visit_ancestors("x", NodeKinds::DRAW, false), ["d", "c.wrapped", "b", "a.wrapped"]);
}

// ---- TraversableNode ---------------------------------------------------------------------------

static SHARED: TraverseKey = TraverseKey::new("shared");
static OTHER: TraverseKey = TraverseKey::new("other");
static DELEGATABLE_ONE: TraverseKey = TraverseKey::new("delegatable one");
static DELEGATABLE_TWO: TraverseKey = TraverseKey::new("delegatable two");

macro_rules! traversable {
    ($($element:ident, $node:ident, $key:ident;)*) => {$(
        #[derive(Clone, Copy, PartialEq)]
        struct $element(&'static str);

        struct $node {
            label: &'static str,
        }

        impl ModifierNodeElement for $element {
            type Node = $node;
            fn create(&self) -> $node {
                $node { label: self.0 }
            }
            fn update(&self, _: &mut $node) {}
            fn same(&self, other: &Self) -> bool {
                self == other
            }
        }

        impl ModifierNode for $node {
            fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
                Some(self)
            }
            fn on_attach(&mut self, context: &mut NodeContext<'_>) {
                remember_handle(self.label, context);
            }
        }

        impl TraversableNode for $node {
            fn traverse_key(&self) -> &'static TraverseKey {
                &$key
            }
        }
    )*};
}

traversable! {
    ClassOne, ClassOneNode, SHARED;
    ClassTwo, ClassTwoNode, SHARED;
    ClassThree, ClassThreeNode, OTHER;
    DelegatableOne, DelegatableOneNode, DELEGATABLE_ONE;
    DelegatableTwo, DelegatableTwoNode, DELEGATABLE_TWO;
}

/// As the test's `DelegateRoot`: a traversable node delegating to another.
struct DelegateRoot {
    label: &'static str,
    key: &'static TraverseKey,
    delegates: Delegates,
}

#[derive(Clone, Copy)]
struct DelegateRootElement {
    label: &'static str,
    key: &'static TraverseKey,
    delegate_one: bool,
}

impl ModifierNodeElement for DelegateRootElement {
    type Node = DelegateRoot;
    fn create(&self) -> DelegateRoot {
        let mut delegates = Delegates::new();
        if self.delegate_one {
            delegates.delegate(DelegatableOneNode { label: "delegatable_A" });
        } else {
            delegates.delegate(DelegatableTwoNode { label: "delegatable_B" });
        }
        DelegateRoot { label: self.label, key: self.key, delegates }
    }
    fn update(&self, _: &mut DelegateRoot) {}
    fn same(&self, other: &Self) -> bool {
        self.label == other.label
    }
}

impl ModifierNode for DelegateRoot {
    fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
        Some(self)
    }
    fn delegates(&mut self) -> Option<&mut Delegates> {
        Some(&mut self.delegates)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        remember_handle(self.label, context);
    }
}

impl TraversableNode for DelegateRoot {
    fn traverse_key(&self) -> &'static TraverseKey {
        self.key
    }
}

/// The hierarchy of the Compose test's `createUi`.
fn traversal_ui() -> UiHost {
    show(|| {
        Column!(modifier = Modifier.then_custom(ClassOne("Parent")), {
            Row!(modifier = Modifier.then_custom(ClassOne("Child_A")), {
                Leaf(Modifier.then_custom(ClassOne("Grandchild_A")));
                Leaf(Modifier.then_custom(ClassTwo("Grandchild_B")));
                Leaf(Modifier.then_custom(ClassThree("Grandchild_C")));
            });
            Row!(modifier = Modifier.then_custom(ClassTwo("Child_B")), {
                Leaf(Modifier.then_custom(ClassOne("Grandchild_D")));
                Leaf(Modifier.then_custom(ClassTwo("Grandchild_E")));
                Leaf(Modifier.then_custom(ClassThree("Grandchild_F")));
            });
            Row!(modifier = Modifier.then_custom(ClassThree("Child_C")), {
                Leaf(Modifier.then_custom(ClassOne("Grandchild_G")));
                Leaf(Modifier.then_custom(ClassTwo("Grandchild_H")));
                Leaf(Modifier.then_custom(ClassThree("Grandchild_I")));
            });
            Row!(modifier = Modifier.then_custom(ClassTwo("Child_D")), {
                Leaf(Modifier.then_custom(ClassOne("Grandchild_J")));
            });
            // The root has another key than its delegate, the same as its child's.
            let child_e = DelegateRootElement {
                label: "Child_E",
                key: &DELEGATABLE_ONE,
                delegate_one: false,
            };
            Row!(modifier = Modifier.then_custom(child_e), {
                Leaf(Modifier.then_custom(DelegatableOne("Grandchild_E2")));
            });
            // The root has its delegate's key, another than its child's.
            let child_f =
                DelegateRootElement { label: "Child_F", key: &DELEGATABLE_ONE, delegate_one: true };
            Row!(modifier = Modifier.then_custom(child_f), {
                Leaf(Modifier.then_custom(DelegatableTwo("Grandchild_F2")));
            });
        });
    })
}

/// A traversable node's label and its class's number (1 to 3; 0 for the others).
fn describe(node: &dyn TraversableNode) -> (&'static str, u8) {
    if let Some(node) = node.downcast_ref::<ClassOneNode>() {
        (node.label, 1)
    } else if let Some(node) = node.downcast_ref::<ClassTwoNode>() {
        (node.label, 2)
    } else if let Some(node) = node.downcast_ref::<ClassThreeNode>() {
        (node.label, 3)
    } else if let Some(node) = node.downcast_ref::<DelegatableOneNode>() {
        (node.label, 0)
    } else if let Some(node) = node.downcast_ref::<DelegatableTwoNode>() {
        (node.label, 0)
    } else if let Some(node) = node.downcast_ref::<DelegateRoot>() {
        (node.label, 0)
    } else {
        ("", 0)
    }
}

/// Runs `f` with the context of the node `label`, of type `N`.
fn at<N: ModifierNode, R>(label: &str, f: impl FnOnce(&mut NodeContext<'_>) -> R) -> R {
    handle(label)
        .with_node(|_: &mut N, context| f(context))
        .expect("the tree is free between frames")
}

fn nearest_with_key<N: ModifierNode>(
    label: &str,
    key: &'static TraverseKey,
) -> Option<&'static str> {
    at::<N, _>(label, |context| {
        context.find_nearest_ancestor_with_key(key).map(|found| describe(&*found).0)
    })
}

/// The labels `traverse_ancestors_with_key` gives from `label`.
fn ancestors_with_key<N: ModifierNode>(
    label: &str,
    key: &'static TraverseKey,
) -> Vec<&'static str> {
    let mut found = Vec::new();
    at::<N, _>(label, |context| {
        context.traverse_ancestors_with_key(key, |node, _| {
            found.push(describe(node).0);
            true
        });
    });
    found
}

fn children_with_key(key: &'static TraverseKey) -> Vec<(&'static str, u8)> {
    let mut found = Vec::new();
    at::<ClassOneNode, _>("Parent", |context| {
        context.traverse_children_with_key(key, |node, _| {
            found.push(describe(node));
            true
        });
    });
    found
}

/// The nodes `traverse_descendants_with_key` gives from the parent, each answered by `action`.
fn descendants_with_key(
    key: &'static TraverseKey,
    action: impl Fn(u8) -> TraverseDescendantsAction,
) -> Vec<(&'static str, u8)> {
    let mut found = Vec::new();
    at::<ClassOneNode, _>("Parent", |context| {
        context.traverse_descendants_with_key(key, |node, _| {
            let description = describe(node);
            found.push(description);
            action(description.1)
        });
    });
    found
}

fn count_of_class(found: &[(&'static str, u8)], class: u8) -> usize {
    found.iter().filter(|(_, found)| *found == class).count()
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_ancestorsWithTheSameClass
#[test]
fn nearest_ancestor_of_the_same_class_and_key() {
    let _host = traversal_ui();
    let nearest = |label| {
        at::<ClassOneNode, _>(label, |context| {
            context.find_nearest_ancestor::<ClassOneNode>().map(|found| found.label)
        })
    };
    assert_eq!(nearest("Grandchild_A"), Some("Child_A"));
    assert_eq!(nearest("Grandchild_D"), Some("Parent"));
    assert_eq!(nearest("Grandchild_G"), Some("Parent"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_delegateWithDifferentKeyAsRootNode_useDelegateKey
#[test]
fn nearest_ancestor_by_key_finds_a_delegate_with_it() {
    let _host = traversal_ui();
    assert_eq!(
        nearest_with_key::<DelegatableOneNode>("Grandchild_E2", &DELEGATABLE_TWO),
        Some("delegatable_B")
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_delegateWithDifferentKeyAsRootNode_useGrandChildDelegateKey
#[test]
fn nearest_ancestor_by_key_finds_the_root_with_it() {
    let _host = traversal_ui();
    assert_eq!(
        nearest_with_key::<DelegatableOneNode>("Grandchild_E2", &DELEGATABLE_ONE),
        Some("Child_E")
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_delegateWithSameKeyAsRootNode_useDelegateKey
#[test]
fn nearest_ancestor_by_key_finds_the_root_before_its_delegate() {
    let _host = traversal_ui();
    assert_eq!(
        nearest_with_key::<DelegatableTwoNode>("Grandchild_F2", &DELEGATABLE_ONE),
        Some("Child_F")
    );
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_delegateWithSameKeyAsRootNode_useGrandChildDelegateKey
#[test]
fn nearest_ancestor_by_a_key_nothing_above_has_is_none() {
    let _host = traversal_ui();
    assert_eq!(nearest_with_key::<DelegatableTwoNode>("Grandchild_F2", &DELEGATABLE_TWO), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_ancestorsWithOutTheSameClass
#[test]
fn nearest_ancestor_of_a_class_nothing_above_is_is_none() {
    let _host = traversal_ui();
    let from_b = at::<ClassTwoNode, _>("Grandchild_B", |context| {
        context.find_nearest_ancestor::<ClassTwoNode>().map(|found| found.label)
    });
    assert_eq!(from_b, None);
    let from_c = at::<ClassThreeNode, _>("Grandchild_C", |context| {
        context.find_nearest_ancestor::<ClassThreeNode>().map(|found| found.label)
    });
    assert_eq!(from_c, None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_ancestorsWithTheSameKey
#[test]
fn nearest_ancestor_by_key_is_of_any_class() {
    let _host = traversal_ui();
    assert_eq!(nearest_with_key::<ClassOneNode>("Grandchild_A", &SHARED), Some("Child_A"));
    assert_eq!(nearest_with_key::<ClassOneNode>("Grandchild_D", &SHARED), Some("Child_B"));
    assert_eq!(nearest_with_key::<ClassOneNode>("Grandchild_G", &SHARED), Some("Parent"));
    assert_eq!(nearest_with_key::<ClassOneNode>("Grandchild_G", &OTHER), Some("Child_C"));
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, findNearestAncestor_ancestorsWithoutTheSameKey
#[test]
fn nearest_ancestor_by_a_key_no_ancestor_has_is_none() {
    let _host = traversal_ui();
    assert_eq!(nearest_with_key::<ClassOneNode>("Grandchild_A", &OTHER), None);
    assert_eq!(nearest_with_key::<ClassTwoNode>("Grandchild_B", &OTHER), None);
    assert_eq!(nearest_with_key::<ClassThreeNode>("Grandchild_C", &OTHER), None);
    assert_eq!(nearest_with_key::<ClassThreeNode>("Grandchild_F", &OTHER), None);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseAncestors_sameClass
#[test]
fn traversing_ancestors_of_the_same_class_and_key() {
    let _host = traversal_ui();
    let count = |label| {
        let mut count = 0;
        at::<ClassOneNode, _>(label, |context| {
            context.traverse_ancestors::<ClassOneNode>(|_, _| {
                count += 1;
                true
            });
        });
        count
    };
    assert_eq!(count("Grandchild_A"), 2);
    assert_eq!(count("Grandchild_D"), 1);
    assert_eq!(count("Grandchild_G"), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseAncestors_sameClassWithCancellation
#[test]
fn traversing_ancestors_stops_when_told() {
    let _host = traversal_ui();
    let mut count = 0;
    at::<ClassOneNode, _>("Grandchild_A", |context| {
        context.traverse_ancestors::<ClassOneNode>(|_, _| {
            count += 1;
            false
        });
    });
    assert_eq!(count, 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseAncestors_sameKey
#[test]
fn traversing_ancestors_by_key_goes_by_any_class() {
    let _host = traversal_ui();
    assert_eq!(ancestors_with_key::<ClassOneNode>("Grandchild_A", &SHARED), ["Child_A", "Parent"]);
    assert_eq!(ancestors_with_key::<ClassOneNode>("Grandchild_D", &SHARED), ["Child_B", "Parent"]);
    assert_eq!(ancestors_with_key::<ClassOneNode>("Grandchild_G", &SHARED), ["Parent"]);
    assert_eq!(ancestors_with_key::<ClassOneNode>("Grandchild_G", &OTHER), ["Child_C"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseAncestors_differentKeyFromCallingNode
#[test]
fn traversing_ancestors_by_a_key_none_has_finds_nothing() {
    let _host = traversal_ui();
    assert_eq!(ancestors_with_key::<ClassOneNode>("Grandchild_A", &OTHER), NONE);
    assert_eq!(ancestors_with_key::<ClassOneNode>("Grandchild_D", &OTHER), NONE);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseChildren_sameClass
#[test]
fn traversing_children_of_the_same_class_and_key() {
    let _host = traversal_ui();
    let mut found = Vec::new();
    at::<ClassOneNode, _>("Parent", |context| {
        context.traverse_children::<ClassOneNode>(|node, _| {
            found.push(node.label);
            true
        });
    });
    assert_eq!(found, ["Child_A"]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseChildren_sameKey
#[test]
fn traversing_children_by_key_goes_by_any_class() {
    let _host = traversal_ui();
    let found = children_with_key(&SHARED);
    assert_eq!(found.len(), 3);
    assert_eq!(count_of_class(&found, 1), 1);
    assert_eq!(count_of_class(&found, 2), 2);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseChildren_differentKeyFromCallingNode
#[test]
fn traversing_children_by_another_key() {
    let _host = traversal_ui();
    assert_eq!(children_with_key(&OTHER), [("Child_C", 3)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameClassAndAlwaysContinueTraversal
#[test]
fn traversing_descendants_of_the_same_class_and_key() {
    let _host = traversal_ui();
    let mut count = 0;
    at::<ClassOneNode, _>("Parent", |context| {
        context.traverse_descendants::<ClassOneNode>(|_, _| {
            count += 1;
            ContinueTraversal
        });
    });
    assert_eq!(count, 5);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameClassAndAlwaysSkipSubtree
#[test]
fn traversing_descendants_of_the_same_class_skipping_each_subtree() {
    let _host = traversal_ui();
    let mut count = 0;
    at::<ClassOneNode, _>("Parent", |context| {
        context.traverse_descendants::<ClassOneNode>(|_, _| {
            count += 1;
            SkipSubtreeAndContinueTraversal
        });
    });
    assert_eq!(count, 4);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameClassAndConditionallySkipSubtree
#[test]
fn traversing_descendants_of_the_same_class_skipping_one_subtree() {
    let _host = traversal_ui();
    let mut count = 0;
    at::<ClassOneNode, _>("Parent", |context| {
        context.traverse_descendants::<ClassOneNode>(|node, _| {
            count += 1;
            if node.label == "Child_A" {
                SkipSubtreeAndContinueTraversal
            } else {
                ContinueTraversal
            }
        });
    });
    assert_eq!(count, 4);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_alwaysContinueTraversal
#[test]
fn traversing_descendants_by_key_goes_by_any_class() {
    let _host = traversal_ui();
    let found = descendants_with_key(&SHARED, |_| ContinueTraversal);
    assert_eq!(found.len(), 10);
    assert_eq!(count_of_class(&found, 1), 5);
    assert_eq!(count_of_class(&found, 2), 5);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_alwaysCancelTraversal
#[test]
fn traversing_descendants_by_key_stops_when_told() {
    let _host = traversal_ui();
    let found = descendants_with_key(&SHARED, |_| CancelTraversal);
    assert_eq!(found, [("Child_A", 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_alwaysSkipSubtree
#[test]
fn traversing_descendants_by_key_skipping_each_subtree() {
    let _host = traversal_ui();
    let found = descendants_with_key(&SHARED, |_| SkipSubtreeAndContinueTraversal);
    assert_eq!(found.len(), 5);
    assert_eq!(count_of_class(&found, 1), 2);
    assert_eq!(count_of_class(&found, 2), 3);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_skipSubtreeOfSameClass
#[test]
fn traversing_descendants_by_key_skipping_the_subtrees_of_one_class() {
    let _host = traversal_ui();
    let found = descendants_with_key(&SHARED, |class| {
        if class == 1 { SkipSubtreeAndContinueTraversal } else { ContinueTraversal }
    });
    assert_eq!(found.len(), 8);
    assert_eq!(count_of_class(&found, 1), 4);
    assert_eq!(count_of_class(&found, 2), 4);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_cancelTraversalOfSameClass
#[test]
fn traversing_descendants_by_key_stopping_at_the_same_class() {
    let _host = traversal_ui();
    let found =
        descendants_with_key(
            &SHARED,
            |class| {
                if class == 1 { CancelTraversal } else { ContinueTraversal }
            },
        );
    assert_eq!(found, [("Child_A", 1)]);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_skipSubtreeOfDifferentClassSameKey
#[test]
fn traversing_descendants_by_key_skipping_the_subtrees_of_another_class() {
    let _host = traversal_ui();
    let found = descendants_with_key(&SHARED, |class| {
        if class == 2 { SkipSubtreeAndContinueTraversal } else { ContinueTraversal }
    });
    assert_eq!(found.len(), 7);
    assert_eq!(count_of_class(&found, 1), 3);
    assert_eq!(count_of_class(&found, 2), 4);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_sameKeyFromCallingNode_cancelTraversalOfDifferentClassSameKey
#[test]
fn traversing_descendants_by_key_stopping_at_another_class() {
    let _host = traversal_ui();
    let found =
        descendants_with_key(
            &SHARED,
            |class| {
                if class == 2 { CancelTraversal } else { ContinueTraversal }
            },
        );
    assert_eq!(found.len(), 3);
    assert_eq!(count_of_class(&found, 1), 2);
    assert_eq!(count_of_class(&found, 2), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_differentKeyFromCallingNode_alwaysContinueTraversal
#[test]
fn traversing_descendants_by_another_key() {
    let _host = traversal_ui();
    let found = descendants_with_key(&OTHER, |_| ContinueTraversal);
    assert_eq!(found.len(), 4);
    assert_eq!(count_of_class(&found, 3), 4);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/TraversableModifierNodeTest.kt, traverseDescendants_differentKeyFromCallingNode_alwaysSkipSubtree
#[test]
fn traversing_descendants_by_another_key_skipping_each_subtree() {
    let _host = traversal_ui();
    let found = descendants_with_key(&OTHER, |_| SkipSubtreeAndContinueTraversal);
    assert_eq!(found.len(), 3);
    assert_eq!(count_of_class(&found, 3), 3);
}

// ---- What only compose-rs's shape needs ---------------------------------------------------------

/// A node of one type whose instances have different keys.
struct Keyed(&'static str, &'static TraverseKey);
struct KeyedNode(&'static str, &'static TraverseKey);

impl ModifierNodeElement for Keyed {
    type Node = KeyedNode;
    fn create(&self) -> KeyedNode {
        KeyedNode(self.0, self.1)
    }
    fn update(&self, _: &mut KeyedNode) {}
    fn same(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl ModifierNode for KeyedNode {
    fn as_traversable(&mut self) -> Option<&mut dyn TraversableNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        remember_handle(self.0, context);
    }
}

impl TraversableNode for KeyedNode {
    fn traverse_key(&self) -> &'static TraverseKey {
        self.1
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/TraversableNode.kt, traverseAncestors
#[test]
fn a_typed_traversal_looks_for_the_calling_node_s_key_too() {
    let _host = show(|| {
        Stack!(modifier = Modifier.then_custom(Keyed("shared_above", &SHARED)), {
            Stack!(modifier = Modifier.then_custom(Keyed("other_above", &OTHER)), {
                Leaf(Modifier.then_custom(Keyed("from", &SHARED)));
            });
        });
    });
    let mut found = Vec::new();
    at::<KeyedNode, _>("from", |context| {
        context.traverse_ancestors::<KeyedNode>(|node, _| {
            found.push(node.0);
            true
        });
    });
    assert_eq!(found, ["shared_above"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatableNode.kt, visitAncestors
#[test]
fn a_delegate_s_traversal_starts_at_its_owner_s_place() {
    let _host = show(|| {
        let root = DelegateRootElement { label: "owner", key: &SHARED, delegate_one: true };
        Leaf(
            Modifier
                .then_custom(ClassOne("before"))
                .then_custom(root)
                .then_custom(ClassOne("after")),
        );
    });
    let (above, below) = at::<DelegatableOneNode, _>("delegatable_A", |context| {
        let mut above = Vec::new();
        context.traverse_ancestors_with_key(&SHARED, |node, _| {
            above.push(describe(node).0);
            true
        });
        let mut below = Vec::new();
        context.traverse_descendants_with_key(&SHARED, |node, _| {
            below.push(describe(node).0);
            ContinueTraversal
        });
        (above, below)
    });
    assert_eq!(above, ["before"]);
    assert_eq!(below, ["after"]);
}

/// Traverses in its `on_attach`, and gets to its ancestors' contexts.
struct Traversing;
struct TraversingNode;

impl ModifierNodeElement for Traversing {
    type Node = TraversingNode;
    fn create(&self) -> TraversingNode {
        TraversingNode
    }
    fn update(&self, _: &mut TraversingNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for TraversingNode {
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        context.traverse_ancestors_with_key(&SHARED, |node, context| {
            log(format!("attached {} {}", describe(node).0, context.is_attached()));
            context.invalidate_draw();
            true
        });
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/TraversableNode.kt, traverseAncestors
#[test]
fn a_traversal_from_a_callback_gives_each_node_its_context() {
    let _host = show(|| {
        Stack!(modifier = Modifier.then_custom(ClassOne("above")), {
            Leaf(Modifier.then_custom(Traversing));
        });
    });
    // Children attach before their parents, but every node is marked attached by then.
    assert_eq!(take_log(), ["attached above true"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatableNode.kt, visitSubtreeIf
#[test]
fn a_traversal_passes_over_content_kept_for_reuse() {
    let active = mutable_state_of(true);
    let a = active.handle();
    let mut host = show(move || {
        Stack!(modifier = Modifier.then_custom(ClassOne("Parent")), {
            ReusableContentHost(a.get(), move || {
                ReusableContent(0, move || Leaf(Modifier.then_custom(ClassOne("kept"))));
            });
        });
    });
    let descendants = || {
        let mut found = Vec::new();
        at::<ClassOneNode, _>("Parent", |context| {
            context.traverse_descendants::<ClassOneNode>(|node, _| {
                found.push(node.label);
                ContinueTraversal
            });
            context.traverse_children::<ClassOneNode>(|node, _| {
                found.push(node.label);
                true
            });
        });
        found
    };
    assert_eq!(descendants(), ["kept", "kept"]);
    active.set(false);
    host.frame();
    assert_eq!(descendants(), NONE);
}

// ---- A review's scenarios ---------------------------------------------------------------------

/// A layout node, to measure what a traversal asked from under it.
struct LayoutTracked(&'static str);
struct LayoutTrackedNode(&'static str);

impl ModifierNodeElement for LayoutTracked {
    type Node = LayoutTrackedNode;
    fn create(&self) -> LayoutTrackedNode {
        LayoutTrackedNode(self.0)
    }
    fn update(&self, _: &mut LayoutTrackedNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for LayoutTrackedNode {
    fn as_layout(&mut self) -> Option<&mut dyn LayoutModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        remember_handle(self.0, context);
    }
}

impl LayoutModifierNode for LayoutTrackedNode {
    fn measure(
        &mut self,
        scope: &mut MeasureScope<'_>,
        content: Measurable,
        constraints: Constraints,
    ) -> MeasureResult {
        log(format!("measure {}", self.0));
        let placeable = scope.measure(content, constraints);
        scope.place(&placeable, 0, 0);
        scope.layout(placeable.width, placeable.height)
    }
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/LayoutModifierNode.kt, remeasureSync
#[test]
fn remeasure_sync_of_a_node_a_traversal_found_measures_once_every_node_is_back() {
    // The found node is before the caller in its chain, so measuring them is measuring the
    // caller, a layout node, whose code is still running.
    let _host = show(|| {
        Leaf(Modifier.then_custom(ClassOne("above")).then_custom(LayoutTracked("from")));
    });
    take_log();
    at::<LayoutTrackedNode, _>("from", |context| {
        context.traverse_ancestors_with_key(&SHARED, |_, context| {
            context.remeasure_sync();
            true
        });
        log("the traversal ended");
    });
    assert_eq!(take_log(), ["the traversal ended", "measure from"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/TraversableNode.kt, findNearestAncestor
// Keys compare by identity: two of one name are two keys.
#[test]
fn two_keys_of_one_name_are_two_keys() {
    static FIRST: TraverseKey = TraverseKey::new("one name");
    static SECOND: TraverseKey = TraverseKey::new("one name");
    assert_ne!(&FIRST, &SECOND);
    assert_eq!(&FIRST, &FIRST);
    assert_eq!(FIRST.name(), SECOND.name());
    let _host = show(|| {
        Stack!(modifier = Modifier.then_custom(Keyed("above", &FIRST)), {
            Leaf(Modifier.then_custom(Keyed("from", &SECOND)));
        });
    });
    let found = at::<KeyedNode, _>("from", |context| {
        context.find_nearest_ancestor_with_key(&SECOND).map(|found| found.traverse_key().name())
    });
    assert_eq!(found, None);
    let found = at::<KeyedNode, _>("from", |context| {
        context.find_nearest_ancestor_with_key(&FIRST).map(|found| found.traverse_key().name())
    });
    assert_eq!(found, Some("one name"));
}

// Compose: compose/ui/ui/src/androidMain/kotlin/androidx/compose/ui/window/AndroidPopup.android.kt, Popup
// A popup is a separate root: traversals stop at it, both ways.
#[test]
fn traversals_stop_at_a_popup() {
    let _host = show(|| {
        Stack!(modifier = nodes(&["above"]), {
            Leaf(nodes(&["sibling"]));
            Stack!(
                modifier = Modifier.overlay(Rc::new(WindowPosition), false).then(nodes(&["popup"])),
                {
                    Leaf(nodes(&["in_popup"]));
                }
            );
        });
    });
    assert_eq!(visit_ancestors("in_popup", NodeKinds::ANY, false), ["popup"]);
    assert_eq!(visit_ancestors("popup", NodeKinds::ANY, false), NONE);
    assert_eq!(visit_subtree("above", NodeKinds::ANY, false, |_| true), ["sibling"]);
    assert_eq!(visit_children("above", NodeKinds::ANY, false), ["sibling"]);
    assert_eq!(visit_children("above", NodeKinds::ANY, true), ["sibling"]);
    assert_eq!(visit_subtree("popup", NodeKinds::ANY, false, |_| true), ["in_popup"]);
}

// Compose: compose/ui/ui/src/commonMain/kotlin/androidx/compose/ui/node/DelegatableNode.kt, visitAncestors
// Compose's throws for a detached caller; compose-rs's returns quietly (D1178).
#[test]
fn a_detached_node_s_traversal_finds_what_it_can_and_doesn_t_panic() {
    let active = mutable_state_of(true);
    let a = active.handle();
    let mut host = show(move || {
        Stack!(modifier = nodes(&["above"]), {
            ReusableContentHost(a.get(), move || {
                ReusableContent(0, move || Leaf(nodes(&["kept"])));
            });
        });
    });
    active.set(false);
    host.frame();
    let mut children = 0;
    from("kept", |context| {
        assert!(!context.is_attached());
        context.visit_children(NodeKinds::ANY, false, &mut |_, _| {
            children += 1;
            ControlFlow::Continue(())
        });
    });
    assert_eq!(children, 0);
}
