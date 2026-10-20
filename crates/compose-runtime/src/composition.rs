//! `Composition`: owns a slot table and an applier, and runs passes.

use std::any::Any;
use std::cell::RefCell;
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::applier::{Applier, ErasedApplier};
use crate::composer::{Frame, Mode, NodeOp, apply_node_op, current, flush_node_moves, run_walk};
use crate::error::{ComposeError, misuse};
use crate::hash::key_of;
use crate::remember::dispatch_forgotten;
use crate::runtime::{
    ActivePass, CompositionData, CompositionId, Deferred, Runtime, remove_siblings, request_frames,
    try_with_runtime, with_runtime,
};
use crate::scope::ScopeId;
use crate::slot_table::{GroupKind, NONE, SENTINEL};
use crate::state::CellValue;

/// The key of the restart group that holds a composition's content.
const ROOT_KEY: u64 = key_of("compose_runtime::Composition::content");

/// A composition: the slot table of one tree of composables, and the applier its nodes go to.
///
/// Owned: dropping it disposes everything it holds, in rule R1's order (nodes and their
/// handlers, then remembered values and state).
pub struct Composition {
    id: CompositionId,
    _not_send: PhantomData<*const ()>,
}

impl Composition {
    /// A composition emitting nodes into `applier`.
    pub fn new<A: Applier>(applier: A) -> Composition {
        let root_node = applier.root();
        let applier: Rc<RefCell<dyn ErasedApplier>> = Rc::new(RefCell::new(applier));
        let id = with_runtime(|runtime| {
            runtime.compositions.alloc(CompositionData::new(applier, root_node))
        });
        // Registered after the runtime, so (thread-locals being destroyed in reverse order) it
        // disposes the compositions left at thread exit while the runtime still works.
        let _ = EXIT_GUARD.try_with(|_| {});
        Composition { id, _not_send: PhantomData }
    }

    /// Sets the content and composes it at once. Content set before is recomposed against the
    /// new one: groups with the same keys keep their state.
    pub fn set_content(&self, content: impl Fn() + 'static) {
        let content: Rc<dyn Fn()> = Rc::new(content);
        // A composition closed at thread exit composes nothing.
        let Some(pass) = Pass::begin(self.id, Mode::Normal) else { return };
        compose_root(content);
        pass.finish();
    }

    /// Runs a pass if any scope is invalid. Returns whether a pass ran.
    pub fn recompose(&self) -> bool {
        let Some(pass) = Pass::begin(self.id, Mode::Walk) else {
            return false;
        };
        run_walk(0);
        pass.finish();
        true
    }

    /// Whether some scope is invalid, so [`recompose`](Composition::recompose) would run a pass.
    pub fn has_invalidations(&self) -> bool {
        with_runtime(|runtime| self.data(runtime).has_invalidations())
    }

    /// Sets the function called when a scope of this composition becomes invalid while none
    /// was: the host's way to schedule a frame.
    pub fn set_frame_requester(&self, requester: impl Fn() + 'static) {
        let requester: Rc<dyn Fn()> = Rc::new(requester);
        let old = with_runtime(|runtime| {
            let data = self.data(runtime);
            let old = data.frame_requester.replace(requester.clone());
            // Invalidated before a requester was set: it gets the frame now.
            if data.has_invalidations() {
                data.pending = true;
                runtime.frame_requests.push(requester);
            }
            old
        });
        drop(old);
        request_frames();
    }

    /// Runs `f` with the applier, as type `A`.
    pub fn with_applier<A: Applier, R>(&self, f: impl FnOnce(&mut A) -> R) -> R {
        if with_runtime(|runtime| {
            runtime.active.is_some_and(|active| active.composition == self.id)
        }) {
            flush_node_moves();
        }
        let applier = with_runtime(|runtime| self.data(runtime).applier.clone());
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        let applier = applier
            .as_any_mut()
            .downcast_mut::<A>()
            .unwrap_or_else(|| misuse(ComposeError::WrongApplierType));
        f(applier)
    }

