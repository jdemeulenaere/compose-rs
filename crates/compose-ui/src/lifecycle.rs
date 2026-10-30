//! Modifier nodes' lifecycle, as Compose's `Modifier.Node` (`onAttach`, `onDetach`, `onReset`,
//! D1153): queued while a composition pass applies its changes, run in Compose's orders when it
//! ends; and the requests nodes make through their contexts and handles
//! (docs/parity/design/modifier-nodes.md §3.3–3.5, docs/06-ui.md "Modifiers").

use std::mem;
use std::panic::{self, AssertUnwindSafe};

use smallvec::SmallVec;

use crate::delegation::{self, detach_with_delegates};
use crate::modifier::{ModifierNode, ReleaseContext};
use crate::node_chain::{ChainEntry, EntryId, NodeKinds, entry_state, traverse_key_of};
use crate::node_context::{Invalidation, NodeContext};
use crate::nodes::Taken;
use crate::tree::{NodeId, UiTree};
use crate::tree_access::TreeAccess;

thread_local! {
    /// A default lifecycle callback of `ModifierNode` ran (debug builds' check of built-ins).
    static DEFAULT_CALLBACK_RAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The node types and callbacks checked so far.
    static CHECKED_DEFAULTS: std::cell::RefCell<std::collections::HashSet<(std::any::TypeId, usize)>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

/// Called by `ModifierNode`'s default `on_attach`, `on_detach`, `on_density_change` and
/// `on_layout_direction_change`: what `check_default_callback` reads, in debug builds.
#[inline]
pub(crate) fn default_callback_ran() {
    if cfg!(debug_assertions) {
        DEFAULT_CALLBACK_RAN.set(true);
    }
}

/// A lifecycle step queued in a pass, run when it ends (`UiTree::run_lifecycle`), in order.
pub(crate) enum Lifecycle {
    /// A subtree attached in the pass (inserted, or a paused composition's node attached):
    /// its nodes' `on_attach`, children before parents, head to tail in each chain.
    Attach(NodeId),
    /// New entries of a node attached before the pass (a chain change): their `on_attach`.
    AttachEntries(NodeId),
    /// A subtree removed in the pass: its nodes' `on_detach`, parents before children, tail to
    /// head in each chain (if it wasn't released first: `UiTree::release` detaches it then).
    Detach(NodeId),
    /// Entries a chain change removed from a node, each with where it was in the new chain:
    /// `on_detach` (those attached), then `on_release`.
    Removed(NodeId, SmallVec<[(ChainEntry, usize); 2]>),
    /// A node reused for other content: its nodes' `on_reset`, `on_detach`, then `on_attach`.
    Reuse(NodeId),
    /// A node a paused composition's resume reused: its nodes' `on_reset` and `on_detach`, at
    /// the resume's end (before the content its later resumes compose); their `on_attach` waits
    /// for the apply (`AttachEntries`, held apart, D1162).
    Reset(NodeId),
    /// An element's update delegated or undelegated: the delegates brought up to date.
    SyncDelegates(NodeId, EntryId),
    /// A node's layout direction changed: its nodes' `on_layout_direction_change`.
    LayoutDirection(NodeId),
    /// A node's density changed: its nodes' `on_density_change`, then a pointer input node's
    /// `on_cancel_pointer_input`, as `PointerInputModifierNode.onDensityChange`'s default.
    Density(NodeId),
    /// The view configuration where a node is changed: its nodes'
    /// `on_view_configuration_change`, then the same.
    ViewConfiguration(NodeId),
    /// What a node reads composition locals from changed: its consumers are told.
    CompositionLocals(NodeId),
}

/// A chain entry's node out of its chain, its window yet to open: goes back when dropped.
pub(crate) struct EntryOut<'a> {
    pub(crate) access: TreeAccess<'a>,
    node: NodeId,
    index: usize,
    pub(crate) entry: Box<dyn crate::modifier::ModifierNode>,
}

impl Drop for EntryOut<'_> {
    fn drop(&mut self) {
        let entry = mem::replace(&mut self.entry, Box::new(Taken));
        let (node, index) = (self.node, self.index);
        let mut tree = self.access.enter();
        if let Some(slot) = tree.get_mut(node).and_then(|value| value.chain.get_mut(index)) {
            // `Taken` has no size and nothing to drop.
            mem::forget(mem::replace(&mut slot.node, entry));
        }
    }
}

