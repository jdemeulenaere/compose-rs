//! `SubcomposeLayout`, as Compose's: a layout that composes parts of its content while it's measured,
//! each slot a composition under this place, its nodes under a virtual node of the layout's
//! (composition-context.md §3.4, D1309). Measuring holds the tree, so a slot's pass borrows it
//! (`with_lent_applier`), with the access window shut: what asks the tree from a slot's effect in that
//! pass finds it busy (D1211).

use std::any::Any;
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

use compose_runtime::platform::{Applier, NodeRef, with_lent_applier};
use compose_runtime::snapshots::Snapshot;
use compose_runtime::{
    Callback, Composition, CompositionContext, DisposableEffect, PausableComposition,
    PausedComposition, ShouldPauseCallback, SideEffect, composable, remember_composition_context,
};

use crate::constraints::Constraints;
use crate::layout::{
    IntrinsicMeasurable, IntrinsicMeasureScope, Measurable, MeasurePolicy, MeasureResult,
    MeasureScope,
};
use crate::node_chain::NodeKinds;
use crate::node_context::NodeContext;
use crate::traversal::{TraversableNode, TraverseDescendantsAction, TraverseKey};
use crate::tree::{NodeId, UiTree};
use crate::{IntSize, Modifier};

/// A slot's id: anything comparable, as Compose's `Any?`.
pub trait SlotId: Any {
    fn same_slot(&self, other: &dyn SlotId) -> bool;
    fn as_any(&self) -> &dyn Any;
}

impl<T: PartialEq + 'static> SlotId for T {
    fn same_slot(&self, other: &dyn SlotId) -> bool {
        other.as_any().downcast_ref::<T>().is_some_and(|other| self == other)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

type SlotKey = Rc<dyn SlotId>;

/// What a slot's id is once its layout's content has gone from under it: anything can use it, as
/// `ReusedSlotId`.
#[derive(PartialEq)]
struct ReusedSlotId;

fn is_reused(key: &SlotKey) -> bool {
    (**key).as_any().is::<ReusedSlotId>()
}

/// The slots a reuse policy's asked about, as `SlotIdsSet`, the last used first.
#[derive(Default)]
pub struct SlotIdsSet {
    ids: Vec<SlotKey>,
}

impl SlotIdsSet {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn clear(&mut self) {
        self.ids.clear();
    }

    /// The ids, in order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Any> {
        self.ids.iter().map(|id| (**id).as_any())
    }

    /// Keeps only those `keep` says, as `retainAll`.
    pub fn retain(&mut self, mut keep: impl FnMut(&dyn Any) -> bool) {
        self.ids.retain(|id| keep((**id).as_any()));
    }

    /// Leaves out those `remove` says, as `removeAll`.
    pub fn remove_all(&mut self, mut remove: impl FnMut(&dyn Any) -> bool) {
        self.ids.retain(|id| !remove((**id).as_any()));
    }

    /// Leaves out `id`, as `remove`: whether it was there.
    pub fn remove(&mut self, id: impl PartialEq + 'static) -> bool {
        let before = self.ids.len();
        self.ids.retain(|other| !other.same_slot(&id));
        self.ids.len() != before
    }

    /// Keeps the first `size`, as `trimToSize`.
    pub fn trim_to_size(&mut self, size: usize) {
        self.ids.truncate(size);
    }

    fn contains(&self, id: &dyn SlotId) -> bool {
        self.ids.iter().any(|other| other.same_slot(id))
    }

    /// In order, once each: those whose content went are one id.
    fn add(&mut self, id: SlotKey) {
        if !self.contains(&*id) {
            self.ids.push(id);
        }
    }
}

/// Which slots are kept for reuse, and what they can be reused for, as `SubcomposeSlotReusePolicy`.
pub trait SubcomposeSlotReusePolicy: 'static {
    /// Leaves in `slots` those to keep, the rest being disposed.
    fn get_slots_to_retain(&self, slots: &mut SlotIdsSet);
    /// Whether a composition kept for `reusable` can compose `slot`'s content.
    fn are_compatible(&self, slot: &dyn Any, reusable: &dyn Any) -> bool;
}

struct FixedCount(usize);

impl SubcomposeSlotReusePolicy for FixedCount {
    fn get_slots_to_retain(&self, slots: &mut SlotIdsSet) {
        slots.trim_to_size(self.0);
    }

    fn are_compatible(&self, _slot: &dyn Any, _reusable: &dyn Any) -> bool {
        true
    }
}

struct NoReuse;

impl SubcomposeSlotReusePolicy for NoReuse {
    fn get_slots_to_retain(&self, slots: &mut SlotIdsSet) {
        slots.clear();
    }

    fn are_compatible(&self, _slot: &dyn Any, _reusable: &dyn Any) -> bool {
        false
    }
}

/// A policy keeping up to `max_slots_to_retain_for_reuse`, each usable for any slot, as
/// `SubcomposeSlotReusePolicy(maxSlotsToRetainForReuse)`.
pub fn subcompose_slot_reuse_policy(
    max_slots_to_retain_for_reuse: usize,
) -> impl SubcomposeSlotReusePolicy {
    FixedCount(max_slots_to_retain_for_reuse)
}

/// A slot. In `Subcompositions::slots`, in the order of its virtual nodes: those of this measurement,
/// then those kept for reuse, then those precomposed.
struct Slot {
    node: NodeId,
    key: SlotKey,
    composition: Option<Rc<PausableComposition>>,
    /// A composition precomposed in pauses, not applied yet.
    paused: Option<Rc<PausedComposition>>,
    content: Option<Callback>,
    force_recompose: bool,
    force_reuse: bool,
    active: bool,
    /// Its first layout as its composition's passes left it, and their count then.
    first: Option<(u64, Option<Measurable>)>,
}

