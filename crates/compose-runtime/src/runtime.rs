//! The per-thread runtime: state cells, recompose scopes, compositions and the pass in progress.
//!
//! Everything lives in one thread-local `RefCell`. The rule that keeps it sound and re-entrant:
//! **no user code runs while the runtime is borrowed.** Composable bodies, `remember`
//! calculations, `ComposeArg::same`, `Clone` and `Drop` of user values, appliers and effects all
//! run between borrows. Values the runtime gives up are moved out first and dropped after the
//! borrow ends.

use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::rc::Rc;

use crate::applier::{ErasedApplier, NodeRef};
use crate::composer::{Frame, NodeOp};
use crate::error::{ComposeError, misuse};
use crate::remember::ErasedObserver;
use crate::scope::{ScopeId, Scopes};
use crate::slot_table::{GroupKind, NONE, SENTINEL, SlotTable, SlotValue};
use crate::state::Cells;

thread_local! {
    // No destructor (`ManuallyDrop`): the runtime stays usable from every other thread-local's
    // destructor, whatever their order. At thread exit, the exit guard (composition.rs) releases
    // the compositions' contents while everything works; what remains (arena storage, cells
    // still owned by other thread-locals) is leaked rather than dropped where using state would
    // abort.
    static RUNTIME: RefCell<ManuallyDrop<Runtime>> =
        const { RefCell::new(ManuallyDrop::new(Runtime::new())) };
}

pub(crate) struct Runtime {
    pub cells: Cells,
    pub scopes: Scopes,
    pub compositions: Compositions,
    pub active: Option<ActivePass>,
    /// Moves on each time a provider's values change in a way that recomposes its content (a
    /// static local's, an entry added or dropped): what a `CompositionLocalAnchor` is valid for.
    pub providers_epoch: u32,
    /// The frames of the pass in progress; kept between passes for its capacity.
    pub frames: Vec<Frame>,
    /// Scratch stack for removing subtrees; kept for its capacity.
    pub removal_stack: Vec<RemovalStep>,
    /// Frame requesters to call once the borrow ends; kept for its capacity.
    pub frame_requests: Vec<Rc<dyn Fn()>>,
    /// The thread's snapshot world (D750, D751): the global snapshot's changes and observers.
    pub snapshots: crate::snapshots::Snapshots,
    /// Observers of reads outside composition, and the observation in progress.
    pub observers: crate::observe::Observers,
    pub observing: Option<crate::observe::Observing>,
    /// A node move waiting to be coalesced with the next one (see `apply_node_op`).
    pub pending_move: Option<NodeOp>,
    /// Compositions taken out of the arena and still releasing their values.
    pub disposing: usize,
    /// Passes applying their end-of-pass work (releases, observers, drops, effects).
    pub finishing: usize,
    /// The thread is exiting: once nothing is left, the runtime releases its storage.
    pub exiting: bool,
    /// Composable bodies run on this thread: frame statistics and tests.
    pub bodies_run: u64,
    /// State reads on this thread so far (tracked or not): what a caller checks to know whether
    /// code it ran read state (`platform::state_reads`).
    pub state_reads: u64,
    /// Slots written and side effects recorded on this thread: what Compose's change list
    /// holds besides node changes. For tests (`platform::recorded_changes`).
    pub recorded_changes: u64,
    /// The removal of the nodes of a branch group `start_group` just deleted (D715), for
    /// `start_branch_group_of` to apply first.
    pub branch_removal: crate::composer::NodeOp,
    /// Body runs per composable, while tracking is on (`track_recompositions`).
    pub recomposition_counts: Option<std::collections::HashMap<&'static str, u64>>,
    /// A group guard was dropped by an unwinding panic in this pass, leaving its group open.
    pub abandoned_groups: bool,
    /// The depth (`frames.len()`) of the reusable group whose content is being reused, or 0:
    /// as Compose's `reusing`, slots read as empty, bodies run and reusable nodes are kept
    /// until that group ends.
    pub reusing: usize,
    /// The depth (`frames.len()`) of the provider group whose provided values changed in a way
    /// its content's composables can't see through state (a static local's value, a computed
    /// one, the set of locals), or 0: as Compose's `providersInvalid`, every body under it runs
    /// until that group ends.
    pub providers_invalid: usize,
    /// The paused composition's callback, during its passes (`PausedComposition::resume`): asked
    /// at each pause point.
    pub pause: Option<Rc<dyn crate::pausable::ShouldPauseCallback>>,
    /// The pass is a paused composition's: pausable content inserted or reused in it is paused
    /// content (a region).
    pub pausable_pass: bool,
    /// The scope a resume restarts next was paused: its group doesn't pause at its own start
    /// (Compose's `RecomposeScopeImpl.resuming`); its segment, and whether it reuses its content.
    pub resume_next: Option<(u32, bool)>,
}

