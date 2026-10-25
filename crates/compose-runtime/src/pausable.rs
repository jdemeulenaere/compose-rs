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
//! reused in a paused composition's first pass ([`Composition::recompose_pausable`]); the rest
//! of that pass is an ordinary pass, applied at its end. Inside a region:
//!
//! - restartable composables pause where Compose's do, when inserting or reusing;
//! - node changes are recorded and applied at [`apply`](PausedComposition::apply): a new node
//!   is made in no parent (`Applier::create`) and attached then, moves and removals of its
//!   nodes wait too; the pass updates the nodes it makes meanwhile (Compose records the
//!   updates too, compose-rs's nodes are updated through the applier at once), and a reused
//!   node is reset at once (`Applier::reuse`), as the pass then updates it;
//! - remember observers, side effects, deactivations, releases and forgotten values wait for
//!   `apply`, observers and effects in the order they'd have had without pausing;
//! - the region's nodes count as those it had before (none if new) for the composition around
//!   it until `apply`, so other passes place their nodes as if it weren't there;
//! - the resumes walk the regions only (their paused scopes, and those invalidated since), and
//!   a state write invalidating a scope inside a region marks it for the next resume, not for
//!   the composition's next pass (as Compose's paused composition takes its invalidations);
//! - a pass of the composition that would run a body around a region completes the paused
//!   composition and applies it first (Compose's `subcompose` of a precomposed item completes
//!   its paused composition), so passes never run over content that isn't applied.

use std::cell::Cell;
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
const PAUSABLE_KEY: u64 = key_of("compose_runtime::PausableContent");

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
    Attach { parent: NodeRef, index: u32, node: NodeRef },
    Move { parent: NodeRef, from: u32, to: u32, count: u32 },
    Remove { parent: NodeRef, index: u32, count: u32 },
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
}

/// A composition's paused composition, in its data.
pub(crate) struct Paused {
    pub state: Rc<Cell<PausedState>>,
    /// Cancelled while its content couldn't be discarded (during a pass, or a layout): it is
    /// before the composition's next pass, and holds its regions until then.
    pub cancel_requested: bool,
    pub regions: Vec<Region>,
    /// Segment 0 holds the regions' observers and effects in their first pass's order, and the
    /// places of the scopes paused in it; each paused scope has one of its own.
    pub segments: Vec<Vec<Entry>>,
}

impl Paused {
    fn new(state: Rc<Cell<PausedState>>) -> Paused {
        Paused { state, cancel_requested: false, regions: Vec::new(), segments: vec![Vec::new()] }
    }

