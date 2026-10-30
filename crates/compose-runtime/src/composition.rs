//! `Composition`: owns a slot table and an applier, and runs passes.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::fmt::Write as _;
use std::marker::PhantomData;
use std::rc::Rc;

use crate::applier::{Applier, ErasedApplier, NodeRef};
use crate::composer::{Frame, Mode, NodeOp, apply_node_op, current, flush_node_moves, run_walk};
use crate::context::CompositionContext;
use crate::error::{ComposeError, misuse};
use crate::hash::key_of;
use crate::recomposer::Recomposer;
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
    pub(crate) id: CompositionId,
    _not_send: PhantomData<*const ()>,
}

impl Composition {
    /// A composition emitting nodes into `applier`.
    pub fn new<A: Applier>(applier: A) -> Composition {
        let root_node = applier.root();
        let applier: Rc<RefCell<dyn ErasedApplier>> =
            Rc::new(RefCell::new(crate::applier::SharedApplier::new(applier)));
        let id = with_runtime(|runtime| {
            runtime.compositions.alloc(CompositionData::new(applier, root_node))
        });
        register_exit_guard();
        Composition { id, _not_send: PhantomData }
    }

    /// A composition emitting nodes into `applier`, under `parent`, as `Composition(applier,
    /// parent)`.
    pub fn with_parent<A: Applier>(applier: A, parent: CompositionContext) -> Composition {
        let composition = Composition::new(applier);
        with_runtime(|runtime| runtime.compositions.register(composition.id, parent));
        composition
    }

    /// A composition under `parent` using the applier of its composition, its nodes below `root`
    /// (`platform::subcomposition`).
    pub(crate) fn sharing_applier(parent: CompositionContext, root: NodeRef) -> Composition {
        let applier = crate::context::parent_applier(parent);
        let id = with_runtime(|runtime| {
            let id = runtime.compositions.alloc(CompositionData::new(applier, root));
            runtime.compositions.register(id, parent);
            id
        });
        register_exit_guard();
        Composition { id, _not_send: PhantomData }
    }

    /// What it's under, as `parent`.
    pub fn parent(&self) -> CompositionContext {
        let (recomposer, place) = with_runtime(|runtime| {
            let data = self.data(runtime);
            (data.recomposer, data.parent)
        });
        if place.is_some() {
            return CompositionContext { recomposer, place };
        }
        recomposer.unwrap_or_else(Recomposer::current_thread).as_context()
    }

    /// A composition whose content can be reused and deactivated, as
    /// `ReusableComposition(applier, parent)`.
    pub fn reusable<A: Applier>(applier: A, parent: CompositionContext) -> ReusableComposition {
        ReusableComposition::of(Composition::with_parent(applier, parent), false)
    }

    /// A composition whose content can be composed in pauses, as
    /// `PausableComposition(applier, parent)`.
    pub fn pausable<A: Applier>(applier: A, parent: CompositionContext) -> PausableComposition {
        PausableComposition::of(Composition::with_parent(applier, parent))
    }

    /// Disposes it now, as `dispose`: what it holds goes in rule R1's order (nodes and their
    /// handlers, then remembered values and state), and it composes nothing again: setting its
    /// content panics. Its `Drop` is this, if it wasn't. Disposing it again does nothing.
    pub fn dispose(&self) {
        // Paused content is discarded first, its nodes released and its observers abandoned.
        crate::pausable::cancel_for_disposal(self.id);
        let disposal = with_runtime(|runtime| {
            if runtime.compositions.get(self.id).is_some_and(|data| data.composing) {
                misuse(ComposeError::DisposedDuringPass)
            }
            let disposal = Disposal::close(runtime, self.id);
            if let Some(data) = runtime.compositions.get_mut(self.id) {
                data.disposed = true;
            }
            disposal
        });
        if let Some(disposal) = disposal {
            disposal.run();
        }
    }

    /// Whether [`dispose`](Composition::dispose) was called, as `isDisposed`.
    pub fn is_disposed(&self) -> bool {
        with_runtime(|runtime| runtime.compositions.get(self.id).is_none_or(|data| data.disposed))
    }

    /// Every scope of it invalid, so its next pass composes it all, as `invalidateAll`.
    pub fn invalidate_all(&self) {
        with_runtime(|runtime| invalidate_all_scopes(runtime, self.id));
        request_frames();
    }

    /// Sets the content and composes it at once. Content set before is recomposed against the
    /// new one: groups with the same keys keep their state.
    ///
    /// # Panics
    ///
    /// Once it's disposed, as Compose's.
    pub fn set_content(&self, content: impl Fn() + 'static) {
        with_runtime(|runtime| {
            let Some(data) = runtime.compositions.get(self.id) else { return };
            if data.disposed {
                misuse(ComposeError::Disposed)
            }
            if data.parent.is_some() && !data.closed && !root_is_there(runtime, self.id) {
                misuse(ComposeError::RootNodeGone)
            }
        });
        self.compose_content(content);
    }

    /// [`set_content`](Self::set_content), also below a root that's gone, as what deactivates
    /// content there has only slots to go through.
    fn compose_content(&self, content: impl Fn() + 'static) {
        let content: Rc<dyn Fn()> = Rc::new(content);
        // Composed in this frame, by a tree whose work's being applied: its work goes first.
        run_unapplied(self.id);
        // The changes of the turn so far invalidate their readers first (D751), as `recompose`;
        // those of a pass it's nested in are for that pass's apply, and a measurement lending it
        // the tree sends nothing, as Compose's `composeInitial`.
        if !with_runtime(|runtime| runtime.is_composing() || !runtime.lent.is_empty()) {
            crate::snapshots::Snapshot::send_apply_notifications();
        }
        // What was asked of the paused composition, if any, first.
        crate::pausable::run_requests(self.id);
        // A composition closed at thread exit composes nothing.
        let Some(pass) = Pass::begin(self.id, Mode::Normal) else { return };
        compose_root(content);
        pass.finish();
    }