#[derive(Clone, Copy)]
pub(crate) struct ActivePass {
    pub composition: CompositionId,
    /// The pass is composing (not applying its end-of-pass changes): the composer may be used,
    /// and state writes are a misuse.
    pub composing: bool,
}

impl Runtime {
    const fn new() -> Runtime {
        Runtime {
            cells: Cells::new(),
            scopes: Scopes::new(),
            compositions: Compositions::new(),
            providers_epoch: 0,
            active: None,
            frames: Vec::new(),
            removal_stack: Vec::new(),
            frame_requests: Vec::new(),
            snapshots: crate::snapshots::Snapshots::new(),
            observers: crate::observe::Observers::new(),
            observing: None,
            pending_move: None,
            disposing: 0,
            finishing: 0,
            exiting: false,
            bodies_run: 0,
            state_reads: 0,
            recorded_changes: 0,
            branch_removal: crate::composer::NodeOp::None,
            recomposition_counts: None,
            abandoned_groups: false,
            reusing: 0,
            providers_invalid: 0,
            pause: None,
            pausable_pass: false,
            resume_next: None,
        }
    }

    /// The scope state reads register: the innermost restart group of the pass in progress.
    pub fn reading_scope(&self) -> ScopeId {
        match self.active {
            Some(ActivePass { composing: true, .. }) => {
                self.frames.last().map_or(ScopeId::NONE, |frame| frame.scope)
            }
            _ => ScopeId::NONE,
        }
    }

    /// Whether a pass is composing or finishing, or a composition is being disposed: work whose
    /// frame requests and retired cells wait until it ends.
    pub fn is_busy(&self) -> bool {
        self.active.is_some() || self.finishing > 0 || self.disposing > 0
    }

    pub fn is_composing(&self) -> bool {
        matches!(self.active, Some(ActivePass { composing: true, .. }))
    }

    /// The applier of the composition whose pass is running.
    pub fn active_applier(&self) -> Rc<RefCell<dyn ErasedApplier>> {
        let Some(active) = self.active else { misuse(ComposeError::NotComposing) };
        self.compositions.data(active.composition.index).applier.clone()
    }
}

/// Runs `f` with the thread's runtime borrowed. `f` must not run user code.
#[inline]
pub(crate) fn with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RUNTIME.with(|runtime| match runtime.try_borrow_mut() {
        Ok(mut runtime) => f(&mut runtime),
        Err(_) => reentered(),
    })
}

/// Like [`with_runtime`], for drop paths: `None` once the thread's runtime is gone, or while
/// unwinding through a borrow.
pub(crate) fn try_with_runtime<R>(f: impl FnOnce(&mut Runtime) -> R) -> Option<R> {
    RUNTIME
        .try_with(|runtime| match runtime.try_borrow_mut() {
            Ok(mut runtime) => Some(f(&mut runtime)),
            Err(_) if std::thread::panicking() => None,
            Err(_) => reentered(),
        })
        .ok()
        .flatten()
}

