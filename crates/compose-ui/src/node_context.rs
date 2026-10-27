//! What a modifier node reaches of its tree, as Compose's `DelegatableNode` extensions: a
//! `NodeContext` while its code runs (its lifecycle callbacks), a `NodeHandle` afterwards (a
//! task, a callback, a stored closure). docs/parity/design/modifier-nodes.md §3.3, docs/06-ui.md
//! "Modifiers".

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::BuildHasherDefault;
use std::marker::PhantomData;
use std::rc::{Rc, Weak};

use crate::alignment::LayoutDirection;
use crate::focus_requester::FocusRequests;
use crate::modifier::ModifierNode;
use crate::node_chain::{EntryId, entry_state};
use crate::tree::{NodeId, UiTree};
use crate::unit::Density;

/// What a node asks its tree to do again, as the `invalidate*` functions of Compose's node
/// interfaces.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Invalidation {
    /// `DrawModifierNode.invalidateDraw`: what the node draws in draws again.
    Draw,
    /// `LayoutModifierNode.invalidateMeasurement`.
    Measurement,
    /// `LayoutModifierNode.invalidatePlacement` (a measurement until placement is a pass of its
    /// own).
    Placement,
    /// `LayoutModifierNode.invalidateLayer`: the graphics layer's properties.
    Layer,
    /// `SemanticsModifierNode.invalidateSemantics`.
    Semantics,
    /// `ParentDataModifierNode.invalidateParentData`.
    ParentData,
    /// `DelegatableNode.invalidateSubtree`.
    Subtree,
    /// `DelegatableNode.invalidateMeasurementForSubtree`.
    MeasurementForSubtree,
    /// `DelegatableNode.invalidateDrawForSubtree`.
    DrawForSubtree,
}

/// What a node's callback reaches: its own place in the tree and the tree's services, as
/// Compose's `DelegatableNode` and its extension functions. Handed to `ModifierNode::on_attach`,
/// `on_detach`, `on_reset`, `on_density_change` and `on_layout_direction_change`; the node runs
/// out of its chain meanwhile, so through it the node reaches the rest of the tree, never
/// itself.
pub struct NodeContext<'a> {
    pub(crate) tree: &'a mut UiTree,
    pub(crate) node: NodeId,
    /// The entry's index in its chain while the callback runs; `None` for an entry out of its
    /// chain (removed by a chain change, detached before it's released).
    pub(crate) index: Option<usize>,
    pub(crate) entry: EntryId,
    /// The `entry_state` flags of an entry out of its chain.
    pub(crate) out_state: u8,
}

