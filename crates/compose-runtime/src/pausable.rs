//! Pausable composition, as Compose 1.12.1's `PausableComposition` and `PausedComposition`
//! (docs/04, "Pausable composition"; D590-D596).
//!
//! Compose pauses a composition by skipping: at a restartable composable's start (`shouldExecute`)
//! in content being inserted or reused, it asks the paused composition's `ShouldPauseCallback`,
//! and if told to pause, it skips the body (the group stays empty, or keeps the content being
//! reused) and notes the scope, which the next `resume` recomposes from its start. Each item of
//! a Compose lazy list is a subcomposition of its own, and the paused composition is that one;
//! its changes go to a `RecordingApplier` and its remember events to a dispatcher of its own,
//! both played at `apply`.
//!
//! compose-rs composes a lazy list's items in the list's composition, so the paused content is
//! a part of a composition: a *region*. A region is a [`PausableContent`] group inserted or
//! reused in a paused composition's first pass ([`Composition::recompose_pausable`]; one made
//! for a target, [`Composition::recompose_pausable_for`], only that target's
//! [`PausableContentFor`]); the rest of that pass is an ordinary pass, applied at its end.
//! Inside a region:
//!
//! - restartable composables pause where Compose's do, when inserting or reusing;
//! - node changes are recorded and applied at [`apply`](PausedComposition::apply): a new node
//!   is made in no parent (`Applier::create`) and attached then, moves and removals of its
//!   nodes wait too, and so does resetting a reused node (`Applier::reuse`, Compose's recorded
//!   `REUSE`); the pass updates the nodes it makes and reuses meanwhile (Compose records the
//!   updates too, compose-rs's nodes are updated through the applier at once);
//! - remember observers, side effects, deactivations, releases and forgotten values wait for
//!   `apply`, observers and effects in the order they'd have had without pausing;
//! - the region's nodes count as those it had before (none if new) for the composition around
//!   it until `apply`, so other passes place their nodes as if it weren't there;
//! - the resumes walk the regions only (their paused scopes, and those invalidated since), and
//!   a state write invalidating a scope inside a region marks it for the next resume, not for
//!   the composition's next pass (as Compose's paused composition takes its invalidations);
//! - a pass of the composition reaching a region's group (a body around it ran) composes what
//!   changed in it into the region, recorded as the region's, and leaves its paused scopes to
//!   the resumes (a paused scope reached through its parent's body resumes there, as Compose's
//!   `addRecomposeScope`); one removing or deactivating it discards it (the paused composition
//!   is cancelled if nothing's left), as Compose disposes a precomposed slot; others leave it
//!   alone, as Compose's paused composition is a composition of its own; a layout needing it
//!   asks for it to be completed before the next pass (`request_completion`), as Compose's
//!   measurement completes a precomposed item; dropped unapplied, it's cancelled.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

use crate::applier::{ErasedApplier, NodeRef};
use crate::composer::{Frame, Mode, current, run_walk};
use crate::composition::{Composition, Finishing, Pass, PassKind};
use crate::error::{ComposeError, misuse};
use crate::hash::key_of;
use crate::remember::{ErasedObserver, dispatch_forgotten};
use crate::runtime::{
    CompositionId, Deferred, DeferredEffect, Runtime, invalidate_scope_parts, remove_siblings,
    request_frames, try_with_runtime, with_runtime,
};
use crate::scope::ScopeId;
use crate::slot_table::{GroupKind, NONE, SENTINEL, SlotTable, SlotValue};

/// No region: a frame outside paused content.
pub(crate) const NO_REGION: u32 = u32::MAX;

/// The group key of [`PausableContent`].
pub(crate) const PAUSABLE_KEY: u64 = key_of("compose_runtime::PausableContent");

/// What a paused composition asks at each pause point, as Compose's `ShouldPauseCallback`:
/// whether to pause there. A closure `Fn() -> bool` is one.
pub trait ShouldPauseCallback {
    fn should_pause(&self) -> bool;
}

impl<F: Fn() -> bool> ShouldPauseCallback for F {
    fn should_pause(&self) -> bool {
        self()
    }
}

/// Where a paused composition is, as Compose's `PausedCompositionState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PausedState {
    /// Not composed yet: the first `resume` runs the first pass.
    InitialPending,
    /// Paused, or invalidated since: `resume` has work left.
    RecomposePending,
    /// Complete: `apply` applies it.
    ApplyPending,
    Applied,
    Cancelled,
}

/// A node change a region recorded, applied by `apply`. An index into the region's node parent
/// counts from the region's first node there; one into a node of the region, from its first
/// child.
#[derive(Clone, Copy, Debug)]
pub(crate) enum RecordedOp {
    Attach {
        parent: NodeRef,
        index: u32,
        node: NodeRef,
    },
    Move {
        parent: NodeRef,
        from: u32,
        to: u32,
        count: u32,
    },
    Remove {
        parent: NodeRef,
        index: u32,
        count: u32,
    },
    /// `Applier::reuse`: as Compose's recorded `reuse`, the node is reset (and a deactivated one
    /// made active, its focus requesters attached) only as it's applied.
    Reuse {
        node: NodeRef,
    },
    /// `Applier::deactivate`: in order with what's reused, as content goes and comes while paused.
    Deactivate {
        node: NodeRef,
    },
}

/// A remember observer, a side effect or a forgotten value waiting for `apply`, or the place of
/// a paused scope's (its segment), so the resumed scope's go where they would have gone without
/// the pause.
pub(crate) enum Entry {
    Observer(Rc<dyn ErasedObserver>),
    Effect(DeferredEffect),
    Forgotten(SlotValue),
    Segment(u32),
}