#[derive(Default)]
struct Subcompositions {
    root: Option<NodeRef>,
    context: Option<CompositionContext>,
    /// A composition of the layout's tree, composing nothing: what the tree's reached through.
    handle: Option<Rc<Composition>>,
    slots: Vec<Slot>,
    current_index: usize,
    reusable_count: usize,
    precomposed_count: usize,
    /// Precomposed slots let go while the tree was busy (a layout measuring): let go at the
    /// layout's next measurement, which has the tree.
    deferred: Vec<SlotKey>,
}

thread_local! {
    /// The one policy of a state keeping nothing, as `NoOpSubcomposeSlotReusePolicy`: a layout
    /// given a new such state has the same policy.
    static NO_REUSE: Rc<dyn SubcomposeSlotReusePolicy> = Rc::new(NoReuse);
}

/// What's given to a `SubcomposeLayout`, as `SubcomposeLayoutState`: the policy for what's kept for
/// reuse, and the way to precompose ahead of a measurement. Its slots are the layout's, whichever
/// state it's given next. Shared by `Rc`, so a test or a lazy list holds one.
pub struct SubcomposeLayoutState {
    policy: Rc<dyn SubcomposeSlotReusePolicy>,
    layout: RefCell<Option<Rc<SubcompositionsState>>>,
}

impl SubcomposeLayoutState {
    /// With no slots kept for reuse, as `SubcomposeLayoutState()`.
    pub fn new() -> Rc<SubcomposeLayoutState> {
        Rc::new(SubcomposeLayoutState {
            policy: NO_REUSE.with(Rc::clone),
            layout: RefCell::new(None),
        })
    }

    pub fn with_policy(policy: impl SubcomposeSlotReusePolicy) -> Rc<SubcomposeLayoutState> {
        Rc::new(SubcomposeLayoutState { policy: Rc::new(policy), layout: RefCell::new(None) })
    }

    fn layout(&self) -> Option<Rc<SubcompositionsState>> {
        self.layout.borrow().clone()
    }

    /// `content` under `slot_id`, ahead of the measurement that will ask for it, as `precompose`.
    /// A slot already composed in a measurement stays as it is, and the handle has nothing of it;
    /// so with no layout there.
    pub fn precompose(
        &self,
        slot_id: impl PartialEq + 'static,
        content: Callback,
    ) -> PrecomposedSlotHandle {
        let key: SlotKey = Rc::new(slot_id);
        let slot = self.layout().filter(|layout| layout.precompose(&key, content, false));
        PrecomposedSlotHandle { slot: slot.map(|layout| (layout, key)) }
    }

    /// [`precompose`](Self::precompose) in steps, as `createPausedPrecomposition`: nothing's
    /// composed before its first [`resume`](PausedPrecomposition::resume), and nothing's applied
    /// before its [`apply`](PausedPrecomposition::apply), or the measurement taking the slot.
    pub fn create_paused_precomposition(
        &self,
        slot_id: impl PartialEq + 'static,
        content: Callback,
    ) -> PausedPrecomposition {
        let key: SlotKey = Rc::new(slot_id);
        let slot = self.layout().filter(|layout| layout.precompose(&key, content, true));
        PausedPrecomposition { slot: slot.map(|layout| (layout, key)) }
    }

    /// How many are kept for reuse, for tests and tools.
    pub fn reusable_count(&self) -> usize {
        self.layout().map_or(0, |layout| layout.inner.borrow().reusable_count)
    }
}

/// A layout's slots, as `LayoutNodeSubcompositionsState`: its own, as its node's, past a new state
/// and past reuse.
struct SubcompositionsState {
    policy: RefCell<Rc<dyn SubcomposeSlotReusePolicy>>,
    inner: RefCell<Subcompositions>,
}

/// What the layout's group keeps: gone, the layout's gone, and its slots with it, as `onRelease`.
/// Not an effect, as deactivating the layout forgets those and leaves it this (D1309).
struct LayoutKeeper(Rc<SubcompositionsState>);

impl Drop for LayoutKeeper {
    fn drop(&mut self) {
        self.0.dispose_current_nodes();
    }
}

impl SubcompositionsState {
    fn new(policy: Rc<dyn SubcomposeSlotReusePolicy>) -> SubcompositionsState {
        SubcompositionsState { policy: RefCell::new(policy), inner: RefCell::default() }
    }

    fn set_place(&self, context: CompositionContext, root: NodeRef) {
        let mut inner = self.inner.borrow_mut();
        inner.context = Some(context);
        inner.root = Some(root);
        if inner.handle.is_none() {
            inner.handle = Some(Rc::new(compose_runtime::platform::subcomposition(context, root)));
        }
    }

    /// The body of the layout ran: each slot composes anew at its next measurement, as what it's
    /// under may have changed in ways no state tells (`forceRecomposeChildren`).
    fn force_recompose_children(&self) {
        let (handle, root) = {
            let mut inner = self.inner.borrow_mut();
            for slot in &mut inner.slots {
                slot.force_recompose = true;
            }
            if inner.slots.is_empty() {
                return;
            }
            (inner.handle.clone(), inner.root)
        };
        if let (Some(handle), Some(root)) = (handle, root) {
            handle.with_applier(|tree: &mut UiTree| {
                let root = tree.id_of(root);
                tree.invalidate_measure(root);
            });
        }
    }

    /// A new policy: what's composed is out of use, for the next measurement to take or to give to
    /// the policy, as `slotReusePolicy`'s setter.
    fn set_policy(&self, policy: &Rc<dyn SubcomposeSlotReusePolicy>) {
        if Rc::ptr_eq(&self.policy.borrow(), policy) {
            return;
        }
        *self.policy.borrow_mut() = policy.clone();
        self.mark_active_as_reused();
        let (handle, root) = {
            let inner = self.inner.borrow();
            (inner.handle.clone(), inner.root)
        };
        if let (Some(handle), Some(root)) = (handle, root) {
            handle.with_applier(|tree: &mut UiTree| {
                let root = tree.id_of(root);
                tree.invalidate_measure(root);
            });
        }
    }