#[cold]
fn reentered() -> ! {
    panic!(
        "compose-runtime: the runtime was re-entered while busy; user code ran inside the runtime \
         (a bug in compose-runtime)"
    )
}

/// A generation above `base` and every one in `generations`, odd (as live entries' are).
pub(crate) fn next_base(base: u32, generations: impl Iterator<Item = u32>) -> u32 {
    let highest = generations.fold(base, u32::max);
    (highest.wrapping_add(2)) | 1
}

/// At thread exit, once no composition, state cell or scope is left and nothing is running,
/// replaces the runtime by an empty one, so a thread that dropped everything leaks nothing. The
/// new arenas continue the old generations, so a stale handle still reads as freed.
pub(crate) fn release_if_empty() {
    let old = try_with_runtime(|runtime| {
        let empty = runtime.exiting
            && !runtime.is_busy()
            && runtime.compositions.live() == 0
            && runtime.cells.live_count() == 0
            && runtime.scopes.live_count() == 0
            && runtime.observers.is_empty()
            && runtime.snapshots.is_empty();
        if !empty {
            return None;
        }
        let mut fresh = Runtime::new();
        fresh.exiting = true;
        fresh.cells.set_base(runtime.cells.next_base());
        fresh.scopes.set_base(runtime.scopes.next_base());
        fresh.compositions.base = runtime.compositions.next_base();
        fresh.snapshots.continue_from(&runtime.snapshots);
        Some(std::mem::replace(runtime, fresh))
    })
    .flatten();
    // Queued frame requests (user closures) drop here, outside the borrow.
    drop(old);
}

/// Calls the frame requesters queued by invalidations, outside the borrow. While a pass or a
/// disposal is in progress they wait: a requester that recomposes at once must not start a pass
/// in the middle of one, and Compose runs all of a pass's effects before the next pass. The pass
/// or disposal calls this again when it ends.
///
/// Requests are taken one at a time, so a requester that panics loses nothing for the others;
/// requests that couldn't be delivered (a panic ended the work that held them) are delivered the
/// next time a state write or invalidation asks for frames.
pub(crate) fn request_frames() {
    while let Some(request) = with_runtime(|runtime| {
        if runtime.is_busy() || runtime.frame_requests.is_empty() {
            None
        } else {
            Some(runtime.frame_requests.remove(0))
        }
    }) {
        request();
    }
}

/// Whether frame requests are waiting: after an invalidation, `request_frames` is due.
pub(crate) fn frames_requested(runtime: &Runtime) -> bool {
    !runtime.frame_requests.is_empty()
}

/// Marks a scope invalid and its ancestors dirty. Returns whether a frame request was queued.
pub(crate) fn invalidate_scope(runtime: &mut Runtime, id: ScopeId) -> bool {
    let Runtime { scopes, compositions, frame_requests, active, frames, .. } = runtime;
    let pass = PassFrames::of(*active, frames);
    invalidate_scope_parts(scopes, compositions, frame_requests, pass, id, None)
}

/// The frames of the pass composing now, for an invalidation made during it to tell whether the
/// pass will still reach the scope.
pub(crate) struct PassFrames<'a> {
    pub composition: u32,
    pub frames: &'a mut [Frame],
}

impl PassFrames<'_> {
    /// The composing pass's frames, if one is composing.
    pub fn of(active: Option<ActivePass>, frames: &mut [Frame]) -> Option<PassFrames<'_>> {
        match active {
            Some(ActivePass { composition, composing: true }) => {
                Some(PassFrames { composition: composition.index, frames })
            }
            _ => None,
        }
    }
}

