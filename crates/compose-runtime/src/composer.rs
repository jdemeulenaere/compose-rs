//! The composer: the protocol generated code drives, and the pass mechanics behind it.
//!
//! # Frames
//!
//! Each open group has a frame. A frame splits its group's children in two: the ones placed so
//! far in this pass (up to `last_placed`), followed by the old ones not yet matched (from
//! `cursor`). Starting a child group reuses the first old child with the same kind and keys,
//! moving it in front of the cursor if it isn't there already, or inserts a new one. Ending a
//! group removes the old children nobody matched.
//!
//! A frame is in one of three modes:
//! - `Normal`: the group's body is running.
//! - `Walk`: the group's body was skipped, but some descendant scopes are invalid; the runtime
//!   walks the children, descending into dirty ones and restarting invalid scopes.
//! - `Restart`: a walking frame whose child at `cursor` is being restarted; groups started by the
//!   restart function match only within `[cursor, limit)`.

use std::any::{Any, TypeId};
use std::marker::PhantomData;
use std::rc::Rc;

use crate::applier::{Applier, NodeRef};
use crate::error::{ComposeError, misuse};
use crate::runtime::{DeferredEffect, Runtime, remove_siblings, try_with_runtime, with_runtime};
use crate::scope::{RecomposeScope, ScopeId};
use crate::slot_table::{GroupKind, NONE, Slot, SlotValue};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Normal,
    Walk,
    Restart,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    pub group: u32,
    pub mode: Mode,
    /// The group is new in this pass: every child is new too.
    pub inserting: bool,
    /// The group's scope was invalid when it started: its body must run.
    pub forced: bool,
    /// `skip_to_group_end` was called: the remaining children and slots are kept.
    pub skipped: bool,
    /// The group had invalid descendant scopes when it started.
    pub dirty: bool,
    /// The first old child not matched yet.
    pub cursor: u32,
    /// In `Restart` mode, the end of the range the restart function matches in.
    pub limit: u32,
    pub last_placed: u32,
    pub slot_cursor: u32,
    /// The node the group's nodes are children of.
    pub node_parent: NodeRef,
    /// The index in `node_parent` of the next node.
    pub node_index: u32,
    pub node_start: u32,
    /// The scope state reads register: this restart group's, or the nearest enclosing one's.
    pub scope: ScopeId,
    /// The scope's epoch before this run.
    pub saved_epoch: u32,
    /// The body is known to run (`skipping()` said no, or the kept arguments changed): the
    /// scope's epoch was bumped, so reads from earlier runs that aren't repeated go stale.
    pub body_started: bool,
}

/// A change to the node tree, applied right after the runtime's borrow ends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NodeOp {
    None,
    Move { parent: NodeRef, from: u32, to: u32, count: u32 },
    Remove { parent: NodeRef, index: u32, count: u32 },
}

pub(crate) enum NodeStart {
    NotNode,
    Reused(NodeRef),
    Insert { parent: NodeRef, index: u32 },
}

pub(crate) enum WalkStep {
    Done,
    Restart(fn(&dyn Any), Option<Rc<dyn Any>>),
}

/// A restart group's kept arguments, when it has any.
type Kept = Option<Rc<dyn Any>>;

/// Where `slot_next` reserved a slot, so `slot_set` writes exactly there.
#[derive(Clone, Copy)]
pub(crate) struct SlotPosition {
    depth: u32,
    group: u32,
    index: u32,
}

impl Runtime {
    /// The index of the composition whose pass is composing.
    #[inline]
    fn check_composing(&self) -> u32 {
        match self.active {
            Some(active) if active.composing => active.composition.index,
            _ => misuse(ComposeError::NotComposing),
        }
    }

    /// The top frame, which must be running its body.
    #[inline]
    fn body_frame(&mut self) -> &mut Frame {
        // The bottom frame belongs to the sentinel, whose "body" is the pass driver itself.
        let depth = self.frames.len();
        let frame = self.frames.last_mut().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        if frame.mode != Mode::Normal || depth < 2 {
            misuse(ComposeError::RestartWithoutGroup)
        }
        if frame.skipped {
            misuse(ComposeError::UseAfterSkip)
        }
        frame
    }