/// Pausable content inserted or reused in a paused composition's first pass, until applied.
pub(crate) struct Region {
    /// Its group (a `PausableContent`'s).
    pub root: u32,
    /// The node its top-level nodes are children of.
    pub node_parent: NodeRef,
    /// The nodes it has in `node_parent` now, which the composition around it counts: none if
    /// it's new, its old ones if reused. Its group's `node_count` says this until applied.
    pub attached: u32,
    /// Its top-level nodes once applied, as last composed.
    pub nodes: u32,
    /// Scopes in it wait for a resume (paused, or invalidated since).
    pub pending: bool,
    pub ops: Vec<RecordedOp>,
    /// The nodes it made (in no parent until applied).
    pub created: Vec<NodeRef>,
    /// Its top-level nodes before it was reused: what cancelling it removes.
    pub old_nodes: Vec<NodeRef>,
    /// Its deactivated and released nodes; its observers, effects and forgotten values are in
    /// the paused composition's segments (forgotten values wait here until `settle`).
    pub deferred: Deferred,
    /// Its segment (in segment 0, in the first pass's order).
    pub segment: u32,
    /// A pass of the composition reached its group: what it composes there is the region's
    /// (recorded, applied by `apply`), its paused scopes left to the resumes. Reset as the pass
    /// ends.
    pub adopted: bool,
    /// Discarded by a pass removing or deactivating it (`discard_regions_in`): no longer
    /// paused content, its group gone or emptied; skipped by resumes, `apply` and `cancel`.
    pub dead: bool,
}

/// A composition's paused composition, in its data.
pub(crate) struct Paused {
    pub state: Rc<Cell<PausedState>>,
    /// Cancelled while its content couldn't be discarded (during a pass, or a layout): it is
    /// before the composition's next pass, and holds its regions until then.
    pub cancel_requested: bool,
    /// A layout needs it: completed and applied before the composition's next pass.
    pub complete_requested: bool,
    pub regions: Vec<Region>,
    /// The pausable content it pauses: `PausableContentFor` this target only
    /// ([`Composition::recompose_pausable_for`]), or any (`None`).
    pub target: Option<u64>,
    /// Segment 0 holds the regions' observers and effects in their first pass's order, and the
    /// places of the scopes paused in it; each paused scope has one of its own.
    pub segments: Vec<Vec<Entry>>,
}

impl Paused {
    fn new(state: Rc<Cell<PausedState>>, target: Option<u64>) -> Paused {
        Paused {
            state,
            cancel_requested: false,
            complete_requested: false,
            regions: Vec::new(),
            target,
            segments: vec![Vec::new()],
        }
    }

    /// What a deactivation just deferred for `region` goes to its record, in order with its reuses.
    pub fn record_deactivations(&mut self, region: u32) {
        let region = &mut self.regions[region as usize];
        for node in region.deferred.deactivated_nodes.drain(..) {
            region.ops.push(RecordedOp::Deactivate { node });
        }
    }

    /// Neither applied nor cancelled.
    pub fn in_progress(&self) -> bool {
        !matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled)
    }

    /// Its regions wait for `apply` (or a resume): their content isn't the composition's yet.
    /// From its first pass on, which makes them.
    pub fn holds_regions(&self) -> bool {
        self.regions.iter().any(|region| !region.dead)
            && (self.cancel_requested
                || !matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled))
    }

    /// The live region whose root is `group`.
    pub fn region_rooted_at(&self, group: u32) -> Option<&Region> {
        self.regions.iter().find(|region| region.root == group && !region.dead)
    }

    /// The live region holding `group` (an ancestor of it is a region's root).
    pub fn region_of(&self, table: &SlotTable, group: u32) -> Option<usize> {
        let mut current = table[group].parent;
        while current != NONE {
            if let Some(index) =
                self.regions.iter().position(|region| region.root == current && !region.dead)
            {
                return Some(index);
            }
            current = table[current].parent;
        }
        None
    }

    /// A pass of the composition starts `group`: if it's a live region's (not adopted yet), what
    /// the pass composes in it from there is recorded as the region's (D592). The region and its
    /// segment.
    pub fn adopt(&mut self, group: u32) -> Option<(u32, u32)> {
        if !self.holds_regions() {
            return None;
        }
        let index = self.regions.iter().position(|region| region.root == group && !region.dead)?;
        let region = &mut self.regions[index];
        if region.adopted {
            return None;
        }
        region.adopted = true;
        Some((index as u32, region.segment))
    }

    /// A pass removes `removed` (siblings), or deactivates their content (`deactivating`): the
    /// regions inside go (D593), what they made released, the values their reused content
    /// replaced forgotten (into `deferred`, the pass's), their observers abandoned with the
    /// removed slots. Deactivating, a region's group is removed now, its old nodes from their
    /// parent (the node changes returned); removing, its group goes with the rest and
    /// `deferred` releases its nodes once.
    pub fn discard_regions_in(
        &mut self,
        table: &mut SlotTable,
        scopes: &mut crate::scope::Scopes,
        removal_stack: &mut Vec<crate::runtime::RemovalStep>,
        deferred: &mut Deferred,
        removed: Vec<u32>,
        deactivating: bool,
    ) -> Vec<crate::composer::NodeOp> {
        let mut ops = Vec::new();
        for index in 0..self.regions.len() {
            if self.regions[index].dead {
                continue;
            }
            let root = self.regions[index].root;
            // Inside the removed groups? Under a node of theirs (released with it)?
            let (mut current, mut under_node, mut inside) = (root, false, false);
            loop {
                if current != root && table[current].kind == GroupKind::Node {
                    under_node = true;
                }
                if removed.contains(&current) {
                    inside = true;
                    break;
                }
                current = table[current].parent;
                if current == NONE {
                    break;
                }
            }
            if !inside {
                continue;
            }
            let segment = self.regions[index].segment;
            let mut flat = Flat::default();
            flatten(&mut self.segments, segment, &mut flat);
            let region = &mut self.regions[index];
            region.dead = true;
            deferred.removed_values.append(&mut region.deferred.removed_values);
            deferred.removed_values.append(&mut flat.forgotten);
            deferred.released_nodes.append(&mut region.created);
            if deactivating {
                // The content stays (deactivated) but this region's group goes now.
                let offset = node_offset(table, root);
                if region.attached > 0 {
                    ops.push(crate::composer::NodeOp::Remove {
                        parent: region.node_parent,
                        index: offset,
                        count: region.attached,
                    });
                }
                deferred.released_nodes.append(&mut region.old_nodes);
                set_node_count(table, root, 0);
                let (parent, next) = (table[root].parent, table[root].next_sibling);
                let mut previous = NONE;
                let mut sibling = table[parent].first_child;
                while sibling != root {
                    previous = sibling;
                    sibling = table[sibling].next_sibling;
                }
                table.link_after(parent, previous, next);
                let mut gone = Deferred::default();
                remove_siblings(table, &mut gone, scopes, removal_stack, root, next);
                deferred.removed_values.append(&mut gone.removed_values);
            } else {
                if !under_node {
                    deferred.released_nodes.append(&mut region.old_nodes);
                }
                // Its top-level nodes are released by the removal too: once.
                deferred.repeated_releases = true;
            }
        }
        if !self.regions.iter().any(|region| !region.dead) {
            self.state.set(PausedState::Cancelled);
        }
        ops
    }

    /// The deferred work of `region`'s nodes and values.
    pub fn deferred(&mut self, region: u32) -> &mut Deferred {
        &mut self.regions[region as usize].deferred
    }

    pub fn record(&mut self, region: u32, op: RecordedOp) {
        self.regions[region as usize].ops.push(op);
    }

    /// Moves the values `region` forgot just now to `segment`, so they're forgotten in the order
    /// they would have been without pausing.
    pub fn settle(&mut self, region: u32, segment: u32) {
        let values = &mut self.regions[region as usize].deferred.removed_values;
        if !values.is_empty() {
            let entries = &mut self.segments[segment as usize];
            entries.extend(values.drain(..).map(Entry::Forgotten));
        }
    }
}