    /// Runs a pass if any scope is invalid. Returns whether a pass ran.
    pub fn recompose(&self) -> bool {
        // The changes of the turn so far invalidate their readers first (D751).
        crate::snapshots::Snapshot::send_apply_notifications();
        // What was asked of the paused composition, if any, first (a cancel during a layout, a
        // completion a layout needs).
        crate::pausable::run_requests(self.id);
        if with_runtime(|runtime| self.data(runtime).child_count != 0) {
            return recompose_tree(self.id);
        }
        let Some(pass) = Pass::begin(self.id, Mode::Walk) else {
            return false;
        };
        run_walk(0);
        pass.finish();
        true
    }

    /// Whether some scope is invalid here or in a composition under it, so
    /// [`recompose`](Composition::recompose) would run a pass.
    pub fn has_invalidations(&self) -> bool {
        // The turn's changes invalidate their readers first (D751).
        let (invalid, busy, unsent) = with_runtime(|runtime| {
            let unsent = runtime.snapshots.has_changes();
            (self.invalid_in_tree(runtime), runtime.is_busy(), unsent)
        });
        if busy || !unsent {
            return invalid;
        }
        crate::snapshots::Snapshot::send_apply_notifications();
        with_runtime(|runtime| self.invalid_in_tree(runtime))
    }

    #[inline]
    fn invalid_in_tree(&self, runtime: &mut Runtime) -> bool {
        let compositions = &runtime.compositions;
        let data = compositions.get(self.id).expect("a live composition");
        data.has_invalidations()
            || (!data.dirty_children.is_empty() && compositions.has_work_below(data))
    }