    pub(crate) fn start_group(
        &mut self,
        kind: GroupKind,
        key: u64,
        data_key: u64,
        identity: Option<TypeId>,
    ) -> (NodeOp, NodeStart) {
        let composition = self.check_composing();
        let Runtime { compositions, scopes, frames, .. } = self;
        let table = &mut compositions.data_mut(composition).table;
        let parent = frames.last_mut().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        match parent.mode {
            Mode::Walk => misuse(ComposeError::UnbalancedGroups),
            Mode::Normal if parent.skipped => misuse(ComposeError::UseAfterSkip),
            // A walking frame is skipped by definition; the restart function starts one group.
            Mode::Normal | Mode::Restart => {}
        }

        let mut op = NodeOp::None;
        let mut found = NONE;
        if !parent.inserting {
            let mut previous = parent.last_placed;
            let mut current = parent.cursor;
            let mut nodes_before = 0;
            while current != parent.limit && current != NONE {
                let group = &table[current];
                // A restart group of another composable with the same key (an instantiation of a
                // generic composable, a same-named composable) doesn't match: it would lend its
                // kept arguments and state.
                let same_composable = match (group.identity, identity) {
                    (Some(old), Some(new)) => old == new,
                    _ => true,
                };
                if group.key == key
                    && group.kind == kind
                    && group.data_key == data_key
                    && same_composable
                {
                    found = current;
                    break;
                }
                nodes_before += group.node_count;
                previous = current;
                current = group.next_sibling;
            }
            if found != NONE {
                if found == parent.cursor {
                    parent.cursor = table[found].next_sibling;
                } else {
                    // Move the match in front of the cursor, and its nodes with it.
                    table[previous].next_sibling = table[found].next_sibling;
                    table[found].next_sibling = parent.cursor;
                    table.link_after(parent.group, parent.last_placed, found);
                    let count = table[found].node_count;
                    // Past node-less groups only, its nodes are already in place.
                    if count > 0 && nodes_before > 0 {
                        op = NodeOp::Move {
                            parent: parent.node_parent,
                            from: parent.node_index + nodes_before,
                            to: parent.node_index,
                            count,
                        };
                    }
                }
                parent.last_placed = found;
            }
        }

        let inserting = found == NONE;
        let index = if inserting {
            let index = table.alloc(kind, key, data_key, parent.group);
            table[index].identity = identity;
            table[index].next_sibling = parent.cursor;
            table.link_after(parent.group, parent.last_placed, index);
            parent.last_placed = index;
            index
        } else {
            found
        };

        let parent = *parent;
        let group = &mut table[index];
        let dirty = group.dirty;
        group.dirty = false;
        let mut frame = Frame {
            group: index,
            mode: Mode::Normal,
            inserting,
            forced: false,
            skipped: false,
            dirty,
            cursor: if inserting { NONE } else { group.first_child },
            limit: NONE,
            last_placed: NONE,
            slot_cursor: 0,
            node_parent: parent.node_parent,
            node_index: parent.node_index,
            node_start: parent.node_index,
            scope: parent.scope,
            saved_epoch: 0,
            body_started: false,
        };
        let node_start = match kind {
            GroupKind::Node => {
                frame.node_parent = group.node;
                frame.node_index = 0;
                frame.node_start = 0;
                if inserting {
                    NodeStart::Insert { parent: parent.node_parent, index: parent.node_index }
                } else {
                    NodeStart::Reused(group.node)
                }
            }
            GroupKind::Restart => {
                if inserting {
                    group.scope = scopes.alloc(composition, index);
                }
                let scope = scopes.get_mut(group.scope).expect("restart group without a scope");
                frame.forced = scope.invalid;
                scope.invalid = false;
                // The epoch is bumped only once the body is known to run (`begin_body`): until
                // then the scope may still skip, and its reads from the last run stay current.
                frame.saved_epoch = scope.epoch;
                frame.scope = group.scope;
                NodeStart::NotNode
            }
            _ => NodeStart::NotNode,
        };
        frames.push(frame);
        (op, node_start)
    }