    /// A text dump of the groups and slots, for tests.
    pub fn dump(&self) -> String {
        with_runtime(|runtime| {
            let data = runtime.compositions.get(self.id).expect("a live composition");
            let table = &data.table;
            let mut out = String::new();
            let mut stack = vec![(table[SENTINEL].first_child, 0usize)];
            while let Some((group, depth)) = stack.pop() {
                if group == NONE {
                    continue;
                }
                let entry = &table[group];
                stack.push((entry.next_sibling, depth));
                let indent = "  ".repeat(depth);
                let _ = write!(out, "{indent}{:?} {:#018x}", entry.kind, entry.key);
                if entry.kind == GroupKind::Movable {
                    let _ = write!(out, " data={:#018x}", entry.data_key);
                }
                if entry.kind == GroupKind::Node {
                    let _ = write!(out, " node={:#x}", entry.node.bits());
                }
                if let Some(scope) = runtime.scopes.get(entry.scope)
                    && scope.invalid
                {
                    let _ = write!(out, " invalid");
                }
                if entry.node_count > 0 && entry.kind != GroupKind::Node {
                    let _ = write!(out, " nodes={}", entry.node_count);
                }
                out.push('\n');
                for slot in &entry.slots {
                    let _ = writeln!(out, "{indent}  - {}", slot.type_name);
                }
                stack.push((entry.first_child, depth + 1));
            }
            out
        })
    }

    /// Checks the slot table's invariants, panicking with a description of the first broken one.
    /// For tests.
    #[doc(hidden)]
    pub fn validate(&self) {
        let result = with_runtime(|runtime| validate(runtime, self.id));
        if let Err(error) = result {
            panic!("compose-runtime: invalid slot table: {error}\n{}", self.dump());
        }
    }

    /// The number of groups and live recompose scopes in this composition. For tests.
    #[doc(hidden)]
    pub fn group_count(&self) -> usize {
        with_runtime(|runtime| self.data(runtime).table.live_count())
    }

    /// Whether a panic during a pass poisoned this composition.
    pub fn is_poisoned(&self) -> bool {
        with_runtime(|runtime| self.data(runtime).poisoned)
    }

    fn data<'a>(&self, runtime: &'a mut Runtime) -> &'a mut CompositionData {
        runtime.compositions.get_mut(self.id).expect("a live composition")
    }
}

impl Drop for Composition {
    fn drop(&mut self) {
        let Some(Some(disposal)) = try_with_runtime(|runtime| Disposal::take(runtime, self.id))
        else {
            return;
        };
        disposal.run();
    }
}

thread_local! {
    static EXIT_GUARD: ExitGuard = const { ExitGuard };
}

/// Disposes the compositions still alive when the thread exits (one kept in a thread-local, or
/// leaked), so their values' `Drop` can still use state.
struct ExitGuard;

impl Drop for ExitGuard {
    fn drop(&mut self) {
        let Some(ids) = try_with_runtime(|runtime| runtime.compositions.live_ids()) else { return };
        for id in ids {
            if let Some(Some(disposal)) = try_with_runtime(|runtime| Disposal::close(runtime, id)) {
                disposal.run();
            }
        }
    }
}

/// The content of a composition, as a hand-expanded restartable composable whose one argument
/// is the content lambda.
fn compose_root(content: Rc<dyn Fn()>) {
    let composer = current();
    composer.start_restart_group(ROOT_KEY);
    let changed =
        composer.update_kept(|kept: &Rc<dyn Fn()>| !Rc::ptr_eq(kept, &content), || content.clone());
    if !changed && composer.skipping() {
        composer.skip_to_group_end();
    } else {
        content();
    }
    composer.end_restart_group(|kept: &dyn Any| {
        let content = kept.downcast_ref::<Rc<dyn Fn()>>().expect("kept content").clone();
        compose_root(content)
    });
}

/// A pass in progress. If it is dropped without [`finish`](Pass::finish), a panic unwound
/// through it: the composition is poisoned.
struct Pass {
    id: CompositionId,
    finished: bool,
}

