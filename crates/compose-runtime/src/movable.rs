//! Movable content, as Compose's `movableContentOf`: a lambda whose state and nodes move with it
//! when it's called from another place in the same pass (D1305).
//!
//! As Compose's, a call found where it was composes there, and any other inserts a placeholder and
//! composes nothing yet: when the pass has composed what it was going to, each placeholder takes
//! the content of its identity that was removed in it, if any, or composes new, in a walk of the
//! same pass (Compose's late changes). A removal parks the content's group rather than removing it:
//! out of the table's tree, its nodes out of theirs, its values kept. What's still parked when no
//! placeholder is left is removed, in rule R1's order, as what's removed anywhere.

use std::any::TypeId;
use std::cell::Cell;
use std::rc::Rc;

use crate::applier::NodeRef;
use crate::callback::Callback;
use crate::compose_arg::ComposeArg;
use crate::composer::{current, flush_node_moves};
use crate::error::{ComposeError, misuse};
use crate::hash::hash_key;
use crate::runtime::{CompositionId, Deferred, Runtime, with_runtime};
use crate::scope::Scopes;
use crate::slot_table::{
    CONTAINS_MOVABLE, GroupKind, MOVABLE, MOVABLE_PENDING, NONE, SENTINEL, SlotTable,
};

/// A group of content parked by a removal in the pass under way.
pub(crate) struct Parked {
    group: u32,
    /// Its top nodes, in no parent now.
    nodes: Vec<NodeRef>,
    /// The nearest provider where it was.
    provider: u32,
    /// It came from another composition's table: nothing it holds of where it was is any use, so
    /// all of it composes where it goes.
    transplanted: bool,
}

/// What a table holds of the movable content in the pass under way.
#[derive(Default)]
pub(crate) struct MovableState {
    /// A removal parks, as a pass that isn't a paused composition's does.
    pub parking: bool,
    /// Last parked last: an insertion takes the last, as `removeLast`.
    pub parked: Vec<Parked>,
    /// Placeholders not resolved yet.
    pub pending: u32,
    /// Nodes to take out of a node that's being removed, which would release them with it: its
    /// node, where they are, and how many, for `apply_node_op`, last first.
    pub detach: Vec<(NodeRef, u32, u32)>,
    /// What's removed of what stayed parked: for after what the pass remembered, before its effects.
    pub removed: Option<Box<Deferred>>,
    /// The sentinel's frame is walking for the placeholders, in the pass's end. What's invalid
    /// beside them waits for the next pass, as Compose's late changes run moved content only.
    pub walking: bool,
    /// What such a walk passed: invalid still, in need of the next pass.
    pub waiting: Vec<u32>,
    /// How many placeholders have been resolved: what tells a walk got somewhere.
    pub resolved: u64,
    /// The placeholders, for a walk to find again (those resolved are told by their flags).
    pub placeholders: Vec<u32>,
    /// A late pass takes only what's been removed, the rest waiting to see if a move frees more,
    /// as Compose doesn't mix moves with new content: identities with something to take.
    pub move_only: Option<Vec<u64>>,
    /// The contexts removed by this pass with compositions under them: what's in those is
    /// available too, as Compose's `reportAllMovableContent`.
    pub going_contexts: Vec<crate::context::ContextId>,
}

impl Parked {
    pub(crate) fn group(&self) -> u32 {
        self.group
    }
}

impl MovableState {
    /// Whether a pass has anything left to do for it, before it ends.
    #[inline]
    pub fn has_work(&self) -> bool {
        self.pending != 0 || !self.parked.is_empty()
    }
}

fn content_key<F: 'static>() -> u64 {
    hash_key(&TypeId::of::<F>())
}

struct MovableInner<P> {
    /// What a composite key hash holds of it: of the function, so two made of one hash alike, and
    /// not of where it's called, so what's in it hashes alike wherever it moves.
    key: u64,
    content: Box<dyn Fn(P)>,
}

/// A lambda whose state and nodes move with it when it's called from another place in the same
/// pass, as `movableContentOf`: its groups, what they remember, and the nodes they emit. Called
/// from two places at once, the second is another instance, and the state goes with one of them.
///
/// ```ignore
/// let content = remember(|| movable_content_of(|| Text("moves")));
/// if portrait { Column!(|| content()) } else { Row!(|| content()) }
/// ```
pub fn movable_content_of<F: Fn() + 'static>(content: F) -> Callback<dyn Fn()> {
    let inner = Rc::new(MovableInner::<()> {
        key: content_key::<F>(),
        content: Box::new(move |()| content()),
    });
    Callback::from_rc(Rc::new(move || compose_movable(inner.clone(), ())))
}

