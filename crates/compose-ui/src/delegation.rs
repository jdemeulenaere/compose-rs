//! A modifier node made of several, as Compose's `DelegatingNode`: delegates with their own
//! kinds and lifecycle, and dispatch by kind in delegation order
//! (docs/parity/design/modifier-nodes.md §3.7, docs/06-ui.md "Modifiers").

use std::cell::Cell;
use std::marker::PhantomData;
use std::mem;
use std::ops::ControlFlow;

use smallvec::SmallVec;

use crate::modifier::{ModifierNode, ReleaseContext};
use crate::node_chain::{EntryId, NodeKinds, entry_state};
use crate::node_context::NodeContext;
use crate::tree::{NodeId, UiTree};

thread_local! {
    static NEXT_DELEGATE: Cell<u64> = const { Cell::new(0) };
}

/// Delegates' ids never meet chain entries', which come from a counter of their tree.
const DELEGATE_ID: u64 = 1 << 63;

/// One delegate of a node.
pub(crate) struct DelegateEntry {
    node: Box<dyn ModifierNode>,
    /// Its type's name, for the error of a node with two layout delegates.
    name: &'static str,
    /// What it implements itself.
    own: NodeKinds,
    /// That and its own delegates' kinds.
    kinds: NodeKinds,
    id: EntryId,
    /// Its `entry_state` flags: delegates are attached and detached as chain entries are.
    state: u8,
}

impl DelegateEntry {
    pub(crate) fn node_mut(&mut self) -> &mut dyn ModifierNode {
        &mut *self.node
    }

    pub(crate) fn state(&self) -> u8 {
        self.state
    }

    pub(crate) fn set_state(&mut self, state: u8) {
        self.state = state;
    }

    pub(crate) fn kinds(&self) -> NodeKinds {
        self.kinds
    }

    pub(crate) fn id(&self) -> EntryId {
        self.id
    }

    pub(crate) fn traverse_key(&mut self) -> Option<&'static crate::traversal::TraverseKey> {
        crate::node_chain::traverse_key_of(&mut *self.node, self.own)
    }
}

/// A node's delegates, as `DelegatingNode.delegate` keeps them: a node owning one returns it from
/// `ModifierNode::delegates`, and delegates from anywhere its code runs (its element's `create`
/// or `update`, a callback). Once that code returns, the tree attaches what was delegated (if
/// the node is attached) and detaches and releases what was undelegated, and invalidates by their
/// kinds. Delegated or undelegated from a node's measuring or drawing, that waits for the next
/// frame's start.
#[derive(Default)]
pub struct Delegates {
    entries: SmallVec<[DelegateEntry; 2]>,
    /// Undelegated since the tree last looked: detached and released then.
    removed: SmallVec<[DelegateEntry; 1]>,
    /// Delegated or undelegated since the tree last looked.
    changed: bool,
}

/// A typed reference to a delegate, as the node Compose's `delegate()` returns: what `Delegates`
/// gets it, and undelegates it, by. `Copy` and `'static`.
pub struct Delegate<N> {
    id: EntryId,
    _node: PhantomData<fn() -> N>,
}

impl<N> Clone for Delegate<N> {
    fn clone(&self) -> Delegate<N> {
        *self
    }
}

impl<N> Copy for Delegate<N> {}

impl Delegates {
    pub fn new() -> Delegates {
        Delegates::default()
    }

    /// Makes `node` a delegate, as `DelegatingNode.delegate`: its kinds become the owner's, and
    /// it follows the owner's lifecycle. If the owner is attached, it's attached and auto-invalidated
    /// as inserted once the owner's code running returns.
    ///
    /// A node may have at most one delegate of the layout kind unless it implements
    /// `as_layout` itself, as Compose's rule: the tree panics when it looks otherwise.
    pub fn delegate<N: ModifierNode>(&mut self, mut node: N) -> Delegate<N> {
        let own = NodeKinds::of_delegate(&mut node);
        let kinds = match node.delegates() {
            Some(delegates) => own | delegates.refresh_kinds(own),
            None => own,
        };
        let id = NEXT_DELEGATE.with(|next| {
            next.set(next.get() + 1);
            EntryId(DELEGATE_ID | next.get())
        });
        let name = std::any::type_name::<N>();
        self.entries.push(DelegateEntry { node: Box::new(node), name, own, kinds, id, state: 0 });
        self.changed = true;
        Delegate { id, _node: PhantomData }
    }

