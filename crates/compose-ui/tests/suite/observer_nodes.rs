//! `ObserverModifierNode` and `NodeContext::observe_reads` (docs/parity/design/modifier-nodes.md
//! §3.9, step MN-7).

use crate::support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use compose_runtime::*;
use compose_ui::*;
use support::*;

type Held = Rc<RefCell<Option<NodeHandle>>>;

/// As ObserverModifierNodeTest's `TestObserverNode`: counts its `on_observed_reads_changed`, and
/// keeps its handle, through which the test observes reads from the node.
struct Observer(Rc<Cell<u32>>, Held);

struct ObserverNode(Rc<Cell<u32>>, Held);

impl ModifierNodeElement for Observer {
    type Node = ObserverNode;
    fn create(&self) -> ObserverNode {
        ObserverNode(self.0.clone(), self.1.clone())
    }
    fn update(&self, _: &mut ObserverNode) {}
    fn same(&self, _: &Self) -> bool {
        true
    }
}

impl ModifierNode for ObserverNode {
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        *self.1.borrow_mut() = Some(context.handle());
    }
}

impl ObserverModifierNode for ObserverNode {
    fn on_observed_reads_changed(&mut self, _: &mut NodeContext<'_>) {
        self.0.set(self.0.get() + 1);
    }
}