/// The deferred work a frame's changes go to: its region's, if it's in one.
pub(crate) fn deferred_of<'a>(
    deferred: &'a mut Deferred,
    paused: &'a mut Option<Box<Paused>>,
    region: u32,
) -> &'a mut Deferred {
    match paused {
        Some(paused) if region != NO_REGION => paused.deferred(region),
        _ => deferred,
    }
}

/// The nodes `group`'s content has in its nearest node ancestor: its node groups not inside
/// another node group, in order.
fn top_level_nodes(table: &SlotTable, group: u32) -> Vec<crate::applier::NodeRef> {
    let mut nodes = Vec::new();
    let mut stack = Vec::new();
    let mut child = table[group].first_child;
    while child != NONE {
        stack.push(child);
        child = table[child].next_sibling;
    }
    stack.reverse();
    while let Some(index) = stack.pop() {
        let entry = &table[index];
        if entry.kind == GroupKind::Node {
            nodes.push(entry.node);
            continue;
        }
        let start = stack.len();
        let mut child = entry.first_child;
        while child != NONE {
            stack.push(child);
            child = table[child].next_sibling;
        }
        stack[start..].reverse();
    }
    nodes
}

/// Where `group`'s nodes start in its nearest node ancestor (or the composition's root node):
/// the nodes of the groups before it, at each level up to that node's group.
fn node_offset(table: &SlotTable, group: u32) -> u32 {
    let mut offset = 0;
    let mut current = group;
    loop {
        let parent = table[current].parent;
        if parent == NONE {
            return offset;
        }
        let mut sibling = table[parent].first_child;
        while sibling != current && sibling != NONE {
            offset += table[sibling].node_count;
            sibling = table[sibling].next_sibling;
        }
        if table[parent].kind == GroupKind::Node || parent == SENTINEL {
            return offset;
        }
        current = parent;
    }
}

/// Sets `group`'s node count to `count`, and its ancestors' up to its nearest node ancestor.
fn set_node_count(table: &mut SlotTable, group: u32, count: u32) {
    let old = table[group].node_count;
    if old == count {
        return;
    }
    table[group].node_count = count;
    let mut current = table[group].parent;
    while current != NONE && table[current].kind != GroupKind::Node {
        let entry = &mut table[current];
        entry.node_count = entry.node_count - old + count;
        current = entry.parent;
    }
}

impl Runtime {
    /// Starts a region at the group just started (a [`PausableContent`]'s), if this is a paused
    /// composition's first pass and the group is being inserted or reused there.
    pub(crate) fn begin_region(&mut self, target: Option<u64>) {
        let composition = self.check_composing();
        if !self.pausable_pass {
            return;
        }
        let Runtime { compositions, frames, reusing, .. } = self;
        let Some(frame) = frames.last_mut() else { return };
        if frame.region != NO_REGION || !(frame.inserting || *reusing != 0) {
            return;
        }
        let data = compositions.data_mut(composition);
        let Some(paused) = data.paused.as_mut() else { return };
        // Another item's content (another list's, composed in this pass for writes of its
        // own): ordinary content, as Compose's other subcompositions aren't this one.
        if paused.target.is_some() && paused.target != target {
            return;
        }
        let table = &data.table;
        let (attached, old_nodes) = if frame.inserting {
            (0, Vec::new())
        } else {
            (table[frame.group].node_count, top_level_nodes(table, frame.group))
        };
        frame.region = paused.regions.len() as u32;
        let segment = paused.segments.len() as u32;
        paused.segments.push(Vec::new());
        paused.segments[0].push(Entry::Segment(segment));
        frame.segment = segment;
        // Counted from the region's start: other passes may place nodes before it until it's
        // applied.
        frame.node_index = 0;
        frame.node_start = 0;
        paused.regions.push(Region {
            root: frame.group,
            node_parent: frame.node_parent,
            attached,
            nodes: attached,
            pending: false,
            ops: Vec::new(),
            created: Vec::new(),
            old_nodes,
            deferred: Deferred::default(),
            segment,
            adopted: false,
            dead: false,
        });
    }