    /// Stops delegating to `delegate`, as `DelegatingNode.undelegate`: once the owner's code
    /// running returns, it's auto-invalidated as removed, detached if it's attached, and released
    /// (compose-rs's nodes are owned by what holds them: to delegate to it again, make a new one).
    ///
    /// Panics if it isn't a delegate of this node (any more), as Compose's.
    pub fn undelegate<N: ModifierNode>(&mut self, delegate: Delegate<N>) {
        let index = self.position(delegate.id);
        self.removed.push(self.entries.remove(index));
        self.changed = true;
    }

    /// The delegate. Panics if it isn't one of this node's (any more).
    pub fn get<N: ModifierNode>(&self, delegate: Delegate<N>) -> &N {
        let index = self.position(delegate.id);
        self.entries[index].node.downcast_ref::<N>().expect("compose-ui: a delegate of its type")
    }

    /// The delegate. Panics if it isn't one of this node's (any more).
    pub fn get_mut<N: ModifierNode>(&mut self, delegate: Delegate<N>) -> &mut N {
        let index = self.position(delegate.id);
        self.entries[index].node.downcast_mut::<N>().expect("compose-ui: a delegate of its type")
    }

    /// Its delegates, in the order they were delegated to.
    pub fn nodes(&mut self) -> impl Iterator<Item = &mut dyn ModifierNode> {
        self.entries.iter_mut().map(|entry| &mut *entry.node as &mut dyn ModifierNode)
    }

    fn position(&self, id: EntryId) -> usize {
        self.entries
            .iter()
            .position(|entry| entry.id == id)
            .expect("compose-ui: could not find the delegate: it isn't one of this node's")
    }

    /// Whether anything was delegated or undelegated here or below since the tree last looked.
    pub(crate) fn needs_sync(&mut self) -> bool {
        self.changed
            || self
                .entries
                .iter_mut()
                .any(|entry| entry.node.delegates().is_some_and(|delegates| delegates.needs_sync()))
    }

    /// The union of its delegates' kinds, theirs brought up to date first; `owner` is what the
    /// node holding it implements itself. Panics on two layout delegates under an owner that
    /// isn't a layout node itself, as Compose's `validateDelegateKindSet`.
    pub(crate) fn refresh_kinds(&mut self, owner: NodeKinds) -> NodeKinds {
        let mut union = NodeKinds::NONE;
        let mut layout: Option<&'static str> = None;
        for entry in &mut self.entries {
            if let Some(delegates) = entry.node.delegates() {
                entry.kinds = entry.own | delegates.refresh_kinds(entry.own);
            }
            union |= entry.kinds;
            if entry.kinds.any(NodeKinds::LAYOUT) && !owner.any(NodeKinds::LAYOUT) {
                if let Some(first) = layout {
                    panic!(
                        "compose-ui: delegating to several layout nodes ({first}, {}) without the \
                         delegating node being one itself (`as_layout`) isn't allowed",
                        entry.name
                    );
                }
                layout = Some(entry.name);
            }
        }
        union
    }
}

/// Calls `f` for `node`, whose own kinds are `own`, if it's of `kind`, else for each of its
/// delegates of `kind` (down their own delegates), in delegation order, as Compose's
/// `dispatchForKind`, with the delegate's id (`NO_DELEGATE`: `node` itself).
pub(crate) fn dispatch_for_kind(
    node: &mut dyn ModifierNode,
    own: NodeKinds,
    kind: NodeKinds,
    id: EntryId,
    f: &mut dyn FnMut(&mut dyn ModifierNode, EntryId),
) {
    if own.any(kind) {
        f(node, id);
    } else if let Some(delegates) = node.delegates() {
        for entry in &mut delegates.entries {
            if entry.kinds.any(kind) {
                dispatch_for_kind(&mut *entry.node, entry.own, kind, entry.id, f);
            }
        }
    }
}