    /// Sets the function called when a scope of this composition becomes invalid while none
    /// was: the host's way to schedule a frame.
    pub fn set_frame_requester(&self, requester: impl Fn() + 'static) {
        let requester: Rc<dyn Fn()> = Rc::new(requester);
        let old = with_runtime(|runtime| {
            let data = self.data(runtime);
            let old = data.frame_requester.replace(requester.clone());
            // Invalidated before a requester was set: it gets the frame now.
            if self.invalid_in_tree(runtime) {
                self.data(runtime).pending = true;
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
        let applier = with_runtime(|runtime| {
            let data = runtime.compositions.get(self.id).expect("a live composition");
            crate::context::resolve_applier(&runtime.lent, &data.applier)
        });
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        let applier = applier
            .as_any_mut()
            .downcast_mut::<A>()
            .unwrap_or_else(|| misuse(ComposeError::WrongApplierType));
        f(applier)
    }

    /// A weak reference to the applier, as type `A`, that reaches it from outside this
    /// composition's borrows (a modifier node's handle, used from a task between frames): it
    /// doesn't keep the applier alive.
    pub fn applier_ref<A: Applier>(&self) -> ApplierRef<A> {
        let applier = with_runtime(|runtime| Rc::downgrade(&self.data(runtime).applier));
        ApplierRef { applier, _type: PhantomData }
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
                if matches!(entry.kind, GroupKind::Movable | GroupKind::Reusable) {
                    let _ = write!(out, " data={:#018x}", entry.data_key);
                }
                if entry.deactivated() {
                    let _ = write!(out, " deactivated");
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
        // Paused content is discarded first, its nodes released and its observers abandoned.
        crate::pausable::cancel_for_disposal(self.id);
        let Some(Some(disposal)) = try_with_runtime(|runtime| Disposal::take(runtime, self.id))
        else {
            return;
        };
        disposal.run();
    }
}

/// Every scope of `id` invalid.
pub(crate) fn invalidate_all_scopes(runtime: &mut Runtime, id: CompositionId) {
    let Some(data) = runtime.compositions.get(id) else { return };
    let table = &data.table;
    let mut scopes = Vec::new();
    let mut stack = vec![table[SENTINEL].first_child];
    while let Some(group) = stack.pop() {
        let mut group = group;
        while group != NONE {
            if table[group].kind == GroupKind::Restart && !table[group].scope.is_none() {
                scopes.push(table[group].scope);
            }
            stack.push(table[group].first_child);
            group = table[group].next_sibling;
        }
    }
    for scope in scopes {
        crate::runtime::invalidate_scope(runtime, scope);
    }
}

thread_local! {
    static EXIT_GUARD: ExitGuard = const { ExitGuard };
}

/// Registers the thread's exit guard (once; then a cheap check).
pub(crate) fn register_exit_guard() {
    let _ = EXIT_GUARD.try_with(|_| {});
}

/// Disposes the compositions still alive when the thread exits (one kept in a thread-local, or
/// leaked), so their values' `Drop` can still use state.
struct ExitGuard;

impl Drop for ExitGuard {
    fn drop(&mut self) {
        let Some(ids) = try_with_runtime(|runtime| {
            runtime.exiting = true;
            runtime.compositions.live_ids()
        }) else {
            return;
        };
        for id in ids {
            crate::pausable::cancel_for_disposal(id);
            if let Some(Some(disposal)) = try_with_runtime(|runtime| Disposal::close(runtime, id)) {
                disposal.run();
            }
        }
        crate::runtime::release_if_empty();
    }
}

/// What a [`ReusableComposition`]'s content is under: the groups that reuse and deactivate it, as
/// `ReusableContent` and `ReusableContentHost` do for content in a composition, a pausable
/// composition's content being paused content inside them.
struct ReusableRoot {
    /// A new one reuses the content.
    reuse_key: Cell<u64>,
    active: Cell<bool>,
    pausable: bool,
    content: RefCell<Rc<dyn Fn()>>,
}

impl ReusableRoot {
    fn new(pausable: bool) -> ReusableRoot {
        ReusableRoot {
            reuse_key: Cell::new(0),
            active: Cell::new(true),
            pausable,
            content: RefCell::new(Rc::new(|| {})),
        }
    }

    /// A root content under this: new each time, so a pass composes it.
    fn content(self: &Rc<Self>) -> Rc<dyn Fn()> {
        let root = self.clone();
        Rc::new(move || {
            let content = root.content.borrow().clone();
            crate::reusable::ReusableRootContent(root.active.get(), root.reuse_key.get(), || {
                // In a scope of its own, inside what pauses, as Compose's content lambda.
                if root.pausable {
                    crate::pausable::PausableContent(|| compose_root(content.clone()));
                } else {
                    compose_root(content.clone());
                }
            });
        })
    }

    /// # Panics
    ///
    /// While a paused composition of it is in progress, as Compose's `clearDeactivated`.
    fn set(&self, id: CompositionId, content: impl Fn() + 'static, reuse: bool) {
        with_runtime(|runtime| {
            if runtime.compositions.get(id).is_some_and(|data| data.has_paused_in_progress()) {
                misuse(ComposeError::PausedCompositionState)
            }
        });
        *self.content.borrow_mut() = Rc::new(content);
        self.active.set(true);
        if reuse {
            self.reuse_key.set(self.reuse_key.get() + 1);
        }
    }
}

/// A composition whose content can be reused and deactivated, as `ReusableComposition`: what a
/// lazy item's composition is in Compose, an item's content going to the next item.
pub struct ReusableComposition {
    composition: Composition,
    root: Rc<ReusableRoot>,
}

impl std::ops::Deref for ReusableComposition {
    type Target = Composition;

    fn deref(&self) -> &Composition {
        &self.composition
    }
}

impl ReusableComposition {
    pub(crate) fn of(composition: Composition, pausable: bool) -> ReusableComposition {
        ReusableComposition { composition, root: Rc::new(ReusableRoot::new(pausable)) }
    }

    /// Sets the content, as [`Composition::set_content`]: deactivated, it comes back reusing what
    /// was there.
    pub fn set_content(&self, content: impl Fn() + 'static) {
        self.root.set(self.composition.id, content, false);
        let root = self.root.content();
        self.composition.set_content(move || root());
    }

    /// Sets the content and composes it reusing what's there, as `setContentWithReuse`: slots read
    /// as empty, so values are remembered anew and the old ones forgotten, reusable nodes are kept
    /// and updated, others replaced.
    pub fn set_content_with_reuse(&self, content: impl Fn() + 'static) {
        self.root.set(self.composition.id, content, true);
        let root = self.root.content();
        self.composition.set_content(move || root());
    }

    /// Deactivates the content, as `deactivate`: its nodes are deactivated and its observers
    /// forgotten, its groups kept for what's set next to reuse.
    ///
    /// # Panics
    ///
    /// While a paused composition of it is in progress, as Compose's.
    pub fn deactivate(&self) {
        let id = self.composition.id;
        crate::pausable::run_requests(id);
        with_runtime(|runtime| {
            if runtime.compositions.get(id).is_some_and(|data| data.has_paused_in_progress()) {
                misuse(ComposeError::PausedCompositionState)
            }
        });
        self.root.active.set(false);
        let root = self.root.content();
        self.composition.compose_content(move || root());
    }
}

/// A composition whose content can be composed in pauses, as `PausableComposition`.
pub struct PausableComposition {
    reusable: ReusableComposition,
    /// The last paused composition's state: one cancelled leaves the composition with nothing to
    /// go on from, as Compose's refuses then.
    last: RefCell<Option<Rc<Cell<crate::pausable::PausedState>>>>,
}

impl std::ops::Deref for PausableComposition {
    type Target = ReusableComposition;

    fn deref(&self) -> &ReusableComposition {
        &self.reusable
    }
}

impl PausableComposition {
    pub(crate) fn of(composition: Composition) -> PausableComposition {
        PausableComposition {
            reusable: ReusableComposition::of(composition, true),
            last: RefCell::new(None),
        }
    }

    /// Sets the content, to compose in the paused composition this returns, as
    /// `setPausableContent`: deactivated, it comes back reusing what was there.
    ///
    /// # Panics
    ///
    /// If the last one was cancelled, or another one is in progress, as Compose's.
    pub fn set_pausable_content(
        &self,
        content: impl Fn() + 'static,
    ) -> crate::pausable::PausedComposition {
        self.paused(content, false)
    }

    /// Sets the content, to compose in the paused composition this returns reusing what's there,
    /// as `setPausableContentWithReuse`.
    ///
    /// # Panics
    ///
    /// If the last one was cancelled, or another one is in progress, as Compose's.
    pub fn set_pausable_content_with_reuse(
        &self,
        content: impl Fn() + 'static,
    ) -> crate::pausable::PausedComposition {
        self.paused(content, true)
    }

    fn paused(
        &self,
        content: impl Fn() + 'static,
        reuse: bool,
    ) -> crate::pausable::PausedComposition {
        let cancelled = self
            .last
            .borrow()
            .as_ref()
            .is_some_and(|last| last.get() == crate::pausable::PausedState::Cancelled);
        if cancelled {
            misuse(ComposeError::PausedCompositionState)
        }
        with_runtime(|runtime| {
            let id = self.reusable.composition.id;
            let child = runtime.compositions.get(id).is_some_and(|data| data.parent.is_some());
            if child && !root_is_there(runtime, id) {
                misuse(ComposeError::RootNodeGone)
            }
        });
        self.reusable.root.set(self.reusable.composition.id, content, reuse);
        let paused = self.reusable.composition.paused_content(self.reusable.root.content());
        *self.last.borrow_mut() = Some(paused.state());
        paused
    }
}

/// The content of a composition, as a hand-expanded restartable composable whose one argument
/// is the content lambda.
pub(crate) fn compose_root(content: Rc<dyn Fn()>) {
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

/// Which pass: a pass of the whole composition, or a paused composition's.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PassKind {
    /// `set_content` or `recompose`.
    Whole,
    /// A paused composition's first: `recompose`'s, in which pausable content inserted or
    /// reused is paused content.
    Pausable,
    /// A paused composition's resume: the walks of its regions.
    Resume,
}

/// Whether the node `id`, a child composition, composes below is there. It says what's asked again
/// is pointless: nothing's to compose until its owner disposes it.
pub(crate) fn root_is_there(runtime: &mut Runtime, id: CompositionId) -> bool {
    let data = runtime.compositions.get(id).expect("a live composition");
    let applier = crate::context::resolve_applier(&runtime.lent, &data.applier);
    let root = data.root_node;
    let there = applier.try_borrow().map_or(true, |applier| applier.contains(root));
    if !there {
        runtime.compositions.get_mut(id).expect("a live composition").root_gone = true;
    }
    there
}

/// Whether what `id`, a child composition, reads above its root moved since its last pass, as
/// `startRoot`'s `changed(parentProvider)`: its whole content composes, from its root, then. Its
/// first pass has nothing to compare, nor one whose place is gone, which keeps what it had.
fn providers_changed(runtime: &mut Runtime, id: CompositionId, mode: Mode) -> bool {
    let compositions = &mut runtime.compositions;
    let data = compositions.get(id).expect("a live composition");
    let Some(epoch) = data.parent.and_then(|place| compositions.provider_epoch(place)) else {
        return false;
    };
    let data = compositions.get_mut(id).expect("a live composition");
    let last = data.provider_epoch.replace(epoch);
    if !last.is_some_and(|last| last != epoch) {
        return false;
    }
    // A walk reaches its content by what's invalid.
    let root = data.table[SENTINEL].first_child;
    if mode == Mode::Walk && root != NONE && !data.table[root].scope.is_none() {
        let scope = data.table[root].scope;
        crate::runtime::invalidate_scope(runtime, scope);
    }
    true
}

/// What a pass nested in another (a child composition composing at once, in a body of its parent)
/// keeps of the pass it's in, which goes on when it ends: every per-pass field of the runtime
/// (`take` names them all, so a new field is decided on).
struct OuterPass {
    active: Option<ActivePass>,
    frames: Vec<Frame>,
    abandoned_groups: bool,
    reusing: usize,
    providers_invalid: usize,
    pause: Option<Rc<dyn crate::pausable::ShouldPauseCallback>>,
    pausable_pass: bool,
    resume_next: Option<(u32, bool)>,
}

impl OuterPass {
    fn take(runtime: &mut Runtime) -> Option<Box<OuterPass>> {
        // No `..`: what a new field means here is to be said.
        let Runtime {
            active,
            frames,
            abandoned_groups,
            reusing,
            providers_invalid,
            pause,
            pausable_pass,
            resume_next,
            // Flushed before a nested pass begins (`Pass::begin_as`).
            pending_move: _,
            // Held only from `start_group` to `start_branch_group_of`, no user code between.
            branch_removal: _,
            // Empty but during a removal or a walk's step.
            removal_stack: _,
            // Whole-thread: these go on through nested passes.
            node_changes: _,
            cells: _,
            scopes: _,
            compositions: _,
            lent: _,
            lending_cells: _,
            unapplied: _,
            tree_frames: _,
            provider_versions: _,
            disposing_data: _,
            frame_requests: _,
            snapshots: _,
            observers: _,
            observing: _,
            disposing: _,
            finishing: _,
            exiting: _,
            bodies_run: _,
            state_reads: _,
            recorded_changes: _,
            recomposition_counts: _,
        } = runtime;
        active.is_some().then(|| {
            Box::new(OuterPass {
                active: active.take(),
                frames: std::mem::take(frames),
                abandoned_groups: *abandoned_groups,
                reusing: *reusing,
                providers_invalid: *providers_invalid,
                pause: pause.take(),
                pausable_pass: *pausable_pass,
                resume_next: resume_next.take(),
            })
        })
    }

    fn restore(self, runtime: &mut Runtime) {
        runtime.active = self.active;
        runtime.frames = self.frames;
        runtime.abandoned_groups = self.abandoned_groups;
        runtime.reusing = self.reusing;
        runtime.providers_invalid = self.providers_invalid;
        runtime.pause = self.pause;
        runtime.pausable_pass = self.pausable_pass;
        runtime.resume_next = self.resume_next;
    }
}

/// A pass in progress. If it is dropped without [`finish`](Pass::finish), a panic unwound
/// through it: the composition is poisoned.
pub(crate) struct Pass {
    id: CompositionId,
    kind: PassKind,
    finished: bool,
    /// What it composes in, as `Recomposer.composing` (§3.13).
    snapshot: Option<crate::snapshots::PassSnapshot>,
    /// The pass it's nested in, if any.
    outer: Option<Box<OuterPass>>,
}

impl Pass {
    pub(crate) fn begin(id: CompositionId, mode: Mode) -> Option<Pass> {
        Pass::begin_as(id, mode, PassKind::Whole, None)
    }

    /// Begins a pass of `kind`, a paused composition's asking `should_pause` at its pause
    /// points. A resume's walks its regions (the caller pushes their frames): the sentinel
    /// frame composes nothing.
    pub(crate) fn begin_as(
        id: CompositionId,
        mode: Mode,
        kind: PassKind,
        should_pause: Option<Rc<dyn crate::pausable::ShouldPauseCallback>>,
    ) -> Option<Pass> {
        let resume = kind == PassKind::Resume;
        // A nested pass may share the applier of the one it's in: that one's moves go first.
        if with_runtime(|runtime| runtime.active.is_some()) {
            flush_node_moves();
        }
        let (applier, snapshot, outer) = with_runtime(|runtime| {
            let data = runtime.compositions.get_mut(id).expect("a live composition");
            if data.composing {
                misuse(ComposeError::PassInProgress)
            }
            if data.closed {
                return None;
            }
            if data.poisoned {
                misuse(ComposeError::Poisoned)
            }
            if data.finishing && mode != Mode::Walk {
                misuse(ComposeError::PassInProgress)
            }
            if data.finishing {
                return None;
            }
            let sentinel = &mut data.table[SENTINEL];
            if mode == Mode::Walk && !sentinel.dirty && !resume {
                return None;
            }
            // Its root, a node of its parent's, gone with what was there: nothing can show.
            if mode == Mode::Walk && !resume && data.parent.is_some() && !root_is_there(runtime, id)
            {
                return None;
            }
            crate::snapshots::PassSnapshot::check_can_begin(&runtime.snapshots);
            let providers_changed =
                runtime.compositions.get(id).is_some_and(|data| data.parent.is_some())
                    && providers_changed(runtime, id, mode);
            let outer = OuterPass::take(runtime);
            let Runtime {
                compositions,
                snapshots,
                cells,
                frames,
                active,
                abandoned_groups,
                reusing,
                providers_invalid,
                pause,
                pausable_pass,
                resume_next,
                lent,
                ..
            } = runtime;
            let data = compositions.get_mut(id).expect("a live composition");
            let sentinel = &mut data.table[SENTINEL];
            let dirty = sentinel.dirty && !resume;
            if !resume {
                sentinel.dirty = false;
                data.pending = false;
            }
            // What it writes of what's made before is a change, what it makes isn't (D756).
            let snapshot = crate::snapshots::PassSnapshot::begin(snapshots);
            *active = Some(ActivePass { composition: id, composing: true });
            *abandoned_groups = false;
            *reusing = 0;
            // The sentinel's depth: to the end of the pass.
            *providers_invalid = usize::from(providers_changed);
            *pause = should_pause;
            *pausable_pass = kind != PassKind::Whole;
            *resume_next = None;
            data.composing = true;
            // Rule R2's "the next pass" is the next outermost one.
            if outer.is_none() {
                cells.begin_pass();
                frames.clear();
            }
            frames.push(Frame {
                group: SENTINEL,
                mode: if resume { Mode::Normal } else { mode },
                inserting: false,
                forced: false,
                skipped: mode == Mode::Walk || resume,
                dirty,
                cursor: if resume { NONE } else { sentinel.first_child },
                limit: NONE,
                last_placed: NONE,
                slot_cursor: 0,
                node_parent: data.root_node,
                node_index: 0,
                node_start: 0,
                scope: ScopeId::NONE,
                saved_epoch: 0,
                body_started: false,
                region: crate::pausable::NO_REGION,
                segment: 0,
                resuming: false,
                paused: false,
            });
            Some((crate::context::resolve_applier(lent, &data.applier), snapshot, outer))
        })?;
        let pass = Pass { id, kind, finished: false, snapshot: Some(snapshot), outer };
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
    pub(crate) fn finish(self) {
        apply_work(self.finish_composing());
    }

    /// Ends the pass's composing: what [`finish`](Pass::finish) does before the work, which a
    /// frame of several compositions applies once they've all composed (D796, D1301).
    pub(crate) fn finish_composing(mut self) -> Composed {
        let resume = self.kind == PassKind::Resume;
        let (op, applier) = with_runtime(|runtime| {
            let Runtime {
                compositions,
                scopes,
                frames,
                removal_stack,
                abandoned_groups,
                recorded_changes,
                ..
            } = runtime;
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
            let mut op = NodeOp::None;
            if !frame.skipped
                && frame.cursor != NONE
                && let Some(paused) = data.paused.as_mut()
                && paused.holds_regions()
            {
                let removed = crate::composer::siblings_from(&data.table, frame.cursor, NONE);
                paused.discard_regions_in(
                    &mut data.table,
                    scopes,
                    removal_stack,
                    &mut data.deferred,
                    removed,
                    false,
                );
            }
            let table = &mut data.table;
            if !frame.skipped && frame.cursor != NONE {
                *recorded_changes += 1;
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
            if !resume {
                table[SENTINEL].node_count = frame.node_index - frame.node_start;
            }
            (op, data.applier.clone())
        });
        apply_node_op(op);
        flush_node_moves();
        // Paused content the pass reached or removed.
        if self.kind == PassKind::Whole {
            crate::pausable::finish_pass_regions(self.id);
        }
        self.end_composing(applier)
    }

    /// Composing is over: the pass's snapshot left and applied, as Compose applies it before the
    /// changes: what it made is initialized, what it wrote is everybody's. The composition counts
    /// as finishing from here until [`apply_work`].
    fn end_composing(&mut self, applier: Rc<RefCell<dyn ErasedApplier>>) -> Composed {
        let mut snapshot = self.snapshot.take().expect("the pass's snapshot");
        let outer = self.outer.take();
        let (deferred, freeing) = with_runtime(|runtime| {
            snapshot.leave(&mut runtime.snapshots);
            let freeing = if outer.is_some() { Vec::new() } else { runtime.cells.end_pass() };
            match outer {
                Some(outer) => outer.restore(runtime),
                None => {
                    runtime.active = None;
                    runtime.pause = None;
                    runtime.pausable_pass = false;
                    runtime.resume_next = None;
                }
            }
            runtime.finishing += 1;
            let data = runtime.compositions.get_mut(self.id).expect("a live composition");
            data.composing = false;
            data.finishing = true;
            (std::mem::take(&mut data.deferred), freeing)
        });
        // From here on the slot table is consistent: a panic in an effect doesn't poison it.
        self.finished = true;
        let finishing = Finishing { id: self.id, freeing };
        let committed = snapshot.commit();
        Composed { id: self.id, frame: 0, applier, deferred, finishing, committed }
    }
}

/// A frame's number, for the work it composes to be told from that of a frame around it.
pub(crate) fn begin_tree_frame() -> u64 {
    with_runtime(|runtime| {
        runtime.tree_frames += 1;
        runtime.tree_frames
    })
}

/// [`compose_tree`] of a tree's root, what's invalidated below it going to this frame.
pub(crate) fn compose_tree_of(id: CompositionId, frame: u64) -> std::thread::Result<()> {
    let outer = with_runtime(|runtime| runtime.compositions.composing_tree.replace(id));
    let composed = compose_tree(id, frame);
    with_runtime(|runtime| runtime.compositions.composing_tree = outer);
    composed
}

/// Composes what's invalid in `id` and in what's queued under it, parents first: each composes, its
/// snapshot applied, before those under it, its work left in `Runtime::unapplied`, and those it
/// invalidates are reached too. A panic poisons the composition it was composing and comes back:
/// what's composed is for the caller to apply.
fn compose_tree(id: CompositionId, frame: u64) -> std::thread::Result<()> {
    // Those a later frame looks at again: it wouldn't do more with them in this one.
    let mut later = Vec::new();
    let mut panic = None;
    // One whose pass panicked isn't its parent's to report again.
    let poisoned = with_runtime(|runtime| runtime.compositions.get(id).is_none_or(|d| d.poisoned));
    if !poisoned {
        let composed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::pausable::run_requests(id);
            let pass = Pass::begin(id, Mode::Walk)?;
            run_walk(0);
            Some(pass.finish_composing())
        }));
        match composed {
            Ok(Some(mut composed)) => {
                composed.frame = frame;
                with_runtime(|runtime| runtime.unapplied.push_back(composed));
            }
            Ok(None) => {}
            Err(caught) => {
                panic = Some(caught);
                with_runtime(|runtime| {
                    for child in take_queued(runtime, id) {
                        ask_again(runtime, child);
                        later.push(child);
                    }
                });
            }
        }
    }
    'taking: while panic.is_none() {
        let queued = with_runtime(|runtime| take_queued(runtime, id));
        if queued.is_empty() {
            break;
        }
        let mut queued = queued.into_iter();
        while let Some(child) = queued.next() {
            // What holds it may have just gone, in the parent's pass.
            let going = with_runtime(|runtime| {
                let compositions = &runtime.compositions;
                compositions.get(child).map(|data| compositions.is_going(data))
            });
            match going {
                None => {}
                Some(true) => {
                    with_runtime(|runtime| ask_again(runtime, child));
                    later.push(child);
                }
                Some(false) => {
                    let composed = compose_tree(child, frame);
                    // Invalidated while it's finishing, or with those under it left.
                    with_runtime(|runtime| {
                        let data = runtime.compositions.get(child);
                        if data.is_some_and(|d| d.finishing || !d.dirty_children.is_empty()) {
                            later.push(child);
                        }
                    });
                    if let Err(caught) = composed {
                        panic = Some(caught);
                        with_runtime(|runtime| {
                            for child in queued {
                                ask_again(runtime, child);
                                later.push(child);
                            }
                        });
                        break 'taking;
                    }
                }
            }
        }
    }
    with_runtime(|runtime| {
        let compositions = &mut runtime.compositions;
        for child in later {
            let Some(data) = compositions.get_mut(child) else { continue };
            if !data.queued {
                data.queued = true;
                if let Some(data) = compositions.get_mut(id) {
                    data.dirty_children.push(child);
                }
            }
        }
    });
    match panic {
        Some(panic) => Err(panic),
        None => Ok(()),
    }
}

/// What's queued under `id`, each no longer queued, in the order they were made.
fn take_queued(runtime: &mut Runtime, id: CompositionId) -> Vec<CompositionId> {
    let compositions = &mut runtime.compositions;
    let Some(data) = compositions.get_mut(id) else { return Vec::new() };
    if data.dirty_children.is_empty() {
        return Vec::new();
    }
    let mut queued = std::mem::take(&mut data.dirty_children);
    queued.retain(|child| compositions.get(*child).is_some());
    if queued.len() > 1 {
        queued
            .sort_unstable_by_key(|child| compositions.get(*child).map_or(0, |data| data.created));
    }
    for child in &queued {
        compositions.get_mut(*child).expect("just kept").queued = false;
    }
    queued
}

/// A frame left `child` with what it had to do: a later write asks for a frame as any does, and
/// this one asks for it now.
fn ask_again(runtime: &mut Runtime, child: CompositionId) {
    let compositions = &mut runtime.compositions;
    let Some(data) = compositions.get_mut(child) else { return };
    data.pending = false;
    let data = compositions.get(child).expect("just found");
    if let Some(requester) = compositions.frame_requester_of(data) {
        runtime.frame_requests.push(requester);
    }
}

/// [`Composition::recompose`] of a composition with compositions under it: all of them compose,
/// then all of their work runs, parents first, as the Recomposer's `toApply`.
fn recompose_tree(id: CompositionId) -> bool {
    if with_runtime(|runtime| runtime.compositions.get(id).is_some_and(|data| data.poisoned)) {
        misuse(ComposeError::Poisoned)
    }
    let frame = begin_tree_frame();
    let mut panic = compose_tree_of(id, frame).err();
    let changed = with_runtime(|runtime| runtime.unapplied.iter().any(|work| work.frame == frame));
    if let Some(caught) = apply_unapplied(frame) {
        panic.get_or_insert(caught);
    }
    request_frames();
    if let Some(panic) = panic {
        std::panic::resume_unwind(panic);
    }
    changed
}

/// Runs what a frame composed, in order, what's panicking not stopping the rest: the first panic.
pub(crate) fn apply_unapplied(frame: u64) -> Option<Box<dyn Any + Send>> {
    let mut panic = None;
    loop {
        let composed = with_runtime(|runtime| {
            let at = runtime.unapplied.iter().position(|work| work.frame == frame)?;
            runtime.unapplied.remove(at)
        });
        let Some(composed) = composed else { return panic };
        let applied =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| apply_work(composed)));
        if let Err(caught) = applied {
            panic.get_or_insert(caught);
        }
    }
}