/// [`movable_content_of`] with a parameter, as `movableContentOf<P>`: a new one composes it where
/// it is. Several are a tuple, as the 2 to 4 of Compose's.
pub fn movable_content_of_with<P, F>(content: F) -> Callback<dyn Fn(P)>
where
    P: ComposeArg + Clone + 'static,
    F: Fn(P) + 'static,
{
    let inner = Rc::new(MovableInner { key: content_key::<F>(), content: Box::new(content) });
    Callback::from_rc(Rc::new(move |param| compose_movable(inner.clone(), param)))
}

/// [`movable_content_of_with`] with a receiver, as `movableContentWithReceiverOf`: its receiver
/// and parameters are what it's called with, a change of either composing it.
pub fn movable_content_with_receiver_of<R, P, F>(content: F) -> Callback<dyn Fn(R, P)>
where
    R: ComposeArg + Clone + 'static,
    P: ComposeArg + Clone + 'static,
    F: Fn(R, P) + 'static,
{
    let inner: Rc<MovableInner<(R, P)>> = Rc::new(MovableInner {
        key: content_key::<F>(),
        content: Box::new(move |(receiver, param)| content(receiver, param)),
    });
    Callback::from_rc(Rc::new(move |receiver, param| {
        compose_movable(inner.clone(), (receiver, param));
    }))
}

/// What a call of it keeps: its content, its parameter and the nearest provider, with what that
/// provided, when it last ran.
type MovableKept<P> = (Rc<MovableInner<P>>, P, Cell<(u32, u32)>);

fn compose_movable<P: ComposeArg + Clone + 'static>(inner: Rc<MovableInner<P>>, param: P) {
    let composer = current();
    let identity = Rc::as_ptr(&inner) as *const () as usize as u64;
    let start = start_movable_content(inner.key, identity);
    // What's provided here, which no state tells of a move or a static value's change, whose
    // forcing ended with the provider's frame by the time a late walk gets here: as Compose
    // compares its group's locals (`reader.groupAux != locals`).
    let locals = with_runtime(|runtime| runtime.locals_here());
    let mut last_locals = locals;
    let changed = composer.update_kept(
        |kept: &MovableKept<P>| {
            last_locals = kept.2.replace(locals);
            !kept.1.same(&param)
        },
        || (inner.clone(), param.clone(), Cell::new(locals)),
    );
    let locals_moved = last_locals != locals
        && with_runtime(|runtime| runtime.force_if_moved(last_locals, locals));
    let changed = changed || locals_moved;
    if start != Start::Placeholder {
        if !changed && composer.skipping() {
            composer.skip_to_group_end();
        } else {
            (inner.content)(param);
        }
    }
    composer.end_restart_group(|kept| {
        let (inner, param, _) =
            kept.downcast_ref::<MovableKept<P>>().expect("the kept arguments of its group");
        compose_movable(inner.clone(), param.clone());
    });
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Start {
    Composing,
    Placeholder,
}

fn start_movable_content(key: u64, identity: u64) -> Start {
    flush_node_moves();
    let (op, start, attach) = with_runtime(|runtime| runtime.start_movable_content(key, identity));
    crate::composer::apply_node_op(op);
    flush_detached();
    if let Some((parent, index, nodes)) = attach {
        let applier = with_runtime(|runtime| {
            runtime.node_changes += 1;
            runtime.active_applier()
        });
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        for (offset, node) in nodes.into_iter().enumerate() {
            applier.attach(parent, index as usize + offset, node);
        }
    }
    start
}

/// Takes out of `id`'s nodes what parked of them, with no pass of its own under way.
pub(crate) fn flush_detached_of(id: CompositionId) {
    let (applier, detach) = with_runtime(|runtime| {
        let Some(data) = runtime.compositions.get_mut(id) else {
            return (None, Vec::new());
        };
        let detach = std::mem::take(&mut data.table.movable.detach);
        let applier = data.applier.clone();
        (Some(crate::context::resolve_applier(&runtime.lent, &applier)), detach)
    });
    let Some(applier) = applier else { return };
    if detach.is_empty() {
        return;
    }
    let mut applier =
        applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
    for (parent, index, count) in detach.into_iter().rev() {
        if applier.contains(parent) {
            applier.remove(parent, index as usize, count as usize);
        }
    }
}