    /// Records the node a new node group inserted.
    pub(crate) fn set_inserted_node(&mut self, node: NodeRef) {
        let composition = self.check_composing();
        let Runtime { compositions, frames, .. } = self;
        let table = &mut compositions.data_mut(composition).table;
        let frame = frames.last_mut().expect("a node frame");
        let group = &mut table[frame.group];
        assert!(
            group.kind == GroupKind::Node && group.node.is_none() && frame.inserting,
            "compose-runtime: a node factory used the composer"
        );
        group.node = node;
        frame.node_parent = node;
    }

    pub(crate) fn end_group(&mut self, restart: Option<fn(&dyn Any)>) -> NodeOp {
        let composition = self.check_composing();
        let Runtime { compositions, scopes, frames, removal_stack, abandoned_groups, .. } = self;
        let data = compositions.data_mut(composition);
        let (table, deferred) = (&mut data.table, &mut data.deferred);
        if frames.len() < 2 {
            misuse(ComposeError::UnbalancedGroups)
        }
        let frame = *frames.last().expect("checked above");
        let kind = table[frame.group].kind;
        if frame.mode != Mode::Normal || (kind == GroupKind::Restart) != restart.is_some() {
            misuse(if *abandoned_groups {
                ComposeError::CaughtPanic
            } else {
                ComposeError::UnbalancedGroups
            })
        }
        frames.pop();

        let mut op = NodeOp::None;
        if !frame.skipped {
            if frame.cursor != NONE {
                let count =
                    remove_siblings(table, deferred, scopes, removal_stack, frame.cursor, NONE);
                table.set_next(frame.group, frame.last_placed, NONE);
                if count > 0 {
                    op = NodeOp::Remove {
                        parent: frame.node_parent,
                        index: frame.node_index,
                        count,
                    };
                }
            }
            let slots = &mut table[frame.group].slots;
            let keep = (frame.slot_cursor as usize).min(slots.len());
            deferred.removed_values.extend(slots.drain(keep..).map(|slot| slot.value));
        }

        let group = &mut table[frame.group];
        let count = if kind == GroupKind::Node { 1 } else { frame.node_index - frame.node_start };
        group.node_count = count;
        if restart.is_some() {
            group.restart = restart;
            if frame.skipped && frame.body_started {
                // Skipped after all (a protocol slip): reads from the last run still belong.
                if let Some(scope) = scopes.get_mut(group.scope) {
                    scope.epoch = frame.saved_epoch;
                }
            }
        }
        let parent = frames.last_mut().expect("the sentinel frame");
        parent.node_index += count;
        op
    }

    /// Starts skipping the rest of the current group. Returns the depth to walk from if some
    /// descendant scopes are invalid.
    pub(crate) fn begin_skip(&mut self) -> Option<usize> {
        let composition = self.check_composing();
        let Runtime { compositions, frames, .. } = self;
        let table = &compositions.data_mut(composition).table;
        let depth = frames.len();
        let frame = frames.last_mut().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        if frame.mode != Mode::Normal || depth < 2 {
            misuse(ComposeError::UnbalancedGroups)
        }
        if frame.skipped {
            misuse(ComposeError::UseAfterSkip)
        }
        frame.skipped = true;
        if frame.dirty {
            frame.mode = Mode::Walk;
            return Some(depth - 1);
        }
        let group = &table[frame.group];
        if frame.cursor == group.first_child
            && frame.last_placed == NONE
            && group.kind != GroupKind::Node
        {
            frame.node_index = frame.node_start + group.node_count;
        } else {
            let mut current = frame.cursor;
            while current != NONE {
                frame.node_index += table[current].node_count;
                current = table[current].next_sibling;
            }
        }
        frame.cursor = NONE;
        None
    }