/// `id`'s work, if a frame composed it and hasn't applied it.
pub(crate) fn run_unapplied(id: CompositionId) {
    let composed = with_runtime(|runtime| {
        if !runtime.compositions.get(id).is_some_and(|data| data.finishing) {
            return None;
        }
        let at = runtime.unapplied.iter().position(|work| work.id == id)?;
        runtime.unapplied.remove(at)
    });
    if let Some(composed) = composed {
        apply_work(composed);
    }
}

/// A pass that's composed, its snapshot's apply done: what [`apply_work`] needs.
pub(crate) struct Composed {
    pub(crate) id: CompositionId,
    /// The frame that composed it, for it to apply this; 0, for a lone pass.
    pub(crate) frame: u64,
    applier: Rc<RefCell<dyn ErasedApplier>>,
    deferred: Deferred,
    finishing: Finishing,
    committed: crate::snapshots::Committed,
}

/// The pass's work, in what was current before it: deferred changes, retired cells, and what the
/// apply came to.
pub(crate) fn apply_work(composed: Composed) {
    use crate::snapshots::Committed;
    let Composed { id, applier, mut deferred, mut finishing, committed, .. } = composed;
    // The one it's using now, if it's lent.
    let applier = with_runtime(|runtime| crate::context::resolve_applier(&runtime.lent, &applier));
    let observer_panic = match committed {
        Committed::Applied => None,
        Committed::ObserverPanicked(panic) => Some(panic),
        // Nothing applied: nor does its work run.
        Committed::Conflict | Committed::MergePanicked(_) => {
            with_runtime(|runtime| {
                if let Some(data) = runtime.compositions.get_mut(id) {
                    data.poisoned = true;
                }
            });
            if let Ok(mut applier) = applier.try_borrow_mut() {
                applier.abandon_changes();
            }
            drop(finishing);
            drop(deferred);
            match committed {
                Committed::MergePanicked(panic) => std::panic::resume_unwind(panic),
                _ => panic!(
                    "Unsupported concurrent change during composition. A state object was \
                     modified by composition as well as being modified outside composition."
                ),
            }
        }
    };
    // Disposed by another's work in this frame: its changes are cleared, as `dispose` clears them.
    let disposed = with_runtime(|runtime| {
        runtime.compositions.get(id).is_none_or(|data| data.disposed || data.closed)
    });
    apply_deferred(&applier, &mut deferred, !disposed);
    // Rule R2: the cells retired before this pass go once its cleanups and effects ran.
    let freeing = std::mem::take(&mut finishing.freeing);
    let freed = with_runtime(|runtime| runtime.cells.free_all(freeing));
    drop(freed);
    free_retired_providers(id);
    drop(finishing);
    with_runtime(|runtime| {
        // What its effects made is initialized too, as the end of Compose's frame (D754); nested
        // in a pass, at that pass's, as `composerWasComposing`.
        if !runtime.is_composing() {
            crate::snapshots::advance_after_pass(runtime);
        }
        // An effect may have dropped the composition.
        if let Some(data) = runtime.compositions.get_mut(id) {
            data.deferred.restore(deferred);
            data.pending = data.has_invalidations();
        }
    });
    request_frames();
    crate::runtime::release_if_empty();
    if let Some(panic) = observer_panic {
        std::panic::resume_unwind(panic);
    }
}