impl UiTree {
    /// Runs `f` with entry `index` of `node`'s chain and its context, the entry out of its chain
    /// meanwhile (so its code may reach the rest of the tree, never itself); put back even if
    /// `f` panics, the panic resumed then.
    pub(crate) fn with_entry<R>(
        &mut self,
        node: NodeId,
        index: usize,
        f: impl FnOnce(&mut dyn ModifierNode, &mut NodeContext<'_>) -> R,
    ) -> R {
        let (result, delegating) = self.run_entry(node, index, f);
        // What it delegated or undelegated.
        if delegating {
            if self.delegates_wait {
                self.note_delegates_changed(node, index);
            } else {
                self.sync_delegates(node, index);
            }
        }
        // What's waited for every node to be back in its chain.
        if self.entries_running == 0 && !self.delegates_wait && !self.delegates_noted.is_empty() {
            self.sync_noted_delegates();
        }
        self.drain_remeasures();
        result
    }

    /// Measures what `remeasure_sync` asked, if no node's code is running any more: measuring
    /// one may run any node of its chain, so all must be back in their chains (a traversal runs
    /// the nodes it finds inside the code of the one that asked).
    pub(crate) fn drain_remeasures(&mut self) {
        if self.entries_running == 0 && !self.remeasure_after.is_empty() {
            for node in mem::take(&mut self.remeasure_after) {
                self.remeasure_now(node);
            }
        }
    }

    /// `with_entry` without what follows a node's code.
    pub(crate) fn with_entry_raw<R>(
        &mut self,
        node: NodeId,
        index: usize,
        f: impl FnOnce(&mut dyn ModifierNode, &mut NodeContext<'_>) -> R,
    ) -> R {
        self.run_entry(node, index, f).0
    }

    /// Runs `f` as `with_entry_raw`, and says whether the entry may have delegates. Each event's
    /// handlers come through here (a dozen calls for a pointer event, in `scroll a pixel`'s
    /// frames): it looks its node up as few times as it can.
    #[inline]
    fn run_entry<R>(
        &mut self,
        node: NodeId,
        index: usize,
        f: impl FnOnce(&mut dyn ModifierNode, &mut NodeContext<'_>) -> R,
    ) -> (R, bool) {
        let entry = &mut self.node_mut(node).chain[index];
        let id = entry.id;
        let own = entry.own;
        let delegating = entry.state & entry_state::DELEGATING != 0;
        let mut taken = mem::replace(&mut entry.node, Box::new(Taken));
        let traverse_key = traverse_key_of(&mut *taken, own);
        self.entries_running += 1;
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut context = NodeContext {
                tree: TreeAccess::new(&mut *self),
                node,
                index: Some(index),
                entry: id,
                out_state: 0,
                delegated: false,
                place: index,
                traverse_key,
            };
            let _open = context.tree.open();
            f(&mut *taken, &mut context)
        }));
        self.entries_running -= 1;
        if let Some(value) = self.get_mut(node)
            && let Some(entry) = value.chain.get_mut(index)
        {
            // `Taken` has no size and nothing to drop.
            mem::forget(mem::replace(&mut entry.node, taken));
        }
        match result {
            Ok(result) => (result, delegating),
            Err(panic) => panic::resume_unwind(panic),
        }
    }