    /// Advances the walk started at `base` to the next invalid scope.
    pub(crate) fn walk_step(&mut self, base: usize) -> WalkStep {
        let Some(active) = self.active else { misuse(ComposeError::NotComposing) };
        let Runtime { compositions, scopes, frames, .. } = self;
        let table = &mut compositions.data_mut(active.composition.index).table;
        loop {
            let top = frames.len() - 1;
            let frame = frames[top];
            debug_assert_eq!(frame.mode, Mode::Walk);
            if frame.cursor == NONE {
                if top == base {
                    frames[top].mode = Mode::Normal;
                    return WalkStep::Done;
                }
                frames.pop();
                let group = &mut table[frame.group];
                let count = if group.kind == GroupKind::Node {
                    1
                } else {
                    frame.node_index - frame.node_start
                };
                group.node_count = count;
                let next = group.next_sibling;
                let parent = frames.last_mut().expect("walk base frame");
                parent.node_index += count;
                parent.last_placed = frame.group;
                parent.cursor = next;
                continue;
            }

            let child = frame.cursor;
            let group = &mut table[child];
            if group.kind == GroupKind::Restart
                && scopes.get(group.scope).is_some_and(|scope| scope.invalid)
            {
                let restart = group
                    .restart
                    .expect("compose-runtime: an invalid restart group has no restart function");
                let kept = group.kept.clone();
                let walking = &mut frames[top];
                walking.mode = Mode::Restart;
                walking.limit = group.next_sibling;
                return WalkStep::Restart(restart, kept);
            }
            if group.dirty {
                group.dirty = false;
                let is_node = group.kind == GroupKind::Node;
                let node_index = if is_node { 0 } else { frame.node_index };
                frames.push(Frame {
                    group: child,
                    mode: Mode::Walk,
                    inserting: false,
                    forced: false,
                    skipped: true,
                    dirty: true,
                    cursor: group.first_child,
                    limit: NONE,
                    last_placed: NONE,
                    slot_cursor: 0,
                    node_parent: if is_node { group.node } else { frame.node_parent },
                    node_index,
                    node_start: node_index,
                    scope: if group.kind == GroupKind::Restart { group.scope } else { frame.scope },
                    saved_epoch: 0,
                    body_started: false,
                });
                continue;
            }
            let walking = &mut frames[top];
            walking.node_index += group.node_count;
            walking.last_placed = child;
            walking.cursor = group.next_sibling;
        }
    }

    /// Ends a restart started by [`walk_step`]: removes what the restart function did not match.
    pub(crate) fn finish_restart(&mut self) -> NodeOp {
        let Some(active) = self.active else { misuse(ComposeError::NotComposing) };
        let Runtime { compositions, scopes, frames, removal_stack, .. } = self;
        let data = compositions.data_mut(active.composition.index);
        let (table, deferred) = (&mut data.table, &mut data.deferred);
        let frame = frames.last_mut().expect("walk frame");
        if frame.mode != Mode::Restart {
            misuse(if self.abandoned_groups {
                ComposeError::CaughtPanic
            } else {
                ComposeError::UnclosedGroups
            })
        }
        let mut op = NodeOp::None;
        if frame.cursor != frame.limit {
            let count =
                remove_siblings(table, deferred, scopes, removal_stack, frame.cursor, frame.limit);
            table.set_next(frame.group, frame.last_placed, frame.limit);
            if count > 0 {
                op = NodeOp::Remove { parent: frame.node_parent, index: frame.node_index, count };
            }
        }
        frame.cursor = frame.limit;
        frame.limit = NONE;
        frame.mode = Mode::Walk;
        op
    }

    pub(crate) fn skipping(&mut self) -> bool {
        self.check_composing();
        let frame = self.body_frame();
        let skipping = !frame.inserting && !frame.forced;
        if !skipping {
            self.begin_body();
        }
        skipping
    }