    /// Pauses the restart group composing now, which skips its body (Compose's `shouldExecute`
    /// told to pause): its scope waits for the next resume, invalid, its ancestors in the
    /// region dirty, and gets a segment for its observers and effects in its place.
    pub(crate) fn pause_current(&mut self) {
        let composition = self.check_composing();
        let Runtime { compositions, scopes, frames, reusing, .. } = self;
        let frame = frames.last_mut().expect("a restart group composing");
        frame.paused = true;
        let data = compositions.data_mut(composition);
        let paused = data.paused.as_mut().expect("a paused composition's pass");
        let segment = paused.segments.len() as u32;
        paused.segments.push(Vec::new());
        paused.segments[frame.segment as usize].push(Entry::Segment(segment));
        if let Some(scope) = scopes.get_mut(frame.scope) {
            scope.invalid = true;
            scope.paused = true;
            scope.reusing = *reusing != 0;
            scope.segment = segment;
        }
        let region = &mut paused.regions[frame.region as usize];
        region.pending = true;
        let table = &mut data.table;
        let mut group = table[frame.group].parent;
        while group != region.root && group != NONE && !table[group].dirty {
            table[group].dirty = true;
            group = table[group].parent;
        }
    }

    /// Records a node made in a region, to attach as it's applied.
    pub(crate) fn record_attach(
        &mut self,
        region: u32,
        parent: NodeRef,
        index: u32,
        node: NodeRef,
    ) {
        let composition = self.check_composing();
        let paused =
            self.compositions.data_mut(composition).paused.as_mut().expect("a region's pass");
        let region_data = &mut paused.regions[region as usize];
        region_data.created.push(node);
        region_data.ops.push(RecordedOp::Attach { parent, index, node });
    }

    /// Records a node reused in a region, to reset as it's applied.
    pub(crate) fn record_reuse(&mut self, region: u32, node: NodeRef) {
        let composition = self.check_composing();
        let paused =
            self.compositions.data_mut(composition).paused.as_mut().expect("a region's pass");
        paused.regions[region as usize].ops.push(RecordedOp::Reuse { node });
    }

    /// Opens a resume's walk of `region` (its paused and invalidated scopes).
    fn push_region_frame(&mut self, region: u32) {
        let composition = self.check_composing();
        let data = self.compositions.data_mut(composition);
        let paused = data.paused.as_mut().expect("a paused composition");
        let region_data = &mut paused.regions[region as usize];
        region_data.pending = false;
        let (root, segment) = (region_data.root, region_data.segment);
        self.frames.push(Frame {
            group: root,
            mode: Mode::Walk,
            inserting: false,
            forced: false,
            skipped: true,
            dirty: true,
            cursor: data.table[root].first_child,
            limit: NONE,
            last_placed: NONE,
            slot_cursor: 0,
            node_parent: region_data.node_parent,
            node_index: 0,
            node_start: 0,
            scope: ScopeId::NONE,
            saved_epoch: 0,
            body_started: false,
            region,
            segment,
            resuming: false,
            paused: false,
        });
    }

    fn pop_region_frame(&mut self, region: u32) {
        let composition = self.check_composing();
        let frame = self.frames.pop().expect("the region's frame");
        let paused = self.compositions.data_mut(composition).paused.as_mut().expect("paused");
        let region_data = &mut paused.regions[region as usize];
        debug_assert_eq!(frame.group, region_data.root);
        region_data.nodes = frame.node_index - frame.node_start;
    }
}

/// A paused composition, as Compose's `PausedComposition`: from
/// [`Composition::recompose_pausable`], resumed until complete, then applied (or cancelled).
pub struct PausedComposition {
    composition: CompositionId,
    state: Rc<Cell<PausedState>>,
    /// The content its first pass sets, a pausable composition's.
    content: RefCell<Option<Rc<dyn Fn()>>>,
    _not_send: PhantomData<*const ()>,
}

impl Drop for PausedComposition {
    /// A paused composition no one holds can't be resumed or applied: it's cancelled, so it
    /// doesn't hold the composition's paused content (and its next paused composition) for good.
    fn drop(&mut self) {
        if !matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled) {
            self.cancel();
        }
    }
}

impl Composition {
    /// A paused composition of this composition's invalid scopes, as Compose's
    /// `PausableComposition.setPausableContent`: its first [`resume`](PausedComposition::resume)
    /// runs the pass `recompose` would, in which each [`PausableContent`] inserted or reused is
    /// paused content: its composables may pause when the callback says so, and nothing of it
    /// is applied before [`apply`](PausedComposition::apply). A pass of this composition before
    /// then that reaches paused content composes it and applies it; one removing it discards it.
    ///
    /// # Panics
    ///
    /// If another paused composition of this composition is in progress (not applied or
    /// cancelled), as Compose's.
    pub fn recompose_pausable(&self) -> PausedComposition {
        self.recompose_pausable_with(None)
    }

    /// As [`recompose_pausable`](Composition::recompose_pausable), its paused content only the
    /// [`PausableContentFor`] of `target` (a lazy item's, as Compose's paused composition is
    /// the item's subcomposition): other pausable content its first pass inserts or reuses is
    /// ordinary content, applied at that pass's end (D597).
    pub fn recompose_pausable_for(&self, target: u64) -> PausedComposition {
        self.recompose_pausable_with(Some(target))
    }