/// Takes out of the nodes being removed what's parked of theirs.
pub(crate) fn flush_detached() {
    let (applier, detach) = with_runtime(|runtime| {
        let active = runtime.active.expect("composing");
        let table = &mut runtime.compositions.data_mut(active.composition.index).table;
        if table.movable.detach.is_empty() {
            return (None, Vec::new());
        }
        let detach = std::mem::take(&mut table.movable.detach);
        (Some(runtime.active_applier()), detach)
    });
    let Some(applier) = applier else { return };
    let mut applier =
        applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
    for (parent, index, count) in detach.into_iter().rev() {
        applier.remove(parent, index as usize, count as usize);
    }
}

/// `group`'s top nodes, in order.
fn top_nodes(table: &SlotTable, group: u32, nodes: &mut Vec<NodeRef>) {
    let mut child = table[group].first_child;
    while child != NONE {
        let entry = &table[child];
        if entry.kind == GroupKind::Node {
            nodes.push(entry.node);
        } else if entry.node_count > 0 {
            top_nodes(table, child, nodes);
        }
        child = entry.next_sibling;
    }
}

/// What's under `group` contributes this many nodes, what parked having gone from it.
fn recount_nodes(table: &mut SlotTable, group: u32) -> u32 {
    if table[group].kind == GroupKind::Node {
        return 1;
    }
    let (mut count, mut child) = (0, table[group].first_child);
    while child != NONE {
        count += recount_nodes(table, child);
        child = table[child].next_sibling;
    }
    table[group].node_count = count;
    count
}

/// Takes `child` out of `parent`'s children, after `previous`.
fn unlink(table: &mut SlotTable, parent: u32, previous: u32, child: u32) {
    let next = table[child].next_sibling;
    table.link_after(parent, previous, next);
    table[child].next_sibling = NONE;
    table[child].parent = NONE;
}

/// The placeholders in what's under `group`: none of them are reached while it's parked.
fn placeholders_under(table: &SlotTable, group: u32) -> u32 {
    let mut count = 0;
    let mut child = table[group].first_child;
    while child != NONE {
        let entry = &table[child];
        if entry.flags & MOVABLE_PENDING != 0 {
            count += 1;
        } else if entry.flags & (MOVABLE | CONTAINS_MOVABLE) != 0 {
            count += placeholders_under(table, child);
        }
        child = entry.next_sibling;
    }
    count
}

fn park(table: &mut SlotTable, group: u32) {
    table.movable.pending -= placeholders_under(table, group);
    let mut nodes = Vec::with_capacity(table[group].node_count as usize);
    top_nodes(table, group, &mut nodes);
    let provider = table[group].enclosing_provider;
    table.movable.parked.push(Parked { group, nodes, provider, transplanted: false });
}

/// `group`'s going: if it's a context's, what's under that goes too, at the end of the pass.
fn note_going_context(table: &mut SlotTable, group: u32) {
    if let Some(context) = crate::context::context_of_group(table, group) {
        table.movable.going_contexts.push(context);
    }
}

/// A removal reached `root`, which is or holds movable content: parks it. Returns whether `root`
/// itself was, so nothing's left to remove.
pub(crate) fn park_in(table: &mut SlotTable, root: u32) -> bool {
    let flags = table[root].flags;
    if flags & MOVABLE_PENDING != 0 {
        table.movable.pending -= 1;
        return false;
    }
    if flags & MOVABLE != 0 {
        park(table, root);
        return true;
    }
    note_going_context(table, root);
    if table[root].kind == GroupKind::Node {
        park_children(table, root, Some(table[root].node), &mut 0);
    } else {
        park_children(table, root, None, &mut 0);
    }
    false
}