    /// The current restart group's body is going to run: bump its scope's epoch, once.
    fn begin_body(&mut self) {
        let composition = self.check_composing();
        let Runtime { compositions, scopes, frames, .. } = self;
        let Some(frame) = frames.last_mut() else { return };
        let is_restart =
            compositions.data(composition).table[frame.group].kind == GroupKind::Restart;
        if frame.body_started || !is_restart {
            return;
        }
        frame.body_started = true;
        if let Some(scope) = scopes.get_mut(frame.scope) {
            scope.epoch = scope.epoch.wrapping_add(1);
        }
    }

    pub(crate) fn inserting(&mut self) -> bool {
        self.check_composing();
        self.body_frame().inserting
    }

    /// Takes the kept arguments of the current restart group out, for comparison.
    pub(crate) fn take_kept(&mut self) -> Option<Rc<dyn Any>> {
        let composition = self.check_composing();
        let group = self.body_frame().group;
        let group = &mut self.compositions.data_mut(composition).table[group];
        if group.kind != GroupKind::Restart {
            misuse(ComposeError::UnbalancedGroups)
        }
        group.kept.take()
    }

    /// Puts the kept arguments back; `changed` means the body runs. Returns what was there (only
    /// after a misuse), to be dropped outside the borrow.
    /// Puts the kept arguments back; `changed` means the body runs. Returns what was there (only
    /// after a misuse), to be dropped outside the borrow; on misuse, gives `kept` back too.
    pub(crate) fn put_kept(
        &mut self,
        kept: Option<Rc<dyn Any>>,
        changed: bool,
    ) -> Result<Kept, (Kept, ComposeError)> {
        let Some(active) = self.active.filter(|active| active.composing) else {
            return Err((kept, ComposeError::NotComposing));
        };
        let Some(frame) = self.frames.last().filter(|frame| frame.mode == Mode::Normal) else {
            return Err((kept, ComposeError::UnbalancedGroups));
        };
        let group = &mut self.compositions.data_mut(active.composition.index).table[frame.group];
        if group.kind != GroupKind::Restart {
            return Err((kept, ComposeError::UnbalancedGroups));
        }
        let old = std::mem::replace(&mut group.kept, kept);
        if changed {
            self.begin_body();
        }
        Ok(old)
    }

    /// Reserves the next slot of the current group, and returns its value if it holds a value
    /// of type `type_id`.
    pub(crate) fn slot_next(&mut self, type_id: TypeId) -> (SlotPosition, Option<Rc<dyn Any>>) {
        let composition = self.check_composing();
        let depth = self.frames.len() as u32;
        let frame = self.body_frame();
        let index = frame.slot_cursor;
        frame.slot_cursor += 1;
        let group = frame.group;
        let slots = &self.compositions.data_mut(composition).table[group].slots;
        let value = slots
            .get(index as usize)
            .filter(|slot| slot.value.value_type_id() == type_id)
            .map(|slot| slot.value.to_any());
        (SlotPosition { depth, group, index }, value)
    }

    /// Writes a slot reserved by [`slot_next`]. Returns the value it replaces, if any, to be
    /// forgotten at the end of the pass.
    /// On misuse, gives the value back, to be dropped outside the borrow before panicking.
    pub(crate) fn slot_set(
        &mut self,
        position: SlotPosition,
        value: SlotValue,
        type_name: &'static str,
    ) -> Result<(), (SlotValue, ComposeError)> {
        let Some(active) = self.active.filter(|active| active.composing) else {
            return Err((value, ComposeError::NotComposing));
        };
        let composition = active.composition.index;
        let depth = self.frames.len() as u32;
        let in_order = self.frames.last().is_some_and(|frame| {
            frame.mode == Mode::Normal
                && !frame.skipped
                && depth == position.depth
                && frame.group == position.group
                && position.index < frame.slot_cursor
        });
        let len = self.compositions.data(composition).table[position.group].slots.len();
        if !in_order || position.index as usize > len {
            return Err((value, ComposeError::SlotOutOfOrder));
        }
        let data = self.compositions.data_mut(composition);
        if let SlotValue::Observer(observer) = &value {
            data.deferred.remembered.push(observer.clone());
        }
        let slots = &mut data.table[position.group].slots;
        let index = position.index as usize;
        if index == slots.len() {
            slots.push(Slot { value, type_name });
        } else {
            let old = std::mem::replace(&mut slots[index], Slot { value, type_name });
            data.deferred.removed_values.push(old.value);
        }
        Ok(())
    }

