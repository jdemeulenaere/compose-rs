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
use crate::node_context::{Invalidation, NodeContext};
use crate::tree::{NodeId, UiTree};

thread_local! {
    static NEXT_DELEGATE: Cell<u64> = const { Cell::new(0) };
}

/// Delegates' ids never meet chain entries', which come from a counter of their tree.
const DELEGATE_ID: u64 = 1 << 63;

/// One delegate of a node.
pub(crate) struct DelegateEntry {
    node: Box<dyn ModifierNode>,
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
/// `ModifierNode::delegates`, and delegates from anywhere its code runs. `delegate` is for an
/// element's `create` and `update`, which have no context: the tree attaches what it delegated
/// (if the node is attached) and detaches and releases what was undelegated, and invalidates by
/// their kinds, once that code returns. From a callback, `delegate_attached` does it inside the
/// call, as Compose's `delegate()`. Delegated or undelegated from a node's measuring or drawing,
/// that waits for the next layout round or frame.
#[derive(Default)]
pub struct Delegates {
    entries: SmallVec<[DelegateEntry; 2]>,
    /// Undelegated since the tree last looked: detached and released then.
    removed: Vec<DelegateEntry>,
    /// Delegated or undelegated since the tree last looked.
    changed: bool,
    /// `ask_for_context` since the tree last looked.
    wants_context: bool,
    /// The kinds of what was attached, detached or released since the tree last brought the
    /// chain up to date: what it invalidates then, and its kinds again.
    pending: NodeKinds,
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
    pub fn delegate<N: ModifierNode>(&mut self, node: N) -> Delegate<N> {
        let id = self.delegate_boxed(Box::new(node));
        Delegate { id, _node: PhantomData }
    }

    /// `delegate` of a node whose type isn't known (an indication factory's), by its name.
    pub(crate) fn delegate_boxed(&mut self, mut node: Box<dyn ModifierNode>) -> EntryId {
        let own = NodeKinds::of_delegate(&mut *node);
        let kinds = match node.delegates() {
            Some(delegates) => own | delegates.refresh_kinds(own),
            None => own,
        };
        let id = NEXT_DELEGATE.with(|next| {
            next.set(next.get() + 1);
            EntryId(DELEGATE_ID | next.get())
        });
        self.entries.push(DelegateEntry { node, own, kinds, id, state: 0 });
        self.changed = true;
        id
    }

    /// Asks for `ModifierNode::on_update_with_context` once the code running returns, if the node
    /// is attached: what an element's `update`, which has no context, leaves to do with one
    /// (Compose's `update` is the node's own code, with all it can reach). compose-rs's, for its
    /// own nodes; asked from `on_update_with_context`, it's dropped.
    #[doc(hidden)]
    pub fn ask_for_context(&mut self) {
        self.wants_context = true;
        self.changed = true;
    }

    /// Makes `node` a delegate from a callback, attaching it inside this call if the owner is
    /// attached, as Compose's `delegate()`: the owner's kinds are brought up to date first, then
    /// its `on_attach` runs. `context` is the owner's. Inside a measurement it waits, as
    /// `delegate`: the layers can't change under it.
    pub fn delegate_attached<N: ModifierNode>(
        &mut self,
        node: N,
        context: &mut NodeContext<'_>,
    ) -> Delegate<N> {
        let id = self.delegate_boxed_attached(Box::new(node), context);
        Delegate { id, _node: PhantomData }
    }

    /// `delegate_attached` of a node whose type isn't known, by its name.
    pub(crate) fn delegate_boxed_attached(
        &mut self,
        node: Box<dyn ModifierNode>,
        context: &mut NodeContext<'_>,
    ) -> EntryId {
        let changed = self.changed;
        let id = self.delegate_boxed(node);
        if context.tree.delegates_wait {
            return id;
        }
        self.changed = changed;
        let entry = self.entries.last_mut().expect("just delegated to");
        self.pending |= entry.kinds;
        context.tree.add_entry_kinds(context.node, context.index, entry.kinds);
        if context.is_attached() {
            let requests = context.tree.focus_requests.clone();
            mark_delegate_attached(entry, &requests);
            run_delegate_attach(entry, context);
        }
        id
    }

    /// Stops delegating to `delegate`, as `DelegatingNode.undelegate`: once the owner's code
    /// running returns, it's auto-invalidated as removed, detached if it's attached, and released
    /// (compose-rs's nodes are owned by what holds them: to delegate to it again, make a new one).
    ///
    /// Panics if it isn't a delegate of this node (any more), as Compose's.
    pub fn undelegate<N: ModifierNode>(&mut self, delegate: Delegate<N>) {
        self.undelegate_boxed(delegate.id);
    }