/// Where the pass composing now will reach `group`: the depth of the innermost open frame above
/// it, and that frame's child holding it, if that child is still ahead of the frame's cursor
/// (an old child not reached yet). `None` if the pass has gone past it, won't go into it
/// (skipped, or new content), or is inside it.
fn ahead_of_the_pass(table: &SlotTable, frames: &[Frame], group: u32) -> Option<(usize, u32)> {
    // The frames are the path from the sentinel to the group composing now: the innermost open
    // ancestor of `group` is the deepest match walking up from it.
    let mut depth = 0;
    let mut ancestor = table[group].parent;
    while ancestor != NONE {
        depth += 1;
        ancestor = table[ancestor].parent;
    }
    let (mut child, mut current) = (NONE, group);
    loop {
        if depth < frames.len() && frames[depth].group == current {
            break;
        }
        if depth == 0 {
            return None;
        }
        child = current;
        current = table[current].parent;
        depth -= 1;
    }
    if child == NONE {
        // The group is open: its body runs now (or ran).
        return None;
    }
    let mut pending = frames[depth].cursor;
    while pending != NONE {
        if pending == child {
            return Some((depth, child));
        }
        pending = table[pending].next_sibling;
    }
    None
}

/// Where a scope's group is.
struct ScopeLocation {
    composition: u32,
    group: u32,
}

