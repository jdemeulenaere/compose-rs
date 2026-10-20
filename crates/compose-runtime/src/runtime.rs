//! The per-thread runtime: state cells, recompose scopes, compositions and the pass in progress.
//!
//! Everything lives in one thread-local `RefCell`. The rule that keeps it sound and re-entrant:
//! **no user code runs while the runtime is borrowed.** Composable bodies, `remember`
//! calculations, `ComposeArg::same`, `Clone` and `Drop` of user values, appliers and effects all
//! run between borrows. Values the runtime gives up are moved out first and dropped after the
//! borrow ends.

use std::cell::RefCell;
use std::rc::Rc;

use crate::applier::{ErasedApplier, NodeRef};
use crate::composer::{Frame, NodeOp};
use crate::error::{ComposeError, misuse};
use crate::remember::ErasedObserver;
use crate::scope::{ScopeId, Scopes};
use crate::slot_table::{GroupKind, NONE, SENTINEL, SlotTable, SlotValue};
use crate::state::Cells;

thread_local! {
    static RUNTIME: RefCell<Runtime> = const { RefCell::new(Runtime::new()) };
}

pub(crate) struct Runtime {
    pub cells: Cells,
    pub scopes: Scopes,
    pub compositions: Compositions,
    pub active: Option<ActivePass>,
    /// The frames of the pass in progress; kept between passes for its capacity.
    pub frames: Vec<Frame>,
    /// Scratch stack for removing subtrees; kept for its capacity.
    pub removal_stack: Vec<(u32, bool)>,
    /// Frame requesters to call once the borrow ends; kept for its capacity.
    pub frame_requests: Vec<Rc<dyn Fn()>>,
    /// A node move waiting to be coalesced with the next one (see `apply_node_op`).
    pub pending_move: Option<NodeOp>,
    /// Compositions taken out of the arena and still releasing their values.
    pub disposing: usize,
    /// Passes applying their end-of-pass work (releases, observers, drops, effects).
    pub finishing: usize,
    /// A group guard was dropped by an unwinding panic in this pass, leaving its group open.
    pub abandoned_groups: bool,
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
            active: None,
            frames: Vec::new(),
            removal_stack: Vec::new(),
            frame_requests: Vec::new(),
            pending_move: None,
            disposing: 0,
            finishing: 0,
            abandoned_groups: false,
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
    let Runtime { scopes, compositions, frame_requests, .. } = runtime;
    invalidate_scope_parts(scopes, compositions, frame_requests, id)
}

pub(crate) fn invalidate_scope_parts(
    scopes: &mut Scopes,
    compositions: &mut Compositions,
    frame_requests: &mut Vec<Rc<dyn Fn()>>,
    id: ScopeId,
) -> bool {
    let Some(scope) = scopes.get_mut(id) else {
        return false;
    };
    if scope.invalid {
        return false;
    }
    scope.invalid = true;
    let data = compositions.data_mut(scope.composition);
    let table = &mut data.table;
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

/// Work a pass defers to its end, in rule R1's order: release removed nodes, then forget and
/// drop removed values, then dispatch new remember observers, then run side effects.
#[derive(Default)]
pub(crate) struct Deferred {
    pub released_nodes: Vec<NodeRef>,
    pub removed_values: Vec<SlotValue>,
    pub remembered: Vec<Rc<dyn ErasedObserver>>,
    pub side_effects: Vec<DeferredEffect>,
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
}

impl Compositions {
    const fn new() -> Compositions {
        Compositions { entries: Vec::new(), free: Vec::new(), live: 0, closed: 0 }
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
            self.entries.push(CompositionEntry { generation: 1, data: Some(data) });
            CompositionId { index, generation: 1 }
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

    pub fn data(&self, index: u32) -> &CompositionData {
        self.entries[index as usize].data.as_ref().expect("composition of a live scope or pass")
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
    stack: &mut Vec<(u32, bool)>,
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

/// Removes one group and its subtree, in pre-order: a group's slots before its children's.
fn remove_subtree(
    table: &mut SlotTable,
    deferred: &mut Deferred,
    scopes: &mut Scopes,
    stack: &mut Vec<(u32, bool)>,
    root: u32,
) {
    stack.clear();
    stack.push((root, false));
    while let Some((index, under_node)) = stack.pop() {
        let group = &mut table[index];
        debug_assert!(group.kind != GroupKind::Free, "removing a free group");
        let is_node = group.kind == GroupKind::Node;
        // Only the topmost nodes are released: releasing a node releases its subtree.
        if is_node && !under_node && !group.node.is_none() {
            deferred.released_nodes.push(group.node);
        }
        deferred.removed_values.extend(group.slots.drain(..).map(|slot| slot.value));
        if let Some(kept) = group.kept.take() {
            deferred.removed_values.push(SlotValue::Plain(kept));
        }
        if !group.scope.is_none() {
            scopes.free(group.scope);
            group.scope = crate::scope::ScopeId::NONE;
        }
        let mut child = group.first_child;
        let start = stack.len();
        while child != NONE {
            stack.push((child, under_node || is_node));
            child = table[child].next_sibling;
        }
        stack[start..].reverse();
        table.free(index);
    }
}