/// Parks what's under `group`: its nodes are under `node`, at `index` on, if that's in what's
/// removed, or go with what's removing it.
fn park_children(table: &mut SlotTable, group: u32, node: Option<NodeRef>, index: &mut u32) {
    let (mut previous, mut child) = (NONE, table[group].first_child);
    while child != NONE {
        let entry = &table[child];
        let (next, flags, kind, count, child_node) =
            (entry.next_sibling, entry.flags, entry.kind, entry.node_count, entry.node);
        if flags & MOVABLE_PENDING != 0 {
            // A placeholder removed: nothing to resolve, and it holds nothing.
            table.movable.pending -= 1;
            previous = child;
        } else if flags & MOVABLE != 0 {
            if let Some(node) = node
                && count > 0
            {
                table.movable.detach.push((node, *index, count));
            }
            *index += count;
            park(table, child);
            unlink(table, group, previous, child);
        } else if flags & CONTAINS_MOVABLE == 0 {
            *index += count;
            previous = child;
        } else if kind == GroupKind::Node {
            note_going_context(table, child);
            park_children(table, child, Some(child_node), &mut 0);
            *index += count;
            previous = child;
        } else {
            note_going_context(table, child);
            park_children(table, child, node, index);
            previous = child;
        }
        child = next;
    }
}

/// Finds content of `identity` under `group`: its previous sibling, where its nodes are if under a
/// node in there, and its index there or among `group`'s top nodes.
fn find_nested(
    table: &SlotTable,
    group: u32,
    identity: u64,
    node: Option<NodeRef>,
    index: &mut u32,
) -> Option<(u32, u32, Option<NodeRef>, u32)> {
    let (mut previous, mut child) = (NONE, table[group].first_child);
    while child != NONE {
        let entry = &table[child];
        if entry.flags & MOVABLE != 0
            && entry.flags & MOVABLE_PENDING == 0
            && entry.data_key == identity
        {
            return Some((child, previous, node, *index));
        }
        if entry.flags & (MOVABLE | CONTAINS_MOVABLE) != 0 {
            let found = if entry.kind == GroupKind::Node {
                find_nested(table, child, identity, Some(entry.node), &mut 0)
            } else {
                find_nested(table, child, identity, node, index)
            };
            if found.is_some() {
                return found;
            }
            if entry.kind == GroupKind::Node {
                *index += entry.node_count;
            }
        } else {
            *index += entry.node_count;
        }
        previous = child;
        child = entry.next_sibling;
    }
    None
}

/// Takes content of `identity` from what's parked, the last parked first: at the top of what's
/// parked, or out of what's in there.
fn take_parked(table: &mut SlotTable, identity: u64) -> Option<Parked> {
    let top = table
        .movable
        .parked
        .iter()
        .rposition(|parked| table.groups[parked.group as usize].data_key == identity);
    if let Some(at) = top {
        let parked = table.movable.parked.remove(at);
        table.movable.pending += placeholders_under(table, parked.group);
        return Some(parked);
    }
    for at in (0..table.movable.parked.len()).rev() {
        let outer = table.movable.parked[at].group;
        if table[outer].flags & CONTAINS_MOVABLE == 0 {
            continue;
        }
        let Some((group, previous, node, index)) =
            find_nested(table, outer, identity, None, &mut 0)
        else {
            continue;
        };
        let count = table[group].node_count;
        // What's around it has that many nodes fewer, up to a node.
        let mut around = table[group].parent;
        loop {
            let entry = &mut table[around];
            if entry.kind == GroupKind::Node {
                break;
            }
            entry.node_count -= count;
            if around == outer {
                break;
            }
            around = entry.parent;
        }
        let parent = table[group].parent;
        unlink(table, parent, previous, group);
        match node {
            Some(node) if count > 0 => table.movable.detach.push((node, index, count)),
            Some(_) => {}
            None => {
                let start = index as usize;
                table.movable.parked[at].nodes.drain(start..start + count as usize);
            }
        }
        let mut nodes = Vec::with_capacity(count as usize);
        top_nodes(table, group, &mut nodes);
        let provider = table[group].enclosing_provider;
        table.movable.pending += placeholders_under(table, group);
        return Some(Parked { group, nodes, provider, transplanted: false });
    }
    None
}

/// Whether there's content of `identity` to take from what's parked in `table`.
fn can_take(table: &SlotTable, identity: u64) -> bool {
    table.movable.parked.iter().any(|parked| {
        let group = &table[parked.group];
        group.data_key == identity
            || (group.flags & CONTAINS_MOVABLE != 0
                && find_nested(table, parked.group, identity, None, &mut 0).is_some())
    })
}