    /// The layout's group forgot what it remembers: reused or deactivated, its node stays and all
    /// its slots are out of use, as `onReuse` and `onDeactivate`; removed (its node gone first, as
    /// rule R1 releases nodes before remembered values), its slots are disposed now, as its
    /// `onRelease` among the forgotten in reverse order: a nested layout's before the effects
    /// remembered ahead of it in the slot holding it (D2394). What's kept ([`LayoutKeeper`])
    /// finds nothing left then.
    fn on_layout_forgotten(&self) {
        let (handle, root) = {
            let inner = self.inner.borrow();
            let (Some(handle), Some(root)) = (inner.handle.clone(), inner.root) else { return };
            (handle, root)
        };
        if handle.is_disposed() {
            return;
        }
        if handle.with_applier(|tree: &mut UiTree| tree.contains(root)) {
            self.mark_active_as_reused();
        } else {
            self.dispose_current_nodes();
        }
    }

    /// Every slot goes out of use, deactivated, to be taken by any next measurement:
    /// `markActiveNodesAsReused`.
    fn mark_active_as_reused(&self) {
        let (compositions, handle, root) = {
            let mut inner = self.inner.borrow_mut();
            inner.precomposed_count = 0;
            let (Some(handle), Some(root)) = (inner.handle.clone(), inner.root) else { return };
            if inner.reusable_count == inner.slots.len() {
                return;
            }
            inner.reusable_count = inner.slots.len();
            let mut compositions = Vec::new();
            for slot in &mut inner.slots {
                if std::mem::replace(&mut slot.active, false) {
                    slot.key = Rc::new(ReusedSlotId);
                    slot.force_reuse = true;
                    compositions.push(OutOfUse::of(slot));
                }
            }
            (compositions, handle, root)
        };
        with_the_tree(&handle, root, None, || {
            for composition in compositions {
                composition.run();
            }
        });
    }

    /// Its slots' compositions go with it, as its `onRelease`.
    fn dispose_current_nodes(&self) {
        let (slots, handle) = {
            let mut inner = self.inner.borrow_mut();
            inner.current_index = 0;
            inner.reusable_count = 0;
            inner.precomposed_count = 0;
            inner.root = None;
            (std::mem::take(&mut inner.slots), inner.handle.take())
        };
        for slot in slots {
            if let Some(composition) = slot.composition {
                composition.dispose();
            }
        }
        if let Some(handle) = handle {
            handle.dispose();
        }
    }

    fn slot_index(&self, key: &dyn SlotId, range: std::ops::Range<usize>) -> Option<usize> {
        let inner = self.inner.borrow();
        range.into_iter().find(|at| inner.slots[*at].key.same_slot(key))
    }

    /// The slot of `key` among those kept for reuse, or one compatible, moved to the front of
    /// those, out of them: `takeNodeFromReusables`. Returns where it is now.
    fn take_from_reusables(&self, key: &SlotKey, tree: &mut UiTree) -> Option<usize> {
        let policy = self.policy.borrow().clone();
        let mut inner = self.inner.borrow_mut();
        if inner.reusable_count == 0 {
            return None;
        }
        let end = inner.slots.len() - inner.precomposed_count;
        let start = end - inner.reusable_count;
        let chosen =
            (start..end).rev().find(|at| inner.slots[*at].key.same_slot(&**key)).or_else(|| {
                (start..end).rev().find(|at| {
                    let reusable = &inner.slots[*at].key;
                    is_reused(reusable)
                        || policy.are_compatible((**key).as_any(), (**reusable).as_any())
                })
            })?;
        let root = tree.id_of(inner.root.expect("it's measured or precomposed under a place"));
        move_slot(&mut inner, tree, root, chosen, start);
        inner.reusable_count -= 1;
        let slot = &mut inner.slots[start];
        slot.key = key.clone();
        slot.active = true;
        slot.force_reuse = true;
        slot.force_recompose = true;
        Some(start)
    }

    /// `at`'s composition composes `content` if it has to, in a paused composition if `pausable`.
    /// `lend` is the tree a measurement holds. As `subcompose(node, slotId, pausable, content)`.
    fn compose_slot(
        &self,
        at: usize,
        content: Callback,
        mut lend: Option<&mut UiTree>,
        pausable: bool,
    ) {
        let (same, paused) = {
            let inner = self.inner.borrow();
            let slot = &inner.slots[at];
            let same = slot.content.as_ref().is_some_and(|last| last.ptr_eq(&content));
            (same, slot.paused.is_some())
        };
        if paused {
            if !same {
                // What's there can't be applied as this content.
                self.cancel_paused(at, lend.as_deref_mut());
            } else if pausable {
                return;
            } else {
                self.apply_paused(at, true, lend.as_deref_mut());
            }
        }
        let (composition, reuse, handle, root) = {
            let mut inner = self.inner.borrow_mut();
            let context = inner.context.expect("under a place");
            let handle = inner.handle.clone().expect("under a place");
            let root = inner.root.expect("under a place");
            let slot = &mut inner.slots[at];
            let pending = slot.composition.as_ref().is_none_or(|it| it.has_invalidations());
            if same && !pending && !slot.force_recompose {
                return;
            }
            slot.content = Some(content.clone());
            slot.force_recompose = false;
            let reuse = std::mem::take(&mut slot.force_reuse);
            let node = slot.node.to_ref();
            let composition = slot
                .composition
                .get_or_insert_with(|| {
                    Rc::new(compose_runtime::platform::pausable_subcomposition(context, node))
                })
                .clone();
            (composition, reuse, handle, root)
        };
        let pass = move || match (pausable, reuse) {
            (true, true) => Some(
                composition.set_pausable_content_with_reuse(move || SlotContent(content.clone())),
            ),
            (true, false) => {
                Some(composition.set_pausable_content(move || SlotContent(content.clone())))
            }
            (false, true) => {
                composition.set_content_with_reuse(move || SlotContent(content.clone()));
                None
            }
            (false, false) => {
                composition.set_content(move || SlotContent(content.clone()));
                None
            }
        };
        // Setting pausable content composes nothing, so `at` is still where it was.
        if let Some(paused) = with_the_tree(&handle, root, lend, pass) {
            self.inner.borrow_mut().slots[at].paused = Some(Rc::new(paused));
        }
    }