impl Pass {
    fn begin(id: CompositionId, mode: Mode) -> Option<Pass> {
        let applier = with_runtime(|runtime| {
            if runtime.active.is_some() {
                misuse(ComposeError::PassInProgress)
            }
            let Runtime { compositions, cells, frames, active, abandoned_groups, .. } = runtime;
            let data = compositions.get_mut(id).expect("a live composition");
            if data.closed {
                return None;
            }
            if data.poisoned {
                misuse(ComposeError::Poisoned)
            }
            if data.finishing {
                // Its last pass is still applying its end-of-pass work: the next pass waits
                // until it ends, which then requests a frame for it.
                if mode == Mode::Walk {
                    return None;
                }
                misuse(ComposeError::PassInProgress)
            }
            let sentinel = &mut data.table[SENTINEL];
            if mode == Mode::Walk && !sentinel.dirty {
                return None;
            }
            let dirty = sentinel.dirty;
            sentinel.dirty = false;
            data.pending = false;
            *active = Some(ActivePass { composition: id, composing: true });
            *abandoned_groups = false;
            cells.begin_pass();
            frames.clear();
            frames.push(Frame {
                group: SENTINEL,
                mode,
                inserting: false,
                forced: false,
                skipped: mode == Mode::Walk,
                dirty,
                cursor: sentinel.first_child,
                limit: NONE,
                last_placed: NONE,
                slot_cursor: 0,
                node_parent: data.root_node,
                node_index: 0,
                node_start: 0,
                scope: ScopeId::NONE,
                saved_epoch: 0,
                body_started: false,
            });
            Some(data.applier.clone())
        })?;
        let pass = Pass { id, finished: false };
        applier
            .try_borrow_mut()
            .unwrap_or_else(|_| misuse(ComposeError::ApplierBusy))
            .on_begin_changes();
        Some(pass)
    }

    /// Ends the pass, then applies its deferred changes: releases removed nodes, forgets removed
    /// values, dispatches new remember observers, runs side effects, frees retired state cells.
    ///
    /// The pass is over before any of that runs, so effects, observers and drops may compose
    /// other compositions (a popup from an effect), and a frame requester may recompose at once.
    fn finish(mut self) {
        let (op, applier) = with_runtime(|runtime| {
            let Runtime { compositions, scopes, frames, removal_stack, abandoned_groups, .. } =
                runtime;
            let data = compositions.get_mut(self.id).expect("a live composition");
            if frames.len() != 1 {
                misuse(if *abandoned_groups {
                    ComposeError::CaughtPanic
                } else {
                    ComposeError::UnclosedGroups
                })
            }
            let frame = frames.pop().expect("the sentinel frame");
            if frame.mode != Mode::Normal {
                misuse(ComposeError::UnclosedGroups)
            }
            let table = &mut data.table;
            let mut op = NodeOp::None;
            if !frame.skipped && frame.cursor != NONE {
                let count = remove_siblings(
                    table,
                    &mut data.deferred,
                    scopes,
                    removal_stack,
                    frame.cursor,
                    NONE,
                );
                table.set_next(SENTINEL, frame.last_placed, NONE);
                if count > 0 {
                    op = NodeOp::Remove {
                        parent: frame.node_parent,
                        index: frame.node_index,
                        count,
                    };
                }
            }
            table[SENTINEL].node_count = frame.node_index - frame.node_start;
            (op, data.applier.clone())
        });
        apply_node_op(op);
        flush_node_moves();
        let (mut deferred, freeing) = with_runtime(|runtime| {
            runtime.active = None;
            runtime.finishing += 1;
            let data = runtime.compositions.get_mut(self.id).expect("a live composition");
            data.finishing = true;
            (std::mem::take(&mut data.deferred), runtime.cells.end_pass())
        });
        // From here on the slot table is consistent: a panic in an effect doesn't poison it.
        self.finished = true;
        let mut finishing = Finishing { id: self.id, freeing };
        apply_deferred(&applier, &mut deferred, true);
        // Rule R2: the cells retired before this pass go once its cleanups and effects ran.
        let freeing = std::mem::take(&mut finishing.freeing);
        let freed = with_runtime(|runtime| runtime.cells.free_all(freeing));
        drop(freed);
        drop(finishing);
        with_runtime(|runtime| {
            // An effect may have dropped the composition.
            if let Some(data) = runtime.compositions.get_mut(self.id) {
                data.deferred.restore(deferred);
                data.pending = data.has_invalidations();
            }
        });
        request_frames();
    }
}

