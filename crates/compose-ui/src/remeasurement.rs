//! Measuring a layout node on demand, as Compose's `Remeasurement` and `RemeasurementModifier`
//! (ledger 3.12, docs/parity/design/modifier-nodes.md §3.4).

use std::marker::PhantomData;
use std::rc::Rc;

use crate::modifier::{Modifier, ModifierNode, ModifierNodeElement};
use crate::node_context::NodeContext;
use crate::tree::{NodeId, UiTree};

/// Measures its layout node on demand, as Compose's `Remeasurement`: what a
/// `RemeasurementModifier` is given. `Copy`, `'static`, `!Send`, never stale (a gone node is
/// measured no more).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Remeasurement {
    tree: u32,
    node: NodeId,
    _not_send: PhantomData<*const ()>,
}

impl Remeasurement {
    pub(crate) fn new(tree: u32, node: NodeId) -> Remeasurement {
        Remeasurement { tree, node, _not_send: PhantomData }
    }

    /// Measures the node now, with its last constraints, even if nothing asked, as
    /// `forceRemeasure`: its measurement and its content's placement are done when this
    /// returns, if the tree is free (between frames, a task, a side effect). Where the tree is
    /// busy (in a pass, a callback, an event handler), the node is measured in the next layout
    /// instead, before anything is drawn.
    pub fn force_remeasure(&self) {
        let node = self.node;
        let ran = crate::node_context::with_free_tree(self.tree, &mut |tree: &mut UiTree| {
            tree.remeasure_now(node);
        });
        if !ran && let Some(requests) = crate::node_context::requests_of(self.tree) {
            requests.measure_requests.borrow_mut().push(node);
            requests.wake();
        }
    }
}

/// An element given its layout node's `Remeasurement`, as Compose's `RemeasurementModifier`:
/// `on_remeasurement_available` is called when it's attached, and when a new one replaces it
/// on an attached node. Added with `Modifier.remeasurement_modifier`. A closure taking a
/// `Remeasurement` is one.
pub trait RemeasurementModifier: 'static {
    /// The layout node's `Remeasurement`, to keep. Called as the composition applies its
    /// changes: keep it, don't write state.
    fn on_remeasurement_available(&self, remeasurement: Remeasurement);
}

impl<F: Fn(Remeasurement) + 'static> RemeasurementModifier for F {
    fn on_remeasurement_available(&self, remeasurement: Remeasurement) {
        self(remeasurement)
    }
}

impl Modifier {
    /// Gives `modifier` its layout node's `Remeasurement`, as a `RemeasurementModifier` in a
    /// modifier chain. Elements compare by the `Rc`'s identity: keep one across recompositions.
    pub fn remeasurement_modifier(self, modifier: Rc<dyn RemeasurementModifier>) -> Modifier {
        self.then_custom(RemeasurementElement(modifier))
    }
}

struct RemeasurementElement(Rc<dyn RemeasurementModifier>);

struct RemeasurementNode {
    modifier: Rc<dyn RemeasurementModifier>,
    /// Its `Remeasurement`, while attached: what a new modifier is given at once.
    remeasurement: Option<Remeasurement>,
}

impl ModifierNodeElement for RemeasurementElement {
    type Node = RemeasurementNode;
    fn create(&self) -> RemeasurementNode {
        RemeasurementNode { modifier: self.0.clone(), remeasurement: None }
    }
    fn update(&self, node: &mut RemeasurementNode) {
        node.modifier = self.0.clone();
        if let Some(remeasurement) = node.remeasurement {
            node.modifier.on_remeasurement_available(remeasurement);
        }
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
    fn name(&self) -> &'static str {
        "RemeasurementModifier"
    }
}

impl ModifierNode for RemeasurementNode {
    fn should_auto_invalidate(&self) -> bool {
        false
    }
    fn on_attach(&mut self, context: &mut NodeContext<'_>) {
        let remeasurement = context.remeasurement();
        self.remeasurement = Some(remeasurement);
        self.modifier.on_remeasurement_available(remeasurement);
    }
    fn on_detach(&mut self, _: &mut NodeContext<'_>) {
        self.remeasurement = None;
    }
}