    /// `applyPausedPrecomposition`: with `complete`, what's left of it is composed first.
    fn apply_paused(&self, at: usize, complete: bool, lend: Option<&mut UiTree>) {
        let (paused, handle, root) = {
            let inner = self.inner.borrow();
            let Some(paused) = inner.slots[at].paused.clone() else { return };
            (paused, inner.handle.clone().expect("it's precomposed"), inner.root.expect("too"))
        };
        with_the_tree(&handle, root, lend, || {
            if complete {
                while !paused.is_complete() {
                    paused.resume(|| false);
                }
            }
            paused.apply();
        });
        self.inner.borrow_mut().slots[at].paused = None;
    }

    /// `cancelPausedPrecomposition`: it goes with its composition, the next one being new.
    fn cancel_paused(&self, at: usize, lend: Option<&mut UiTree>) {
        let (composition, handle, root) = {
            let mut inner = self.inner.borrow_mut();
            let handle = inner.handle.clone().expect("it's precomposed");
            let root = inner.root.expect("it's precomposed");
            let slot = &mut inner.slots[at];
            (OutOfUse::of(slot), handle, root)
        };
        with_the_tree(&handle, root, lend, || composition.run());
    }

    /// From `start` on: those the policy retains are kept for reuse, deactivated, and the others
    /// disposed: `disposeOrReuseStartingFromIndex`.
    fn dispose_or_reuse_from(&self, start: usize, tree: Option<&mut UiTree>) {
        let policy = self.policy.borrow().clone();
        let mut set = SlotIdsSet::default();
        let (last, handle) = {
            let mut inner = self.inner.borrow_mut();
            inner.reusable_count = 0;
            let end = inner.slots.len() - inner.precomposed_count;
            if start >= end {
                return;
            }
            for at in start..end {
                set.add(inner.slots[at].key.clone());
            }
            (end, inner.handle.clone().expect("under a place"))
        };
        policy.get_slots_to_retain(&mut set);
        let root = self.inner.borrow().root.expect("under a place");
        let mut gone = Vec::new();
        let leave = || {
            for at in (start..last).rev() {
                let keep = {
                    let inner = self.inner.borrow();
                    set.contains(&*inner.slots[at].key)
                };
                if keep {
                    let composition = {
                        let mut inner = self.inner.borrow_mut();
                        inner.reusable_count += 1;
                        let slot = &mut inner.slots[at];
                        let was_active = std::mem::replace(&mut slot.active, false);
                        slot.force_reuse = true;
                        was_active.then(|| OutOfUse::of(slot))
                    };
                    if let Some(composition) = composition {
                        composition.run();
                    }
                } else {
                    let slot = self.inner.borrow_mut().slots.remove(at);
                    if let Some(composition) = &slot.composition {
                        composition.dispose();
                    }
                    gone.push(slot.node);
                }
            }
        };
        // Their virtual nodes go once their compositions have, and the tree's back in hand.
        match tree {
            Some(tree) => {
                with_the_tree(&handle, root, Some(&mut *tree), leave);
                let host = tree.id_of(root);
                let ignoring = tree.ignoring_remeasure.replace(host);
                tree.remove_virtual_nodes(host, &gone);
                tree.ignoring_remeasure = ignoring;
            }
            None => {
                with_the_tree(&handle, root, None, leave);
                with_the_tree(&handle, root, None, || {
                    handle.with_applier(|tree: &mut UiTree| {
                        let host = tree.id_of(root);
                        tree.remove_virtual_nodes(host, &gone);
                    });
                });
            }
        }
    }

    /// `key`'s slot composing `content`, unless a measurement holds it already or there's no layout
    /// any more: then it says so.
    fn precompose(self: &Rc<Self>, key: &SlotKey, content: Callback, pausable: bool) -> bool {
        let (handle, root, end) = {
            let inner = self.inner.borrow();
            let Some(handle) = inner.handle.clone() else { return false };
            let end = inner.slots.len();
            (handle, inner.root.expect("with its handle"), end)
        };
        let active_end = {
            let inner = self.inner.borrow();
            end - inner.precomposed_count - inner.reusable_count
        };
        if self.slot_index(&**key, 0..active_end).is_some() {
            return false;
        }
        let precomposed = end - self.inner.borrow().precomposed_count;
        let at = match self.slot_index(&**key, precomposed..end) {
            Some(at) => at,
            None => handle.with_applier(|tree: &mut UiTree| {
                let host = tree.id_of(root);
                let ignoring = tree.ignoring_remeasure.replace(host);
                let at = match self.take_from_reusables(key, tree) {
                    Some(taken) => {
                        let mut inner = self.inner.borrow_mut();
                        let last = inner.slots.len() - 1;
                        move_slot(&mut inner, tree, host, taken, last);
                        last
                    }
                    None => {
                        let mut inner = self.inner.borrow_mut();
                        let at = inner.slots.len();
                        add_slot(&mut inner, tree, host, key.clone(), at);
                        at
                    }
                };
                tree.ignoring_remeasure = ignoring;
                self.inner.borrow_mut().precomposed_count += 1;
                at
            }),
        };
        self.compose_slot(at, content, None, pausable);
        true
    }