/// A pass applying its end-of-pass work (or a paused composition being applied or cancelled);
/// ends it, even when an effect panics.
pub(crate) struct Finishing {
    pub id: CompositionId,
    /// The cells to free once the work has run; after a panic, they wait for the next pass.
    pub freeing: Vec<crate::state::StateId>,
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
        let mut snapshot = self.snapshot.take();
        let outer = self.outer.take();
        // It began its changes and won't end them; those of what's around it go on.
        let applier = try_with_runtime(|runtime| {
            let data = runtime.compositions.get(self.id)?;
            Some(crate::context::resolve_applier(&runtime.lent, &data.applier))
        });
        if let Some(Some(applier)) = applier
            && let Ok(mut applier) = applier.try_borrow_mut()
        {
            applier.abandon_changes();
        }
        let cleaned = try_with_runtime(|runtime| {
            if let Some(snapshot) = &mut snapshot {
                snapshot.leave(&mut runtime.snapshots);
            }
            if runtime.active.is_some_and(|active| active.composition == self.id) {
                runtime.pending_move = None;
                match outer {
                    Some(outer) => outer.restore(runtime),
                    None => {
                        runtime.active = None;
                        runtime.frames.clear();
                        runtime.reusing = 0;
                        runtime.providers_invalid = 0;
                        runtime.pause = None;
                        runtime.pausable_pass = false;
                        runtime.resume_next = None;
                        runtime.cells.abort_pass();
                    }
                }
            }
            if let Some(data) = runtime.compositions.get_mut(self.id) {
                data.composing = false;
                data.poisoned = true;
            }
        });
        // What it wrote applies, as `composing`'s `finally`, with the pass over: observers and
        // values' `Drop`s are user code, which may recompose, or panic with this one unwinding. A
        // conflict's error doesn't replace this panic.
        if let (Some(_), Some(snapshot)) = (cleaned, snapshot) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| snapshot.commit()));
        }
    }
}