/// Moves `old` of `source` and what's under it to `target`, as a tree of its own to be linked:
/// its scopes and contexts follow it, and what's nested provides from the new groups.
#[allow(clippy::too_many_arguments)]
fn move_group(
    source: &mut SlotTable,
    target: &mut SlotTable,
    scopes: &mut Scopes,
    contexts: &mut crate::context::Contexts,
    target_id: CompositionId,
    moved_contexts: &mut Vec<crate::context::ContextId>,
    old: u32,
    parent: u32,
    provider: u32,
) -> u32 {
    let new = {
        let group = &source[old];
        target.alloc(group.kind, group.key, group.data_key, parent)
    };
    let context = crate::context::context_of_group(source, old);
    let group = &mut source[old];
    let (first_child, scope) = (group.first_child, group.scope);
    {
        let moved = &mut target[new];
        moved.dirty = group.dirty;
        moved.flags = group.flags;
        moved.slot_position = group.slot_position;
        moved.enclosing_provider = provider;
        moved.node_count = group.node_count;
        moved.node = group.node;
        moved.scope = scope;
        moved.slots = std::mem::take(&mut group.slots);
        moved.kept = group.kept.take();
        moved.restart = group.restart;
        moved.identity = group.identity.take();
    }
    group.scope = crate::scope::ScopeId::NONE;
    if let Some(entry) = scopes.get_mut(scope) {
        entry.composition = target_id.index;
        entry.group = new;
    }
    if let Some(context) = context {
        contexts.moved_to(context, target_id, new);
        moved_contexts.push(context);
    }
    if target[new].flags & MOVABLE_PENDING != 0 {
        target.movable.placeholders.push(new);
    }
    let inner = if target[new].is_provider() { new } else { provider };
    let (mut previous, mut child) = (NONE, first_child);
    while child != NONE {
        let next = source[child].next_sibling;
        let moved = move_group(
            source,
            target,
            scopes,
            contexts,
            target_id,
            moved_contexts,
            child,
            new,
            inner,
        );
        target.link_after(new, previous, moved);
        previous = moved;
        child = next;
    }
    source.free(old);
    new
}

/// Points what's in `group`, which moved, at the providers where it is.
fn relink_providers(table: &mut SlotTable, group: u32, provider: u32) {
    table[group].enclosing_provider = provider;
    let inner = if table[group].is_provider() { group } else { provider };
    let mut child = table[group].first_child;
    while child != NONE {
        relink_providers(table, child, inner);
        child = table[child].next_sibling;
    }
}

/// What a movable content's nodes are attached to, where and which.
type Attach = (NodeRef, u32, Vec<NodeRef>);