    pub(crate) fn record_side_effect(&mut self, effect: DeferredEffect) {
        let composition = self.check_composing();
        self.compositions.data_mut(composition).deferred.side_effects.push(effect);
    }

    pub(crate) fn current_scope(&mut self) -> ScopeId {
        self.check_composing();
        self.frames.last().map_or(ScopeId::NONE, |frame| frame.scope)
    }
}

/// Applies a node change outside the runtime's borrow.
///
/// Moves are coalesced, as Compose's `recordMoveNode` does: a move waits for the next change,
/// and absorbs it if it moves the nodes right after its own to right after its destination.
/// Removing the first of many keyed items then costs one move instead of one per item.
pub(crate) fn apply_node_op(op: NodeOp) {
    if op == NodeOp::None {
        return;
    }
    let work = with_runtime(|runtime| {
        if let (
            Some(NodeOp::Move { parent: pending_parent, from, to, count }),
            NodeOp::Move { parent, from: next_from, to: next_to, count: next_count },
        ) = (runtime.pending_move, op)
            && pending_parent == parent
            && next_from == from + count
            && next_to == to + count
        {
            runtime.pending_move =
                Some(NodeOp::Move { parent, from, to, count: count + next_count });
            return None;
        }
        let flushed = runtime.pending_move.take();
        let now = match op {
            NodeOp::Move { .. } => {
                runtime.pending_move = Some(op);
                None
            }
            _ => Some(op),
        };
        Some((runtime.active_applier(), flushed, now))
    });
    if let Some((applier, flushed, now)) = work {
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        for op in [flushed, now].into_iter().flatten() {
            match op {
                NodeOp::None => {}
                NodeOp::Move { parent, from, to, count } => {
                    applier.move_nodes(parent, from as usize, to as usize, count as usize)
                }
                NodeOp::Remove { parent, index, count } => {
                    applier.remove(parent, index as usize, count as usize)
                }
            }
        }
    }
}

/// Applies a pending coalesced move, before anything else touches the applier.
pub(crate) fn flush_node_moves() {
    let work = with_runtime(|runtime| {
        let pending = runtime.pending_move.take()?;
        Some((runtime.active_applier(), pending))
    });
    if let Some((applier, NodeOp::Move { parent, from, to, count })) = work {
        applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy)).move_nodes(
            parent,
            from as usize,
            to as usize,
            count as usize,
        );
    }
}

/// Walks the frame at `base` in `Walk` mode, restarting the invalid scopes below it in order.
pub(crate) fn run_walk(base: usize) {
    loop {
        match with_runtime(|runtime| runtime.walk_step(base)) {
            WalkStep::Done => return,
            WalkStep::Restart(restart, kept) => {
                match &kept {
                    Some(kept) => restart(&**kept),
                    None => restart(&()),
                }
                drop(kept);
                let op = with_runtime(|runtime| runtime.finish_restart());
                apply_node_op(op);
            }
        }
    }
}

// Non-generic cores of the composer's generic methods: generated code instantiates the generic
// shims per composable, so they stay thin, and the work happens here, compiled once.

fn start_group(kind: GroupKind, key: u64, data_key: u64) -> NodeStart {
    let (op, start) = with_runtime(|runtime| runtime.start_group(kind, key, data_key, None));
    apply_node_op(op);
    start
}