    fn recompose_pausable_with(&self, target: Option<u64>) -> PausedComposition {
        let state = Rc::new(Cell::new(PausedState::InitialPending));
        let id = self.id;
        run_requests(id);
        with_runtime(|runtime| {
            let data = runtime.compositions.get_mut(id).expect("a live composition");
            // One finished (applied or discarded by a pass) is gone.
            if data.paused.as_ref().is_some_and(|paused| {
                matches!(paused.state.get(), PausedState::Applied | PausedState::Cancelled)
                    && !paused.cancel_requested
            }) {
                data.paused = None;
            }
            if data.paused.is_some() {
                misuse(ComposeError::PausedCompositionState)
            }
            data.paused = Some(Box::new(Paused::new(state.clone(), target)));
        });
        PausedComposition {
            composition: id,
            state,
            content: RefCell::new(None),
            _not_send: PhantomData,
        }
    }

    /// A paused composition setting `content`: a [`PausableComposition`]'s.
    pub(crate) fn paused_content(&self, content: Rc<dyn Fn()>) -> PausedComposition {
        with_runtime(|runtime| {
            if runtime.compositions.get(self.id).is_some_and(|data| data.disposed) {
                misuse(ComposeError::Disposed)
            }
        });
        let paused = self.recompose_pausable_with(None);
        *paused.content.borrow_mut() = Some(content);
        paused
    }
}

/// What one keeps of a [`PausedComposition`] to know whether its content is in the composition
/// unapplied, whoever holds the paused composition: a lazy layout's check that an item's content
/// is its to measure (D599), in place of a remember observer in every item.
#[derive(Clone)]
pub struct PausedWatch {
    composition: CompositionId,
    state: Rc<Cell<PausedState>>,
}

impl PausedWatch {
    /// Whether paused content of its is there, not applied: a live region, paused or complete,
    /// or one cancelled with the removal waiting for the composition's next pass. Not composed
    /// yet, it has none; complete without paused content, none either (what its first pass
    /// composed outside paused content was applied at its end).
    pub fn holds_unapplied_content(&self) -> bool {
        let state = self.state.get();
        if matches!(state, PausedState::Applied | PausedState::InitialPending) {
            return false;
        }
        try_with_runtime(|runtime| {
            runtime
                .compositions
                .get(self.composition)
                .and_then(|data| data.paused.as_ref())
                .filter(|paused| Rc::ptr_eq(&paused.state, &self.state))
                .is_some_and(|paused| paused.holds_regions())
        })
        .unwrap_or(false)
    }
}

impl PausedComposition {
    /// What its state is held by: what a pausable composition keeps of its last.
    pub(crate) fn state(&self) -> Rc<Cell<PausedState>> {
        self.state.clone()
    }

    /// A watch of its content (`PausedWatch`), which outlives it.
    pub fn watch(&self) -> PausedWatch {
        PausedWatch { composition: self.composition, state: self.state.clone() }
    }

    /// Whether it's composed: [`apply`](PausedComposition::apply) applies it (or it was applied).
    pub fn is_complete(&self) -> bool {
        matches!(self.state.get(), PausedState::ApplyPending | PausedState::Applied)
    }

    pub fn is_applied(&self) -> bool {
        self.state.get() == PausedState::Applied
    }

    pub fn is_cancelled(&self) -> bool {
        self.state.get() == PausedState::Cancelled
    }

    /// Composes what's left, asking `should_pause` at each pause point, as Compose's `resume`:
    /// a restartable composable starting in content being inserted or reused pauses (skips its
    /// body, to run it at the next resume) when it says so. Returns whether it's complete.
    ///
    /// # Panics
    ///
    /// Once complete, applied or cancelled, or during a pass, as Compose's.
    pub fn resume(&self, should_pause: impl ShouldPauseCallback + 'static) -> bool {
        // What the turn wrote may make it incomplete again.
        crate::snapshots::Snapshot::send_apply_notifications();
        if self.state.get() == PausedState::ApplyPending {
            misuse(ComposeError::PausedCompositionState)
        }
        self.resume_with(Some(Rc::new(should_pause)))
    }

    fn resume_with(&self, should_pause: Option<Rc<dyn ShouldPauseCallback>>) -> bool {
        // The turn's changes invalidate their readers first (D751), as before a pass.
        crate::snapshots::Snapshot::send_apply_notifications();
        match self.state.get() {
            PausedState::ApplyPending => return true,
            PausedState::Applied | PausedState::Cancelled => {
                misuse(ComposeError::PausedCompositionState)
            }
            PausedState::InitialPending => {
                // Applying a pass's end (an effect resuming it): no pass now, nothing done.
                let finishing = with_runtime(|runtime| {
                    runtime.compositions.get(self.composition).is_some_and(|data| data.finishing)
                });
                if finishing {
                    return false;
                }
                let content = self.content.borrow_mut().take();
                let mode = if content.is_some() { Mode::Normal } else { Mode::Walk };
                match Pass::begin_as(self.composition, mode, PassKind::Pausable, should_pause) {
                    Some(pass) => {
                        match content {
                            Some(content) => crate::composition::compose_root(content),
                            None => run_walk(0),
                        }
                        pass.finish();
                    }
                    // A disposed or closed composition, or nothing to compose: nothing to apply.
                    None if self.state.get() == PausedState::InitialPending => {
                        self.state.set(PausedState::ApplyPending);
                        return true;
                    }
                    None => return self.is_complete(),
                }
            }
            PausedState::RecomposePending => {
                let regions: Vec<u32> = with_runtime(|runtime| {
                    let Some(paused) = (runtime.compositions.get(self.composition))
                        .and_then(|data| data.paused.as_ref())
                    else {
                        return Vec::new();
                    };
                    (paused.regions.iter().enumerate())
                        .filter(|(_, region)| region.pending && !region.dead)
                        .map(|(index, _)| index as u32)
                        .collect()
                });
                if let Some(pass) =
                    Pass::begin_as(self.composition, Mode::Walk, PassKind::Resume, should_pause)
                {
                    for region in regions {
                        with_runtime(|runtime| runtime.push_region_frame(region));
                        run_walk(1);
                        with_runtime(|runtime| runtime.pop_region_frame(region));
                    }
                    pass.finish();
                }
            }
        }
        // Complete once nothing waits in its regions.
        with_runtime(|runtime| {
            let Some(paused) = runtime
                .compositions
                .get_mut(self.composition)
                .and_then(|data| data.paused.as_mut())
            else {
                return;
            };
            // Cancelled meanwhile (from its callback, during the pass), or finished by a pass of
            // the composition: as it is.
            if !Rc::ptr_eq(&paused.state, &self.state)
                || paused.cancel_requested
                || matches!(paused.state.get(), PausedState::Applied | PausedState::Cancelled)
            {
                return;
            }
            let pending = paused.regions.iter().any(|region| region.pending && !region.dead);
            paused.state.set(if pending {
                PausedState::RecomposePending
            } else {
                PausedState::ApplyPending
            });
        });
        self.is_complete()
    }