    /// `undelegate` by its name.
    pub(crate) fn undelegate_boxed(&mut self, id: EntryId) {
        let index = self.position(id);
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
    pub(crate) fn has_changed(&mut self) -> bool {
        self.changed
            || self.entries.iter_mut().any(|entry| {
                entry.node.delegates().is_some_and(|delegates| delegates.has_changed())
            })
    }

    /// Whether the tree has anything to do here: `has_changed`, or a chain to bring up to date.
    pub(crate) fn needs_sync(&mut self) -> bool {
        !self.pending.is_empty()
            || self.changed
            || self
                .entries
                .iter_mut()
                .any(|entry| entry.node.delegates().is_some_and(|delegates| delegates.needs_sync()))
    }

    /// The kinds attached, detached or released here and below since the last call.
    fn take_pending(&mut self) -> NodeKinds {
        let mut pending = mem::take(&mut self.pending);
        for entry in &mut self.entries {
            if let Some(delegates) = entry.node.delegates() {
                pending |= delegates.take_pending();
            }
        }
        pending
    }

    /// `refresh_kinds` for a chain's entry: with `DELEGATING` if it has delegates, which a hit
    /// test asks for clips.
    pub(crate) fn entry_kinds(&mut self, owner: NodeKinds) -> NodeKinds {
        let union = self.refresh_kinds(owner);
        if union.is_empty() { union } else { union | NodeKinds::DELEGATING }
    }

    /// The union of its delegates' kinds, theirs brought up to date first; `owner` is what the
    /// node holding it implements itself. Panics on two layout delegates under an owner that
    /// isn't a layout node itself, as Compose's `validateDelegateKindSet`.
    pub(crate) fn refresh_kinds(&mut self, owner: NodeKinds) -> NodeKinds {
        let mut union = NodeKinds::NONE;
        let mut layouts = 0;
        for entry in &mut self.entries {
            if let Some(delegates) = entry.node.delegates() {
                entry.kinds = entry.own | delegates.refresh_kinds(entry.own);
            }
            union |= entry.kinds;
            layouts += usize::from(entry.kinds.any(NodeKinds::LAYOUT));
        }
        assert!(
            layouts < 2 || owner.any(NodeKinds::LAYOUT),
            "compose-ui: delegating to several layout nodes without the delegating node being \
             one itself (`as_layout`) isn't allowed"
        );
        union
    }
}

/// The clip of `node` or, if it has none, of its first delegate with one: compose-rs's extra, as
/// a kind's dispatch but asked of each, a graphics layer's being known only when asked.
pub(crate) fn hit_clip_of(
    node: &mut dyn ModifierNode,
    size: crate::unit::Size,
    density: crate::unit::Density,
) -> Option<crate::graphics::Shape> {
    if let Some(clip) = node.hit_clip_at(size, density) {
        return Some(clip);
    }
    let delegates = node.delegates()?;
    delegates.entries.iter_mut().find_map(|entry| hit_clip_of(&mut *entry.node, size, density))
}

/// `node`'s `bring_into_view` or, if it doesn't scroll, its first delegate's that does.
pub(crate) fn bring_into_view_of(
    node: &mut dyn ModifierNode,
    rect: crate::unit::Rect,
    viewport: crate::unit::IntSize,
) -> Option<crate::unit::Rect> {
    if let Some(moved) = node.bring_into_view(rect, viewport) {
        return Some(moved);
    }
    let delegates = node.delegates()?;
    delegates
        .entries
        .iter_mut()
        .find_map(|entry| bring_into_view_of(&mut *entry.node, rect, viewport))
}

/// `node` if it's selectable, or its first delegate that is, for `as_selectable`.
pub(crate) fn selectable_of(
    node: &mut dyn ModifierNode,
) -> Option<&mut dyn crate::text_selection::Selectable> {
    if node.as_selectable().is_some() {
        return node.as_selectable();
    }
    let delegates = node.delegates()?;
    delegates.entries.iter_mut().find_map(|entry| selectable_of(&mut *entry.node))
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

/// `dispatch_for_kind` from `delegate`.
pub(crate) fn dispatch_delegate_for_kind(
    delegate: &mut DelegateEntry,
    kind: NodeKinds,
    f: &mut dyn FnMut(&mut dyn ModifierNode, EntryId),
) {
    dispatch_for_kind(&mut *delegate.node, delegate.own, kind, delegate.id, f);
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

/// `node`, whose own kinds are `own`, if it's of `kind`, else the first delegate that is,
/// breadth first, the newest first, as Compose's `asKind` and `asLayoutModifierNode`, its list of
/// delegates having the newest at its head. For the kinds a node has one of.
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
    let id = first_delegate_id(node, kind)?;
    find_delegate(node, id).map(|found| &mut *found.node)
}

/// The first delegate of `kind` below `node`, a level at a time.
fn first_delegate_id(node: &mut dyn ModifierNode, kind: NodeKinds) -> Option<EntryId> {
    let mut depth = 0;
    loop {
        let (found, deeper) = delegate_at_depth(node, kind, depth);
        if found.is_some() || !deeper {
            return found;
        }
        depth += 1;
    }
}

/// The first delegate of `kind` `depth` levels below `node`, and whether there are levels below.
fn delegate_at_depth(
    node: &mut dyn ModifierNode,
    kind: NodeKinds,
    depth: usize,
) -> (Option<EntryId>, bool) {
    let Some(delegates) = node.delegates() else { return (None, false) };
    let mut deeper = false;
    for entry in delegates.entries.iter_mut().rev() {
        if !entry.kinds.any(kind) {
            continue;
        }
        if depth == 0 {
            if entry.own.any(kind) {
                return (Some(entry.id), false);
            }
            deeper = true;
        } else {
            let (found, below) = delegate_at_depth(&mut *entry.node, kind, depth - 1);
            if found.is_some() {
                return (found, false);
            }
            deeper |= below;
        }
    }
    (None, deeper)
}

/// `first_of_kind` for a node out of a tree, its own kinds probed: what tests of `asKind` use.
/// Not for apps.
#[doc(hidden)]
pub fn first_of_kind_of<'a>(
    node: &'a mut (dyn ModifierNode + 'static),
    kind: NodeKinds,
) -> Option<&'a mut (dyn ModifierNode + 'static)> {
    let own = NodeKinds::of_delegate(node);
    first_of_kind(node, own, kind)
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

/// Runs `callback` on the delegates of `node` whose attach ran, as `DelegatingNode.reset`: the
/// newest delegate first, each before its own delegates.
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

/// `node`'s `on_detach` and its delegates', as `DelegatingNode.runDetachLifecycle`: the node, then
/// the newest delegate first, each before its own. What each delegates or undelegates there is
/// attached or detached before going on, as Compose's `delegate` and `undelegate` do at once, so
/// a delegate attached in an `on_detach` hears its own `on_detach`, and one undelegated hears it
/// even if its owner is leaving its chain. Each `on_detach` follows one `on_attach`.
pub(crate) fn detach_with_delegates(node: &mut dyn ModifierNode, context: &mut NodeContext<'_>) {
    node.on_detach(context);
    // What an update left to do with a context is over: attached again, it starts from nothing.
    if let Some(delegates) = node.delegates() {
        delegates.wants_context = false;
    }
    sync_node(node, context.is_attached(), context);
    let Some(delegates) = node.delegates() else { return };
    for entry in delegates.entries.iter_mut().rev() {
        detach_delegate(entry, context);
    }
}

fn detach_delegate(entry: &mut DelegateEntry, context: &mut NodeContext<'_>) {
    if entry.state & entry_state::ATTACH_RAN == 0 {
        return;
    }
    context.with_delegate(entry, detach_with_delegates);
    entry.state &= !entry_state::ATTACH_RAN;
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
        let Some(entry) = self.get_mut(node).and_then(|value| value.chain.get_mut(index)) else {
            return;
        };
        if !entry.node.delegates().is_some_and(|delegates| delegates.needs_sync()) {
            return;
        }
        // Until nothing changes: the callbacks may delegate too.
        loop {
            let Some(entry) = self.get_mut(node).and_then(|value| value.chain.get_mut(index))
            else {
                return;
            };
            if !entry.node.delegates().is_some_and(|delegates| delegates.has_changed()) {
                break;
            }
            let attached = entry.state & entry_state::ATTACHED != 0;
            self.with_entry_raw(node, index, |modifier_node, context| {
                sync_node(modifier_node, attached, context);
            });
        }
        let entry = &mut self.node_mut(node).chain[index];
        let (id, own, before) = (entry.id, entry.own, entry.kinds);
        let (kinds, changes) = match entry.node.delegates() {
            Some(delegates) => (own | delegates.entry_kinds(own), delegates.take_pending()),
            None => (own, NodeKinds::NONE),
        };
        entry.kinds = kinds;
        let parent_data_changed = kinds != before && self.entry_kinds_changed(node, before, kinds);
        if changes.any(NodeKinds::PARENT_DATA) {
            // The fold asks every node of the chain, and one may be running: where the tree
            // applies its nodes' requests, as a node's own `invalidate_parent_data`.
            let requests = &self.focus_requests;
            requests.node_invalidations.borrow_mut().push((node, id, Invalidation::ParentData));
            requests.wake();
        }
        if changes.any(NodeKinds::FOCUS_TARGET | NodeKinds::FOCUS_PROPERTIES) {
            self.focus_delegates_changed(node);
        }
        if changes.any(NodeKinds::FOCUS_EVENT) {
            self.focus.invalidated_events.push((node, id));
        }
        self.auto_invalidate(node, changes, changes, parent_data_changed);
        self.drain_remeasures();
    }

    /// Entry `index` of `node` took on `kinds` (a delegate attached at once): its chain knows.
    pub(crate) fn add_entry_kinds(&mut self, node: NodeId, index: Option<usize>, kinds: NodeKinds) {
        let Some(index) = index else { return };
        let entry = &mut self.node_mut(node).chain[index];
        let before = entry.kinds;
        if !before.all(kinds) {
            entry.kinds |= kinds | NodeKinds::DELEGATING;
            let after = entry.kinds;
            self.entry_kinds_changed(node, before, after);
        }
    }

    /// `node`'s chain after an entry's kinds went from `before` to `after`: its aggregates and
    /// layers, and what holds its layers (`remap_captures`). Returns whether its parent data
    /// changed.
    fn entry_kinds_changed(&mut self, node: NodeId, before: NodeKinds, after: NodeKinds) -> bool {
        let layers_changed = before.any(NodeKinds::LAYOUT) != after.any(NodeKinds::LAYOUT);
        if layers_changed {
            let value = self.node_mut(node);
            value.chain_version = value.chain_version.wrapping_add(1);
        }
        let parent_data_changed = self.refresh_chain(node);
        if layers_changed {
            self.remap_same_entries(node);
        }
        parent_data_changed
    }

    /// The same nodes in the same places of `node`'s chain, but in other layers: what holds them
    /// (a gesture, the hover) follows.
    pub(crate) fn remap_same_entries(&mut self, node: NodeId) {
        let same: SmallVec<[Option<usize>; 8]> =
            (0..self.node(node).chain.len()).map(Some).collect();
        self.remap_captures(node, &same);
    }
}