/// A pass applying its end-of-pass work; ends it, even when an effect panics.
struct Finishing {
    id: CompositionId,
    /// The cells to free once the work has run; after a panic, they wait for the next pass.
    freeing: Vec<crate::state::StateId>,
}

impl Drop for Finishing {
    fn drop(&mut self) {
        let freeing = std::mem::take(&mut self.freeing);
        try_with_runtime(|runtime| {
            runtime.cells.retire_all(freeing);
            runtime.finishing -= 1;
            if runtime.finishing == 0 {
                runtime.cells.release_held_by_passes();
            }
            if let Some(data) = runtime.compositions.get_mut(self.id) {
                data.finishing = false;
                // Invalidated while it finished: a frame for it.
                if data.has_invalidations() && !data.pending {
                    data.pending = true;
                    if let Some(requester) = &data.frame_requester {
                        runtime.frame_requests.push(requester.clone());
                    }
                }
            }
        });
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // A panic unwound through the pass. Nothing can be trusted to continue it; keep the
        // slot table for disposal, which frees everything exactly once.
        try_with_runtime(|runtime| {
            if runtime.active.is_some_and(|active| active.composition == self.id) {
                runtime.active = None;
                runtime.frames.clear();
                runtime.pending_move = None;
                runtime.cells.abort_pass();
            }
            if let Some(data) = runtime.compositions.get_mut(self.id) {
                data.poisoned = true;
            }
        });
    }
}

/// Applies the end-of-pass work in rule R1's order. With `run_effects` false (disposal), side
/// effects are dropped without running.
fn apply_deferred(
    applier: &Rc<RefCell<dyn ErasedApplier>>,
    deferred: &mut Deferred,
    run_effects: bool,
) {
    {
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        for node in deferred.released_nodes.drain(..) {
            applier.release(node);
        }
        applier.on_end_changes();
    }
    dispatch_forgotten(&deferred.removed_values);
    deferred.removed_values.clear();
    for observer in deferred.remembered.drain(..) {
        observer.dispatch_remembered();
    }
    if run_effects {
        for effect in deferred.side_effects.drain(..) {
            effect.run();
        }
    } else {
        deferred.side_effects.clear();
    }
}

/// A composition taken out of the runtime, to be released outside the borrow.
struct Disposal {
    /// The composition's data, or `None` when it is only closed (at thread exit) and its handle
    /// still owns it.
    data: Option<CompositionData>,
    applier: Rc<RefCell<dyn ErasedApplier>>,
    root_node: crate::applier::NodeRef,
    deferred: Deferred,
    top_level_nodes: u32,
}

impl Disposal {
    fn take(runtime: &mut Runtime, id: CompositionId) -> Option<Disposal> {
        if runtime.active.is_some_and(|active| active.composition == id) {
            if std::thread::panicking() {
                // Unwinding through our own pass: leak rather than abort with a double panic.
                return None;
            }
            misuse(ComposeError::DisposedDuringPass)
        }
        let data = runtime.compositions.remove(id)?;
        Some(Disposal::empty(runtime, data))
    }

    /// Empties a composition still owned by its handle, at thread exit, and closes it: the handle
    /// stays usable (an empty tree; passes compose nothing) until its owner drops it.
    fn close(runtime: &mut Runtime, id: CompositionId) -> Option<Disposal> {
        if runtime.active.is_some_and(|active| active.composition == id) {
            return None;
        }
        let data = runtime.compositions.get_mut(id).filter(|data| !data.closed)?;
        data.closed = true;
        let data = std::mem::replace(data, CompositionData::closed(data));
        runtime.compositions.closed += 1;
        let mut disposal = Disposal::empty(runtime, data);
        disposal.data = None;
        Some(disposal)
    }

