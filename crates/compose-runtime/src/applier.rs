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
/// The runtime calls it during a pass (insertions, moves and removals, top-down, as composition
/// reaches them) and at the end of the pass (releases), never while the runtime itself is busy.
/// During the pass an applier runs as part of composition: a state it writes then is a write
/// during composition (allowed, as in Compose, D703: a composable that read it before
/// recomposes in the next pass). Releases run after composition.
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

    /// `node` is about to be reused by content composed in its place (`ReusableContent` with
    /// another key, `ReusableContentHost` active again), as Compose's `Applier.reuse` (which
    /// calls the node's `ComposeNodeLifecycleCallback.onReuse`): the applier resets what the
    /// node keeps of its last content, and the pass then updates it. Called during the pass, as
    /// composition reaches the node, for each node a reusable node group keeps
    /// (`start_reusable_node`): composition is running, so the applier must not run code that
    /// writes state then; it may defer such work to `on_end_changes` (`UiTree` resets its
    /// modifier nodes there, D434).
    fn reuse(&mut self, _node: NodeRef) {}

    /// `node` is about to be reused in paused content (D591): returns whether the applier
    /// reused it now (as [`reuse`](Applier::reuse), before the pass updates it, as unpaused:
    /// a node still shown, whose reset nothing waits for), or leaves it to `reuse` at the
    /// paused composition's `apply` (what showing it again does: a deactivated node's). By
    /// default, at `apply`.
    fn reuse_paused(&mut self, _node: NodeRef) -> bool {
        false
    }

    /// `node`'s content was deactivated (`ReusableContentHost` inactive), as Compose's
    /// `ComposeNodeLifecycleCallback.onDeactivate`: it stays in the tree, its groups kept for
    /// reuse, and the applier resets what it keeps (its modifiers' state) and leaves it out of
    /// what it shows. Called at the end of the pass, before releases, for every node of the
    /// deactivated content, outer before inner.
    fn deactivate(&mut self, _node: NodeRef) {}

    /// Makes `node` a node of the tree in no parent yet, and returns its handle: a node a paused
    /// composition creates (content in a [`PausableContent`]), which it attaches only when it's
    /// applied ([`attach`](Applier::attach)), as Compose's paused composition creates its nodes
    /// as it composes and inserts them into the tree when applied (`RecordingApplier`). The
    /// pass updates it meanwhile. An applier that composes no pausable content needn't
    /// implement it.
    ///
    /// [`PausableContent`]: crate::platform::PausableContent
    fn create(&mut self, _node: Self::Node) -> NodeRef {
        panic!(
            "compose-runtime: this applier doesn't create nodes apart (Applier::create): it can't \
             compose PausableContent in a paused composition"
        )
    }

    /// Inserts `node`, made by [`create`](Applier::create) and in no parent, as the child of
    /// `parent` at `index`: a paused composition's node, as it's applied.
    fn attach(&mut self, _parent: NodeRef, _index: usize, _node: NodeRef) {
        panic!("compose-runtime: this applier doesn't attach nodes made apart (Applier::attach)")
    }

    /// The node `node` refers to, for [`ComposeNode`]'s [`Updater`] to update, as Compose's
    /// `Applier.current`. An applier whose nodes aren't updated that way needn't implement it.
    ///
    /// [`ComposeNode`]: crate::ComposeNode
    /// [`Updater`]: crate::Updater
    fn node_mut(&mut self, _node: NodeRef) -> &mut Self::Node {
        panic!(
            "compose-runtime: this applier doesn't lend its nodes (Applier::node_mut): \
             ComposeNode's Updater can't update them"
        )
    }

    /// Whether `node` is in the tree, by its generation: a composition sharing this applier may be
    /// disposed after the node it's under was released, with its nodes, and the node's place given
    /// to another. An applier that composes no composition below a node of a composition's needn't
    /// implement it.
    fn contains(&self, _node: NodeRef) -> bool {
        true
    }

    /// Called before a pass applies its first change.
    fn on_begin_changes(&mut self) {}

    /// Called after a pass applied its last change and released its removed nodes.
    fn on_end_changes(&mut self) {}

    /// A paused composition was applied (before that apply's `on_end_changes`) or discarded:
    /// what the applier deferred while its content resumed (a node reused with
    /// [`reuse_paused`](Applier::reuse_paused), whose new content is only complete now) is due.
    fn on_paused_end(&mut self) {}
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
    fn reuse(&mut self, node: NodeRef);
    fn reuse_paused(&mut self, node: NodeRef) -> bool;
    fn deactivate(&mut self, node: NodeRef);
    /// Takes the node out of `node`, as `insert_erased`, and makes it in no parent.
    fn create_erased(&mut self, node: &mut dyn Any) -> Option<NodeRef>;
    fn attach(&mut self, parent: NodeRef, index: usize, node: NodeRef);
    fn contains(&self, node: NodeRef) -> bool;
    fn on_begin_changes(&mut self);
    /// `nested`: with a pass composing around it, whose changes may be of this tree.
    fn on_end_changes(&mut self, nested: bool);
    /// A pass that won't end its changes, by a panic.
    fn abandon_changes(&mut self);
    fn on_paused_end(&mut self);
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// An applier as compositions share it: a pass nested in another of the tree doesn't end its
/// changes while that one composes, with its nodes in the tree (D31). With none composing, a pass's
/// work ends them for real, before its effects (rule R1), beginning them again for the next
/// composition's work in the frame.
pub(crate) struct SharedApplier<A> {
    applier: A,
    changing: u32,
}

