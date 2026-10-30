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
use crate::pausable::{Entry, NO_REGION, RecordedOp, ShouldPauseCallback, deferred_of};
use crate::runtime::{
    CompositionData, DeferredEffect, RemovalStep, Runtime, remove_children_and_slots,
    remove_siblings, try_with_runtime, with_runtime,
};
use crate::scope::{RecomposeScope, ScopeId, Scopes};
use crate::slot_table::{GroupKind, NONE, Slot, SlotTable, SlotValue};

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
    /// The paused composition's region the group is in (`NO_REGION` if none): its node changes
    /// are recorded, its observers, effects and removals wait for `apply`.
    pub region: u32,
    /// The region's segment its observers and effects go to.
    pub segment: u32,
    /// A paused scope a resume restarted: it doesn't pause at its own start.
    pub resuming: bool,
    /// The paused composition paused this restart group: it skips its body.
    pub paused: bool,
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
    /// A node kept by content being reused: the applier resets it first (`Applier::reuse`), at
    /// once, or in a region (the second field) when it's applied.
    Reusing(NodeRef, u32),
    Insert {
        parent: NodeRef,
        index: u32,
        /// The region it's made in, if any: made apart, attached when applied.
        region: u32,
    },
}

/// What `skipping` found: whether the body may skip, or a pause point's question.
pub(crate) enum Skip {
    Yes,
    No,
    Ask(Rc<dyn ShouldPauseCallback>),
}

pub(crate) enum WalkStep {
    Done,
    Restart(fn(&dyn Any), Option<Rc<dyn Any>>),
    /// A scope invalid only if one of these derived states changed: recompute them (outside
    /// the borrow), then settle it (`settle_conditions`).
    Check(ScopeId, Vec<crate::state::StateId>),
}