impl Runtime {
    fn start_movable_content(
        &mut self,
        key: u64,
        identity: u64,
    ) -> (crate::composer::NodeOp, Start, Option<Attach>) {
        let composition = self.check_composing();
        let parent = *self.frames.last().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        let table = &mut self.compositions.data_mut(composition).table;
        // A placeholder reached by the walk that resolves it.
        let resolving = parent.cursor != NONE
            && parent.cursor != parent.limit
            && table[parent.cursor].flags & MOVABLE_PENDING != 0
            && table[parent.cursor].data_key == identity;
        let mut adopted = None;
        if resolving {
            table.movable.pending -= 1;
            table.movable.resolved += 1;
            table[parent.cursor].flags &= !MOVABLE_PENDING;
            let taken = match take_parked(table, identity) {
                Some(parked) => Some(parked),
                None => self.take_from_another(composition, identity),
            };
            if let Some(parked) = taken {
                self.adopt_for_placeholder(
                    composition,
                    parent.cursor,
                    parent.last_placed,
                    parent.group,
                    &parked,
                );
                adopted = Some(parked);
            }
        }
        let (op, _) = self.start_group(GroupKind::Restart, key, identity, None, false);
        let Runtime { compositions, scopes, frames, recorded_changes, providers_invalid, .. } =
            self;
        let table = &mut compositions.data_mut(composition).table;
        let frame = frames.last_mut().expect("just started");
        let group = frame.group;
        if let Some(parked) = adopted {
            *recorded_changes += 1;
            let provider = table[group].enclosing_provider;
            let attach = (frame.node_parent, frame.node_index, parked.nodes);
            if parked.provider != provider {
                relink_providers(table, group, provider);
            }
            // From another table: what its nodes and calls keep of places is of that one.
            if parked.transplanted {
                frame.forced = true;
                if *providers_invalid == 0 {
                    *providers_invalid = frames.len();
                }
            }
            return (op, Start::Composing, Some(attach));
        }
        if resolving {
            // Nothing to take: new content, as it was going to be.
            frame.inserting = true;
            return (op, Start::Composing, None);
        }
        // A placeholder met elsewhere than at the cursor, a body around it having run again: new.
        if table[group].flags & MOVABLE_PENDING != 0 {
            table[group].flags &= !MOVABLE_PENDING;
            table.movable.pending -= 1;
            table.movable.resolved += 1;
            frame.inserting = true;
            return (op, Start::Composing, None);
        }
        if !frame.inserting {
            return (op, Start::Composing, None);
        }
        table[group].flags |= MOVABLE;
        // The groups holding it, those holding a composition it's in too: a removal looks in them.
        let mut around = table[group].parent;
        while around != NONE && table[around].flags & CONTAINS_MOVABLE == 0 {
            table[around].flags |= CONTAINS_MOVABLE;
            around = table[around].parent;
        }
        if around == NONE {
            compositions.mark_contains_movable_above(composition);
        }
        let table = &mut compositions.data_mut(composition).table;
        // In paused content: new, as paused content doesn't take what's removed (D1305).
        if frame.region != crate::pausable::NO_REGION {
            return (op, Start::Composing, None);
        }
        table[group].flags |= MOVABLE_PENDING;
        table.movable.pending += 1;
        table.movable.placeholders.push(group);
        if let Some(scope) = scopes.get_mut(table[group].scope) {
            scope.invalid = true;
        }
        let mut around = table[group].parent;
        while around != NONE && !table[around].dirty {
            table[around].dirty = true;
            around = table[around].parent;
        }
        (op, Start::Placeholder, None)
    }

    /// The nearest provider of the group composing now, with what it provides as a number.
    fn locals_here(&self) -> (u32, u32) {
        let composition = self.active.expect("composing").composition.index;
        let frame = self.frames.last().expect("composing");
        let table = &self.compositions.data(composition).table;
        let provider = table[frame.group].enclosing_provider;
        if provider == NONE {
            return (NONE, 0);
        }
        (provider, table[provider].provider_version())
    }

    /// The group composing now last composed under `last`, and is under `now`: if what's provided
    /// isn't the same, all of it composes. Returns whether it does.
    fn force_if_moved(&mut self, last: (u32, u32), now: (u32, u32)) -> bool {
        let composition = self.active.expect("composing").composition.index;
        let table = &self.compositions.data(composition).table;
        // A provider that's still what it was can be asked what it provides.
        let known = last.0 == NONE
            || table.groups.get(last.0 as usize).is_some_and(|group| {
                group.kind != GroupKind::Free
                    && group.is_provider()
                    && group.provider_version() == last.1
            });
        let same =
            known && crate::local::same_locals_at(&self.compositions, composition, last.0, now.0);
        if same {
            return false;
        }
        if self.providers_invalid == 0 {
            self.providers_invalid = self.frames.len();
        }
        self.frames.last_mut().expect("composing").forced = true;
        true
    }

    /// Content of `identity` parked in another composition this one shares its nodes' tree with, the
    /// last to be available first: its groups come to this table.
    fn take_from_another(&mut self, composition: u32, identity: u64) -> Option<Parked> {
        let target_id = self.compositions.id_at(composition);
        let applier = self.compositions.get(target_id)?.applier.clone();
        let from = self.movable_pool.iter().rev().copied().find(|id| {
            *id != target_id
                && self.compositions.get(*id).is_some_and(|data| {
                    Rc::ptr_eq(&data.applier, &applier) && can_take(&data.table, identity)
                })
        })?;
        let source = &mut self.compositions.data_mut(from.index).table;
        let parked = take_parked(source, identity)?;
        // What a nested one's taking out of what's around it is done by this pass.
        let detach = std::mem::take(&mut source.movable.detach);
        let pending = placeholders_under(source, parked.group);
        // Not parked in `source` now: those of it are counted where it goes.
        source.movable.pending -= pending;
        let group = self.transplant(from, target_id, parked.group);
        let target = &mut self.compositions.data_mut(composition).table;
        target.movable.detach.extend(detach);
        target.movable.pending += pending;
        Some(Parked { group, nodes: parked.nodes, provider: NONE, transplanted: true })
    }