/// Marks a scope invalid, outright or (with `condition`) only if that derived state's value
/// changed, and its ancestors dirty. Returns whether a frame request was queued.
///
/// During a pass of the scope's composition (a state written during composition, D703), a
/// scope the pass will still reach is invalidated for it (Compose's imminent invalidation): the
/// groups between it and the open frame above are marked dirty, not the open ones, and no frame
/// is asked for. One the pass has gone past, or is running, recomposes in the next pass, which
/// its dirty ancestors and a frame request ask for, as Compose's deferred invalidation.
pub(crate) fn invalidate_scope_parts(
    scopes: &mut Scopes,
    compositions: &mut Compositions,
    frame_requests: &mut Vec<Rc<dyn Fn()>>,
    pass: Option<PassFrames<'_>>,
    id: ScopeId,
    condition: Option<crate::state::StateId>,
) -> bool {
    let Some(scope) = scopes.get_mut(id) else {
        return false;
    };
    if scope.invalid {
        let conditional = scope.conditional;
        match condition {
            // Conditional: one more condition. Invalid outright already: nothing more.
            Some(cell) if conditional => scopes.add_condition(id, cell),
            Some(_) => {}
            // Conditional until now: invalid outright.
            None => scopes.clear_conditions(id),
        }
        return false;
    }
    scope.invalid = true;
    let (composition, group) = (scope.composition, scope.group);
    if let Some(cell) = condition {
        scopes.add_condition(id, cell);
    }
    let scope = ScopeLocation { composition, group };
    let data = compositions.data_mut(scope.composition);
    let table = &mut data.table;
    // In paused content not applied yet: its paused composition's next resume recomposes it, as
    // Compose's paused composition takes the invalidations of its scopes (`markIncomplete`); no
    // pass of the composition does.
    if let Some(paused) = data.paused.as_mut()
        && paused.holds_regions()
        && let Some(region) = paused.region_of(table, scope.group)
    {
        let region = &mut paused.regions[region];
        region.pending = true;
        let mut group = table[scope.group].parent;
        while group != region.root && !table[group].dirty {
            table[group].dirty = true;
            group = table[group].parent;
        }
        if paused.state.get() == crate::pausable::PausedState::ApplyPending {
            paused.state.set(crate::pausable::PausedState::RecomposePending);
        }
        return false;
    }
    if let Some(pass) = pass
        && pass.composition == scope.composition
        && let Some((depth, child)) = ahead_of_the_pass(table, pass.frames, scope.group)
    {
        let mut group = scope.group;
        while group != child {
            group = table[group].parent;
            table[group].dirty = true;
        }
        // A body skipping later walks into it.
        pass.frames[depth].dirty = true;
        return false;
    }
    let mut group = table[scope.group].parent;
    while group != NONE && !table[group].dirty {
        table[group].dirty = true;
        group = table[group].parent;
    }
    if !data.pending {
        data.pending = true;
        if let Some(requester) = &data.frame_requester {
            frame_requests.push(requester.clone());
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct CompositionId {
    pub index: u32,
    pub generation: u32,
}

/// Work a pass defers to its end, in rule R1's order: deactivate the nodes of deactivated
/// content and release removed nodes, then forget and drop removed values, then dispatch new
/// remember observers, then run side effects.
#[derive(Default)]
pub(crate) struct Deferred {
    pub deactivated_nodes: Vec<NodeRef>,
    pub released_nodes: Vec<NodeRef>,
    pub removed_values: Vec<SlotValue>,
    pub remembered: Vec<Rc<dyn ErasedObserver>>,
    pub side_effects: Vec<DeferredEffect>,
    /// `released_nodes` may name a node twice (paused content removed with what's around it,
    /// its nodes released by both): released once.
    pub repeated_releases: bool,
}

/// A side effect to run at the end of a pass.
pub(crate) enum DeferredEffect {
    /// Recorded with `Composer::record_side_effect`.
    Boxed(Box<dyn FnOnce()>),
    /// Recorded by `SideEffect`, whose closure waits in a remembered slot: no allocation.
    Slot(Rc<dyn RunEffect>),
}

impl DeferredEffect {
    pub fn run(self) {
        match self {
            DeferredEffect::Boxed(effect) => effect(),
            DeferredEffect::Slot(slot) => slot.run(),
        }
    }
}

pub(crate) trait RunEffect {
    fn run(&self);
}

impl Deferred {
    /// Puts back emptied vectors, so their capacity serves the next pass.
    pub fn restore(&mut self, mut emptied: Deferred) {
        fn put_back<T>(target: &mut Vec<T>, mut source: Vec<T>) {
            if target.is_empty() && target.capacity() < source.capacity() {
                source.clear();
                *target = source;
            }
        }
        put_back(&mut self.deactivated_nodes, std::mem::take(&mut emptied.deactivated_nodes));
        put_back(&mut self.released_nodes, std::mem::take(&mut emptied.released_nodes));
        put_back(&mut self.removed_values, std::mem::take(&mut emptied.removed_values));
        put_back(&mut self.remembered, std::mem::take(&mut emptied.remembered));
        put_back(&mut self.side_effects, std::mem::take(&mut emptied.side_effects));
    }
}

pub(crate) struct CompositionData {
    pub table: SlotTable,
    pub applier: Rc<RefCell<dyn ErasedApplier>>,
    pub root_node: NodeRef,
    /// A frame was requested and not yet served.
    pub pending: bool,
    pub poisoned: bool,
    /// Its last pass is applying its end-of-pass work.
    pub finishing: bool,
    /// Emptied at thread exit while its handle still owns it: passes compose nothing.
    pub closed: bool,
    pub frame_requester: Option<Rc<dyn Fn()>>,
    pub deferred: Deferred,
    /// The paused composition in progress, if any (Compose's `pendingPausedComposition`).
    pub paused: Option<Box<crate::pausable::Paused>>,
}

impl CompositionData {
    pub fn new(applier: Rc<RefCell<dyn ErasedApplier>>, root_node: NodeRef) -> CompositionData {
        CompositionData {
            table: SlotTable::new(),
            applier,
            root_node,
            pending: false,
            poisoned: false,
            finishing: false,
            closed: false,
            frame_requester: None,
            deferred: Deferred::default(),
            paused: None,
        }
    }

    /// What a composition keeps once closed: its applier and root, with an empty slot table.
    pub fn closed(data: &CompositionData) -> CompositionData {
        let mut closed = CompositionData::new(data.applier.clone(), data.root_node);
        closed.closed = true;
        closed
    }

    pub fn has_invalidations(&self) -> bool {
        self.table[SENTINEL].dirty
    }
}

struct CompositionEntry {
    generation: u32,
    data: Option<CompositionData>,
}

pub(crate) struct Compositions {
    entries: Vec<CompositionEntry>,
    free: Vec<u32>,
    live: usize,
    /// Live compositions closed at thread exit.
    pub closed: usize,
    /// The generation of new entries (odd): after a reset, above every earlier one.
    base: u32,
}

impl Compositions {
    const fn new() -> Compositions {
        Compositions { entries: Vec::new(), free: Vec::new(), live: 0, closed: 0, base: 1 }
    }

    pub fn alloc(&mut self, data: CompositionData) -> CompositionId {
        self.live += 1;
        if let Some(index) = self.free.pop() {
            let entry = &mut self.entries[index as usize];
            entry.generation = entry.generation.wrapping_add(1);
            entry.data = Some(data);
            CompositionId { index, generation: entry.generation }
        } else {
            let index = u32::try_from(self.entries.len()).expect("too many compositions");
            self.entries.push(CompositionEntry { generation: self.base, data: Some(data) });
            CompositionId { index, generation: self.base }
        }
    }

    /// Takes a composition's data out of the arena.
    pub fn remove(&mut self, id: CompositionId) -> Option<CompositionData> {
        let entry = self.entries.get_mut(id.index as usize)?;
        if entry.generation != id.generation {
            return None;
        }
        let data = entry.data.take()?;
        entry.generation = entry.generation.wrapping_add(1);
        self.free.push(id.index);
        self.live -= 1;
        if data.closed {
            self.closed -= 1;
        }
        Some(data)
    }

    pub fn get_mut(&mut self, id: CompositionId) -> Option<&mut CompositionData> {
        let entry = self.entries.get_mut(id.index as usize)?;
        if entry.generation != id.generation {
            return None;
        }
        entry.data.as_mut()
    }

    pub fn get(&self, id: CompositionId) -> Option<&CompositionData> {
        let entry = self.entries.get(id.index as usize)?;
        if entry.generation != id.generation {
            return None;
        }
        entry.data.as_ref()
    }

    /// The data of a live composition, by index: for scopes and passes, which hold an index
    /// only while their composition is alive.
    pub fn data_mut(&mut self, index: u32) -> &mut CompositionData {
        self.entries[index as usize].data.as_mut().expect("composition of a live scope or pass")
    }

    /// The composition `index`, if it's live.
    pub fn try_data(&self, index: u32) -> Option<&CompositionData> {
        self.entries.get(index as usize)?.data.as_ref()
    }

    pub fn data(&self, index: u32) -> &CompositionData {
        self.entries[index as usize].data.as_ref().expect("composition of a live scope or pass")
    }

    pub fn live(&self) -> usize {
        self.live
    }

    /// A generation above every one this arena gave out, for the arena that replaces it.
    pub fn next_base(&self) -> u32 {
        next_base(self.base, self.entries.iter().map(|entry| entry.generation))
    }

    /// Live compositions not closed: those that may still run a pass.
    pub fn open(&self) -> usize {
        self.live - self.closed
    }

    pub fn live_ids(&self) -> Vec<CompositionId> {
        (self.entries.iter().enumerate())
            .filter(|(_, entry)| entry.data.is_some())
            .map(|(index, entry)| CompositionId {
                index: index as u32,
                generation: entry.generation,
            })
            .collect()
    }
}

/// Removes the siblings from `first` up to (excluding) `until` and their subtrees: their nodes
/// are queued for release, their slot values for forgetting, their scopes freed. Returns the
/// number of nodes they contributed to their parent node. The caller unlinks them.
pub(crate) fn remove_siblings(
    table: &mut SlotTable,
    deferred: &mut Deferred,
    scopes: &mut Scopes,
    stack: &mut Vec<RemovalStep>,
    first: u32,
    until: u32,
) -> u32 {
    let mut count = 0;
    let mut current = first;
    while current != until && current != NONE {
        let next = table[current].next_sibling;
        count += table[current].node_count;
        remove_subtree(table, deferred, scopes, stack, current);
        current = next;
    }
    count
}

/// Removes `group`'s children from `first` on and its slots from `keep` on, interleaved in table
/// order (by where each child sits among the slots), as a group ending removes what its body
/// didn't reach. Returns the number of nodes the children contributed. The caller unlinks them.
pub(crate) fn remove_children_and_slots(
    table: &mut SlotTable,
    deferred: &mut Deferred,
    scopes: &mut Scopes,
    stack: &mut Vec<RemovalStep>,
    group: u32,
    first: u32,
    keep: usize,
) -> u32 {
    if table[group].slots.len() <= keep {
        return remove_siblings(table, deferred, scopes, stack, first, NONE);
    }
    let trailing: Vec<SlotValue> =
        table[group].slots.drain(keep..).map(|slot| slot.value).collect();
    let mut trailing = trailing.into_iter();
    let mut position = keep;
    let mut count = 0;
    let mut current = first;
    while current != NONE {
        let child_position = usize::from(table[current].slot_position);
        while position < child_position {
            let Some(value) = trailing.next() else { break };
            deferred.removed_values.push(value);
            position += 1;
        }
        let next = table[current].next_sibling;
        count += table[current].node_count;
        remove_subtree(table, deferred, scopes, stack, current);
        current = next;
    }
    deferred.removed_values.extend(trailing);
    count
}

/// A step of a removal walk: a group to enter, or to go on with at its next child.
#[derive(Clone, Copy)]
pub(crate) struct RemovalStep {
    pub group: u32,
    /// The next child to remove (`NONE` once all were).
    next_child: u32,
    /// How many of its slots were taken.
    taken: u32,
    under_node: bool,
    entered: bool,
}

impl RemovalStep {
    pub fn enter(group: u32, under_node: bool) -> RemovalStep {
        RemovalStep { group, next_child: NONE, taken: 0, under_node, entered: false }
    }
}

/// Removes one group and its subtree, in table order: each group's slots interleaved with its
/// children's subtrees by where each child sits among them (`Group::slot_position`), as
/// Compose's slot table holds them, so remember observers are forgotten in the reverse of that
/// order (D714).
fn remove_subtree(
    table: &mut SlotTable,
    deferred: &mut Deferred,
    scopes: &mut Scopes,
    stack: &mut Vec<RemovalStep>,
    root: u32,
) {
    stack.clear();
    stack.push(RemovalStep::enter(root, false));
    while let Some(mut step) = stack.pop() {
        let index = step.group;
        if !step.entered {
            let group = &mut table[index];
            debug_assert!(group.kind != GroupKind::Free, "removing a free group");
            // Only the topmost nodes are released: releasing a node releases its subtree.
            if group.kind == GroupKind::Node && !step.under_node && !group.node.is_none() {
                deferred.released_nodes.push(group.node);
            }
            if !group.scope.is_none() {
                scopes.free(group.scope);
                group.scope = crate::scope::ScopeId::NONE;
            }
            step.entered = true;
            step.next_child = group.first_child;
        }
        // Its slots before the next child.
        let child = step.next_child;
        let until =
            if child == NONE { usize::MAX } else { usize::from(table[child].slot_position) };
        let group = &mut table[index];
        let until = until.min(group.slots.len());
        let from = step.taken as usize;
        if until > from {
            deferred.removed_values.extend(
                group.slots[from..until]
                    .iter_mut()
                    .map(|slot| std::mem::replace(&mut slot.value, SlotValue::Empty)),
            );
            step.taken = until as u32;
        }
        if child == NONE {
            let group = &mut table[index];
            group.slots.clear();
            if let Some(kept) = group.kept.take() {
                deferred.removed_values.push(SlotValue::Plain(kept));
            }
            table.free(index);
            continue;
        }
        let under_node = step.under_node || table[index].kind == GroupKind::Node;
        step.next_child = table[child].next_sibling;
        stack.push(step);
        stack.push(RemovalStep::enter(child, under_node));
    }
}