    /// `with_entry` for the entry or delegate `id` that `entry_index` found at `index`.
    pub(crate) fn with_entry_or_delegate<R>(
        &mut self,
        node: NodeId,
        index: usize,
        id: EntryId,
        f: impl FnOnce(&mut dyn ModifierNode, &mut NodeContext<'_>) -> R,
    ) -> R {
        self.with_entry(node, index, |modifier_node, context| {
            match delegation::find_delegate(modifier_node, id) {
                Some(delegate) => context.with_delegate(delegate, f),
                None => f(modifier_node, context),
            }
        })
    }

    /// The index in `node`'s chain of entry `entry`, or of the entry `entry` is a delegate of, if
    /// it's there.
    pub(crate) fn entry_index(&mut self, node: NodeId, entry: EntryId) -> Option<usize> {
        let chain = &mut self.get_mut(node)?.chain;
        if let Some(index) = chain.iter().position(|candidate| candidate.id == entry) {
            return Some(index);
        }
        chain.iter_mut().position(|candidate| {
            candidate.state & entry_state::DELEGATING != 0
                && delegation::find_delegate(&mut *candidate.node, entry).is_some()
        })
    }

    /// The `entry_state` flags of entry `id` at `index` of `node`'s chain, or of its delegate `id`.
    pub(crate) fn entry_or_delegate_state(
        &mut self,
        node: NodeId,
        index: usize,
        id: EntryId,
    ) -> u8 {
        let entry = &mut self.node_mut(node).chain[index];
        if entry.id == id {
            return entry.state;
        }
        delegation::find_delegate(&mut *entry.node, id).map_or(0, |delegate| delegate.state())
    }

    /// Runs `callback` on `node`'s user-defined entries whose state has every flag of `with`,
    /// from the tail if `tail_first`. The built-in elements' nodes have no attach, detach,
    /// density or layout direction callbacks: they aren't called (most of a first frame's
    /// entries).
    fn for_custom_entries(
        &mut self,
        node: NodeId,
        with: u8,
        tail_first: bool,
        callback: fn(&mut dyn ModifierNode, &mut NodeContext<'_>),
    ) {
        let Some(value) = self.get(node) else { return };
        if !cfg!(debug_assertions) && !value.kinds.any(NodeKinds::OBSERVED) {
            return;
        }
        let count = value.chain.len();
        for step in 0..count {
            let index = if tail_first { count - 1 - step } else { step };
            let Some(value) = self.get(node) else { return };
            let Some(entry) = value.chain.get(index) else { continue };
            if entry.state & with != with {
                continue;
            }
            if entry.kinds.any(NodeKinds::OBSERVED) {
                self.with_entry(node, index, callback);
            } else {
                self.check_default_callback(node, index, callback);
            }
        }
    }

    /// Debug builds: a built-in element's node, whose callback the tree skips, has only the
    /// default one (`ModifierNode`'s, which says it ran).
    fn check_default_callback(
        &mut self,
        node: NodeId,
        index: usize,
        callback: fn(&mut dyn ModifierNode, &mut NodeContext<'_>),
    ) {
        if cfg!(debug_assertions) {
            // Once per node type and callback: cheap enough to leave debug builds' timings be.
            let Some(entry) = self.get(node).and_then(|value| value.chain.get(index)) else {
                return;
            };
            let key = ((*entry.node).type_id(), callback as usize);
            if !CHECKED_DEFAULTS.with(|checked| checked.borrow_mut().insert(key)) {
                return;
            }
            DEFAULT_CALLBACK_RAN.set(false);
            self.with_entry(node, index, callback);
            assert!(
                DEFAULT_CALLBACK_RAN.get(),
                "compose-ui: a built-in element's node overrides a lifecycle callback the tree \
                 skips for built-ins (lifecycle.rs `for_custom_entries`)"
            );
        }
    }

    /// Marks `node`'s entries attached, as `markAsAttached`, if it's in the tree and active.
    fn mark_attached(&mut self, node: NodeId) {
        self.forget_semantics(node);
        let requests = self.focus_requests.clone();
        let Some(value) = self.get_mut(node) else { return };
        if !value.attached || value.deactivated || value.reattach_pending {
            return;
        }
        for entry in &mut value.chain {
            if entry.state & entry_state::ATTACHED == 0 {
                entry.state |= entry_state::ATTACHED;
                if entry.state & entry_state::HAS_HANDLE != 0 {
                    requests.attached_entries.borrow_mut().insert(entry.id.0);
                }
            }
            if entry.state & entry_state::DELEGATING != 0 {
                delegation::mark_delegates_attached(&mut *entry.node, &requests);
            }
        }
    }

    /// Runs the `on_attach` of `node`'s attached entries that haven't run it, head to tail, as
    /// `runAttachLifecycle`.
    fn run_attach(&mut self, node: NodeId) {
        let count = self.get(node).map_or(0, |value| value.chain.len());
        for index in 0..count {
            let Some(entry) = self.get_mut(node).and_then(|value| value.chain.get_mut(index))
            else {
                return;
            };
            if entry.state & (entry_state::ATTACHED | entry_state::ATTACH_RAN)
                != entry_state::ATTACHED
            {
                continue;
            }
            let delegating = entry.state & entry_state::DELEGATING != 0;
            // An event node hears its state when the pass ends, as `autoInvalidateInsertedNode`.
            if entry.kinds.any(NodeKinds::FOCUS_EVENT) {
                let id = entry.id;
                self.focus.invalidated_events.push((node, id));
            }
            let entry = &mut self.node_mut(node).chain[index];
            // A built-in element's node has no `on_attach` (`for_custom_entries`).
            if !entry.kinds.any(NodeKinds::OBSERVED) {
                entry.state |= entry_state::ATTACH_RAN;
                self.check_default_callback(node, index, |modifier_node, context| {
                    modifier_node.on_attach(context)
                });
                continue;
            }
            // Its delegates first, as `DelegatingNode.runAttachLifecycle`; what it delegates in
            // its `on_attach` is attached as it returns.
            self.with_entry(node, index, |modifier_node, context| {
                if delegating {
                    delegation::run_delegates_attach(modifier_node, context);
                }
                modifier_node.on_attach(context);
            });
            if let Some(entry) = self.get_mut(node).and_then(|value| value.chain.get_mut(index)) {
                entry.state |= entry_state::ATTACH_RAN;
            }
        }
    }

    /// Detaches `entry` (its `on_detach` ran or wasn't due): not attached, its scope cancelled,
    /// its handles told.
    pub(crate) fn mark_entry_detached(&mut self, entry: EntryId, state: &mut u8) {
        // A handle may have made one too (`NodeHandle::coroutine_scope`).
        if *state & (entry_state::HAS_SCOPE | entry_state::HAS_HANDLE) != 0 {
            // Taken out first: cancelling drops its tasks, which may reach the requests.
            let scope = self.focus_requests.scopes.borrow_mut().remove(&entry);
            if let Some(scope) = scope {
                scope.cancel();
            }
        }
        if *state & entry_state::HAS_HANDLE != 0 {
            self.focus_requests.attached_entries.borrow_mut().remove(&entry.0);
        }
        *state &= !(entry_state::ATTACHED | entry_state::ATTACH_RAN | entry_state::HAS_SCOPE);
    }

    /// Marks `node`'s entries detached, as `markAsDetached`.
    fn mark_detached(&mut self, node: NodeId) {
        self.forget_semantics(node);
        let count = self.get(node).map_or(0, |value| value.chain.len());
        for index in 0..count {
            let Some(entry) = self.get(node).and_then(|value| value.chain.get(index)) else {
                return;
            };
            let (id, mut state) = (entry.id, entry.state);
            if state & entry_state::ATTACHED == 0 {
                continue;
            }
            if state & entry_state::DELEGATING != 0 {
                let entry = &mut self.node_mut(node).chain[index];
                let mut taken = mem::replace(&mut entry.node, Box::new(Taken));
                self.mark_delegates_detached(&mut *taken);
                self.node_mut(node).chain[index].node = taken;
            }
            self.mark_entry_detached(id, &mut state);
            self.node_mut(node).chain[index].state = state;
        }
    }

    /// `root` and its descendants, in pre-order, deactivated ones included (callers skip their
    /// detached entries by their flags).
    fn subtree(&self, root: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let Some(value) = self.get(node) else { continue };
            order.push(node);
            stack.extend(value.children.iter().rev().map(|child| child.node));
        }
        order
    }

    /// `root` and its descendants, each after its children, which are in their order.
    fn subtree_post_order(&self, root: NodeId) -> Vec<NodeId> {
        let mut order = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let Some(value) = self.get(node) else { continue };
            order.push(node);
            stack.extend(value.children.iter().map(|child| child.node));
        }
        order.reverse();
        order
    }

    /// Attaches the modifier nodes of the subtree at `root`, as `LayoutNode.attach`: every one
    /// marked attached first, then their `on_attach`, children before parents.
    fn attach_subtree(&mut self, root: NodeId) {
        // Post-order, each node's children in their order, as `LayoutNode.attach`.
        let nodes = self.subtree_post_order(root);
        for &node in &nodes {
            self.node_mut(node).attaching = false;
            self.mark_attached(node);
        }
        for node in nodes {
            self.run_attach(node);
        }
    }

    /// Detaches the modifier nodes of the subtree at `root`, as `LayoutNode.detach`: their
    /// `on_detach`, parents before children, tail to head, every one still attached; then they
    /// are marked detached.
    pub(crate) fn detach_subtree(&mut self, root: NodeId) {
        self.focus_subtree_detaching(root);
        let nodes = self.subtree(root);
        if !self.unplaced_aware.is_empty() {
            for &node in &nodes {
                self.tell_unplaced_going(node);
            }
        }
        for &node in &nodes {
            self.for_custom_entries(node, entry_state::ATTACH_RAN, true, detach_with_delegates);
        }
        for &node in &nodes {
            self.mark_detached(node);
        }
    }

    /// Resets and detaches `node`'s modifier nodes, as Compose's `resetModifierState` on a node
    /// reused or deactivated: `on_reset` tail to head, then `on_detach`, then marked detached.
    /// Only attached nodes are reset, as Compose's (`if (isAttached) reset()`): not those of a
    /// node reused from the kept pool, detached when it was deactivated, nor the entries the
    /// pass made (D1156).
    pub(crate) fn reset_and_detach(&mut self, node: NodeId) {
        self.focus_node_resetting(node);
        let count = self.get(node).map_or(0, |value| value.chain.len());
        for step in 0..count {
            let index = count - 1 - step;
            let state = self
                .get(node)
                .and_then(|value| value.chain.get(index))
                .map_or(0, |entry| entry.state);
            let delegating = state & entry_state::DELEGATING != 0;
            if state & entry_state::ATTACH_RAN != 0 {
                // Not synced after: what an update in its pass delegated goes with the detach,
                // and what's left attaches with the new content.
                self.with_entry_raw(node, index, |modifier_node, context| {
                    modifier_node.on_reset(context);
                    if delegating {
                        delegation::run_on_delegates(modifier_node, context, |node, context| {
                            node.on_reset(context)
                        });
                    }
                });
            }
        }
        self.for_custom_entries(node, entry_state::ATTACH_RAN, true, detach_with_delegates);
        self.mark_detached(node);
    }

    /// Detaches (if attached) and releases `entries`, which a chain change removed from `node`:
    /// their `on_detach` tail to head, every one still attached, then each marked detached and
    /// released.
    fn release_removed(&mut self, node: NodeId, mut entries: SmallVec<[(ChainEntry, usize); 2]>) {
        // A context reaches its layout node: none without it (released first, which runs these
        // steps itself, `release_removed_in_subtree`; this guards the rest).
        let alive = self.get(node).is_some();
        if alive {
            self.focus_entries_removed(node, &mut entries);
        }
        for (entry, place) in entries.iter_mut().rev() {
            if alive && entry.state & entry_state::ATTACH_RAN != 0 {
                let traverse_key = traverse_key_of(&mut *entry.node, entry.own);
                let mut context = NodeContext {
                    tree: TreeAccess::new(&mut *self),
                    node,
                    index: None,
                    entry: entry.id,
                    out_state: entry.state,
                    delegated: false,
                    place: *place,
                    traverse_key,
                };
                let open = context.tree.open();
                detach_with_delegates(&mut *entry.node, &mut context);
                drop(open);
                entry.state = context.out_state;
            }
        }
        for (mut entry, _) in entries {
            let mut state = entry.state;
            self.mark_delegates_detached(&mut *entry.node);
            self.mark_entry_detached(entry.id, &mut state);
            self.forget_entry_reads(entry.id, state);
            self.release_delegates(&mut *entry.node);
            self.release_node(entry.node);
        }
    }

    /// Takes entry `index` of `node` out of its chain, for application code that the rest of the
    /// tree may be read under, until what it returns is dropped, a panic included.
    pub(crate) fn entry_out(&mut self, node: NodeId, index: usize) -> EntryOut<'_> {
        let slot = &mut self.node_mut(node).chain[index].node;
        let entry = mem::replace(slot, Box::new(Taken));
        EntryOut { access: TreeAccess::new(&mut *self), node, index, entry }
    }

