//! The interface between the runtime and a tree of nodes.

use std::any::Any;

/// A handle to a node in an applier's tree. The applier decides what the bits mean.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeRef(u64);

impl NodeRef {
    /// No node: what a node group holds before its node is inserted.
    pub const NONE: NodeRef = NodeRef(u64::MAX);

    pub const fn new(bits: u64) -> NodeRef {
        NodeRef(bits)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub const fn is_none(self) -> bool {
        self.0 == u64::MAX
    }
}

/// Applies the composition's node changes to a tree, as Compose's `Applier<N>`.
///
/// The runtime calls it during a pass (insertions, moves and removals, top-down) and at the end
/// of a pass (releases). It never calls it while the runtime is busy, so an applier may read and
/// write state, and the nodes it drops may own state.
pub trait Applier: 'static {
    /// The type of the nodes the composition emits.
    type Node: 'static;

    /// The node the composition's top-level nodes are inserted into.
    fn root(&self) -> NodeRef;

    /// Inserts `node` as the child of `parent` at `index`, and returns its handle.
    fn insert(&mut self, parent: NodeRef, index: usize, node: Self::Node) -> NodeRef;

    /// Detaches the `count` children of `parent` starting at `index`. They stay alive until
    /// [`release`](Applier::release), at the end of the pass.
    fn remove(&mut self, parent: NodeRef, index: usize, count: usize);

    /// Moves `count` children of `parent` from `from` to `to`, where `to` is an index in the list
    /// before the move, as Compose's `Applier.move` (`MutableList.move`).
    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize);

    /// Drops a node removed by the composition, and its subtree. Called at the end of the pass
    /// that removed it, before the state of the group that owned it (rule R1). When a poisoned
    /// composition is dropped, it may be called on a node still attached to its parent: the
    /// applier then detaches it first.
    fn release(&mut self, node: NodeRef);

    /// Called before a pass applies its first change.
    fn on_begin_changes(&mut self) {}

    /// Called after a pass applied its last change and released its removed nodes.
    fn on_end_changes(&mut self) {}
}

/// The applier, with its node type erased so the runtime stays non-generic.
pub(crate) trait ErasedApplier {
    /// Takes the node out of `node` (an `&mut Option<Self::Node>`) and inserts it; `None` if the
    /// node has the wrong type.
    fn insert_erased(
        &mut self,
        parent: NodeRef,
        index: usize,
        node: &mut dyn Any,
    ) -> Option<NodeRef>;
    fn remove(&mut self, parent: NodeRef, index: usize, count: usize);
    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize);
    fn release(&mut self, node: NodeRef);
    fn on_begin_changes(&mut self);
    fn on_end_changes(&mut self);
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<A: Applier> ErasedApplier for A {
    fn insert_erased(
        &mut self,
        parent: NodeRef,
        index: usize,
        node: &mut dyn Any,
    ) -> Option<NodeRef> {
        let node = node.downcast_mut::<Option<A::Node>>()?.take()?;
        Some(Applier::insert(self, parent, index, node))
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        Applier::remove(self, parent, index, count)
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        Applier::move_nodes(self, parent, from, to, count)
    }

    fn release(&mut self, node: NodeRef) {
        Applier::release(self, node)
    }

    fn on_begin_changes(&mut self) {
        Applier::on_begin_changes(self)
    }

    fn on_end_changes(&mut self) {
        Applier::on_end_changes(self)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