/// A weak reference to a composition's applier ([`Composition::applier_ref`]).
pub struct ApplierRef<A> {
    applier: std::rc::Weak<RefCell<dyn ErasedApplier>>,
    _type: PhantomData<fn() -> A>,
}

impl<A> Clone for ApplierRef<A> {
    fn clone(&self) -> Self {
        ApplierRef { applier: self.applier.clone(), _type: PhantomData }
    }
}

impl<A: Applier> ApplierRef<A> {
    /// Runs `f` with the applier if it's free: its composition lives, no pass is under way on
    /// the thread and nothing else borrows it. `None` otherwise.
    pub fn try_with<R>(&self, f: impl FnOnce(&mut A) -> R) -> Option<R> {
        let applier = self.applier.upgrade()?;
        if try_with_runtime(|runtime| runtime.active.is_some()).unwrap_or(true) {
            return None;
        }
        let mut applier = applier.try_borrow_mut().ok()?;
        let applier = applier.as_any_mut().downcast_mut::<A>()?;
        Some(f(applier))
    }

    /// Runs `f` with the applier to read it, if nothing borrows it: as `try_with`, a pass under
    /// way or not. It mustn't change what a pass applies to.
    pub fn try_peek<R>(&self, f: impl FnOnce(&mut A) -> R) -> Option<R> {
        let applier = self.applier.upgrade()?;
        let mut applier = applier.try_borrow_mut().ok()?;
        let applier = applier.as_any_mut().downcast_mut::<A>()?;
        Some(f(applier))
    }
}