    /// Applies it, as Compose's `apply`: its regions' nodes are attached (and moved, removed,
    /// deactivated and released) as recorded, then their forgotten values get `on_forgotten`,
    /// their remember observers `on_remembered` and their side effects run.
    ///
    /// # Panics
    ///
    /// If it isn't complete, or was applied or cancelled.
    pub fn apply(&self) {
        if self.state.get() != PausedState::ApplyPending {
            misuse(ComposeError::PausedCompositionState)
        }
        apply(self.composition, &self.state);
    }

    /// Discards it, as Compose's `cancel`: its regions' content is removed, their nodes
    /// released, their remember observers get `on_abandoned`, and the composable around each is
    /// invalidated, to compose it again (as Compose's cancelled slot is recomposed when it's
    /// subcomposed again, `forceRecompose`). Nothing if it was applied or cancelled. During a
    /// pass, or while the applier is in use (a layout), the discarding waits for the
    /// composition's next pass, or its next paused composition: it's cancelled at once all the
    /// same.
    pub fn cancel(&self) {
        if matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled) {
            return;
        }
        let deferred = try_with_runtime(|runtime| {
            let busy = runtime.active.is_some();
            let Some(data) = runtime.compositions.get_mut(self.composition) else { return false };
            let Some(paused) = data.paused.as_mut() else { return false };
            if !Rc::ptr_eq(&paused.state, &self.state) {
                return false;
            }
            let applier = crate::context::resolve_applier(&runtime.lent, &data.applier);
            let busy = busy || data.finishing || applier.try_borrow_mut().is_err();
            if busy {
                paused.cancel_requested = true;
            }
            busy
        });
        match deferred {
            Some(true) => self.state.set(PausedState::Cancelled),
            Some(false) => cancel(self.composition, &self.state, true),
            // The thread's runtime is gone.
            None => self.state.set(PausedState::Cancelled),
        }
    }

    /// Asks for it to be completed and applied before the composition's next pass (no pause
    /// then), as Compose's measurement completes a precomposed item's paused composition it
    /// needs: what a layout needing its content does, which can't compose itself.
    pub fn request_completion(&self) {
        if matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled) {
            return;
        }
        with_runtime(|runtime| {
            if let Some(paused) =
                runtime.compositions.get_mut(self.composition).and_then(|data| data.paused.as_mut())
                && Rc::ptr_eq(&paused.state, &self.state)
            {
                paused.complete_requested = true;
            }
        });
    }
}

/// Before a pass of `id` or its next paused composition: what was asked of its paused
/// composition and couldn't be done then. A cancel asked during a pass or a layout discards it;
/// a completion a layout asked for resumes it, unpaused, and applies it (once: a composition
/// applying a pass's end runs no pass, and it waits for the next).
pub(crate) fn run_requests(id: CompositionId) {
    let requested = with_runtime(|runtime| {
        let data = runtime.compositions.get_mut(id)?;
        if data.finishing {
            return None;
        }
        let paused = data.paused.as_mut()?;
        if paused.cancel_requested {
            return Some((paused.state.clone(), true));
        }
        // Complete without paused content, it's applied all the same (its request's apply
        // step waits for nothing then).
        if paused.complete_requested
            && (paused.holds_regions() || paused.state.get() == PausedState::ApplyPending)
        {
            paused.complete_requested = false;
            return Some((paused.state.clone(), false));
        }
        None
    });
    match requested {
        Some((state, true)) => cancel(id, &state, true),
        Some((state, false))
            if resume_state(id, &state, None) && state.get() == PausedState::ApplyPending =>
        {
            apply(id, &state)
        }
        _ => {}
    }
}

/// `resume` for whoever holds `state` (a `PausedComposition`, or a request run for it).
fn resume_state(
    id: CompositionId,
    state: &Rc<Cell<PausedState>>,
    should_pause: Option<Rc<dyn ShouldPauseCallback>>,
) -> bool {
    let paused = std::mem::ManuallyDrop::new(PausedComposition {
        content: RefCell::new(None),
        composition: id,
        state: state.clone(),
        _not_send: PhantomData,
    });
    paused.resume_with(should_pause)
}

/// After a pass of `id`: paused content it reached (adopted) stays the paused composition's,
/// with what the pass changed in it recorded, its paused and invalidated scopes left for its
/// resumes, as Compose's paused composition is a composition of its own that its parent's
/// recomposition doesn't run (D592). A paused composition with no paused content left (the pass
/// removed it) is finished.
pub(crate) fn finish_pass_regions(id: CompositionId) {
    with_runtime(|runtime| {
        let Some(data) = runtime.compositions.get_mut(id) else { return };
        let Some(paused) = data.paused.as_mut() else { return };
        for region in &mut paused.regions {
            region.adopted = false;
        }
        if paused.cancel_requested
            || paused.regions.is_empty()
            || paused.regions.iter().any(|region| !region.dead)
        {
            return;
        }
        paused.state.set(PausedState::Cancelled);
        data.paused = None;
    });
}