/// Starts a restart group for the composable whose identity (the `TypeId` of its restart
/// closure) is `identity`. A group of another composable with the same key (an instantiation of a
/// generic composable, a same-named composable) doesn't match. Not the restart function's address:
/// an `#[inline]` composable can have a copy per crate, with distinct addresses.
pub(crate) fn start_restart_group_of(key: u64, identity: TypeId) {
    let (op, _) =
        with_runtime(|runtime| runtime.start_group(GroupKind::Restart, key, 0, Some(identity)));
    apply_node_op(op);
}

/// A group guard dropped while a panic unwinds: its group stays open. Recorded, so the error a
/// caught panic later causes says why.
pub(crate) fn abandon_group() {
    try_with_runtime(|runtime| runtime.abandoned_groups = true);
}

fn insert_node(parent: NodeRef, index: u32, node: &mut dyn Any) -> NodeRef {
    flush_node_moves();
    let applier = with_runtime(|runtime| runtime.active_applier());
    let inserted = applier
        .try_borrow_mut()
        .unwrap_or_else(|_| misuse(ComposeError::ApplierBusy))
        .insert_erased(parent, index as usize, node)
        .unwrap_or_else(|| misuse(ComposeError::WrongNodeType));
    with_runtime(|runtime| runtime.set_inserted_node(inserted));
    inserted
}

fn take_kept() -> Option<Rc<dyn Any>> {
    with_runtime(|runtime| runtime.take_kept())
}

fn put_kept(kept: Option<Rc<dyn Any>>, changed: bool) {
    match with_runtime(|runtime| runtime.put_kept(kept, changed)) {
        Ok(old) => drop(old),
        Err((kept, error)) => {
            drop(kept);
            misuse(error)
        }
    }
}

fn applier_for_use() -> Rc<std::cell::RefCell<dyn crate::applier::ErasedApplier>> {
    flush_node_moves();
    with_runtime(|runtime| {
        runtime.check_composing();
        runtime.active_applier()
    })
}

pub(crate) fn record_effect(effect: DeferredEffect) {
    let rejected = with_runtime(|runtime| {
        if !runtime.is_composing() {
            return Some(effect);
        }
        runtime.record_side_effect(effect);
        None
    });
    if rejected.is_some() {
        drop(rejected);
        misuse(ComposeError::NotComposing)
    }
}

/// The composer of the pass in progress on this thread. Generated code, and hand-written
/// primitives such as `remember` and `key`, are its only users; applications never see it.
///
/// A `Composer` is a handle to its thread's runtime: it works in any pass on that thread, so a
/// lambda may capture it and run again in a later pass.
#[derive(Clone, Copy, Debug)]
pub struct Composer {
    _not_send: PhantomData<*const ()>,
}

/// The composer of the pass in progress on this thread.
///
/// # Panics
///
/// Outside a composition pass.
pub fn current() -> Composer {
    with_runtime(|runtime| runtime.check_composing());
    Composer { _not_send: PhantomData }
}

/// The composer, if a pass is composing on this thread.
pub(crate) fn try_current() -> Option<Composer> {
    with_runtime(|runtime| runtime.is_composing()).then_some(Composer { _not_send: PhantomData })
}

impl Composer {
    /// Starts the group of a restartable composable. End it with
    /// [`end_restart_group`](Composer::end_restart_group).
    pub fn start_restart_group(&self, key: u64) {
        start_group(GroupKind::Restart, key, 0);
    }

    /// Starts a region group: a branch, a loop, or a non-restartable composable.
    pub fn start_replace_group(&self, key: u64) {
        start_group(GroupKind::Replace, key, 0);
    }

    /// Starts a keyed group, matched by `key` and `data_key`, so it moves with its data.
    pub fn start_movable_group(&self, key: u64, data_key: u64) {
        start_group(GroupKind::Movable, key, data_key);
    }