    /// A slot precomposed goes out of use, for the policy to keep or dispose, as
    /// `disposePrecomposedSlot`. With the tree busy (a layout measuring, which may be this one's),
    /// it waits for this layout's next measurement.
    fn dispose_precomposed(&self, key: &SlotKey) {
        if self.precomposed_at(key).is_none() {
            return;
        }
        let Some(handle) = self.inner.borrow().handle.clone() else { return };
        if handle.try_with_applier(|_: &mut UiTree| ()).is_none() {
            let mut inner = self.inner.borrow_mut();
            if !inner.deferred.iter().any(|other| other.same_slot(&**key)) {
                inner.deferred.push(key.clone());
            }
            return;
        }
        self.dispose_precomposed_with(key, None);
    }

    /// `dispose_precomposed`, through `tree` if a measurement holds it.
    fn dispose_precomposed_with(&self, key: &SlotKey, mut tree: Option<&mut UiTree>) {
        let Some(at) = self.precomposed_at(key) else { return };
        let (handle, root, start) = {
            let mut inner = self.inner.borrow_mut();
            inner.reusable_count += 1;
            inner.precomposed_count -= 1;
            let start = inner.slots.len() - inner.precomposed_count - inner.reusable_count;
            (inner.handle.clone().expect("it's precomposed"), inner.root.expect("too"), start)
        };
        if self.inner.borrow().slots[at].paused.is_some() {
            self.cancel_paused(at, tree.as_deref_mut());
        }
        match tree.as_deref_mut() {
            Some(tree) => {
                let host = tree.id_of(root);
                let mut inner = self.inner.borrow_mut();
                move_slot(&mut inner, tree, host, at, start);
            }
            None => with_the_tree(&handle, root, None, || {
                handle.with_applier(|tree: &mut UiTree| {
                    let host = tree.id_of(root);
                    let mut inner = self.inner.borrow_mut();
                    move_slot(&mut inner, tree, host, at, start);
                });
            }),
        }
        self.dispose_or_reuse_from(start, tree);
    }

    /// What was let go while the tree was busy, let go now that a measurement has it.
    fn dispose_deferred(&self, tree: &mut UiTree) {
        let deferred = std::mem::take(&mut self.inner.borrow_mut().deferred);
        for key in deferred {
            self.dispose_precomposed_with(&key, Some(&mut *tree));
        }
    }

    fn precomposed_at(&self, key: &SlotKey) -> Option<usize> {
        let end = self.inner.borrow().slots.len();
        let start = end - self.inner.borrow().precomposed_count;
        self.slot_index(&**key, start..end)
    }

    /// Its slots are its layout's virtual nodes, as `makeSureStateIsConsistent`.
    fn check_consistent(&self, tree: &UiTree) {
        if cfg!(debug_assertions) {
            let inner = self.inner.borrow();
            let root = tree.id_of(inner.root.expect("measured under a place"));
            let nodes = &tree.node(root).virtual_children;
            assert!(
                nodes.iter().eq(inner.slots.iter().map(|slot| &slot.node)),
                "compose-ui: a SubcomposeLayout's slots aren't its virtual nodes"
            );
        }
    }
}

/// A slot's content, as Compose's `ReusableContentHost(active, content)` around it: a skippable
/// call, so a pass the layout forces (its body ran, `force_recompose_children`) skips the content
/// when the same one has nothing invalid, and a scope inside it the recomposer already recomposed
/// isn't run again (D2393). A reused slot's pass reads its groups as new, so it doesn't skip.
#[composable]
#[allow(non_snake_case)]
fn SlotContent(content: Callback) {
    content();
}

/// What's done to a slot's composition as it goes out of use, once the state's not borrowed: as
/// `reuseComposition`, one still paused is cancelled and disposed, another deactivated.
enum OutOfUse {
    Nothing,
    Deactivate(Rc<PausableComposition>),
    Cancel(Rc<PausedComposition>, Option<Rc<PausableComposition>>),
}

impl OutOfUse {
    fn of(slot: &mut Slot) -> OutOfUse {
        match (slot.paused.take(), &slot.composition) {
            (Some(paused), _) => OutOfUse::Cancel(paused, slot.composition.take()),
            (None, Some(composition)) => OutOfUse::Deactivate(composition.clone()),
            (None, None) => OutOfUse::Nothing,
        }
    }

    fn run(self) {
        match self {
            OutOfUse::Nothing => {}
            OutOfUse::Deactivate(composition) => composition.deactivate(),
            OutOfUse::Cancel(paused, composition) => {
                paused.cancel();
                if let Some(composition) = composition {
                    composition.dispose();
                }
            }
        }
    }
}

/// `f` where a pass reaches the tree: lent by what holds it, or as it is, what it does to the
/// layout's children no reason to measure it again.
fn with_the_tree<R>(
    handle: &Rc<Composition>,
    root: NodeRef,
    tree: Option<&mut UiTree>,
    f: impl FnOnce() -> R,
) -> R {
    match tree {
        Some(tree) => {
            let host = tree.id_of(root);
            let ignoring = tree.ignoring_remeasure.replace(host);
            // Restored past a panic in the pass too: the layout has to hear what asks it next.
            let result = catch_unwind(AssertUnwindSafe(|| {
                Snapshot::without_read_observation(|| with_lent_applier(handle, tree, f))
            }));
            tree.ignoring_remeasure = ignoring;
            result.unwrap_or_else(|panic| resume_unwind(panic))
        }
        None => {
            let ignoring = handle.with_applier(|tree: &mut UiTree| {
                let host = tree.id_of(root);
                tree.ignoring_remeasure.replace(host)
            });
            let result = catch_unwind(AssertUnwindSafe(|| Snapshot::without_read_observation(f)));
            handle.with_applier(|tree: &mut UiTree| tree.ignoring_remeasure = ignoring);
            result.unwrap_or_else(|panic| resume_unwind(panic))
        }
    }
}

