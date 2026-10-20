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
    ActivePass, CompositionData, CompositionId, Deferred, Runtime, remove_siblings,
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
        Composition { id, _not_send: PhantomData }
    }

    /// Sets the content and composes it at once. Content set before is recomposed against the
    /// new one: groups with the same keys keep their state.
    pub fn set_content(&self, content: impl Fn() + 'static) {
        let content: Rc<dyn Fn()> = Rc::new(content);
        let pass = Pass::begin(self.id, Mode::Normal).expect("a full pass always starts");
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
        let old = with_runtime(|runtime| self.data(runtime).frame_requester.replace(requester));
        drop(old);
    }

    /// Runs `f` with the applier, as type `A`.
    pub fn with_applier<A: Applier, R>(&self, f: impl FnOnce(&mut A) -> R) -> R {
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
                if let Some(scope) = runtime.scopes.get(entry.scope) {
                    if scope.invalid {
                        let _ = write!(out, " invalid");
                    }
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
            let Runtime { compositions, cells, frames, active, .. } = runtime;
            let data = compositions.get_mut(id).expect("a live composition");
            if data.poisoned {
                misuse(ComposeError::Poisoned)
            }
            let sentinel = &mut data.table[SENTINEL];
            if mode == Mode::Walk && !sentinel.dirty {
                return None;
            }
            let dirty = sentinel.dirty;
            sentinel.dirty = false;
            data.pending = false;
            *active = Some(ActivePass { composition: id, composing: true });
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
            let Runtime { compositions, scopes, frames, removal_stack, .. } = runtime;
            let data = compositions.get_mut(self.id).expect("a live composition");
            if frames.len() != 1 {
                misuse(ComposeError::UnclosedGroups)
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
        let (mut deferred, freed) = with_runtime(|runtime| {
            runtime.active = None;
            let data = runtime.compositions.get_mut(self.id).expect("a live composition");
            (std::mem::take(&mut data.deferred), runtime.cells.end_pass())
        });
        // From here on the slot table is consistent: a panic in an effect doesn't poison it.
        self.finished = true;
        apply_deferred(&applier, &mut deferred, true);
        drop(freed);
        with_runtime(|runtime| {
            // An effect may have dropped the composition.
            if let Some(data) = runtime.compositions.get_mut(self.id) {
                data.deferred.restore(deferred);
                data.pending = data.has_invalidations();
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
    data: CompositionData,
    deferred: Deferred,
    top_level_nodes: u32,
    retired_mark: usize,
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
        let Runtime { compositions, scopes, removal_stack, cells, disposing, .. } = runtime;
        let mut data = compositions.remove(id)?;
        // Counts as live while its values drop: cells they retire wait for the end of the
        // disposal, as they would wait for the end of a pass.
        *disposing += 1;
        let mut deferred = std::mem::take(&mut data.deferred);
        let first = data.table[SENTINEL].first_child;
        remove_siblings(&mut data.table, &mut deferred, scopes, removal_stack, first, NONE);
        data.table[SENTINEL].first_child = NONE;
        let top_level_nodes = if data.poisoned { 0 } else { data.table[SENTINEL].node_count };
        Some(Disposal { data, deferred, top_level_nodes, retired_mark: cells.retired_len() })
    }

    fn run(mut self) {
        let applier = self.data.applier.clone();
        {
            let mut applier =
                applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
            applier.on_begin_changes();
            if self.top_level_nodes > 0 {
                applier.remove(self.data.root_node, 0, self.top_level_nodes as usize);
            }
        }
        apply_deferred(&applier, &mut self.deferred, false);
        drop(applier);
        let Disposal { data, deferred, retired_mark, .. } = self;
        drop(deferred);
        drop(data);
        // Free the state cells this disposal retired, and everything if no composition is left
        // to run the pass that would free them.
        let last = with_runtime(|runtime| {
            runtime.disposing -= 1;
            runtime.compositions.live() == 0 && runtime.disposing == 0
        });
        loop {
            let freed: Vec<CellValue> = with_runtime(|runtime| {
                runtime.cells.free_retired(if last { 0 } else { retired_mark }, last)
            });
            if freed.is_empty() {
                break;
            }
            drop(freed);
        }
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