impl<A> SharedApplier<A> {
    pub(crate) fn new(applier: A) -> SharedApplier<A> {
        SharedApplier { applier, changing: 0 }
    }
}

impl<A: Applier> ErasedApplier for SharedApplier<A> {
    fn insert_erased(
        &mut self,
        parent: NodeRef,
        index: usize,
        node: &mut dyn Any,
    ) -> Option<NodeRef> {
        let node = node.downcast_mut::<Option<A::Node>>()?.take()?;
        Some(Applier::insert(&mut self.applier, parent, index, node))
    }

    fn remove(&mut self, parent: NodeRef, index: usize, count: usize) {
        Applier::remove(&mut self.applier, parent, index, count)
    }

    fn move_nodes(&mut self, parent: NodeRef, from: usize, to: usize, count: usize) {
        Applier::move_nodes(&mut self.applier, parent, from, to, count)
    }

    fn release(&mut self, node: NodeRef) {
        Applier::release(&mut self.applier, node)
    }

    fn reuse(&mut self, node: NodeRef) {
        Applier::reuse(&mut self.applier, node)
    }

    fn reuse_paused(&mut self, node: NodeRef) -> bool {
        Applier::reuse_paused(&mut self.applier, node)
    }

    fn deactivate(&mut self, node: NodeRef) {
        Applier::deactivate(&mut self.applier, node)
    }

    fn create_erased(&mut self, node: &mut dyn Any) -> Option<NodeRef> {
        let node = node.downcast_mut::<Option<A::Node>>()?.take()?;
        Some(Applier::create(&mut self.applier, node))
    }

    fn attach(&mut self, parent: NodeRef, index: usize, node: NodeRef) {
        Applier::attach(&mut self.applier, parent, index, node)
    }

    fn contains(&self, node: NodeRef) -> bool {
        Applier::contains(&self.applier, node)
    }

    fn on_begin_changes(&mut self) {
        if self.changing == 0 {
            Applier::on_begin_changes(&mut self.applier)
        }
        self.changing += 1;
    }

    fn on_end_changes(&mut self, nested: bool) {
        if nested && self.changing > 1 {
            self.changing -= 1;
            return;
        }
        // What's composed of the frame's others has its work's turn after this.
        if self.changing == 0 {
            Applier::on_begin_changes(&mut self.applier)
        }
        self.changing = 0;
        Applier::on_end_changes(&mut self.applier)
    }

    fn abandon_changes(&mut self) {
        self.changing = self.changing.saturating_sub(1);
    }

    fn on_paused_end(&mut self) {
        Applier::on_paused_end(&mut self.applier)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        &mut self.applier
    }
}