    /// Moves `group`, in no tree, from `from`'s table to `to`'s: what's under its contexts is under
    /// `to` from now on.
    fn transplant(&mut self, from: CompositionId, to: CompositionId, group: u32) -> u32 {
        let Runtime { compositions, scopes, .. } = self;
        let mut contexts = std::mem::take(&mut compositions.contexts);
        let mut moved_contexts = Vec::new();
        let (source, target) = compositions.two_tables(from, to);
        let new = move_group(
            source,
            target,
            scopes,
            &mut contexts,
            to,
            &mut moved_contexts,
            group,
            NONE,
            NONE,
        );
        compositions.contexts = contexts;
        compositions.children_moved(from, to, &moved_contexts);
        new
    }

    /// `id`, about to go, has content parked for someone to take: it's its parent's to hold, while
    /// there's a parent. It stays available.
    pub(crate) fn rehome_parked(&mut self, id: CompositionId) {
        let Some(data) = self.compositions.get(id) else { return };
        if data.table.movable.parked.is_empty() {
            return;
        }
        let parent = data.parent.and_then(|place| self.compositions.contexts.parent_of(place));
        let Some(parent) = parent.filter(|parent| self.compositions.get(*parent).is_some()) else {
            return;
        };
        let source = &mut self.compositions.data_mut(id.index).table;
        let parked = std::mem::take(&mut source.movable.parked);
        for one in parked {
            let group = self.transplant(id, parent, one.group);
            let target = &mut self.compositions.data_mut(parent.index).table;
            target.movable.pending += placeholders_under(target, group);
            target.movable.parked.push(Parked {
                group,
                nodes: one.nodes,
                provider: NONE,
                transplanted: true,
            });
        }
        if !self.movable_pool.contains(&parent) {
            self.movable_pool.push(parent);
        }
    }

    /// The identities `composition`'s placeholders can take from what's parked now.
    pub(crate) fn movable_available(&self, composition: CompositionId) -> Vec<u64> {
        let Some(data) = self.compositions.get(composition) else { return Vec::new() };
        let table = &data.table;
        let mut available = Vec::new();
        for group in &table.movable.placeholders {
            if table[*group].flags & MOVABLE_PENDING == 0
                || table[*group].kind != GroupKind::Restart
            {
                continue;
            }
            let identity = table[*group].data_key;
            let there = can_take(table, identity)
                || self.movable_pool.iter().any(|id| {
                    *id != composition
                        && self.compositions.get(*id).is_some_and(|other| {
                            Rc::ptr_eq(&other.applier, &data.applier)
                                && can_take(&other.table, identity)
                        })
                });
            if there {
                available.push(identity);
            }
        }
        available
    }

    /// What's under a context the pass removed: its content parks, for who wants it.
    pub(crate) fn park_going_children(&mut self, id: CompositionId) -> Vec<CompositionId> {
        let mut parked = Vec::new();
        if self.compositions.get(id).is_none_or(|data| data.table.movable.going_contexts.is_empty())
        {
            return parked;
        }
        let mut going = vec![id];
        while let Some(composition) = going.pop() {
            let Some(data) = self.compositions.get_mut(composition) else { continue };
            let contexts = std::mem::take(&mut data.table.movable.going_contexts);
            if contexts.is_empty() {
                continue;
            }
            for child in self.compositions.children_of(&contexts) {
                let data = self.compositions.get_mut(child).expect("just listed");
                let root = data.root_node;
                let table = &mut data.table;
                if table[SENTINEL].flags & CONTAINS_MOVABLE == 0 {
                    continue;
                }
                park_children(table, SENTINEL, Some(root), &mut 0);
                recount_nodes(table, SENTINEL);
                parked.push(child);
                going.push(child);
            }
        }
        parked
    }