/// Applies `id`'s complete paused composition.
fn apply(id: CompositionId, state: &Rc<Cell<PausedState>>) {
    // Below a node of the parent's that went: nowhere to show it, so it goes, as cancelled.
    let root_gone = with_runtime(|runtime| {
        let child = runtime.compositions.get(id).is_some_and(|data| data.parent.is_some());
        child && !crate::composition::root_is_there(runtime, id)
    });
    if root_gone {
        cancel(id, state, false);
        return;
    }
    let taken = with_runtime(|runtime| {
        if runtime.active.is_some() {
            misuse(ComposeError::PassInProgress)
        }
        let data = runtime.compositions.get_mut(id)?;
        let paused = data.paused.take()?;
        data.finishing = true;
        runtime.finishing += 1;
        let applier = data.applier.clone();
        Some((paused, crate::context::resolve_applier(&runtime.lent, &applier)))
    });
    state.set(PausedState::Applied);
    let Some((mut paused, applier)) = taken else { return };
    let finishing = Finishing { id, freeing: Vec::new() };
    let mut deferred = Deferred::default();
    {
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        applier.on_begin_changes();
        // In tree order: a region's nodes go where the regions before it in the same parent
        // left room for them.
        for region in paused.regions.iter_mut().filter(|region| !region.dead) {
            let offset = with_runtime(|runtime| {
                let table = &runtime.compositions.get(id).expect("a live composition").table;
                node_offset(table, region.root)
            });
            replay(&mut *applier, region, offset);
            with_runtime(|runtime| {
                let table = &mut runtime.compositions.get_mut(id).expect("live").table;
                set_node_count(table, region.root, region.nodes);
            });
            let Deferred { deactivated_nodes, released_nodes, removed_values, .. } =
                &mut region.deferred;
            deferred.deactivated_nodes.append(deactivated_nodes);
            deferred.released_nodes.append(released_nodes);
            deferred.removed_values.append(removed_values);
        }
        // Rule R1's order, as a pass's end.
        for node in deferred.deactivated_nodes.drain(..) {
            if applier.contains(node) {
                applier.deactivate(node);
            }
        }
        for node in deferred.released_nodes.drain(..) {
            if applier.contains(node) {
                applier.release(node);
            }
        }
        applier.on_paused_end();
        applier.on_end_changes(with_runtime(|runtime| runtime.is_composing()));
    }
    crate::composition::free_retired_providers(id);
    // The segments in order: each paused scope's where it paused.
    let mut flat = Flat::default();
    flat.forgotten.append(&mut deferred.removed_values);
    flatten(&mut paused.segments, 0, &mut flat);
    // What was never remembered is abandoned last, as Compose's dispatcher.
    let (abandoned, forgotten): (Vec<SlotValue>, Vec<SlotValue>) =
        std::mem::take(&mut flat.forgotten).into_iter().partition(
            |value| matches!(value, SlotValue::Observer(observer) if observer.abandon_later()),
        );
    dispatch_forgotten(&forgotten);
    drop(forgotten);
    for observer in flat.observers {
        observer.dispatch_remembered();
    }
    for effect in flat.effects {
        effect.run();
    }
    dispatch_forgotten(&abandoned);
    drop(abandoned);
    drop(paused);
    drop(finishing);
    request_frames();
}

/// The observers and effects of `segment` and of the segments it holds, in order (a segment
/// holds a paused scope's, nested as deep as pauses were within resumed scopes).
fn flatten(segments: &mut [Vec<Entry>], segment: u32, flat: &mut Flat) {
    for entry in std::mem::take(&mut segments[segment as usize]) {
        match entry {
            Entry::Observer(observer) => flat.observers.push(observer),
            Entry::Effect(effect) => flat.effects.push(effect),
            Entry::Forgotten(value) => flat.forgotten.push(value),
            Entry::Segment(inner) => flatten(segments, inner, flat),
        }
    }
}

/// A paused composition's segments, in order.
#[derive(Default)]
struct Flat {
    observers: Vec<Rc<dyn ErasedObserver>>,
    effects: Vec<DeferredEffect>,
    forgotten: Vec<SlotValue>,
}

/// Plays `region`'s recorded node changes to `applier`; `offset` is where its nodes start in its
/// node parent.
fn replay(applier: &mut dyn ErasedApplier, region: &mut Region, offset: u32) {
    let top = region.node_parent;
    replay_ops(applier, &mut region.ops, top, offset);
}

/// Plays `ops`, those of a region whose nodes start at `offset` in `top`.
fn replay_ops(
    applier: &mut dyn ErasedApplier,
    ops: &mut Vec<RecordedOp>,
    top: NodeRef,
    offset: u32,
) {
    let at =
        |parent: NodeRef, index: u32| (index + if parent == top { offset } else { 0 }) as usize;
    for op in ops.drain(..) {
        match op {
            RecordedOp::Attach { parent, index, node } => {
                applier.attach(parent, at(parent, index), node)
            }
            RecordedOp::Move { parent, from, to, count } => {
                applier.move_nodes(parent, at(parent, from), at(parent, to), count as usize)
            }
            RecordedOp::Remove { parent, index, count } => {
                applier.remove(parent, at(parent, index), count as usize)
            }
            RecordedOp::Reuse { node } => applier.reuse(node),
            RecordedOp::Deactivate { node } => applier.deactivate(node),
        }
    }
}