    /// Tells `node`, out of the tree, it's released, and drops it: what it runs may ask where
    /// something is, the only part of the tree in hand being the text measurer.
    pub(crate) fn release_node(&mut self, mut node: Box<dyn crate::modifier::ModifierNode>) {
        let access = TreeAccess::new(&mut *self);
        let _open = access.open();
        let mut context = ReleaseContext {
            text_measurer: &access.text_measurer,
            held_paragraphs: &access.held_paragraphs,
        };
        node.on_release(&mut context);
        drop(node);
    }

    /// Detaches and releases now the entries chain changes of the pass removed from the nodes of
    /// the subtree at `root`, which is being released: their contexts reach their layout nodes
    /// while they live (the queue's step would come after the release).
    pub(crate) fn release_removed_in_subtree(&mut self, root: NodeId) {
        if !self.lifecycle.iter().any(|step| matches!(step, Lifecycle::Removed(..))) {
            return;
        }
        let nodes = self.subtree(root);
        let mut index = 0;
        while index < self.lifecycle.len() {
            let ours = matches!(&self.lifecycle[index], Lifecycle::Removed(node, _) if nodes.contains(node));
            if !ours {
                index += 1;
                continue;
            }
            if let Lifecycle::Removed(node, entries) = self.lifecycle.remove(index) {
                self.release_removed(node, entries);
            }
        }
    }