    fn empty(runtime: &mut Runtime, mut data: CompositionData) -> Disposal {
        let Runtime { scopes, removal_stack, disposing, .. } = runtime;
        // Counts as live while its values drop: cells they retire wait for the end of the
        // disposal, as they would wait for the end of a pass.
        *disposing += 1;
        let mut deferred = std::mem::take(&mut data.deferred);
        let first = data.table[SENTINEL].first_child;
        remove_siblings(&mut data.table, &mut deferred, scopes, removal_stack, first, NONE);
        data.table[SENTINEL].first_child = NONE;
        let top_level_nodes = if data.poisoned { 0 } else { data.table[SENTINEL].node_count };
        data.table[SENTINEL].node_count = 0;
        Disposal {
            applier: data.applier.clone(),
            root_node: data.root_node,
            data: Some(data),
            deferred,
            top_level_nodes,
        }
    }

    fn run(mut self) {
        let disposing = Disposing;
        let applier = self.applier.clone();
        {
            let mut applier =
                applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
            applier.on_begin_changes();
            if self.top_level_nodes > 0 {
                applier.remove(self.root_node, 0, self.top_level_nodes as usize);
            }
        }
        apply_deferred(&applier, &mut self.deferred, false);
        drop(applier);
        let Disposal { data, deferred, .. } = self;
        drop(deferred);
        drop(data);
        drop(disposing);
        // Free the state cells the disposals retired once none is left, and everything if no
        // composition is left to run the pass that would free them.
        loop {
            let freed: Vec<CellValue> = with_runtime(|runtime| {
                if runtime.disposing > 0 {
                    return Vec::new();
                }
                let all = runtime.compositions.open() == 0 && runtime.finishing == 0;
                runtime.cells.free_held_by_disposals(all)
            });
            if freed.is_empty() {
                break;
            }
            drop(freed);
        }
        request_frames();
    }
}

/// A disposal in progress; ends it, even when a value's `Drop` panics.
struct Disposing;

impl Drop for Disposing {
    fn drop(&mut self) {
        try_with_runtime(|runtime| {
            runtime.disposing -= 1;
            if std::thread::panicking() {
                runtime.cells.abort_disposal();
            }
        });
    }
}

fn validate(runtime: &Runtime, id: CompositionId) -> Result<(), String> {
    let data = runtime.compositions.get(id).ok_or("disposed composition")?;
    let table = &data.table;
    let in_pass = runtime.active.is_some_and(|active| active.composition == id);
    let mut reachable = 0usize;
    let mut stack = vec![SENTINEL];
    while let Some(index) = stack.pop() {
        reachable += 1;
        let group = &table[index];
        if group.kind == GroupKind::Free {
            return Err(format!("group {index} is free but reachable"));
        }
        let mut nodes = 0;
        let mut child = group.first_child;
        while child != NONE {
            let entry = &table[child];
            if entry.parent != index {
                return Err(format!(
                    "group {child} has parent {} instead of {index}",
                    entry.parent
                ));
            }
            nodes += if entry.kind == GroupKind::Node { 1 } else { entry.node_count };
            if !in_pass && entry.dirty && !group.dirty {
                return Err(format!("group {child} is dirty but its parent {index} is not"));
            }
            stack.push(child);
            child = entry.next_sibling;
        }
        let expected = if group.kind == GroupKind::Node { 1 } else { nodes };
        if group.node_count != expected {
            return Err(format!(
                "group {index} counts {} nodes, its children {expected}",
                group.node_count
            ));
        }
        if group.kind == GroupKind::Node && group.node.is_none() {
            return Err(format!("node group {index} has no node"));
        }
        match group.kind {
            GroupKind::Restart => {
                let scope = runtime
                    .scopes
                    .get(group.scope)
                    .ok_or_else(|| format!("restart group {index} has no live scope"))?;
                if scope.group != index || scope.composition != id.index {
                    return Err(format!("scope of group {index} points elsewhere"));
                }
                if !in_pass && group.restart.is_none() {
                    return Err(format!("restart group {index} has no restart function"));
                }
                if !in_pass && scope.invalid && index != SENTINEL && !table[group.parent].dirty {
                    return Err(format!("invalid scope of group {index} has a clean parent"));
                }
            }
            _ => {
                if !group.scope.is_none() || group.kept.is_some() {
                    return Err(format!("group {index} of kind {:?} has a scope", group.kind));
                }
            }
        }
    }
    if reachable != table.live_count() {
        return Err(format!(
            "{} live groups but {reachable} reachable: groups leaked",
            table.live_count()
        ));
    }
    Ok(())
}