/// `dispatch_for_kind` for a node out of a tree, its own kinds probed: what tests of the dispatch
/// order use, as Compose's `assertDispatchOrder`. Not for apps.
#[doc(hidden)]
pub fn dispatch_for_kind_of(
    node: &mut dyn ModifierNode,
    kind: NodeKinds,
    f: &mut dyn FnMut(&mut dyn ModifierNode),
) {
    let own = NodeKinds::of_delegate(node);
    dispatch_for_kind(node, own, kind, EntryId::NO_DELEGATE, &mut |node, _| f(node));
}

/// `dispatch_for_kind` where each node runs with a context: `context` is `node`'s.
/// `include_delegates` also visits the delegates of a node that is of `kind` itself, after it
/// (Compose's `dispatchToDelegates`). Stops at the first `Break`.
pub(crate) fn dispatch_with_context(
    node: &mut dyn ModifierNode,
    own: NodeKinds,
    kind: NodeKinds,
    include_delegates: bool,
    context: &mut NodeContext<'_>,
    f: &mut dyn FnMut(&mut dyn ModifierNode, &mut NodeContext<'_>) -> ControlFlow<()>,
) -> ControlFlow<()> {
    let is_kind = own.any(kind);
    if is_kind {
        f(node, context)?;
    }
    if (!is_kind || include_delegates)
        && let Some(delegates) = node.delegates()
    {
        for entry in &mut delegates.entries {
            if entry.kinds.any(kind) {
                let delegate_own = entry.own;
                context.with_delegate(entry, |node, context| {
                    dispatch_with_context(node, delegate_own, kind, include_delegates, context, f)
                })?;
            }
        }
    }
    ControlFlow::Continue(())
}

/// `node`, whose own kinds are `own`, if it's of `kind`, else its first delegate that is (down
/// their own delegates). For the kinds a node has one of (Compose's `asLayoutModifierNode`).
#[inline]
pub(crate) fn first_of_kind<'a>(
    node: &'a mut (dyn ModifierNode + 'static),
    own: NodeKinds,
    kind: NodeKinds,
) -> Option<&'a mut (dyn ModifierNode + 'static)> {
    if own.any(kind) { Some(node) } else { first_delegate_of_kind(node, kind) }
}

#[cold]
fn first_delegate_of_kind<'a>(
    node: &'a mut (dyn ModifierNode + 'static),
    kind: NodeKinds,
) -> Option<&'a mut (dyn ModifierNode + 'static)> {
    for entry in &mut node.delegates()?.entries {
        if entry.kinds.any(kind) {
            return first_of_kind(&mut *entry.node, entry.own, kind);
        }
    }
    None
}

/// What updating `node`, whose own kinds are `own`, auto-invalidates of `remaining`, as Compose's
/// `autoInvalidateNodeIncludingDelegates`: its own kinds unless it says not to, then what's left
/// for its delegates, each asked apart.
pub(crate) fn updated_kinds(
    node: &mut dyn ModifierNode,
    own: NodeKinds,
    remaining: NodeKinds,
) -> NodeKinds {
    let mut kinds =
        if node.should_auto_invalidate() { own.and(remaining) } else { NodeKinds::NONE };
    if let Some(delegates) = node.delegates() {
        let remaining = remaining.without(own);
        for entry in &mut delegates.entries {
            kinds |= updated_kinds(&mut *entry.node, entry.own, remaining);
        }
    }
    kinds
}

/// The delegate of `node` (or below its delegates) named `id`.
pub(crate) fn find_delegate(
    node: &mut dyn ModifierNode,
    id: EntryId,
) -> Option<&mut DelegateEntry> {
    let delegates = node.delegates()?;
    let entries = &mut delegates.entries;
    if let Some(index) = entries.iter().position(|entry| entry.id == id) {
        return Some(&mut entries[index]);
    }
    entries.iter_mut().find_map(|entry| find_delegate(&mut *entry.node, id))
}

/// Marks the delegates of `node` attached, as `markAsAttached`: the owner's first, each then its
/// own.
pub(crate) fn mark_delegates_attached(
    node: &mut dyn ModifierNode,
    requests: &crate::focus_requester::FocusRequests,
) {
    let Some(delegates) = node.delegates() else { return };
    for entry in &mut delegates.entries {
        mark_delegate_attached(entry, requests);
    }
}