/// The removals' lifecycle has run: `id`'s provider groups removed go, what they provided dropped
/// outside the runtime's borrow.
pub(crate) fn free_retired_providers(id: CompositionId) {
    let slots = with_runtime(|runtime| {
        runtime.compositions.get_mut(id).map(|data| data.table.free_retired_providers())
    });
    drop(slots);
}

/// Applies the end-of-pass work in rule R1's order. With `run_effects` false (disposal), side
/// effects are dropped without running.
fn apply_deferred(
    applier: &Rc<RefCell<dyn ErasedApplier>>,
    deferred: &mut Deferred,
    run_effects: bool,
) {
    {
        // What panics in a release leaves the applier's changes as they'd be after a pass.
        struct Abandon<'a>(&'a Rc<RefCell<dyn ErasedApplier>>, bool);
        impl Drop for Abandon<'_> {
            fn drop(&mut self) {
                if self.1
                    && let Ok(mut applier) = self.0.try_borrow_mut()
                {
                    applier.abandon_changes();
                }
            }
        }
        let mut abandon = Abandon(applier, true);
        let nested = with_runtime(|runtime| runtime.is_composing());
        let mut applier =
            applier.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
        // Those of a composition below a node of its parent's may have gone with it.
        for node in deferred.deactivated_nodes.drain(..) {
            if applier.contains(node) {
                applier.deactivate(node);
            }
        }
        if std::mem::take(&mut deferred.repeated_releases) {
            let mut released = std::collections::HashSet::new();
            deferred.released_nodes.retain(|node| released.insert(node.bits()));
        }
        for node in deferred.released_nodes.drain(..) {
            if applier.contains(node) {
                applier.release(node);
            }
        }
        abandon.1 = false;
        applier.on_end_changes(nested);
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
    id: CompositionId,
    /// The composition's data, dropped once its values are released. When the composition is
    /// only closed (at thread exit), its handle keeps a closed copy in the arena.
    data: Option<CompositionData>,
    applier: Rc<RefCell<dyn ErasedApplier>>,
    root_node: crate::applier::NodeRef,
    deferred: Deferred,
    top_level_nodes: u32,
}