/// Observes `value` from the node `handle` names, as the Compose test's
/// `observerNode.observeReads { value.toString() }` from the test.
fn observe(handle: &Held, value: MutableState<i32>) {
    let node = handle.borrow().expect("the node's handle");
    let observed = node.with_node(|_: &mut ObserverNode, context| {
        context.observe_reads(|| value.get());
    });
    assert!(observed.is_some(), "the tree is free between frames");
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ObserverModifierNodeTest.kt, simplyObservingValue_doesNotTriggerCallback
#[test]
fn observing_a_value_alone_calls_nothing() {
    let value = mutable_state_of(1);
    let (calls, handle) = (Rc::new(Cell::new(0)), Held::default());
    let (c, h) = (calls.clone(), handle.clone());
    let mut host = host(move || Leaf(Modifier.then_custom(Observer(c.clone(), h.clone()))));
    observe(&handle, value.handle());
    host.frame();
    assert_eq!(calls.get(), 0);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ObserverModifierNodeTest.kt, changeInObservedValue_triggersCallback
#[test]
fn a_change_of_an_observed_value_calls_the_node_once() {
    let value = mutable_state_of(1);
    let (calls, handle) = (Rc::new(Cell::new(0)), Held::default());
    let (c, h) = (calls.clone(), handle.clone());
    let mut host = host(move || Leaf(Modifier.then_custom(Observer(c.clone(), h.clone()))));
    observe(&handle, value.handle());
    value.set(3);
    assert!(host.needs_frame(), "the write asks for a frame");
    host.frame();
    assert_eq!(calls.get(), 1);
    // Once: observing again is what keeps observing.
    value.set(4);
    host.frame();
    assert_eq!(calls.get(), 1);
    observe(&handle, value.handle());
    value.set(5);
    host.frame();
    assert_eq!(calls.get(), 2);
}

/// Observing again replaces what the last observation read.
#[test]
fn observing_again_forgets_the_last_observation_s_reads() {
    let (first, second) = (mutable_state_of(1), mutable_state_of(1));
    let (calls, handle) = (Rc::new(Cell::new(0)), Held::default());
    let (c, h) = (calls.clone(), handle.clone());
    let mut host = host(move || Leaf(Modifier.then_custom(Observer(c.clone(), h.clone()))));
    observe(&handle, first.handle());
    observe(&handle, second.handle());
    first.set(2);
    host.frame();
    assert_eq!(calls.get(), 0, "no longer observed");
    second.set(2);
    host.frame();
    assert_eq!(calls.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ObserverModifierNodeTest.kt, detachedNodeCanObserveReads
// compose-rs's nodes are owned by their chains, so a node detached and attached again is one
// kept for reuse (deactivated, then reactivated), not one removed and added back.
#[test]
fn a_detached_node_observes_and_is_called_once_attached_again() {
    let value = mutable_state_of(1);
    let active = mutable_state_of(true);
    let (calls, handle) = (Rc::new(Cell::new(0)), Held::default());
    let (c, h, a) = (calls.clone(), handle.clone(), active.handle());
    let mut host = host(move || {
        let (c, h) = (c.clone(), h.clone());
        ReusableContentHost(a.get(), move || {
            let (c, h) = (c.clone(), h.clone());
            ReusableContent(0, move || Leaf(Modifier.then_custom(Observer(c.clone(), h.clone()))));
        });
    });
    active.set(false);
    host.frame();
    let node = handle.borrow().expect("the node's handle");
    assert!(!node.is_attached());
    observe(&handle, value.handle());
    active.set(true);
    host.frame();
    assert!(node.is_attached());
    value.set(3);
    host.frame();
    assert_eq!(calls.get(), 1);
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/node/ObserverModifierNodeTest.kt, detachedNodeDoesNotCallOnObservedReadsChanged
#[test]
fn a_node_removed_from_its_chain_is_not_called() {
    let value = mutable_state_of(1);
    let attached = mutable_state_of(true);
    let (calls, handle) = (Rc::new(Cell::new(0)), Held::default());
    let (c, h, a) = (calls.clone(), handle.clone(), attached.handle());
    let mut host = host(move || {
        let modifier = match a.get() {
            true => Modifier.then_custom(Observer(c.clone(), h.clone())),
            false => Modifier,
        };
        Leaf(modifier);
    });
    observe(&handle, value.handle());
    attached.set(false);
    host.frame();
    value.set(3);
    host.frame();
    assert_eq!(calls.get(), 0);
}

/// A node detached for reuse when the write is told isn't called, and its observation is
/// spent: attached again, it hears nothing until it observes again, as Compose's.
#[test]
fn a_deactivated_node_is_not_called_and_its_observation_is_spent() {
    let value = mutable_state_of(1);
    let active = mutable_state_of(true);
    let (calls, handle) = (Rc::new(Cell::new(0)), Held::default());
    let (c, h, a) = (calls.clone(), handle.clone(), active.handle());
    let mut host = host(move || {
        let (c, h) = (c.clone(), h.clone());
        ReusableContentHost(a.get(), move || {
            let (c, h) = (c.clone(), h.clone());
            ReusableContent(0, move || Leaf(Modifier.then_custom(Observer(c.clone(), h.clone()))));
        });
    });
    observe(&handle, value.handle());
    active.set(false);
    host.frame();
    value.set(3);
    host.frame();
    assert_eq!(calls.get(), 0);
    active.set(true);
    host.frame();
    value.set(4);
    host.frame();
    assert_eq!(calls.get(), 0);
}

/// As ModifierNodeReuseAndDeactivationTest's `ObserverElement`: observes `block` when attached
/// and again whenever what it read changes.
struct Observing(Rc<dyn Fn()>);

struct ObservingNode(Rc<dyn Fn()>);

impl ModifierNodeElement for Observing {
    type Node = ObservingNode;
    fn create(&self) -> ObservingNode {
        ObservingNode(self.0.clone())
    }
    fn update(&self, node: &mut ObservingNode) {
        node.0 = self.0.clone();
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl ObservingNode {
    fn observe(&mut self, context: &mut NodeContext<'_>) {
        let block = self.0.clone();
        context.observe_reads(|| block());
    }
}

impl ModifierNode for ObservingNode {
    fn as_observer(&mut self) -> Option<&mut dyn ObserverModifierNode> {
        Some(self)
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        self.observe(context);
    }
}

impl ObserverModifierNode for ObservingNode {
    fn on_observed_reads_changed(&mut self, context: &mut NodeContext<'_>) {
        self.observe(context);
    }
}

// Compose: compose/ui/ui/src/androidDeviceTest/kotlin/androidx/compose/ui/modifier/ModifierNodeReuseAndDeactivationTest.kt, reusingModifierObservingState
#[test]
fn a_reused_node_observing_state_observes_again_once_reactivated() {
    let active = mutable_state_of(true);
    let counter = mutable_state_of(0);
    let invalidations = Rc::new(Cell::new(0));
    let (read, runs) = (counter.handle(), invalidations.clone());
    let block: Rc<dyn Fn()> = Rc::new(move || {
        read.get();
        runs.set(runs.get() + 1);
    });
    let a = active.handle();
    let mut host = host(move || {
        let block = block.clone();
        ReusableContentHost(a.get(), move || {
            let block = block.clone();
            ReusableContent(0, move || {
                let modifier = Modifier.then_custom(Observing(block.clone()));
                Layout(
                    modifier,
                    measure_policy(|scope, _, _| scope.layout(100, 100)),
                    callback(|| {}),
                );
            });
        });
    });
    assert_eq!(invalidations.get(), 1);
    active.set(false);
    host.frame();
    assert_eq!(invalidations.get(), 1);
    counter.update(|count| count + 1);
    host.frame();
    active.set(true);
    host.frame();
    assert_eq!(invalidations.get(), 2, "observed again when attached");
    counter.update(|count| count + 1);
    host.frame();
    assert_eq!(invalidations.get(), 3);
}