/// The delegates of `node` synced, `context` being its own: what was undelegated is detached and
/// released, what was delegated attached if the owner is (`attached`), and so on below. What the
/// chain must do of it is left in `pending`, for `sync_delegates`.
fn sync_node(node: &mut dyn ModifierNode, attached: bool, context: &mut NodeContext<'_>) {
    let Some(delegates) = node.delegates() else { return };
    delegates.changed = false;
    for mut gone in mem::take(&mut delegates.removed) {
        delegates.pending |= gone.kinds.without(NodeKinds::LAYOUT_AWARE);
        detach_delegate(&mut gone, context);
        context.tree.mark_delegate_detached(&mut gone);
        context.tree.release_delegate(gone);
    }
    // Between: Compose's `update` undelegates, makes the new one and delegates to it, in order.
    if delegates.wants_context && attached {
        node.on_update_with_context(context);
    }
    let Some(delegates) = node.delegates() else { return };
    delegates.wants_context = false;
    let requests = context.tree.focus_requests.clone();
    for entry in &mut delegates.entries {
        if attached && entry.state & entry_state::ATTACHED == 0 {
            mark_delegate_attached(entry, &requests);
            run_delegate_attach(entry, context);
            delegates.pending |= entry.kinds;
        } else {
            let attached = entry.state & entry_state::ATTACHED != 0;
            context.with_delegate(entry, |node, context| {
                sync_node(node, attached, context);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;
    use crate::text::{SharedString, TextMeasurer};
    use crate::text_selection::Selectable;
    use crate::unit::Offset;

    struct Text(&'static str);

    impl Selectable for Text {
        fn text(&self) -> SharedString {
            SharedString::from(self.0)
        }
        fn offset_at(&self, _: Offset, _: &mut dyn TextMeasurer) -> Option<usize> {
            None
        }
        fn word_at(&self, _: usize) -> Range<usize> {
            0..0
        }
        fn set_selection(&mut self, _: Option<Range<usize>>, _: &mut dyn TextMeasurer) -> bool {
            false
        }
    }

    impl ModifierNode for Text {
        fn as_selectable(&mut self) -> Option<&mut dyn Selectable> {
            Some(self)
        }
    }

    struct Owner(Delegates);

    impl ModifierNode for Owner {
        fn delegates(&mut self) -> Option<&mut Delegates> {
            Some(&mut self.0)
        }
    }

    /// Asked of the node, then of its delegates, in the order they were delegated to, down
    /// theirs.
    #[test]
    fn a_selectable_is_found_on_a_delegate() {
        let mut inner = Delegates::new();
        inner.delegate(Text("inner"));
        let mut outer = Delegates::new();
        outer.delegate(Owner(Delegates::new()));
        outer.delegate(Owner(inner));
        outer.delegate(Text("last"));
        let mut owner = Owner(outer);
        let found = selectable_of(&mut owner).map(|text| text.text());
        assert_eq!(found, Some(SharedString::from("inner")));
        let mut own = Text("own");
        assert_eq!(selectable_of(&mut own).map(|t| t.text()), Some(SharedString::from("own")));
        assert!(selectable_of(&mut Owner(Delegates::new())).is_none());
    }
}