fn mark_delegate_attached(
    entry: &mut DelegateEntry,
    requests: &crate::focus_requester::FocusRequests,
) {
    if entry.state & entry_state::ATTACHED == 0 {
        entry.state |= entry_state::ATTACHED;
        if entry.state & entry_state::HAS_HANDLE != 0 {
            requests.attached_entries.borrow_mut().insert(entry.id.0);
        }
    }
    mark_delegates_attached(&mut *entry.node, requests);
}

/// Runs the `on_attach` of the delegates of `node` that haven't run it, as `runAttachLifecycle`:
/// the newest delegate first, each one's own delegates before it.
pub(crate) fn run_delegates_attach(node: &mut dyn ModifierNode, context: &mut NodeContext<'_>) {
    let Some(delegates) = node.delegates() else { return };
    for entry in delegates.entries.iter_mut().rev() {
        run_delegate_attach(entry, context);
    }
}

fn run_delegate_attach(entry: &mut DelegateEntry, context: &mut NodeContext<'_>) {
    if entry.state & (entry_state::ATTACHED | entry_state::ATTACH_RAN) != entry_state::ATTACHED {
        return;
    }
    context.with_delegate(entry, |node, context| {
        run_delegates_attach(node, context);
        node.on_attach(context);
    });
    entry.state |= entry_state::ATTACH_RAN;
}