/// Discards `id`'s paused composition: removes its regions (their groups and content), releases
/// their nodes, and abandons (or forgets) their values. With `invalidate`, the composable around
/// each region is invalidated, to compose it again.
pub(crate) fn cancel(id: CompositionId, state: &Rc<Cell<PausedState>>, invalidate: bool) {
    /// A region's nodes to remove from their parent, and the nodes to release.
    struct Removal {
        node_parent: NodeRef,
        offset: u32,
        attached: u32,
        release: Vec<NodeRef>,
    }
    state.set(PausedState::Cancelled);
    let taken = try_with_runtime(|runtime| {
        if runtime.compositions.get(id).is_some_and(|data| data.composing) {
            misuse(ComposeError::PassInProgress)
        }
        let Runtime {
            compositions, scopes, removal_stack, finishing, frame_requests, lent, ..
        } = runtime;
        let data = compositions.get_mut(id)?;
        let mut paused = data.paused.take()?;
        if data.poisoned {
            // Disposal releases what the slot table holds; the nodes made apart go with the
            // applier.
            let applier = crate::context::resolve_applier(lent, &data.applier);
            return Some((paused, Vec::new(), Flat::default(), applier, false));
        }
        // Ended by the `Finishing` guard below, whatever there is to remove.
        data.finishing = true;
        *finishing += 1;
        let mut removals = Vec::new();
        let mut flat = Flat::default();
        let mut enclosing = Vec::new();
        // Last first: removing a region's nodes leaves the offsets of those before it.
        for region in paused.regions.iter_mut().rev().filter(|region| !region.dead) {
            let table = &mut data.table;
            let root = region.root;
            let offset = node_offset(table, root);
            set_node_count(table, root, 0);
            // Its group goes too: composed again, it's new (paused content again, in a paused
            // composition).
            let parent = table[root].parent;
            let next = table[root].next_sibling;
            let mut previous = NONE;
            let mut sibling = table[parent].first_child;
            while sibling != root {
                previous = sibling;
                sibling = table[sibling].next_sibling;
            }
            table.link_after(parent, previous, next);
            let mut removed = Deferred::default();
            remove_siblings(table, &mut removed, scopes, removal_stack, root, next);
            flat.forgotten.append(&mut region.deferred.removed_values);
            flat.forgotten.append(&mut removed.removed_values);
            let mut release = std::mem::take(&mut region.old_nodes);
            release.append(&mut region.created);
            removals.push(Removal {
                node_parent: region.node_parent,
                offset,
                attached: region.attached,
                release,
            });
            // The composable around it composes it again.
            let mut group = parent;
            while group != NONE && table[group].kind != GroupKind::Restart {
                group = table[group].parent;
            }
            if invalidate && group != NONE {
                enclosing.push(table[group].scope);
            }
        }
        // The values the regions forgot (reused content's), and their observers, which get
        // `on_abandoned` (never remembered).
        flatten(&mut paused.segments, 0, &mut flat);
        let applier = crate::context::resolve_applier(lent, &data.applier);
        for scope in enclosing {
            invalidate_scope_parts(scopes, compositions, frame_requests, None, scope, None);
        }
        Some((paused, removals, flat, applier, true))
    })
    .flatten();
    let Some((paused, removals, flat, applier, finishing)) = taken else { return };
    // It ends the finishing whatever there is to remove: nothing, with no live region (a paused
    // composition cancelled before its first resume, or complete without paused content).
    let finishing = finishing.then(|| Finishing { id, freeing: Vec::new() });
    if !removals.is_empty() {
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        applier.on_begin_changes();
        for removal in &removals {
            // Below a node of a composition around it, what it holds may have gone with that node.
            if removal.attached > 0 && applier.contains(removal.node_parent) {
                applier.remove(
                    removal.node_parent,
                    removal.offset as usize,
                    removal.attached as usize,
                );
            }
            for &node in &removal.release {
                if applier.contains(node) {
                    applier.release(node);
                }
            }
        }
        applier.on_paused_end();
        applier.on_end_changes(with_runtime(|runtime| runtime.is_composing()));
    } else if let Ok(mut applier) = applier.try_borrow_mut() {
        applier.on_paused_end();
    }
    crate::composition::free_retired_providers(id);
    // Observers remembered in the regions were never told: abandoned; the values of reused
    // content that left are forgotten.
    let Flat { forgotten, effects, observers } = flat;
    dispatch_forgotten(&forgotten);
    drop(forgotten);
    drop(observers);
    drop(effects);
    drop(paused);
    drop(finishing);
    request_frames();
}

/// Cancels `id`'s paused composition, if any, before it's disposed.
pub(crate) fn cancel_for_disposal(id: CompositionId) {
    let state = try_with_runtime(|runtime| {
        let data = runtime.compositions.get(id)?;
        if data.composing {
            return None;
        }
        data.paused.as_ref().map(|paused| paused.state.clone())
    })
    .flatten();
    if let Some(state) = state {
        cancel(id, &state, false);
    }
}

/// Composes `content` as pausable content, a part a paused composition may pause in
/// ([`Composition::recompose_pausable`]): inserted or reused in its first pass, `content` is
/// composed by its resumes and applied by its `apply`; anywhere else it's ordinary content.
/// What Compose's lazy layouts get from composing each item in a subcomposition of its own,
/// which compose-rs's compose in their list's composition (D590).
#[allow(non_snake_case)]
pub fn PausableContent(content: impl FnOnce()) {
    let composer = current();
    composer.start_pausable_group(PAUSABLE_KEY, None);
    content();
    composer.end_group();
}

/// As [`PausableContent`], for `target`: paused content of a paused composition made for it
/// ([`Composition::recompose_pausable_for`]) or for any; of one made for another target,
/// ordinary content. A lazy item's content, its target naming its list and its key.
#[allow(non_snake_case)]
pub fn PausableContentFor(target: u64, content: impl FnOnce()) {
    let composer = current();
    composer.start_pausable_group(PAUSABLE_KEY, Some(target));
    content();
    composer.end_group();
}