    /// `parked` goes where `placeholder` is, which goes.
    fn adopt_for_placeholder(
        &mut self,
        composition: u32,
        placeholder: u32,
        last_placed: u32,
        parent: u32,
        parked: &Parked,
    ) {
        let Runtime { compositions, scopes, frames, reusing, .. } = self;
        let data = compositions.data_mut(composition);
        let table = &mut data.table;
        let group = parked.group;
        let next = table[placeholder].next_sibling;
        table[group].next_sibling = next;
        table[group].parent = parent;
        table.link_after(parent, last_placed, group);
        // Its provider comes from where it is: that of the placeholder, of the same parent.
        table[group].enclosing_provider = table[placeholder].enclosing_provider;
        retire_placeholder(table, &mut data.deferred, scopes, placeholder);
        let frame = frames.last_mut().expect("the walking frame");
        frame.cursor = group;
        // Deactivated where it was: it comes back as content being reused does.
        if table[group].scope.is_none() && *reusing == 0 {
            *reusing = frames.len() + 1;
        }
    }

    /// The pass composed what it was going to, and is about to walk for what's left of its movable
    /// content: every placeholder's scope is invalid, and the sentinel's frame walks to them.
    pub(crate) fn begin_movable_walk(&mut self, composition: u32) {
        let frame = self.frames.last_mut().expect("the sentinel frame");
        let table = &mut self.compositions.data_mut(composition).table;
        // Those a walk passed for another's content have their way back.
        let mut placeholders = std::mem::take(&mut table.movable.placeholders);
        placeholders.retain(|group| {
            table[*group].kind == GroupKind::Restart && table[*group].flags & MOVABLE_PENDING != 0
        });
        for group in &placeholders {
            let mut around = table[*group].parent;
            while around != NONE && !table[around].dirty {
                table[around].dirty = true;
                around = table[around].parent;
            }
        }
        table.movable.placeholders = placeholders;
        table[crate::slot_table::SENTINEL].dirty = false;
        table.movable.walking = true;
        frame.mode = crate::composer::Mode::Walk;
        frame.skipped = true;
        frame.cursor = table[crate::slot_table::SENTINEL].first_child;
        frame.last_placed = NONE;
        frame.node_index = frame.node_start;
        frame.limit = NONE;
    }

    /// What the walks passed is invalid still: the next pass reaches it, and a frame's asked.
    pub(crate) fn leave_for_the_next_pass(&mut self, composition: u32) {
        let Runtime { compositions, scopes, frame_requests, .. } = self;
        let table = &mut compositions.data_mut(composition).table;
        table.movable.walking = false;
        let waiting = std::mem::take(&mut table.movable.waiting);
        let mut any = false;
        for group in waiting {
            let invalid = table[group].kind == GroupKind::Restart
                && scopes.get(table[group].scope).is_some_and(|scope| scope.invalid);
            if !invalid {
                continue;
            }
            any = true;
            let mut around = table[group].parent;
            while around != NONE && !table[around].dirty {
                table[around].dirty = true;
                around = table[around].parent;
            }
        }
        let data = compositions.data_mut(composition);
        if any && !data.pending {
            data.pending = true;
            let id = compositions.id_at(composition);
            compositions.queue_in_parents(composition);
            let data = compositions.get(id).expect("composing");
            frame_requests.extend(compositions.frame_requester_of(data));
            frame_requests.extend(compositions.recomposer_wake.clone());
        }
    }

    /// No placeholder is left: what's parked is removed, its nodes already out of the tree.
    pub(crate) fn remove_parked(&mut self, composition: u32) {
        let Runtime { compositions, scopes, removal_stack, .. } = self;
        let data = compositions.data_mut(composition);
        let parked = std::mem::take(&mut data.table.movable.parked);
        data.table.movable.parking = false;
        // What it holds goes after what the pass remembered, before its effects: one panicking
        // loses nothing of it.
        let mut removed = Deferred::default();
        for parked in parked {
            crate::runtime::remove_parked_group(
                &mut data.table,
                &mut removed,
                scopes,
                removal_stack,
                parked.group,
            );
        }
        if !removed.is_empty() {
            data.table.movable.removed = Some(Box::new(removed));
        }
    }
}

/// `placeholder` gave its place to what it was for.
fn retire_placeholder(
    table: &mut SlotTable,
    deferred: &mut Deferred,
    scopes: &mut Scopes,
    placeholder: u32,
) {
    let group = &mut table[placeholder];
    scopes.free(group.scope);
    group.scope = crate::scope::ScopeId::NONE;
    if let Some(kept) = group.kept.take() {
        deferred.removed_values.push(crate::slot_table::SlotValue::Plain(kept));
    }
    group.slots.clear();
    table.free(placeholder);
}