/// A new slot at `at`, which asks no measurement of `host`, as `createNodeAt`. Adding a virtual
/// node asks none today; the guard keeps it so if that changes.
fn add_slot(inner: &mut Subcompositions, tree: &mut UiTree, host: NodeId, key: SlotKey, at: usize) {
    let ignoring = tree.ignoring_remeasure.replace(host);
    let node = tree.add_virtual_node(host, at);
    tree.ignoring_remeasure = ignoring;
    inner.slots.insert(
        at,
        Slot {
            node,
            key,
            composition: None,
            paused: None,
            content: None,
            force_recompose: false,
            force_reuse: false,
            active: true,
            first: None,
        },
    );
}

/// A slot moved, which asks no measurement of `host`, as `move`.
fn move_slot(inner: &mut Subcompositions, tree: &mut UiTree, host: NodeId, from: usize, to: usize) {
    if from == to {
        return;
    }
    let slot = inner.slots.remove(from);
    inner.slots.insert(to, slot);
    let ignoring = tree.ignoring_remeasure.replace(host);
    tree.move_virtual_node(host, from, to);
    tree.ignoring_remeasure = ignoring;
}

/// A slot being precomposed in steps, as `PausedPrecomposition`.
pub struct PausedPrecomposition {
    slot: Option<(Rc<SubcompositionsState>, SlotKey)>,
}

impl PausedPrecomposition {
    /// Its slot's place, and what's there, while it's waiting to be applied.
    fn paused(&self) -> Option<(usize, Rc<PausedComposition>)> {
        let (state, key) = self.slot.as_ref()?;
        let at = state.precomposed_at(key)?;
        let paused = state.inner.borrow().slots[at].paused.clone()?;
        Some((at, paused))
    }

    /// Whether what's left is only to [`apply`](Self::apply) it, as `isComplete`: what it read
    /// having changed, it's to be resumed first. Nothing's left once it's applied, cancelled or
    /// its slot composed over.
    pub fn is_complete(&self) -> bool {
        self.paused().is_none_or(|(_, paused)| paused.is_complete())
    }

    /// Composes what's left, pausing where `should_pause` says, as `resume`: whether it's complete.
    pub fn resume(&self, should_pause: impl ShouldPauseCallback + 'static) -> bool {
        let Some((_, paused)) = self.paused().filter(|(_, paused)| !paused.is_complete()) else {
            return true;
        };
        let (state, _) = self.slot.as_ref().expect("it's paused");
        let (handle, root) = {
            let inner = state.inner.borrow();
            (inner.handle.clone().expect("it's precomposed"), inner.root.expect("too"))
        };
        with_the_tree(&handle, root, None, || paused.resume(should_pause))
    }

    /// Applies it, as `apply`, for its slot to be used.
    ///
    /// # Panics
    ///
    /// If it isn't [complete](Self::is_complete), as Compose's.
    pub fn apply(&self) -> PrecomposedSlotHandle {
        if let Some((at, _)) = self.paused() {
            let (state, _) = self.slot.as_ref().expect("it's paused");
            state.apply_paused(at, false, None);
        }
        PrecomposedSlotHandle { slot: self.slot.clone() }
    }

    /// Done with it: its slot's out of use, if it's still waiting to be applied, as `cancel`.
    pub fn cancel(&self) {
        if self.paused().is_some() {
            let (state, key) = self.slot.as_ref().expect("it's paused");
            state.dispose_precomposed(key);
        }
    }
}

/// What can be done with a slot precomposed, as `PrecomposedSlotHandle`.
pub struct PrecomposedSlotHandle {
    slot: Option<(Rc<SubcompositionsState>, SlotKey)>,
}

impl PrecomposedSlotHandle {
    /// Done with it: it's kept for reuse or disposed, as the policy says, unless a measurement's
    /// taken it.
    pub fn dispose(&self) {
        if let Some((state, key)) = &self.slot {
            state.dispose_precomposed(key);
        }
    }

    fn node(&self) -> Option<(Rc<Composition>, NodeId)> {
        let (state, key) = self.slot.as_ref()?;
        let at = state.precomposed_at(key)?;
        let inner = state.inner.borrow();
        Some((inner.handle.clone()?, inner.slots[at].node))
    }

    /// Calls `f` with each traversable node under the slot with `key`, as `traverseDescendants`:
    /// what it returns says how to go on. A slot it doesn't hold has none.
    pub fn traverse_descendants(
        &self,
        key: &'static TraverseKey,
        mut f: impl FnMut(&mut dyn TraversableNode, &mut NodeContext<'_>) -> TraverseDescendantsAction,
    ) {
        let Some((handle, node)) = self.node() else { return };
        handle.with_applier(|tree: &mut UiTree| {
            let branches = tree.node(node).children.iter().rev().map(|c| (c.node, 0)).collect();
            tree.walk_subtree_if(
                branches,
                NodeKinds::TRAVERSABLE,
                false,
                &mut |visited, context| match visited.as_traversable() {
                    Some(found) if found.traverse_key() == key => f(found, context),
                    _ => TraverseDescendantsAction::ContinueTraversal,
                },
            );
        });
    }

    /// How many layouts it emitted, as `placeablesCount`.
    pub fn placeables_count(&self) -> usize {
        let Some((handle, node)) = self.node() else { return 0 };
        handle.with_applier(|tree: &mut UiTree| tree.node(node).children.len())
    }

    /// Measures the layout at `index`, as `premeasure`: the measurement that takes the slot finds
    /// it measured with these constraints.
    ///
    /// # Panics
    ///
    /// With an `index` it doesn't have, as Compose's.
    pub fn premeasure(&self, index: usize, constraints: Constraints) {
        let (handle, node) = self.node().expect("compose-ui: premeasure of a slot it doesn't hold");
        handle.with_applier(|tree: &mut UiTree| {
            let count = tree.node(node).children.len();
            assert!(index < count, "Index ({index}) is out of bound of [0, {count})");
            tree.premeasure(node, index, constraints);
        });
    }

    /// The size of the layout at `index`, as `getSize`.
    pub fn get_size(&self, index: usize) -> IntSize {
        let Some((handle, node)) = self.node() else { return IntSize::new(0, 0) };
        handle.with_applier(|tree: &mut UiTree| {
            let count = tree.node(node).children.len();
            assert!(index < count, "Index ({index}) is out of bound of [0, {count})");
            let child = tree.node(node).children[index].node;
            tree.node(child).layers[0].size
        })
    }
}

/// What a `SubcomposeLayout`'s policy measures in: a [`MeasureScope`] that also composes.
pub struct SubcomposeMeasureScope<'s, 'a> {
    scope: &'s mut MeasureScope<'a>,
    state: &'s Rc<SubcompositionsState>,
}

impl<'a> std::ops::Deref for SubcomposeMeasureScope<'_, 'a> {
    type Target = MeasureScope<'a>;