impl Disposal {
    fn take(runtime: &mut Runtime, id: CompositionId) -> Option<Disposal> {
        if runtime.compositions.get(id).is_some_and(|data| data.composing) {
            if std::thread::panicking() {
                // Unwinding through our own pass: leak rather than abort with a double panic.
                return None;
            }
            misuse(ComposeError::DisposedDuringPass)
        }
        let data = runtime.compositions.remove(id)?;
        Some(Disposal::empty(runtime, id, data))
    }

    /// Empties a composition still owned by its handle, at thread exit, and closes it: the handle
    /// stays usable (an empty tree; passes compose nothing) until its owner drops it.
    fn close(runtime: &mut Runtime, id: CompositionId) -> Option<Disposal> {
        if runtime.compositions.get(id).is_some_and(|data| data.composing) {
            return None;
        }
        let data = runtime.compositions.get_mut(id).filter(|data| !data.closed)?;
        data.closed = true;
        let data = std::mem::replace(data, CompositionData::closed(data));
        runtime.compositions.closed += 1;
        // The old data (its frame requester, its applier's last references) drops in `run`,
        // outside the runtime's borrow.
        Some(Disposal::empty(runtime, id, data))
    }

    fn empty(runtime: &mut Runtime, id: CompositionId, mut data: CompositionData) -> Disposal {
        // It's its parent's no more; those under it compose on, by their owners.
        if let Some(place) = data.parent.take() {
            runtime.compositions.remove_child(place);
        }
        let Runtime { scopes, removal_stack, disposing, lent, .. } = runtime;
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
            id,
            applier: crate::context::resolve_applier(lent, &data.applier),
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
            // Below a node of its parent's, what it holds may have been released with that node.
            if self.top_level_nodes > 0 && applier.contains(self.root_node) {
                applier.remove(self.root_node, 0, self.top_level_nodes as usize);
            }
        }
        // Its data stays where a node's `on_detach` finds its providers, even if one panics.
        let readable = ReadableWhileDisposing(self.id);
        if let Some(data) = self.data.take() {
            with_runtime(|runtime| runtime.disposing_data.push((self.id, data)));
        }
        apply_deferred(&applier, &mut self.deferred, false);
        drop(applier);
        let Disposal { deferred, .. } = self;
        drop(deferred);
        drop(readable);
        drop(disposing);
        // What the context it was under kept, if that was what kept it.
        drop(with_runtime(|runtime| runtime.compositions.contexts.take_dropping()));
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
        crate::runtime::release_if_empty();
    }
}

/// A composition being disposed: what it holds dropped when it ends, outside the runtime's
/// borrow.
struct ReadableWhileDisposing(CompositionId);

impl Drop for ReadableWhileDisposing {
    fn drop(&mut self) {
        let data = try_with_runtime(|runtime| {
            let index = runtime.disposing_data.iter().position(|(id, _)| *id == self.0)?;
            Some(runtime.disposing_data.remove(index).1)
        });
        drop(data);
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
    let in_pass = data.composing;
    // Paused content's group: its content's dirty marks and invalid scopes wait for a resume,
    // and it counts the nodes it has in the tree, not those it will have once applied.
    let region_at = |group: u32| {
        (data.paused.as_ref())
            .filter(|paused| paused.holds_regions())
            .and_then(|paused| paused.region_rooted_at(group))
    };
    let mut reachable = 0usize;
    // Each group, whether it's in deactivated content (whose scopes were freed), and whether
    // it's in paused content (which may hold deactivated content's groups it reuses, their
    // scopes freed, where its paused scopes haven't reached yet).
    let mut stack = vec![(SENTINEL, false, false)];
    while let Some((index, deactivated, paused)) = stack.pop() {
        reachable += 1;
        let group = &table[index];
        let inner_deactivated = deactivated || group.deactivated();
        let inner_paused = paused || region_at(index).is_some();
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
            if !in_pass && entry.dirty && !group.dirty && region_at(index).is_none() {
                return Err(format!("group {child} is dirty but its parent {index} is not"));
            }
            stack.push((child, inner_deactivated, inner_paused));
            child = entry.next_sibling;
        }
        let expected = if group.kind == GroupKind::Node { 1 } else { nodes };
        if let Some(region) = region_at(index) {
            if group.node_count != region.attached || (!in_pass && region.nodes != nodes) {
                return Err(format!(
                    "paused content's group {index} counts {} nodes ({} attached), its children \
                     {nodes} ({} once applied)",
                    group.node_count, region.attached, region.nodes
                ));
            }
        } else if group.node_count != expected {
            return Err(format!(
                "group {index} counts {} nodes, its children {expected}",
                group.node_count
            ));
        }
        if group.kind == GroupKind::Node && group.node.is_none() {
            return Err(format!("node group {index} has no node"));
        }
        match group.kind {
            GroupKind::Restart if paused && group.scope.is_none() => {}
            GroupKind::Restart if deactivated => {
                if !group.scope.is_none() {
                    return Err(format!("restart group {index} keeps a scope while deactivated"));
                }
            }
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
                if !in_pass
                    && scope.invalid
                    && index != SENTINEL
                    && !table[group.parent].dirty
                    && region_at(group.parent).is_none()
                {
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
