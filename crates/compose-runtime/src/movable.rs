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
use crate::runtime::{Deferred, Runtime, with_runtime};
use crate::scope::Scopes;
use crate::slot_table::{CONTAINS_MOVABLE, GroupKind, MOVABLE, MOVABLE_PENDING, NONE, SlotTable};

/// A group of content parked by a removal in the pass under way.
pub(crate) struct Parked {
    group: u32,
    /// Its top nodes, in no parent now.
    nodes: Vec<NodeRef>,
    /// The nearest provider where it was.
    provider: u32,
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
    table.movable.parked.push(Parked { group, nodes, provider });
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
            park_children(table, child, Some(child_node), &mut 0);
            *index += count;
            previous = child;
        } else {
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
        return Some(Parked { group, nodes, provider });
    }
    None
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
            if let Some(parked) = take_parked(table, identity) {
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
        let Runtime { compositions, scopes, frames, recorded_changes, .. } = self;
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
        // The groups holding it: a removal looks in them.
        let mut around = table[group].parent;
        while around != NONE && table[around].flags & CONTAINS_MOVABLE == 0 {
            table[around].flags |= CONTAINS_MOVABLE;
            around = table[around].parent;
        }
        // In paused content: new, as paused content doesn't take what's removed (D1305).
        if frame.region != crate::pausable::NO_REGION {
            return (op, Start::Composing, None);
        }
        table[group].flags |= MOVABLE_PENDING;
        table.movable.pending += 1;
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
    fn leave_for_the_next_pass(&mut self, composition: u32) {
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
        self.leave_for_the_next_pass(composition);
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