    fn deref(&self) -> &MeasureScope<'a> {
        self.scope
    }
}

impl<'a> std::ops::DerefMut for SubcomposeMeasureScope<'_, 'a> {
    fn deref_mut(&mut self) -> &mut MeasureScope<'a> {
        self.scope
    }
}

impl SubcomposeMeasureScope<'_, '_> {
    /// `content` under `slot_id` in this measurement, composed if it's new or has changed, as
    /// `subcompose`: its layouts, to measure.
    ///
    /// # Panics
    ///
    /// For a slot already composed in this measurement, as Compose's.
    pub fn subcompose(
        &mut self,
        slot_id: impl PartialEq + 'static,
        content: Callback,
    ) -> Vec<Measurable> {
        let node = self.compose_into_slot(slot_id, content);
        let tree = self.scope.base.tree.enter();
        tree.virtual_measurables(node)
    }

    /// [`subcompose`](Self::subcompose), for a slot of one layout: its first, with no list made
    /// (a lazy layout's item, per item per measurement).
    pub fn subcompose_first(
        &mut self,
        slot_id: impl PartialEq + 'static,
        content: Callback,
    ) -> Option<Measurable> {
        let node = self.compose_into_slot(slot_id, content);
        let tree = self.scope.base.tree.enter();
        tree.first_virtual_measurable(node)
    }

    /// [`subcompose_first`](Self::subcompose_first) for a caller whose content for an id is always
    /// the same in effect (a lazy layout's item, its content by key): a slot already composed at
    /// this measurement's next place, equal to `probe`, with nothing to compose, is taken without
    /// asking for its id or content; otherwise `make` gives them.
    pub fn subcompose_first_with<T: PartialEq + 'static>(
        &mut self,
        probe: &T,
        make: impl FnOnce() -> (T, Callback),
    ) -> Option<Measurable> {
        let (node, at, passes) = match self.take_in_place(probe, None) {
            Some(InPlace { first: Some(first), .. }) => return first,
            Some(InPlace { node, at, passes, .. }) => (node, at, Some(passes)),
            None => {
                let (slot_id, content) = make();
                let node = self.compose_into_slot(slot_id, content);
                (node, self.state.inner.borrow().current_index - 1, None)
            }
        };
        let first = {
            let tree = self.scope.base.tree.enter();
            tree.first_virtual_measurable(node)
        };
        // Kept while its composition's passes stay the same: only they change its nodes.
        let composition = self.state.inner.borrow().slots[at].composition.clone();
        let passes = passes.or_else(|| composition.map(|it| it.measure_state().1));
        self.state.inner.borrow_mut().slots[at].first = passes.map(|passes| (passes, first));
        first
    }

    /// The slot at this measurement's next place, if it's `slot_id`'s and nothing's to compose
    /// in it (its content `content`, if given): taken as it is.
    fn take_in_place(
        &mut self,
        slot_id: &dyn SlotId,
        content: Option<&Callback>,
    ) -> Option<InPlace> {
        let (composition, node, at, cached) = {
            let inner = self.state.inner.borrow();
            let current = inner.current_index;
            let active_end = inner.slots.len() - inner.precomposed_count - inner.reusable_count;
            let slot = inner.slots.get(current).filter(|_| current < active_end)?;
            let same = !slot.force_recompose
                && slot.paused.is_none()
                && slot
                    .content
                    .as_ref()
                    .is_some_and(|last| content.is_none_or(|it| last.ptr_eq(it)))
                && slot.key.same_slot(slot_id);
            if !same {
                return None;
            }
            (slot.composition.clone()?, slot.node, current, slot.first)
        };
        // The changes of the turn were sent before the layout: what's invalid is known.
        let (invalid, passes) = composition.measure_state();
        if invalid {
            return None;
        }
        self.state.inner.borrow_mut().current_index += 1;
        let first = cached.filter(|(at, _)| *at == passes).map(|(_, first)| first);
        Some(InPlace { node, at, passes, first })
    }

    /// The slot of `slot_id` at this measurement's next place, its content composed if it has to
    /// be: its virtual node.
    fn compose_into_slot(
        &mut self,
        slot_id: impl PartialEq + 'static,
        content: Callback,
    ) -> NodeId {
        // The common case, a measurement asking for what the last one did in the same order: the
        // slot at `current`, its content the same and nothing to compose, is taken as it is.
        if let Some(InPlace { node, .. }) = self.take_in_place(&slot_id, Some(&content)) {
            return node;
        }
        let state = self.state.clone();
        let current = state.inner.borrow().current_index;
        let mut tree = self.scope.base.tree.enter();
        let tree: &mut UiTree = &mut tree;
        let root = {
            let inner = state.inner.borrow();
            tree.id_of(inner.root.expect("measured under its place"))
        };
        let (active_end, end) = {
            let inner = state.inner.borrow();
            let end = inner.slots.len();
            (end - inner.precomposed_count - inner.reusable_count, end)
        };
        // Where it was last, usually at `current`, its id compared in place: no allocation.
        let id: &dyn SlotId = &slot_id;
        let found = state.slot_index(id, current..active_end).or_else(|| {
            let precomposed = end - state.inner.borrow().precomposed_count;
            let at = state.slot_index(id, precomposed..end)?;
            state.inner.borrow_mut().precomposed_count -= 1;
            Some(at)
        });
        let at = match found {
            Some(at) => at,
            None => {
                // Not ahead: composed already in this measurement, which Compose forbids, or new.
                assert!(
                    state.slot_index(id, 0..current).is_none(),
                    "Error: currently it is not supported to subcompose the same slot id twice in \
                     a measurement"
                );
                let key: SlotKey = Rc::new(slot_id);
                match state.take_from_reusables(&key, tree) {
                    Some(at) => at,
                    None => {
                        let mut inner = state.inner.borrow_mut();
                        add_slot(&mut inner, tree, root, key, current);
                        current
                    }
                }
            }
        };
        {
            let mut inner = state.inner.borrow_mut();
            move_slot(&mut inner, tree, root, at, current);
            inner.current_index = current + 1;
        }
        state.compose_slot(current, content, Some(tree), false);
        state.inner.borrow().slots[current].node
    }
}