impl NodeContext<'_> {
    fn state(&self) -> u8 {
        match self.index {
            Some(index) => self.tree.node(self.node).chain[index].state,
            None => self.out_state,
        }
    }

    fn state_mut(&mut self) -> &mut u8 {
        match self.index {
            Some(index) => &mut self.tree.node_mut(self.node).chain[index].state,
            None => &mut self.out_state,
        }
    }

    /// A handle naming this node from outside its callbacks.
    pub fn handle(&mut self) -> NodeHandle {
        let attached = self.is_attached();
        let state = self.state_mut();
        if *state & entry_state::HAS_HANDLE == 0 {
            *state |= entry_state::HAS_HANDLE;
            if attached {
                self.tree.focus_requests.attached_entries.borrow_mut().insert(self.entry.0);
            }
        }
        NodeHandle {
            tree: self.tree.key,
            node: self.node,
            entry: self.entry,
            _not_send: PhantomData,
        }
    }

    /// The layout node the modifier node is on.
    pub fn layout_node(&self) -> NodeId {
        self.node
    }

    /// Whether the node is attached, as `Modifier.Node.isAttached`: its layout node is in the
    /// tree and active (not content kept for reuse).
    pub fn is_attached(&self) -> bool {
        self.state() & entry_state::ATTACHED != 0
    }

    /// The node's coroutine scope, as `Modifier.Node.coroutineScope`: made on first use,
    /// cancelled when the node is detached (a node attached again gets a new one). Its tasks run
    /// between frames or in a frame's task step, when the tree is free.
    ///
    /// Panics if the node isn't attached, as Compose's (`requireOwner`).
    pub fn coroutine_scope(&mut self) -> compose_runtime::CoroutineScope {
        assert!(self.is_attached(), "compose-ui: a node's coroutine scope while it isn't attached");
        *self.state_mut() |= entry_state::HAS_SCOPE;
        let scope =
            self.tree.scopes.entry(self.entry).or_insert_with(compose_runtime::CoroutineScope::new);
        **scope
    }

    /// Runs `effect` once the tree applied the changes under way, as `Modifier.Node.sideEffect`
    /// (`onEndApplyChanges`): after the pass's lifecycle callbacks, as soon as the host has
    /// released the tree after the composition pass, before the frame's layout, so the effect
    /// may write state the layout reads and use handles (`NodeHandle::with_node`). Asked
    /// outside a pass (from a task), it asks for a frame, which runs it.
    pub fn side_effect(&mut self, effect: impl FnOnce() + 'static) {
        self.tree.side_effects.push(Box::new(effect));
        self.tree.focus_requests.wake();
    }

    /// The density, as `requireDensity()`.
    pub fn require_density(&self) -> Density {
        self.tree.density
    }

    /// The layout node's layout direction, as `requireLayoutDirection()`.
    pub fn require_layout_direction(&self) -> LayoutDirection {
        self.tree.node(self.node).layout_direction
    }

    /// The text measurer, for a node that lays text out or gives paragraphs back.
    pub fn text_measurer(&mut self) -> &mut dyn crate::text::TextMeasurer {
        self.tree.text_measurer()
    }

    fn invalidate(&mut self, what: Invalidation) {
        // As Compose's: a detached node's invalidations do nothing.
        if self.is_attached() {
            self.tree.invalidate_entry_at(self.node, self.index, what);
        }
    }

    /// Draws again what the node draws in, as `DrawModifierNode.invalidateDraw`.
    pub fn invalidate_draw(&mut self) {
        self.invalidate(Invalidation::Draw);
    }

    /// Measures the node's layout node again, as `invalidateMeasurement`.
    pub fn invalidate_measurement(&mut self) {
        self.invalidate(Invalidation::Measurement);
    }

    /// Places the node's layout node again, as `invalidatePlacement` (compose-rs places as it
    /// measures: a measurement).
    pub fn invalidate_placement(&mut self) {
        self.invalidate(Invalidation::Placement);
    }

    /// Updates the properties of the graphics layer the node is in, as `invalidateLayer`.
    pub fn invalidate_layer(&mut self) {
        self.invalidate(Invalidation::Layer);
    }

    /// The node's semantics changed, as `invalidateSemantics`.
    pub fn invalidate_semantics(&mut self) {
        self.invalidate(Invalidation::Semantics);
    }

    /// The node's parent data changed, as `invalidateParentData`.
    pub fn invalidate_parent_data(&mut self) {
        self.invalidate(Invalidation::ParentData);
    }

    /// Measures, places and draws the layout node and everything below it again, as
    /// `invalidateSubtree`.
    pub fn invalidate_subtree(&mut self) {
        self.invalidate(Invalidation::Subtree);
    }

    /// Measures the layout node and everything below it again, as
    /// `invalidateMeasurementForSubtree`.
    pub fn invalidate_measurement_for_subtree(&mut self) {
        self.invalidate(Invalidation::MeasurementForSubtree);
    }

    /// Draws the layout node and everything below it again, as `invalidateDrawForSubtree`.
    pub fn invalidate_draw_for_subtree(&mut self) {
        self.invalidate(Invalidation::DrawForSubtree);
    }
}

/// A modifier node, named from outside its callbacks (a task, a callback, a stored closure):
/// `Copy`, `'static`, `!Send`, and never stale: a node gone answers `false` or `None`, and what
/// it asks is dropped. Its invalidations are queued and applied where the tree applies its
/// nodes' requests (a frame's start, the end of a composition pass, each layout round), so one
/// asked before a frame's layout is in that frame.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeHandle {
    tree: u32,
    node: NodeId,
    entry: EntryId,
    _not_send: PhantomData<*const ()>,
}

/// A tree's entry in the thread's registry: its requests, and how a handle borrows it.
struct Registration {
    requests: Weak<FocusRequests>,
    /// Runs a function with the tree if it's free; set by its host.
    borrow: Option<Rc<TreeBorrow>>,
}

type TreeBorrow = dyn Fn(&mut dyn FnMut(&mut UiTree)) -> bool;

thread_local! {
    // FNV, not SipHash: looked up for each request a handle makes (an indication's each
    // interaction), by a counter no one chooses.
    static TREES: RefCell<HashMap<u32, Registration, BuildHasherDefault<compose_runtime::KeyHasher>>> =
        RefCell::new(HashMap::default());
    static NEXT_TREE: Cell<u32> = const { Cell::new(0) };
}