    /// Queues `step` for the end of the pass.
    pub(crate) fn queue_lifecycle(&mut self, step: Lifecycle) {
        self.lifecycle.push(step);
    }

    /// Runs the lifecycle steps queued, in order: at the end of a composition pass
    /// (`on_end_changes`), and wherever the tree applies its nodes' requests.
    pub(crate) fn run_lifecycle(&mut self) {
        // The consumers that have read something first: those attaching now read what's there.
        self.run_modifier_local_changes();
        self.run_lifecycle_steps();
        self.flush_focus_invalidations();
        self.removed_parents.clear();
    }

    fn run_lifecycle_steps(&mut self) {
        while !self.lifecycle.is_empty() {
            let steps = mem::take(&mut self.lifecycle);
            // Reset in this list: what their updates delegated attaches with the new content,
            // not to be thrown away at once.
            let reset: SmallVec<[NodeId; 2]> = steps
                .iter()
                .filter_map(|step| match step {
                    Lifecycle::Reuse(node) | Lifecycle::Reset(node) => Some(*node),
                    _ => None,
                })
                .collect();
            for step in steps {
                match step {
                    Lifecycle::Attach(root) => {
                        if self.get(root).is_some_and(|value| value.attached) {
                            self.attach_subtree(root);
                        }
                    }
                    Lifecycle::AttachEntries(node) => {
                        self.mark_attached(node);
                        self.run_attach(node);
                    }
                    Lifecycle::Detach(root) => {
                        if self.get(root).is_some_and(|value| !value.attached) {
                            self.detach_subtree(root);
                        }
                    }
                    Lifecycle::Removed(node, entries) => self.release_removed(node, entries),
                    Lifecycle::Reset(node) => {
                        if self.get(node).is_some_and(|value| !value.deactivated) {
                            self.reset_and_detach(node);
                        }
                    }
                    Lifecycle::Reuse(node) => {
                        // Released, or deactivated since (reset and detached then).
                        if self.get(node).is_none_or(|value| value.deactivated) {
                            continue;
                        }
                        self.reset_and_detach(node);
                        self.mark_attached(node);
                        self.run_attach(node);
                    }
                    Lifecycle::SyncDelegates(node, entry) => {
                        if !reset.contains(&node)
                            && let Some(index) = self.entry_index(node, entry)
                        {
                            self.sync_delegates(node, index);
                        }
                    }
                    Lifecycle::LayoutDirection(node) => {
                        self.for_custom_entries(
                            node,
                            entry_state::ATTACH_RAN,
                            false,
                            |node, context| node.on_layout_direction_change(context),
                        );
                    }
                    Lifecycle::Density(node) => {
                        self.for_custom_entries(
                            node,
                            entry_state::ATTACH_RAN,
                            false,
                            |modifier_node, context| {
                                modifier_node.on_density_change(context);
                                if let Some(pointer_input) = modifier_node.as_pointer_input() {
                                    pointer_input.on_cancel_pointer_input();
                                }
                            },
                        );
                    }
                    Lifecycle::ViewConfiguration(node) => {
                        self.for_custom_entries(
                            node,
                            entry_state::ATTACH_RAN,
                            false,
                            |modifier_node, context| {
                                modifier_node.on_view_configuration_change(context);
                                if let Some(pointer_input) = modifier_node.as_pointer_input() {
                                    pointer_input.on_cancel_pointer_input();
                                }
                            },
                        );
                    }
                    Lifecycle::CompositionLocals(node) => {
                        let count = self.get(node).map_or(0, |value| value.chain.len());
                        for index in 0..count {
                            let Some(entry) =
                                self.get(node).and_then(|value| value.chain.get(index))
                            else {
                                break;
                            };
                            if entry.state & entry_state::ATTACH_RAN != 0
                                && entry.kinds.any(NodeKinds::COMPOSITION_LOCAL_CONSUMER)
                            {
                                let own = entry.own;
                                self.with_entry(node, index, |modifier_node, context| {
                                    let _ = delegation::dispatch_with_context(
                                        modifier_node,
                                        own,
                                        NodeKinds::COMPOSITION_LOCAL_CONSUMER,
                                        // Each, as `autoInvalidateNodeIncludingDelegates`.
                                        true,
                                        context,
                                        &mut |modifier_node, context| {
                                            modifier_node.on_composition_locals_changed(context);
                                            std::ops::ControlFlow::Continue(())
                                        },
                                    );
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    /// Applies what modifier nodes asked through their handles, and runs the lifecycle steps
    /// queued outside a pass: at a frame's start, and before each layout.
    pub fn apply_node_requests(&mut self) {
        // Outside passes: a callback may measure its node (`remeasure_sync`).
        let allowed = mem::replace(&mut self.remeasure_allowed, true);
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            self.run_lifecycle();
            self.sync_noted_delegates();
            // The observer nodes whose observed reads changed (`ObserverModifierNode`).
            self.notify_observer_nodes();
        }));
        self.remeasure_allowed = allowed;
        result.unwrap_or_else(|panic| panic::resume_unwind(panic));
        let requests = self.focus_requests.clone();
        for node in mem::take(&mut *requests.measure_requests.borrow_mut()) {
            self.invalidate_layout_of(node);
        }
        // Nothing taken when nothing's asked: what's taken with no request would lose its capacity.
        while !requests.node_invalidations.borrow().is_empty() {
            let mut pending = mem::take(&mut *requests.node_invalidations.borrow_mut());
            for (node, entry, what) in pending.drain(..) {
                if let Some(index) = self.entry_index(node, entry)
                    && self.entry_or_delegate_state(node, index, entry) & entry_state::ATTACHED != 0
                {
                    self.invalidate_entry_at(node, Some(index), what);
                }
            }
            // Its capacity back, so the next request doesn't allocate.
            let mut current = requests.node_invalidations.borrow_mut();
            if current.is_empty() {
                *current = pending;
            }
        }
    }

    /// Whether nodes asked for something through their handles: what needs a frame. Not the
    /// lifecycle steps queued: a pass's run at its end; a paused composition's resumes' reuses
    /// are held apart until it's applied or discarded (`paused_lifecycle`, D1162); a density
    /// change measures, which asks for the frame running its step. None asks the host for
    /// work by itself (they had kept `prefetch` from running).
    pub(crate) fn has_node_requests(&self) -> bool {
        !self.focus_requests.node_invalidations.borrow().is_empty()
            || !self.focus_requests.measure_requests.borrow().is_empty()
            || !self.delegates_noted.is_empty()
    }

    /// Applies `what` for entry `index` of `node`'s chain (`None`: an entry out of it).
    pub(crate) fn invalidate_entry_at(
        &mut self,
        node: NodeId,
        index: Option<usize>,
        what: Invalidation,
    ) {
        let Some(value) = self.get(node) else { return };
        // Deactivated content draws nothing; a measurement it asks for waits for its reuse
        // (`invalidate_layout_of` marks it so).
        if value.deactivated && !matches!(what, Invalidation::Measurement | Invalidation::Placement)
        {
            return;
        }
        match what {
            Invalidation::Draw => self.invalidate_draw_of_entry(node, index),
            Invalidation::Placement => self.invalidate_placement_of(node),
            Invalidation::Rulers => self.invalidate_rulers_of(node),
            Invalidation::Measurement => self.invalidate_layout_of(node),
            Invalidation::Layer => self.invalidate_layer_of_entry(node, index),
            // Computed again when next asked; the frame says they may have changed.
            Invalidation::Semantics => self.invalidate_semantics_of(node),
            Invalidation::ParentData => {
                let fixed = self.refresh_parent_data(node);
                if (self.refresh_custom_parent_data(node) || fixed)
                    && let Some(parent) = self.parent_of(node)
                {
                    self.invalidate_measure(parent);
                }
            }
            Invalidation::Subtree => {
                for each in self.subtree(node) {
                    self.invalidate_drawing_of_node(each);
                    self.invalidate_semantics_of(each);
                    let value = self.node_mut(each);
                    value.needs_measure = true;
                    value.subtree_needs_measure = true;
                }
                self.mark_ancestors_for_measure(node);
                self.invalidate_draw_of(node);
            }
            Invalidation::MeasurementForSubtree => {
                for each in self.subtree(node) {
                    let value = self.node_mut(each);
                    value.needs_measure = true;
                    value.subtree_needs_measure = true;
                }
                self.mark_ancestors_for_measure(node);
            }
            Invalidation::DrawForSubtree => {
                for each in self.subtree(node) {
                    self.invalidate_drawing_of_node(each);
                }
                self.invalidate_draw_of(node);
            }
        }
    }

    /// Marks `node`'s ancestors as having a descendant to measure.
    fn mark_ancestors_for_measure(&mut self, node: NodeId) {
        let mut parent = self.parent_of(node);
        while let Some(id) = parent {
            let value = self.node_mut(id);
            value.subtree_needs_measure = true;
            parent = value.parent;
        }
    }

    /// Everything `node` drew itself to draw again: its retained drawing and its inner layers.
    pub(crate) fn invalidate_drawing_of_node(&mut self, node: NodeId) {
        let value = self.node_mut(node);
        if let Some(retained) = value.retained.as_mut() {
            retained.dirty = true;
            retained.layer_dirty = true;
        }
        for inner in &mut value.inner_layers {
            inner.dirty = true;
        }
        self.needs_draw = true;
    }

    /// The inner layer entry `index` of `node` draws in: the last before it, if any.
    fn inner_layer_of_entry(&self, node: NodeId, index: usize) -> Option<usize> {
        let value = self.get(node)?;
        value.inner_layers.iter().rposition(|inner| (inner.entry as usize) < index)
    }

    /// Draws again what entry `index` of `node` draws in, as `invalidateDraw`'s coordinator: the
    /// inner layer holding it, else what draws the node before its inner layers.
    fn invalidate_draw_of_entry(&mut self, node: NodeId, index: Option<usize>) {
        match index.and_then(|index| self.inner_layer_of_entry(node, index)) {
            Some(slot) => {
                let value = self.node_mut(node);
                value.inner_layers[slot].dirty = true;
                if value.placed {
                    self.needs_draw = true;
                }
            }
            None if index.is_some() => self.invalidate_draw_before_inner_layers(node),
            None => self.invalidate_draw_of(node),
        }
    }

    /// Updates the properties of the graphics layer entry `index` of `node` is in, as
    /// `invalidateLayer`: the node's own layer's (its group's), or an inner layer's (recorded
    /// with its content).
    fn invalidate_layer_of_entry(&mut self, node: NodeId, index: Option<usize>) {
        let Some(index) = index else {
            self.invalidate_draw_of(node);
            return;
        };
        let value = self.node(node);
        // The graphics layer at or before the entry.
        let layer = value.chain[..=index]
            .iter()
            .rposition(|entry| entry.kinds.any(NodeKinds::GRAPHICS_LAYER));
        // None: the layer around it, as `wrappedBy`'s: what draws the node draws it again.
        let Some(layer) = layer else {
            self.invalidate_draw_before_inner_layers(node);
            return;
        };
        if value
            .retained
            .as_ref()
            .is_some_and(|retained| retained.layer_entry == Some(layer as u16))
        {
            let value = self.node_mut(node);
            if let Some(retained) = value.retained.as_mut() {
                // A layer invalidated records its content again, as `OwnedLayer.invalidate`.
                retained.dirty = true;
                retained.layer_dirty = true;
            }
            self.needs_draw = true;
        } else if let Some(slot) =
            value.inner_layers.iter().position(|inner| inner.entry as usize == layer)
        {
            let value = self.node_mut(node);
            value.inner_layers[slot].dirty = true;
            self.needs_draw = true;
        }
    }
}