    /// Its regions wait for `apply` (or a resume): their content isn't the composition's yet.
    /// From its first pass on, which makes them.
    pub fn holds_regions(&self) -> bool {
        !self.regions.is_empty()
            && (self.cancel_requested
                || !matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled))
    }

    /// The region whose root is `group`.
    pub fn region_rooted_at(&self, group: u32) -> Option<&Region> {
        self.regions.iter().find(|region| region.root == group)
    }

    /// The region holding `group` (an ancestor of it is a region's root).
    pub fn region_of(&self, table: &SlotTable, group: u32) -> Option<usize> {
        let mut current = table[group].parent;
        while current != NONE {
            if let Some(index) = self.regions.iter().position(|region| region.root == current) {
                return Some(index);
            }
            current = table[current].parent;
        }
        None
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
    pub(crate) fn begin_region(&mut self) {
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
        let table = &data.table;
        let (attached, old_nodes) = if frame.inserting {
            (0, Vec::new())
        } else {
            (table[frame.group].node_count, top_level_nodes(table, frame.group))
        };
        frame.region = paused.regions.len() as u32;
        frame.segment = 0;
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

    /// Opens a resume's walk of `region` (its paused and invalidated scopes).
    fn push_region_frame(&mut self, region: u32) {
        let composition = self.check_composing();
        let data = self.compositions.data_mut(composition);
        let paused = data.paused.as_mut().expect("a paused composition");
        let region_data = &mut paused.regions[region as usize];
        region_data.pending = false;
        let root = region_data.root;
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
            segment: 0,
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
    _not_send: PhantomData<*const ()>,
}

impl Composition {
    /// A paused composition of this composition's invalid scopes, as Compose's
    /// `PausableComposition.setPausableContent`: its first [`resume`](PausedComposition::resume)
    /// runs the pass `recompose` would, in which each [`PausableContent`] inserted or reused is
    /// paused content: its composables may pause when the callback says so, and nothing of it
    /// is applied before [`apply`](PausedComposition::apply). A pass of this composition before
    /// then that would run a body around paused content completes and applies it first.
    ///
    /// # Panics
    ///
    /// If another paused composition of this composition is in progress (not applied or
    /// cancelled), as Compose's.
    pub fn recompose_pausable(&self) -> PausedComposition {
        let state = Rc::new(Cell::new(PausedState::InitialPending));
        let id = self.id;
        run_requested_cancel(id);
        with_runtime(|runtime| {
            let data = runtime.compositions.get_mut(id).expect("a live composition");
            if data.paused.is_some() {
                misuse(ComposeError::PausedCompositionState)
            }
            data.paused = Some(Box::new(Paused::new(state.clone())));
        });
        PausedComposition { composition: id, state, _not_send: PhantomData }
    }
}

impl PausedComposition {
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
    /// Once applied or cancelled, or during a pass.
    pub fn resume(&self, should_pause: impl ShouldPauseCallback + 'static) -> bool {
        self.resume_with(Some(Rc::new(should_pause)))
    }

    fn resume_with(&self, should_pause: Option<Rc<dyn ShouldPauseCallback>>) -> bool {
        match self.state.get() {
            PausedState::ApplyPending => return true,
            PausedState::Applied | PausedState::Cancelled => {
                misuse(ComposeError::PausedCompositionState)
            }
            PausedState::InitialPending => {
                crate::derived::flush_derived_states();
                match Pass::begin_as(self.composition, Mode::Walk, PassKind::Pausable, should_pause)
                {
                    Some(pass) => {
                        run_walk(0);
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
                        .filter(|(_, region)| region.pending)
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
            if !Rc::ptr_eq(&paused.state, &self.state) {
                return;
            }
            let pending = paused.regions.iter().any(|region| region.pending);
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
    /// subcomposed again, `forceRecompose`). Nothing if it was applied or cancelled. During a pass, or while the applier is in use (a layout), the
    /// discarding waits for the composition's next pass, or its next paused composition: it's
    /// cancelled at once all the same.
    pub fn cancel(&self) {
        if matches!(self.state.get(), PausedState::Applied | PausedState::Cancelled) {
            return;
        }
        let deferred = with_runtime(|runtime| {
            let busy = runtime.active.is_some();
            let Some(data) = runtime.compositions.get_mut(self.composition) else { return false };
            let Some(paused) = data.paused.as_mut() else { return false };
            if !Rc::ptr_eq(&paused.state, &self.state) {
                return false;
            }
            let busy = busy || data.finishing || data.applier.try_borrow_mut().is_err();
            if busy {
                paused.cancel_requested = true;
            }
            busy
        });
        if deferred {
            self.state.set(PausedState::Cancelled);
        } else {
            cancel(self.composition, &self.state, true);
        }
    }
}

/// Discards `id`'s paused composition if its cancelling waits (requested during a pass or a
/// layout): before the composition's next pass or paused composition.
pub(crate) fn run_requested_cancel(id: CompositionId) {
    let state = with_runtime(|runtime| {
        let paused = runtime.compositions.get(id)?.paused.as_ref()?;
        paused.cancel_requested.then(|| paused.state.clone())
    });
    if let Some(state) = state {
        cancel(id, &state, true);
    }
}

/// Whether `id`'s paused composition holds regions a pass would run a body around (a body
/// enclosing one, its scope invalid, or the content set again with `always`).
fn holds_enclosed_regions(runtime: &Runtime, id: CompositionId, always: bool) -> bool {
    let Some(data) = runtime.compositions.get(id) else { return false };
    let Some(paused) = data.paused.as_ref() else { return false };
    if !paused.holds_regions() {
        return false;
    }
    if always {
        return true;
    }
    let table = &data.table;
    paused.regions.iter().any(|region| {
        let mut group = table[region.root].parent;
        while group != NONE {
            let entry = &table[group];
            if entry.kind == GroupKind::Restart
                && runtime.scopes.get(entry.scope).is_some_and(|scope| scope.invalid)
            {
                return true;
            }
            group = entry.parent;
        }
        false
    })
}

/// Before a pass of `id`: completes and applies its paused composition if the pass would run a
/// body around its paused content (or with `always`, `set_content`), as Compose's subcomposition
/// of a precomposed item completes its paused composition first.
pub(crate) fn complete_if_enclosed(id: CompositionId, always: bool) {
    run_requested_cancel(id);
    let state = with_runtime(|runtime| {
        if !holds_enclosed_regions(runtime, id, always) {
            return None;
        }
        runtime.compositions.get(id)?.paused.as_ref().map(|paused| paused.state.clone())
    });
    let Some(state) = state else { return };
    let paused = PausedComposition { composition: id, state, _not_send: PhantomData };
    while !paused.resume_with(None) {}
    paused.apply();
}

/// Applies `id`'s complete paused composition.
fn apply(id: CompositionId, state: &Rc<Cell<PausedState>>) {
    let taken = with_runtime(|runtime| {
        if runtime.active.is_some() {
            misuse(ComposeError::PassInProgress)
        }
        let data = runtime.compositions.get_mut(id)?;
        let paused = data.paused.take()?;
        data.finishing = true;
        runtime.finishing += 1;
        Some((paused, data.applier.clone()))
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
        for region in &mut paused.regions {
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
            applier.deactivate(node);
        }
        for node in deferred.released_nodes.drain(..) {
            applier.release(node);
        }
        applier.on_end_changes();
    }
    // The segments in order: each paused scope's where it paused.
    let mut flat = Flat::default();
    flat.forgotten.append(&mut deferred.removed_values);
    flatten(&mut paused.segments, 0, &mut flat);
    dispatch_forgotten(&flat.forgotten);
    drop(std::mem::take(&mut flat.forgotten));
    for observer in flat.observers {
        observer.dispatch_remembered();
    }
    for effect in flat.effects {
        effect.run();
    }
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
    let at =
        |parent: NodeRef, index: u32| (index + if parent == top { offset } else { 0 }) as usize;
    for op in region.ops.drain(..) {
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
        if runtime.active.is_some_and(|active| active.composition == id) {
            misuse(ComposeError::PassInProgress)
        }
        let Runtime { compositions, scopes, removal_stack, finishing, frame_requests, .. } =
            runtime;
        let data = compositions.get_mut(id)?;
        let mut paused = data.paused.take()?;
        if data.poisoned {
            // Disposal releases what the slot table holds; the nodes made apart go with the
            // applier.
            return Some((paused, Vec::new(), Flat::default(), data.applier.clone()));
        }
        data.finishing = true;
        *finishing += 1;
        let mut removals = Vec::new();
        let mut flat = Flat::default();
        let mut enclosing = Vec::new();
        // Last first: removing a region's nodes leaves the offsets of those before it.
        for region in paused.regions.iter_mut().rev() {
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
        let applier = data.applier.clone();
        for scope in enclosing {
            invalidate_scope_parts(scopes, compositions, frame_requests, scope);
        }
        Some((paused, removals, flat, applier))
    })
    .flatten();
    let Some((paused, removals, flat, applier)) = taken else { return };
    if removals.is_empty() {
        drop((paused, flat));
        return;
    }
    let finishing = Finishing { id, freeing: Vec::new() };
    {
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        applier.on_begin_changes();
        for removal in &removals {
            if removal.attached > 0 {
                applier.remove(
                    removal.node_parent,
                    removal.offset as usize,
                    removal.attached as usize,
                );
            }
            for &node in &removal.release {
                applier.release(node);
            }
        }
        applier.on_end_changes();
    }
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
        if runtime.active.is_some_and(|active| active.composition == id) {
            return None;
        }
        runtime.compositions.get(id)?.paused.as_ref().map(|paused| paused.state.clone())
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
    composer.start_pausable_group(PAUSABLE_KEY);
    content();
    composer.end_group();
}