/// Registers a new tree, whose requests are `requests`: its key.
pub(crate) fn register_tree(requests: &Rc<FocusRequests>) -> u32 {
    let key = NEXT_TREE.with(|next| {
        next.set(next.get() + 1);
        next.get()
    });
    let registration = Registration { requests: Rc::downgrade(requests), borrow: None };
    TREES.with(|trees| trees.borrow_mut().insert(key, registration));
    key
}

/// Forgets a tree being dropped.
pub(crate) fn unregister_tree(key: u32) {
    // Not at all once the thread's registry is gone (a tree dropped as the thread exits).
    let _ = TREES.try_with(|trees| trees.borrow_mut().remove(&key));
}

/// Sets how handles borrow the tree of `key` (its host's composition holds it).
pub(crate) fn set_tree_borrow(key: u32, borrow: Rc<TreeBorrow>) {
    TREES.with(|trees| {
        if let Some(registration) = trees.borrow_mut().get_mut(&key) {
            registration.borrow = Some(borrow);
        }
    });
}

fn requests_of(key: u32) -> Option<Rc<FocusRequests>> {
    TREES.with(|trees| trees.borrow().get(&key)?.requests.upgrade())
}

impl NodeHandle {
    /// Whether the node is attached, as `isAttached`.
    pub fn is_attached(&self) -> bool {
        requests_of(self.tree)
            .is_some_and(|requests| requests.attached_entries.borrow().contains(&self.entry.0))
    }

    fn request(&self, what: Invalidation) {
        let Some(requests) = requests_of(self.tree) else { return };
        if !requests.attached_entries.borrow().contains(&self.entry.0) {
            // Detached, as Compose's invalidations of a detached node: nothing.
            return;
        }
        requests.node_invalidations.borrow_mut().push((self.node, self.entry, what));
        requests.wake();
    }

    /// As `DrawModifierNode.invalidateDraw`.
    pub fn invalidate_draw(&self) {
        self.request(Invalidation::Draw);
    }

    /// As `LayoutModifierNode.invalidateMeasurement`.
    pub fn invalidate_measurement(&self) {
        self.request(Invalidation::Measurement);
    }

    /// As `LayoutModifierNode.invalidatePlacement`.
    pub fn invalidate_placement(&self) {
        self.request(Invalidation::Placement);
    }

    /// As `LayoutModifierNode.invalidateLayer`.
    pub fn invalidate_layer(&self) {
        self.request(Invalidation::Layer);
    }

    /// As `SemanticsModifierNode.invalidateSemantics`.
    pub fn invalidate_semantics(&self) {
        self.request(Invalidation::Semantics);
    }

    /// As `ParentDataModifierNode.invalidateParentData`.
    pub fn invalidate_parent_data(&self) {
        self.request(Invalidation::ParentData);
    }

    /// As `invalidateSubtree`.
    pub fn invalidate_subtree(&self) {
        self.request(Invalidation::Subtree);
    }

    /// As `invalidateMeasurementForSubtree`.
    pub fn invalidate_measurement_for_subtree(&self) {
        self.request(Invalidation::MeasurementForSubtree);
    }

    /// As `invalidateDrawForSubtree`.
    pub fn invalidate_draw_for_subtree(&self) {
        self.request(Invalidation::DrawForSubtree);
    }

    /// Runs `f` with the node, as an `N`, and its context, now, if the tree is free (a task
    /// resuming between frames or in a frame's task step, a side effect). `None` if the node is
    /// gone, isn't an `N`, or its tree is busy: inside a composition pass, a node's callback,
    /// an event handler or anything else running while the host holds the tree (there, a
    /// node's own context is at hand; another node is reached by invalidating it, which is
    /// queued).
    pub fn with_node<N: ModifierNode, R>(
        &self,
        f: impl FnOnce(&mut N, &mut NodeContext<'_>) -> R,
    ) -> Option<R> {
        let borrow = TREES.with(|trees| trees.borrow().get(&self.tree)?.borrow.clone())?;
        let mut f = Some(f);
        let mut result = None;
        let (node, entry) = (self.node, self.entry);
        borrow(&mut |tree: &mut UiTree| {
            let Some(index) = tree.entry_index(node, entry) else { return };
            let Some(f) = f.take() else { return };
            result = tree.with_entry(node, index, |modifier_node, context| {
                modifier_node.downcast_mut::<N>().map(|typed| f(typed, context))
            });
        });
        result
    }
}