/// A slot taken in place: its virtual node, its place, its composition's passes, and its first
/// layout if that's known for them.
struct InPlace {
    node: NodeId,
    at: usize,
    passes: u64,
    first: Option<Option<Measurable>>,
}

/// Measures a `SubcomposeLayout`, as a [`MeasurePolicy`] does a `Layout`.
pub trait SubcomposeMeasurePolicy: 'static {
    fn measure(
        &self,
        scope: &mut SubcomposeMeasureScope<'_, '_>,
        constraints: Constraints,
    ) -> MeasureResult;
}

impl<F> SubcomposeMeasurePolicy for F
where
    F: Fn(&mut SubcomposeMeasureScope<'_, '_>, Constraints) -> MeasureResult + 'static,
{
    fn measure(
        &self,
        scope: &mut SubcomposeMeasureScope<'_, '_>,
        constraints: Constraints,
    ) -> MeasureResult {
        self(scope, constraints)
    }
}

/// A closure as a policy, its signature inferred.
pub fn subcompose_policy<F>(policy: F) -> Rc<dyn SubcomposeMeasurePolicy>
where
    F: Fn(&mut SubcomposeMeasureScope<'_, '_>, Constraints) -> MeasureResult + 'static,
{
    Rc::new(policy)
}

struct Adapter {
    state: Rc<SubcompositionsState>,
    policy: Rc<dyn SubcomposeMeasurePolicy>,
}

impl MeasurePolicy for Adapter {
    fn measure(
        &self,
        scope: &mut MeasureScope<'_>,
        _measurables: &[Measurable],
        constraints: Constraints,
    ) -> MeasureResult {
        self.state.inner.borrow_mut().current_index = 0;
        let result = {
            let mut scope = SubcomposeMeasureScope { scope: &mut *scope, state: &self.state };
            self.policy.measure(&mut scope, constraints)
        };
        let start = self.state.inner.borrow().current_index;
        let mut tree = scope.base.tree.enter();
        self.state.dispose_or_reuse_from(start, Some(&mut tree));
        self.state.dispose_deferred(&mut tree);
        self.state.check_consistent(&tree);
        result
    }

    fn same(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<Adapter>().is_some_and(|other| {
            Rc::ptr_eq(&self.state, &other.state) && Rc::ptr_eq(&self.policy, &other.policy)
        })
    }

    fn min_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", crate::layout::NO_INTRINSICS_MESSAGE)
    }

    fn min_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", crate::layout::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_width(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", crate::layout::NO_INTRINSICS_MESSAGE)
    }

    fn max_intrinsic_height(
        &self,
        _: &mut IntrinsicMeasureScope<'_>,
        _: &[IntrinsicMeasurable],
        _: i32,
    ) -> i32 {
        panic!("{}", crate::layout::NO_INTRINSICS_MESSAGE)
    }

    fn debug_name(&self) -> &'static str {
        "SubcomposeLayout"
    }
}

/// A layout that composes parts of its content while it's measured, as `SubcomposeLayout`, with
/// state of its own, keeping nothing for reuse.
#[composable]
#[allow(non_snake_case)]
pub fn SubcomposeLayout(
    #[default(Modifier)] modifier: Modifier,
    measure_policy: Rc<dyn SubcomposeMeasurePolicy>,
) {
    let state = compose_runtime::remember(SubcomposeLayoutState::new);
    SubcomposeLayoutWithState!(state, modifier = modifier, measure_policy = measure_policy);
}

/// [`SubcomposeLayout`] with `state`, as `SubcomposeLayout(state, …)`.
#[composable]
#[allow(non_snake_case)]
pub fn SubcomposeLayoutWithState(
    state: Rc<SubcomposeLayoutState>,
    #[default(Modifier)] modifier: Modifier,
    measure_policy: Rc<dyn SubcomposeMeasurePolicy>,
) {
    let context = remember_composition_context();
    let keeper = compose_runtime::platform::remember_across_reuse(|| {
        Rc::new(LayoutKeeper(Rc::new(SubcompositionsState::new(state.policy.clone()))))
    });
    let layout = keeper.0.clone();
    let adapter: Rc<dyn MeasurePolicy> =
        Rc::new(Adapter { state: layout.clone(), policy: measure_policy });
    let node = crate::layout::emit_layout_node(modifier, adapter, || {});
    compose_runtime::platform::current().with_applier(|tree: &mut UiTree| {
        let id = tree.id_of(node);
        tree.node_mut(id).composes_while_measured = true;
    });
    layout.set_place(context, node);
    *state.layout.borrow_mut() = Some(layout.clone());
    let told = layout.clone();
    SideEffect(move || {
        told.set_policy(&state.policy);
        told.force_recompose_children();
    });
    DisposableEffect((), move |scope| scope.on_dispose(move || layout.on_layout_forgotten()));
}