    /// Starts a group that emits one node. `factory` runs only when the group is new; the node
    /// is inserted into the applier's tree at once. End it with [`end_group`](Composer::end_group).
    pub fn start_node<N: 'static>(&self, key: u64, factory: impl FnOnce() -> N) -> NodeRef {
        match start_group(GroupKind::Node, key, 0) {
            NodeStart::Reused(node) => node,
            NodeStart::Insert { parent, index } => insert_node(parent, index, &mut Some(factory())),
            NodeStart::NotNode => unreachable!("a node group"),
        }
    }

    /// Ends a replace, movable or node group.
    pub fn end_group(&self) {
        let op = with_runtime(|runtime| runtime.end_group(None));
        apply_node_op(op);
    }

    /// Ends a restart group and records the function that re-runs it with its kept arguments.
    /// A non-capturing closure coerces to the `fn` pointer, so this allocates nothing.
    pub fn end_restart_group(&self, restart: fn(&dyn Any)) {
        let op = with_runtime(|runtime| runtime.end_group(Some(restart)));
        apply_node_op(op);
    }

    /// Whether the current group may skip its body: it isn't new and its scope isn't invalid.
    /// When it says no, the body is known to run.
    pub fn skipping(&self) -> bool {
        with_runtime(|runtime| runtime.skipping())
    }

    /// Whether the current group is new in this pass.
    pub fn inserting(&self) -> bool {
        with_runtime(|runtime| runtime.inserting())
    }

    /// Skips the rest of the current group, keeping its children and slots, and recomposes the
    /// invalid scopes inside it.
    pub fn skip_to_group_end(&self) {
        if let Some(base) = with_runtime(|runtime| runtime.begin_skip()) {
            run_walk(base);
        }
    }

    /// Compares the current restart group's kept arguments with new ones.
    ///
    /// `changed` gets the kept arguments and says whether the new ones differ; `make` builds the
    /// new kept arguments, called only if they differ or none are kept yet. Returns whether they
    /// changed, in which case the body is known to run. A steady-state call allocates nothing; a
    /// change updates the kept value in place when nothing else holds it.
    pub fn update_kept<K: 'static>(
        &self,
        changed: impl FnOnce(&K) -> bool,
        make: impl FnOnce() -> K,
    ) -> bool {
        let kept = take_kept();
        let differs = match kept.as_deref().and_then(|kept| kept.downcast_ref::<K>()) {
            Some(kept) => changed(kept),
            None => true,
        };
        let kept = match kept {
            Some(kept) if !differs => Some(kept),
            Some(mut kept) => {
                match Rc::get_mut(&mut kept).and_then(|value| value.downcast_mut::<K>()) {
                    Some(value) => {
                        // In place; the old arguments drop here, outside the runtime's borrow.
                        *value = make();
                        Some(kept)
                    }
                    None => {
                        drop(kept);
                        Some(Rc::new(make()) as Rc<dyn Any>)
                    }
                }
            }
            None => Some(Rc::new(make()) as Rc<dyn Any>),
        };
        put_kept(kept, differs);
        differs
    }

    /// The recompose scope state reads register at this point.
    pub fn current_scope(&self) -> RecomposeScope {
        let id = with_runtime(|runtime| runtime.current_scope());
        RecomposeScope { id, _not_send: PhantomData }
    }

    /// Runs `f` with the composition's applier, as type `A`.
    pub fn with_applier<A: Applier, R>(&self, f: impl FnOnce(&mut A) -> R) -> R {
        let applier = applier_for_use();
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        let applier = applier
            .as_any_mut()
            .downcast_mut::<A>()
            .unwrap_or_else(|| misuse(ComposeError::WrongApplierType));
        f(applier)
    }

    /// Runs `effect` after the pass applies its changes, as Compose's `recordSideEffect`.
    pub fn record_side_effect(&self, effect: impl FnOnce() + 'static) {
        record_effect(DeferredEffect::Boxed(Box::new(effect)));
    }
}

/// The recompose scope of the composable running now, as Compose's `currentRecomposeScope`.
pub fn current_recompose_scope() -> RecomposeScope {
    current().current_scope()
}