/// Which composable a group belongs to, beyond its key: two composables can share a key.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Identity {
    /// A restartable composable: the `TypeId` of its restart closure, and its type name (for
    /// recomposition counts).
    Type(TypeId, &'static str),
    /// A non-restartable one: the type name of a marker closure, unique per composable and
    /// instantiation, compared by address first.
    Name(&'static str),
}

impl PartialEq for Identity {
    fn eq(&self, other: &Identity) -> bool {
        match (self, other) {
            (Identity::Type(a, _), Identity::Type(b, _)) => a == b,
            (Identity::Name(a), Identity::Name(b)) => std::ptr::eq(*a, *b) || a == b,
            _ => false,
        }
    }
}

/// Moves a walking frame's cursor past its clean children: neither dirty nor an invalid scope's,
/// so nothing in them runs. A tight loop, as a walk to one changed item of a long list passes
/// over all the others.
fn pass_clean_siblings(walking: &mut Frame, table: &SlotTable, scopes: &Scopes) {
    debug_assert_eq!(walking.mode, Mode::Walk);
    let (mut cursor, mut node_index, mut last_placed) =
        (walking.cursor, walking.node_index, walking.last_placed);
    while cursor != NONE {
        let group = &table[cursor];
        if group.dirty
            || (group.kind == GroupKind::Restart
                && scopes.get(group.scope).is_some_and(|scope| scope.invalid))
        {
            break;
        }
        node_index += group.node_count;
        last_placed = cursor;
        cursor = group.next_sibling;
    }
    walking.cursor = cursor;
    walking.node_index = node_index;
    walking.last_placed = last_placed;
}

/// A restart group's kept arguments, when it has any.
type Kept = Option<Rc<dyn Any>>;

/// A pause point's question: the paused composition's callback, to ask outside the borrow.
type PauseQuestion = Rc<dyn ShouldPauseCallback>;

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
    pub(crate) fn check_composing(&self) -> u32 {
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

    /// Starts a group of `kind`. `reusable` marks a node group as a reusable node's
    /// (`start_reusable_node`): content being reused keeps it, and replaces other node groups.
    pub(crate) fn start_group(
        &mut self,
        kind: GroupKind,
        key: u64,
        data_key: u64,
        identity: Option<Identity>,
        reusable: bool,
    ) -> (NodeOp, NodeStart) {
        let composition = self.check_composing();
        let Runtime {
            compositions,
            scopes,
            frames,
            reusing,
            resume_next,
            providers_invalid,
            removal_stack,
            recorded_changes,
            branch_removal,
            ..
        } = self;
        let providers_invalid = *providers_invalid != 0;
        let CompositionData { table, paused, deferred, .. } = compositions.data_mut(composition);
        let parent = frames.last_mut().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        match parent.mode {
            Mode::Walk => misuse(ComposeError::UnbalancedGroups),
            Mode::Normal if parent.skipped => misuse(ComposeError::UseAfterSkip),
            // A walking frame is skipped by definition; the restart function starts one group.
            Mode::Normal | Mode::Restart => {}
        }

        let mut op = NodeOp::None;
        let mut found = NONE;
        // Content being reused replaces the node groups that aren't reusable nodes, as
        // Compose's `forceReplace` (a `ComposeNode` under `ReusableContent`).
        let force_replace = *reusing != 0 && kind == GroupKind::Node && !reusable;
        // Another branch of the same conditional where this one starts (a branch group: the
        // statement's key, the branch as data key): deleted where it stands, as Compose's
        // `startReplaceGroup` deletes the group it doesn't expect, so what it held is forgotten
        // before what follows (D715). Its nodes' removal is applied before `op`
        // (`start_branch_group_of`). The group starting is then inserted without looking
        // further, as Compose's: in a loop without keys, the next group of this key is the next
        // iteration's, whose state isn't this one's (D716).
        let mut deleted = false;
        if kind == GroupKind::Replace
            && data_key != 0
            && !parent.inserting
            && !force_replace
            && parent.cursor != NONE
            && parent.cursor != parent.limit
            && let other = &table[parent.cursor]
            && other.kind == GroupKind::Replace
            && other.key == key
            && other.data_key != 0
            && other.data_key != data_key
        {
            *recorded_changes += 1;
            *branch_removal = delete_branch(table, paused, deferred, scopes, removal_stack, parent);
            deleted = true;
        }
        if !parent.inserting && !force_replace && !deleted {
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
                // A reusable group matches by its key alone: another data key reuses it.
                if group.key == key
                    && group.kind == kind
                    && (group.data_key == data_key || kind == GroupKind::Reusable)
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
                    *recorded_changes += 1;
                    table[previous].next_sibling = table[found].next_sibling;
                    table[found].next_sibling = parent.cursor;
                    table.link_after(parent.group, parent.last_placed, found);
                    let count = table[found].node_count;
                    // Past node-less groups only, its nodes are already in place.
                    if count > 0 && nodes_before > 0 {
                        let (node_parent, from, to) = (
                            parent.node_parent,
                            parent.node_index + nodes_before,
                            parent.node_index,
                        );
                        match paused {
                            Some(paused) if parent.region != NO_REGION => paused.record(
                                parent.region,
                                RecordedOp::Move { parent: node_parent, from, to, count },
                            ),
                            _ => op = NodeOp::Move { parent: node_parent, from, to, count },
                        }
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
        // Where it sits among its parent's slots (not a restart's: the walking frame has none).
        if parent.mode == Mode::Normal {
            table[index].slot_position = parent.slot_cursor.min(u32::from(u8::MAX)) as u8;
        }
        // A pass reaching paused content's group (a body around it ran): it composes the rest of
        // it, recorded as the region's, and applies it at its end (D592).
        let adopted = match paused {
            Some(paused) if !inserting && parent.region == NO_REGION => paused.adopt(index),
            _ => None,
        };
        let group = &mut table[index];
        let dirty = group.dirty;
        group.dirty = false;
        // A reusable group met with another data key: its content is reused, as Compose's
        // `startReusableGroup` (from here to its end, unless content above is reused already).
        let starts_reuse = !inserting && kind == GroupKind::Reusable && group.data_key != data_key;
        if starts_reuse {
            group.data_key = data_key;
            group.set_flag(crate::slot_table::DEACTIVATED, false);
        }
        let reusing_now = *reusing != 0 || starts_reuse;
        let mut frame = Frame {
            group: index,
            mode: Mode::Normal,
            inserting,
            // Reused content runs: its slots read as empty, as if new, but its groups and
            // nodes are kept. So does the content of a provider whose values changed.
            forced: reusing_now || providers_invalid,
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
            region: parent.region,
            segment: parent.segment,
            resuming: false,
            paused: false,
        };
        // A paused scope restarted by a resume: its observers go to its segment.
        if parent.mode == Mode::Restart
            && let Some((segment, resuming)) = resume_next.take()
        {
            frame.segment = segment;
            frame.resuming = resuming;
        }
        if let Some((region, segment)) = adopted {
            frame.region = region;
            frame.segment = segment;
            frame.node_index = 0;
            frame.node_start = 0;
        }
        let node_start = match kind {
            GroupKind::Node => {
                frame.node_parent = group.node;
                frame.node_index = 0;
                frame.node_start = 0;
                if inserting {
                    NodeStart::Insert {
                        parent: parent.node_parent,
                        index: parent.node_index,
                        region: parent.region,
                    }
                } else if reusing_now {
                    NodeStart::Reusing(group.node, parent.region)
                } else {
                    NodeStart::Reused(group.node)
                }
            }
            GroupKind::Restart => {
                // Deactivated content's scopes were freed: reused, it gets new ones.
                let fresh = inserting || group.scope.is_none();
                if fresh {
                    group.scope = scopes.alloc(composition, index);
                }
                let scope = scopes.get_mut(group.scope).expect("restart group without a scope");
                // Invalid on conditions too: reached through its parent's body, it runs
                // unchecked, as Compose's `addRecomposeScope`.
                frame.forced |= scope.invalid || fresh;
                scope.invalid = false;
                let conditional = scope.conditional;
                // A paused scope met through its parent's body rather than a resume's walk
                // resumes too, as Compose's `addRecomposeScope`: from its start, in its segment,
                // reusing if it paused reusing.
                if scope.paused {
                    scope.paused = false;
                    frame.resuming = true;
                    frame.segment = scope.segment;
                    if std::mem::take(&mut scope.reusing) && *reusing == 0 {
                        *reusing = frames.len() + 1;
                    }
                }
                // The epoch is bumped only once the body is known to run (`begin_body`): until
                // then the scope may still skip, and its reads from the last run stay current.
                frame.saved_epoch = scope.epoch;
                frame.scope = group.scope;
                if conditional {
                    scopes.clear_conditions(group.scope);
                }
                NodeStart::NotNode
            }
            _ => NodeStart::NotNode,
        };
        frames.push(frame);
        if starts_reuse && *reusing == 0 {
            *reusing = frames.len();
        }
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
        let Runtime {
            compositions,
            scopes,
            frames,
            removal_stack,
            abandoned_groups,
            reusing,
            pausable_pass,
            recorded_changes,
            ..
        } = self;
        let CompositionData { table, deferred, paused, .. } = compositions.data_mut(composition);
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
        if *reusing == frames.len() {
            *reusing = 0;
        }
        if self.providers_invalid == frames.len() {
            self.providers_invalid = 0;
        }
        frames.pop();

        let mut op = NodeOp::None;
        if !frame.skipped {
            if frame.cursor != NONE
                && frame.region == NO_REGION
                && let Some(paused) = paused.as_mut()
                && paused.holds_regions()
            {
                let removed = siblings_from(table, frame.cursor, NONE);
                paused.discard_regions_in(table, scopes, removal_stack, deferred, removed, false);
            }
            let deferred = deferred_of(deferred, paused, frame.region);
            let mut removed = 0;
            let keep = (frame.slot_cursor as usize).min(table[frame.group].slots.len());
            if frame.cursor != NONE {
                *recorded_changes += 1;
                // The children not matched and the slots not reached, in table order.
                removed = remove_children_and_slots(
                    table,
                    deferred,
                    scopes,
                    removal_stack,
                    frame.group,
                    frame.cursor,
                    keep,
                );
                table.set_next(frame.group, frame.last_placed, NONE);
            } else {
                let slots = &mut table[frame.group].slots;
                deferred.removed_values.extend(slots.drain(keep..).map(|slot| slot.value));
            }
            if frame.region != NO_REGION
                && let Some(paused) = paused
            {
                paused.settle(frame.region, frame.segment);
            }
            if removed > 0 {
                let (parent, index) = (frame.node_parent, frame.node_index);
                match paused {
                    Some(paused) if frame.region != NO_REGION => paused
                        .record(frame.region, RecordedOp::Remove { parent, index, count: removed }),
                    _ => op = NodeOp::Remove { parent, index, count: removed },
                }
            }
        }

        // A pass of the composition through paused content leaves the dirty marks of what it
        // didn't reach, for the paused composition's resumes: their ancestors keep theirs.
        if frame.region != NO_REGION
            && !*pausable_pass
            && paused
                .as_ref()
                .is_some_and(|paused| paused.regions[frame.region as usize].root != frame.group)
        {
            let mut child = table[frame.group].first_child;
            while child != NONE {
                if table[child].dirty
                    || (table[child].kind == GroupKind::Restart
                        && scopes.get(table[child].scope).is_some_and(|scope| scope.invalid))
                {
                    table[frame.group].dirty = true;
                    break;
                }
                child = table[child].next_sibling;
            }
        }
        let group = &mut table[frame.group];
        let mut count =
            if kind == GroupKind::Node { 1 } else { frame.node_index - frame.node_start };
        // A region's group counts, for the composition around it, the nodes it has there now.
        if frame.region != NO_REGION
            && let Some(paused) = paused
        {
            let region = &mut paused.regions[frame.region as usize];
            if region.root == frame.group {
                region.nodes = count;
                count = region.attached;
            }
        }
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

    /// Deactivates the content of the reusable group just started, and skips it: see
    /// [`Composer::deactivate_to_end_group`].
    pub(crate) fn deactivate_to_end_group(&mut self, changed: bool) -> Vec<NodeOp> {
        let composition = self.check_composing();
        // Content being reused is deactivated whatever the caller says, as Compose's
        // `changed(active)` answers true then (its slots read as empty).
        let changed = changed || self.reusing != 0;
        let Runtime { compositions, scopes, frames, removal_stack, recorded_changes, .. } = self;
        let CompositionData { table, deferred, paused, .. } = compositions.data_mut(composition);
        let mut ops = Vec::new();
        let depth = frames.len();
        let frame = frames.last_mut().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        if frame.mode != Mode::Normal || depth < 2 {
            misuse(ComposeError::UnbalancedGroups)
        }
        if frame.skipped {
            misuse(ComposeError::UseAfterSkip)
        }
        // Nothing composed in it yet, as Compose requires ("No nodes can be emitted before").
        if table[frame.group].kind != GroupKind::Reusable
            || frame.last_placed != NONE
            || frame.slot_cursor != 0
        {
            misuse(ComposeError::UnbalancedGroups)
        }
        frame.skipped = true;
        frame.dirty = false;
        // Paused content in it is discarded first, as Compose's precomposed slot is when it goes
        // to the reuse pool: what it made released, its old nodes removed from their parent.
        if changed
            && !frame.inserting
            && let Some(paused) = paused.as_mut()
            && paused.holds_regions()
        {
            ops = paused.discard_regions_in(
                table,
                scopes,
                removal_stack,
                deferred,
                table_children(table, frame.group),
                true,
            );
        }
        let deferred = deferred_of(deferred, paused, frame.region);
        if changed && !frame.inserting {
            *recorded_changes += 1;
            // Pre-order, as removals: a group's slots before its children's.
            let group = &mut table[frame.group];
            group.set_flag(crate::slot_table::DEACTIVATED, true);
            for slot in &mut group.slots {
                deactivate_slot(&mut slot.value, &mut deferred.removed_values);
            }
            let stack = removal_stack;
            stack.clear();
            let mut child = group.first_child;
            while child != NONE {
                stack.push(RemovalStep::enter(child, false));
                child = table[child].next_sibling;
            }
            stack.reverse();
            while let Some(RemovalStep { group: index, .. }) = stack.pop() {
                let group = &mut table[index];
                group.dirty = false;
                for slot in &mut group.slots {
                    deactivate_slot(&mut slot.value, &mut deferred.removed_values);
                }
                if !group.scope.is_none() {
                    scopes.free(group.scope);
                    group.scope = ScopeId::NONE;
                }
                if group.kind == GroupKind::Node && !group.node.is_none() {
                    deferred.deactivated_nodes.push(group.node);
                }
                let start = stack.len();
                let mut child = group.first_child;
                while child != NONE {
                    stack.push(RemovalStep::enter(child, false));
                    child = table[child].next_sibling;
                }
                stack[start..].reverse();
            }
        }
        if frame.region != NO_REGION
            && let Some(paused) = paused.as_mut()
        {
            paused.record_deactivations(frame.region);
        }
        // Its nodes stay where they are: all its children's (nothing composed in it).
        if !frame.inserting {
            frame.node_index += table[frame.group].node_count;
        }
        frame.cursor = NONE;
        if frame.region != NO_REGION
            && let Some(paused) = paused
        {
            paused.settle(frame.region, frame.segment);
        }
        ops
    }

    /// Starts skipping the rest of the current group. Returns the depth to walk from if some
    /// descendant scopes are invalid.
    pub(crate) fn begin_skip(&mut self) -> Option<usize> {
        let composition = self.check_composing();
        let Runtime { compositions, frames, pausable_pass, .. } = self;
        let pausable_pass = *pausable_pass;
        let table = &mut compositions.data_mut(composition).table;
        let depth = frames.len();
        let frame = frames.last_mut().unwrap_or_else(|| misuse(ComposeError::NotComposing));
        if frame.mode != Mode::Normal || depth < 2 {
            misuse(ComposeError::UnbalancedGroups)
        }
        if frame.skipped {
            misuse(ComposeError::UseAfterSkip)
        }
        frame.skipped = true;
        if frame.dirty && (frame.paused || (frame.region != NO_REGION && !pausable_pass)) {
            // Paused while reusing, its old content's invalid scopes wait for its resume, which
            // reuses that content: no walk into it now (it would compose them unreused). And a
            // pass of the composition through paused content (it reached it) leaves its paused
            // and invalidated scopes to the paused composition's resumes (D592).
            table[frame.group].dirty = true;
        } else if frame.dirty {
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
        let Runtime { compositions, scopes, frames, reusing, resume_next, .. } = self;
        let CompositionData { table, paused, .. } = compositions.data_mut(active.composition.index);
        loop {
            let top = frames.len() - 1;
            pass_clean_siblings(&mut frames[top], table, scopes);
            let frame = frames[top];
            debug_assert_eq!(frame.mode, Mode::Walk);
            if frame.cursor == NONE {
                if top == base {
                    frames[top].mode = Mode::Normal;
                    return WalkStep::Done;
                }
                frames.pop();
                let group = &mut table[frame.group];
                let mut count = if group.kind == GroupKind::Node {
                    1
                } else {
                    frame.node_index - frame.node_start
                };
                if frame.region != NO_REGION
                    && let Some(paused) = paused
                    && paused.regions[frame.region as usize].root == frame.group
                {
                    let region = &mut paused.regions[frame.region as usize];
                    region.nodes = count;
                    count = region.attached;
                }
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
            // Invalid on conditions: checked first, as Compose's `recomposeToGroupEnd` checks
            // `isInvalidFor`. Not a paused scope: its resume runs it whatever.
            if group.kind == GroupKind::Restart
                && let Some(scope) = scopes.get(group.scope)
                && scope.invalid
                && !scope.paused
                && scope.conditional
            {
                return WalkStep::Check(group.scope, scopes.conditions(group.scope));
            }
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
                // In a region (a resume): a paused scope resumes in its segment, from its start,
                // reusing its content if it paused while reusing (Compose's
                // `RecomposeScopeImpl.reusing`); one invalidated since composes after the rest.
                if walking.region != NO_REGION
                    && let Some(scope) = scopes.get_mut(group.scope)
                {
                    if scope.paused {
                        *resume_next = Some((scope.segment, true));
                        if scope.reusing && *reusing == 0 {
                            *reusing = frames.len() + 1;
                        }
                    } else {
                        // Invalidated since: its observers come after the region's others.
                        let segment = paused
                            .as_ref()
                            .map_or(0, |paused| paused.regions[walking.region as usize].segment);
                        *resume_next = Some((segment, false));
                    }
                    scope.paused = false;
                    scope.reusing = false;
                }
                return WalkStep::Restart(restart, kept);
            }
            // Neither clean (`pass_clean_siblings`) nor invalid: dirty. The walk goes into it.
            debug_assert!(group.dirty);
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
                region: frame.region,
                segment: frame.segment,
                resuming: false,
                paused: false,
            });
        }
    }

    /// After the derived states a scope's invalidation depends on were recomputed: if none
    /// changed (none invalidated it outright meanwhile), it's valid again, and the walk passes it
    /// or goes into it if it's dirty.
    pub(crate) fn settle_conditions(&mut self, id: ScopeId) {
        if let Some(scope) = self.scopes.get_mut(id)
            && scope.invalid
            && scope.conditional
        {
            scope.invalid = false;
            self.scopes.clear_conditions(id);
        }
    }

    /// Ends a restart started by [`walk_step`]: removes what the restart function did not match.
    pub(crate) fn finish_restart(&mut self) -> NodeOp {
        let Some(active) = self.active else { misuse(ComposeError::NotComposing) };
        let Runtime { compositions, scopes, frames, removal_stack, recorded_changes, .. } = self;
        let CompositionData { table, deferred, paused, .. } =
            compositions.data_mut(active.composition.index);
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
            *recorded_changes += 1;
            if frame.region == NO_REGION
                && let Some(paused) = paused.as_mut()
                && paused.holds_regions()
            {
                let removed = siblings_from(table, frame.cursor, frame.limit);
                paused.discard_regions_in(table, scopes, removal_stack, deferred, removed, false);
            }
            let deferred = deferred_of(deferred, paused, frame.region);
            let count =
                remove_siblings(table, deferred, scopes, removal_stack, frame.cursor, frame.limit);
            table.set_next(frame.group, frame.last_placed, frame.limit);
            if frame.region != NO_REGION
                && let Some(paused) = paused
            {
                paused.settle(frame.region, frame.segment);
            }
            if count > 0 {
                let (parent, index) = (frame.node_parent, frame.node_index);
                match paused {
                    Some(paused) if frame.region != NO_REGION => {
                        paused.record(frame.region, RecordedOp::Remove { parent, index, count })
                    }
                    _ => op = NodeOp::Remove { parent, index, count },
                }
            }
        }
        frame.cursor = frame.limit;
        frame.limit = NONE;
        frame.mode = Mode::Walk;
        op
    }

    pub(crate) fn skipping(&mut self) -> Skip {
        self.check_composing();
        let frame = self.body_frame();
        if frame.paused {
            return Skip::Yes;
        }
        if !frame.inserting && !frame.forced {
            return Skip::Yes;
        }
        if let Some(callback) = self.pause_due() {
            return Skip::Ask(callback);
        }
        self.begin_body();
        Skip::No
    }

    /// A pause point: the current group is a restart group about to run its body in a region,
    /// inserting or reusing (Compose's `shouldExecute`), and not a paused scope resuming. The
    /// callback to ask, if so.
    fn pause_due(&self) -> Option<Rc<dyn ShouldPauseCallback>> {
        let callback = self.pause.as_ref()?;
        let frame = self.frames.last()?;
        if frame.region == NO_REGION || frame.resuming || !(frame.inserting || self.reusing != 0) {
            return None;
        }
        let active = self.active?;
        let kind = self.compositions.data(active.composition.index).table[frame.group].kind;
        (kind == GroupKind::Restart).then(|| callback.clone())
    }

    /// The pause point's answer: pauses the group, or starts its body.
    pub(crate) fn decide_pause(&mut self, pause: bool) {
        self.check_composing();
        if pause {
            self.pause_current();
        } else {
            self.begin_body();
        }
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
        self.bodies_run += 1;
        if let Some(counts) = &mut self.recomposition_counts
            && let Some(Identity::Type(_, name)) =
                compositions.data(composition).table[frame.group].identity
        {
            *counts.entry(name).or_insert(0) += 1;
        }
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
    ) -> Result<(Kept, Option<PauseQuestion>), (Kept, ComposeError)> {
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
            if let Some(callback) = self.pause_due() {
                return Ok((old, Some(callback)));
            }
            self.begin_body();
        }
        Ok((old, None))
    }

    /// Reserves the next slot of the current group, and returns its value if it holds a value
    /// of type `type_id`.
    /// `across_reuse`: a `remember_across_reuse`, which reused content's kept values answer.
    pub(crate) fn slot_next(
        &mut self,
        type_id: TypeId,
        across_reuse: bool,
    ) -> (SlotPosition, Option<Rc<dyn Any>>) {
        let composition = self.check_composing();
        let depth = self.frames.len() as u32;
        let frame = self.body_frame();
        let index = frame.slot_cursor;
        frame.slot_cursor += 1;
        let group = frame.group;
        let slots = &self.compositions.data_mut(composition).table[group].slots;
        // Reused content's slots read as empty (Compose's `nextSlot` while `reusing`): every
        // `remember` computes anew, and the value it replaces is forgotten.
        let value = slots
            .get(index as usize)
            .filter(|slot| {
                (self.reusing == 0
                    || (across_reuse && matches!(slot.value, SlotValue::KeptOnReuse(_))))
                    && slot.value.value_type_id() == type_id
            })
            .and_then(|slot| slot.value.to_any());
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
        let (region, segment) =
            self.frames.last().map_or((NO_REGION, 0), |frame| (frame.region, frame.segment));
        let CompositionData { table, deferred, paused, .. } =
            self.compositions.data_mut(composition);
        // In a region, observers wait for `apply`, in their segment.
        if let SlotValue::Observer(observer) = &value {
            match paused {
                Some(paused) if region != NO_REGION => {
                    paused.segments[segment as usize].push(Entry::Observer(observer.clone()))
                }
                _ => deferred.remembered.push(observer.clone()),
            }
        }
        self.recorded_changes += 1;
        let slots = &mut table[position.group].slots;
        let index = position.index as usize;
        if index == slots.len() {
            slots.push(Slot { value, type_name });
        } else {
            let old = std::mem::replace(&mut slots[index], Slot { value, type_name });
            match paused {
                Some(paused) if region != NO_REGION => {
                    paused.segments[segment as usize].push(Entry::Forgotten(old.value))
                }
                _ => deferred.removed_values.push(old.value),
            }
        }
        Ok(())
    }

    /// The provider group composing now changed what it provides in a way its content can't
    /// see through state: every body in its content runs, as Compose's `providersInvalid`.
    pub(crate) fn invalidate_providers(&mut self) {
        let composition = self.check_composing();
        let group = self.body_frame().group;
        self.provider_versions += 1;
        let version = self.provider_versions;
        self.compositions.data_mut(composition).table[group].set_provider_version(version);
        if self.providers_invalid == 0 {
            self.providers_invalid = self.frames.len();
        }
    }

    /// Marks the group just started as a composition local provider's, before its content runs:
    /// the groups allocated inside it record it as their nearest provider
    /// (`Group::enclosing_provider`), where reading a local starts.
    pub(crate) fn mark_provider(&mut self) {
        let composition = self.check_composing();
        let frame = self.body_frame();
        let group = frame.group;
        let group = &mut self.compositions.data_mut(composition).table[group];
        // Its children were allocated under it as a provider, or it has none yet.
        debug_assert!(group.is_provider() || group.first_child == NONE);
        group.set_flag(crate::slot_table::PROVIDER, true);
        // New, or what's around it changed: what its content sees isn't what it saw.
        if group.provider_version() == 0 || self.providers_invalid != 0 {
            self.provider_versions += 1;
            group.set_provider_version(self.provider_versions);
        }
    }

    pub(crate) fn record_side_effect(&mut self, effect: DeferredEffect) {
        let composition = self.check_composing();
        self.recorded_changes += 1;
        let (region, segment) =
            self.frames.last().map_or((NO_REGION, 0), |frame| (frame.region, frame.segment));
        let data = self.compositions.data_mut(composition);
        match &mut data.paused {
            Some(paused) if region != NO_REGION => {
                paused.segments[segment as usize].push(Entry::Effect(effect))
            }
            _ => data.deferred.side_effects.push(effect),
        }
    }

    pub(crate) fn current_scope(&mut self) -> ScopeId {
        self.check_composing();
        self.frames.last().map_or(ScopeId::NONE, |frame| frame.scope)
    }
}

/// The siblings from `first` up to (excluding) `until`.
pub(crate) fn siblings_from(table: &SlotTable, first: u32, until: u32) -> Vec<u32> {
    let mut siblings = Vec::new();
    let mut current = first;
    while current != until && current != NONE {
        siblings.push(current);
        current = table[current].next_sibling;
    }
    siblings
}

/// `group`'s children.
fn table_children(table: &SlotTable, group: u32) -> Vec<u32> {
    siblings_from(table, table[group].first_child, NONE)
}

/// Empties a deactivated remember observer's slot: the observer is forgotten at the end of the
/// pass (and dropped), as Compose's `deactivateCurrentGroup`. Other values stay until reused.
fn deactivate_slot(value: &mut SlotValue, removed: &mut Vec<SlotValue>) {
    if let SlotValue::Observer(_) = value {
        removed.push(std::mem::replace(value, SlotValue::Empty));
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
        runtime.node_changes += 1;
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
            WalkStep::Check(scope, conditions) => {
                for cell in conditions {
                    crate::state::current_version(cell);
                }
                with_runtime(|runtime| runtime.settle_conditions(scope));
            }
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

/// Deletes the other branch's group at `parent`'s cursor (D715), returning the removal of its
/// nodes to apply, if any.
#[cold]
#[inline(never)]
fn delete_branch(
    table: &mut SlotTable,
    paused: &mut Option<Box<crate::pausable::Paused>>,
    deferred: &mut crate::runtime::Deferred,
    scopes: &mut Scopes,
    removal_stack: &mut Vec<RemovalStep>,
    parent: &mut Frame,
) -> NodeOp {
    let (branch, next) = (parent.cursor, table[parent.cursor].next_sibling);
    if parent.region == NO_REGION
        && let Some(paused) = paused.as_mut()
        && paused.holds_regions()
    {
        paused.discard_regions_in(table, scopes, removal_stack, deferred, vec![branch], false);
    }
    let deferred = deferred_of(deferred, paused, parent.region);
    let count = remove_siblings(table, deferred, scopes, removal_stack, branch, next);
    table.set_next(parent.group, parent.last_placed, next);
    parent.cursor = next;
    if count == 0 {
        return NodeOp::None;
    }
    let (node_parent, index) = (parent.node_parent, parent.node_index);
    match paused {
        Some(paused) if parent.region != NO_REGION => {
            paused.record(parent.region, RecordedOp::Remove { parent: node_parent, index, count });
            NodeOp::None
        }
        _ => NodeOp::Remove { parent: node_parent, index, count },
    }
}

fn start_group(kind: GroupKind, key: u64, data_key: u64) -> NodeStart {
    start_group_of(kind, key, data_key, false)
}

fn start_group_of(kind: GroupKind, key: u64, data_key: u64, reusable: bool) -> NodeStart {
    let (op, start) =
        with_runtime(|runtime| runtime.start_group(kind, key, data_key, None, reusable));
    apply_node_op(op);
    start
}

/// Starts a branch group (D715): another branch's group deleted where it starts has its nodes
/// removed first.
fn start_branch_group_of(key: u64, branch: u64) {
    let (removal, op) = with_runtime(|runtime| {
        let (op, _) = runtime.start_group(GroupKind::Replace, key, branch, None, false);
        (std::mem::replace(&mut runtime.branch_removal, NodeOp::None), op)
    });
    apply_node_op(removal);
    apply_node_op(op);
}

/// Resets a node kept by content being reused (`Applier::reuse`), before the pass updates it.
fn reuse_node(node: NodeRef, region: u32) {
    flush_node_moves();
    let applier = with_runtime(|runtime| {
        runtime.node_changes += 1;
        runtime.active_applier()
    });
    let mut applier =
        applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
    if region == NO_REGION {
        applier.reuse(node);
        return;
    }
    // Paused content's: reset now if the applier can (a node shown, reset before its update as
    // unpaused), else as it's applied, as Compose's recorded `reuse`.
    if !applier.reuse_paused(node) {
        drop(applier);
        with_runtime(|runtime| runtime.record_reuse(region, node));
    }
}

/// Starts a restart group for the composable whose identity (the `TypeId` of its restart
/// closure) is `identity`. A group of another composable with the same key (an instantiation of a
/// generic composable, a same-named composable) doesn't match. Not the restart function's address:
/// an `#[inline]` composable can have a copy per crate, with distinct addresses.
pub(crate) fn start_restart_group_of(key: u64, identity: crate::__private::RestartIdentity) {
    let identity = Some(Identity::Type(identity.type_id, identity.name));
    let (op, _) =
        with_runtime(|runtime| runtime.start_group(GroupKind::Restart, key, 0, identity, false));
    apply_node_op(op);
}

/// Starts the replace group of a non-restartable composable, identified by the type name of a
/// marker closure (its parameters may borrow, so no `TypeId`).
pub(crate) fn start_replace_group_of(key: u64, identity: &'static str) {
    let identity = Some(Identity::Name(identity));
    let (op, _) =
        with_runtime(|runtime| runtime.start_group(GroupKind::Replace, key, 0, identity, false));
    apply_node_op(op);
}

/// A group guard dropped while a panic unwinds: its group stays open. Recorded, so the error a
/// caught panic later causes says why.
pub(crate) fn abandon_group() {
    try_with_runtime(|runtime| runtime.abandoned_groups = true);
}

fn insert_node(
    parent: NodeRef,
    index: u32,
    region: u32,
    node: &mut dyn Any,
    node_type: &str,
) -> NodeRef {
    flush_node_moves();
    let applier = with_runtime(|runtime| {
        runtime.node_changes += 1;
        runtime.active_applier()
    });
    let mut applier =
        applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
    // In a region: made apart, attached when the paused composition is applied.
    let inserted = if region != NO_REGION {
        applier.create_erased(node)
    } else {
        applier.insert_erased(parent, index as usize, node)
    };
    drop(applier);
    let inserted = inserted.unwrap_or_else(|| {
        panic!(
            "compose-runtime: {} ({node_type}: a composable emitting it was called in a \
                 composition of other nodes, as UI content in a desktop `application` outside \
                 its `Window`s)",
            ComposeError::WrongNodeType
        )
    });
    with_runtime(|runtime| {
        runtime.set_inserted_node(inserted);
        if region != NO_REGION {
            runtime.record_attach(region, parent, index, inserted);
        }
    });
    inserted
}

/// A restart group's kept arguments, taken out of the group by [`Composer::take_kept`] to compare
/// them with new ones and replace them, then put back with [`put`](KeptArguments::put).
///
/// What `#[composable]` expands to instead of [`Composer::update_kept`]: the comparison is
/// written inline, so a composable instantiates no closures and no generic function of its own
/// beyond [`get`](KeptArguments::get) and [`set`](KeptArguments::set).
#[must_use = "the kept arguments are lost unless given back with `put`"]
pub struct KeptArguments(Option<Rc<dyn Any>>);

impl KeptArguments {
    /// The kept arguments, if there are some and they are a `K`.
    pub fn get<K: 'static>(&self) -> Option<&K> {
        match &self.0 {
            Some(kept) => kept.downcast_ref::<K>(),
            None => None,
        }
    }

    /// Replaces the kept arguments with `value`: in place when nothing else holds them, so a
    /// change allocates nothing. The old arguments drop here, outside the runtime's borrow.
    pub fn set<K: 'static>(&mut self, value: K) {
        if let Some(old) = self.unique()
            && let Some(old) = old.downcast_mut::<K>()
        {
            *old = value;
        } else {
            self.0 = Some(Rc::new(value));
        }
    }

    /// The kept arguments, if nothing else holds them.
    fn unique(&mut self) -> Option<&mut dyn Any> {
        Rc::get_mut(self.0.as_mut()?)
    }

    /// Puts the kept arguments back in their group; `changed` says they differ from last time's,
    /// so the body runs. Returns `changed`.
    pub fn put(self, changed: bool) -> bool {
        match with_runtime(|runtime| runtime.put_kept(self.0, changed)) {
            Ok((old, None)) => drop(old),
            Ok((old, Some(callback))) => {
                drop(old);
                // A pause point: paused, the body skips (`skipping` says so).
                let pause = callback.should_pause();
                drop(callback);
                with_runtime(|runtime| runtime.decide_pause(pause));
                return !pause;
            }
            Err((kept, error)) => {
                drop(kept);
                misuse(error)
            }
        }
        changed
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

    /// Starts the group of `branch` (from 1) of a conditional (an `if` chain, a `match`) whose
    /// groups share `key`: another branch's group found where it starts is deleted there, as
    /// Compose's `startReplaceGroup` deletes the group it doesn't expect. End it with
    /// [`end_group`](Composer::end_group).
    pub fn start_branch_group(&self, key: u64, branch: u64) {
        start_branch_group_of(key, branch);
    }

    /// Starts a keyed group, matched by `key` and `data_key`, so it moves with its data.
    pub fn start_movable_group(&self, key: u64, data_key: u64) {
        start_group(GroupKind::Movable, key, data_key);
    }

    /// Starts a group that emits one node, as Compose's `startNode` (`ComposeNode`). `factory`
    /// runs only when the group is new; the node is inserted into the applier's tree at once.
    /// Content being reused (`ReusableContent`) replaces it with a new one. End it with
    /// [`end_group`](Composer::end_group).
    pub fn start_node<N: 'static>(&self, key: u64, factory: impl FnOnce() -> N) -> NodeRef {
        match start_group(GroupKind::Node, key, 0) {
            NodeStart::Reused(node) | NodeStart::Reusing(node, _) => node,
            NodeStart::Insert { parent, index, region } => {
                insert_node(parent, index, region, &mut Some(factory()), std::any::type_name::<N>())
            }
            NodeStart::NotNode => unreachable!("a node group"),
        }
    }

    /// Starts a group that emits one reusable node, as Compose's `startReusableNode`
    /// (`ReusableComposeNode`): like [`start_node`](Composer::start_node), but content being
    /// reused keeps the node, which the applier resets ([`Applier::reuse`]) before the pass
    /// updates it.
    pub fn start_reusable_node<N: 'static>(
        &self,
        key: u64,
        factory: impl FnOnce() -> N,
    ) -> NodeRef {
        match start_group_of(GroupKind::Node, key, 0, true) {
            NodeStart::Reused(node) => node,
            NodeStart::Reusing(node, region) => {
                reuse_node(node, region);
                node
            }
            NodeStart::Insert { parent, index, region } => {
                insert_node(parent, index, region, &mut Some(factory()), std::any::type_name::<N>())
            }
            NodeStart::NotNode => unreachable!("a node group"),
        }
    }

    /// Starts a group whose content is pausable content ([`PausableContent`]'s): inserted or
    /// reused in a paused composition's first pass, it's paused content, which may pause and
    /// is applied by the paused composition's `apply`, unless the paused composition is for
    /// another `target` ([`Composition::recompose_pausable_for`]). End it with
    /// [`end_group`](Composer::end_group).
    ///
    /// [`PausableContent`]: crate::platform::PausableContent
    /// [`Composition::recompose_pausable_for`]: crate::Composition::recompose_pausable_for
    pub fn start_pausable_group(&self, key: u64, target: Option<u64>) {
        let (op, _) = with_runtime(|runtime| {
            let started = runtime.start_group(GroupKind::Replace, key, 0, None, false);
            runtime.begin_region(target);
            started
        });
        apply_node_op(op);
    }

    /// Starts a reusable group, as Compose's `startReusableGroup` (what `ReusableContent` and
    /// `ReusableContentHost` are written on): matched by `key` alone, its `data_key` kept. Met
    /// with another `data_key`, its content is reused until it ends: slots read as empty
    /// (`remember` computes anew, the replaced values are forgotten), bodies run, groups are
    /// matched as usual, reusable nodes are kept and updated ([`Applier::reuse`]) and other
    /// nodes replaced. End it with [`end_reusable_group`](Composer::end_reusable_group).
    pub fn start_reusable_group(&self, key: u64, data_key: u64) {
        start_group(GroupKind::Reusable, key, data_key);
    }

    /// Ends a reusable group, and the reuse it started.
    pub fn end_reusable_group(&self) {
        self.end_group();
    }

    /// Deactivates the rest of the current reusable group, as Compose's `deactivateToEndGroup`
    /// (`ReusableContentHost` inactive): its groups and nodes stay, for content to reuse later;
    /// if `changed` (the content was active), or if content is being reused (when Compose's
    /// `changed(active)` is true too), its remember observers are forgotten at the end of the
    /// pass (their slots emptied), its recompose scopes freed, and its nodes deactivated
    /// ([`Applier::deactivate`]). Called right after the group starts.
    pub fn deactivate_to_end_group(&self, changed: bool) {
        let ops = with_runtime(|runtime| runtime.deactivate_to_end_group(changed));
        for op in ops {
            apply_node_op(op);
        }
    }

    /// Schedules `action` for the end of the frame of this composition's recomposer, as
    /// `Composer.scheduleFrameEndCallback`.
    pub fn schedule_frame_end_callback(
        &self,
        action: impl FnOnce() + 'static,
    ) -> crate::recomposer::CancellationHandle {
        let recomposer = crate::runtime::composing_recomposer()
            .unwrap_or_else(crate::recomposer::Recomposer::current_thread);
        recomposer.schedule_frame_end_callback(action)
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
        match with_runtime(|runtime| runtime.skipping()) {
            Skip::Yes => true,
            Skip::No => false,
            Skip::Ask(callback) => {
                // A pause point: the paused composition's callback runs outside the borrow.
                let pause = callback.should_pause();
                drop(callback);
                with_runtime(|runtime| runtime.decide_pause(pause));
                pause
            }
        }
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
        let mut kept = self.take_kept();
        let differs = match kept.get::<K>() {
            Some(old) => changed(old),
            None => true,
        };
        if differs {
            kept.set(make());
        }
        kept.put(differs)
    }

    /// Takes the current restart group's kept arguments out, to compare them with new ones: the
    /// steps of [`update_kept`](Composer::update_kept), for generated code.
    pub fn take_kept(&self) -> KeptArguments {
        KeptArguments(with_runtime(|runtime| runtime.take_kept()))
    }

    /// The recompose scope state reads register at this point.
    pub fn current_scope(&self) -> RecomposeScope {
        let id = with_runtime(|runtime| runtime.current_scope());
        RecomposeScope { id, _not_send: PhantomData }
    }

    /// Runs `f` with the composition's applier, as type `A`.
    /// Whether the composition's applier is an `A`: a composable emitting `A`'s nodes can say
    /// what's wrong when called in another kind of composition.
    pub fn applier_is<A: Applier>(&self) -> bool {
        let applier = applier_for_use();
        applier.try_borrow_mut().is_ok_and(|mut applier| applier.as_any_mut().is::<A>())
    }

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