/// Runs `callback` on the delegates of `node` whose attach ran, as `runDetachLifecycle` and
/// `reset`: the newest delegate first, each before its own delegates.
pub(crate) fn run_on_delegates(
    node: &mut dyn ModifierNode,
    context: &mut NodeContext<'_>,
    callback: fn(&mut dyn ModifierNode, &mut NodeContext<'_>),
) {
    let Some(delegates) = node.delegates() else { return };
    for entry in delegates.entries.iter_mut().rev() {
        if entry.state & entry_state::ATTACH_RAN != 0 {
            context.with_delegate(entry, |node, context| {
                callback(node, context);
                run_on_delegates(node, context, callback);
            });
        }
    }
}

impl UiTree {
    /// Marks the delegates of `node` detached, as `markAsDetached`: each one's own first, their
    /// scopes cancelled and their handles told.
    pub(crate) fn mark_delegates_detached(&mut self, node: &mut dyn ModifierNode) {
        let Some(delegates) = node.delegates() else { return };
        for entry in delegates.entries.iter_mut().rev() {
            self.mark_delegate_detached(entry);
        }
    }

    fn mark_delegate_detached(&mut self, entry: &mut DelegateEntry) {
        self.mark_delegates_detached(&mut *entry.node);
        if entry.state & entry_state::ATTACHED != 0 {
            self.mark_entry_detached(entry.id, &mut entry.state);
        }
    }

    /// Releases the delegates of `node`, which is being dropped.
    pub(crate) fn release_delegates(&mut self, node: &mut dyn ModifierNode) {
        let Some(delegates) = node.delegates() else { return };
        let entries = mem::take(&mut delegates.entries);
        let removed = mem::take(&mut delegates.removed);
        for entry in entries.into_iter().chain(removed) {
            self.release_delegate(entry);
        }
    }

    fn release_delegate(&mut self, mut entry: DelegateEntry) {
        self.release_delegates(&mut *entry.node);
        self.forget_entry_reads(entry.id, entry.state);
        if entry.state & entry_state::HAS_HANDLE != 0 {
            self.focus_requests.attached_entries.borrow_mut().remove(&entry.id.0);
        }
        let mut context = ReleaseContext { text_measurer: &mut *self.text_measurer };
        entry.node.on_release(&mut context);
    }

    /// Whether what entry `index` of `node` delegates to changed; if so it's looked at later,
    /// where the tree may change (`sync_noted_delegates`): for what runs inside a pass (measuring,
    /// drawing) or dispatching an event.
    pub(crate) fn note_delegates_changed(&mut self, node: NodeId, index: usize) {
        let Some(entry) = self.get_mut(node).and_then(|value| value.chain.get_mut(index)) else {
            return;
        };
        if entry.state & entry_state::DELEGATING != 0
            && entry.node.delegates().is_some_and(|delegates| delegates.needs_sync())
        {
            let id = entry.id;
            self.delegates_noted.push((node, id));
        }
    }

    /// Syncs the nodes `note_delegates_changed` noted.
    pub(crate) fn sync_noted_delegates(&mut self) {
        while !self.delegates_noted.is_empty() {
            for (node, id) in mem::take(&mut self.delegates_noted) {
                if let Some(index) = self.entry_index(node, id) {
                    self.sync_delegates(node, index);
                }
            }
        }
    }

    /// Brings the tree up to date with what entry `index` of `node` delegates to, once a node's
    /// code has returned, as what `DelegatingNode.delegate` and `undelegate` do at once: what was
    /// undelegated is auto-invalidated, detached and released; what was delegated, if the owner
    /// is attached, is attached and auto-invalidated; a change of the kinds goes through the
    /// chain (`refresh_chain`), which makes or drops a layer for a layout delegate (Compose's
    /// `syncCoordinators`).
    pub(crate) fn sync_delegates(&mut self, node: NodeId, index: usize) {
        let mut changes = NodeKinds::NONE;
        let mut any = false;
        // Until nothing changes: the callbacks may delegate too.
        loop {
            let Some(entry) = self.get_mut(node).and_then(|value| value.chain.get_mut(index))
            else {
                return;
            };
            let Some(delegates) = entry.node.delegates() else { return };
            if !delegates.needs_sync() {
                break;
            }
            any = true;
            let own = entry.own;
            let attached = entry.state & entry_state::ATTACHED != 0;
            self.with_entry_raw(node, index, |modifier_node, context| {
                sync_node(modifier_node, attached, context, &mut changes);
                if let Some(delegates) = modifier_node.delegates() {
                    delegates.refresh_kinds(own);
                }
            });
        }
        if !any {
            return;
        }
        let value = self.node_mut(node);
        let entry = &mut value.chain[index];
        let before = entry.kinds;
        let own = entry.own;
        let kinds = match entry.node.delegates() {
            Some(delegates) => own | delegates.refresh_kinds(own),
            None => own,
        };
        entry.kinds = kinds;
        let mut parent_data_changed = false;
        if kinds != before {
            let layers_changed = before.any(NodeKinds::LAYOUT) != kinds.any(NodeKinds::LAYOUT);
            if layers_changed {
                value.chain_version = value.chain_version.wrapping_add(1);
            }
            parent_data_changed = self.refresh_chain(node);
            if layers_changed {
                // The same nodes in the same places, but in other layers.
                let same: SmallVec<[Option<usize>; 8]> =
                    (0..self.node(node).chain.len()).map(Some).collect();
                self.remap_captures(node, &same);
            }
        }
        if changes.any(NodeKinds::PARENT_DATA) {
            parent_data_changed |= self.refresh_custom_parent_data(node);
        }
        if changes.any(NodeKinds::FOCUS_TARGET) {
            self.revalidate_focus(node, None);
        }
        self.auto_invalidate(node, changes, changes, parent_data_changed);
    }
}

/// The delegates of `node` synced: `context` is its own, `attached` whether it's attached.
fn sync_node(
    node: &mut dyn ModifierNode,
    attached: bool,
    context: &mut NodeContext<'_>,
    changes: &mut NodeKinds,
) {
    let Some(delegates) = node.delegates() else { return };
    delegates.changed = false;
    for mut gone in mem::take(&mut delegates.removed) {
        *changes |= gone.kinds;
        if gone.state & entry_state::ATTACH_RAN != 0 {
            context.with_delegate(&mut gone, |node, context| {
                node.on_detach(context);
                run_on_delegates(node, context, |node, context| node.on_detach(context));
            });
        }
        context.tree.mark_delegate_detached(&mut gone);
        context.tree.release_delegate(gone);
    }
    let requests = context.tree.focus_requests.clone();
    for entry in &mut delegates.entries {
        if attached && entry.state & entry_state::ATTACHED == 0 {
            mark_delegate_attached(entry, &requests);
            run_delegate_attach(entry, context);
            *changes |= entry.kinds;
        } else {
            let attached = entry.state & entry_state::ATTACHED != 0;
            context.with_delegate(entry, |node, context| {
                sync_node(node, attached, context, changes);
            });
        }
    }
}
