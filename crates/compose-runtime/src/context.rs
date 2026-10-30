//! Composition contexts, as Compose's `CompositionContext`: what a composition composes under,
//! the Recomposer or a place in another composition, whose locals and composite key hash it
//! continues, whose frames it's recomposed in, parents first (D1303).
//!
//! The place is an entry of `Compositions::contexts`, naming the group of the
//! `remember_composition_context` call. The group's first slot holds what ends the entry, kept on
//! reuse and deactivation: the group going (its slot emptied at once) is what a frame's walk looks
//! at, so a removal costs nothing here, and the slot's value dropping, at the end of that pass, is
//! what's forgotten.

use std::cell::RefCell;
use std::rc::Rc;

use crate::__private::ReplaceGroup;
use crate::applier::{Applier, ErasedApplier, NodeRef};
use crate::composition::{Composition, PausableComposition, ReusableComposition};
use crate::error::{ComposeError, misuse};
use crate::hash::key_of;
use crate::recomposer::{CancellationHandle, Recomposer};
use crate::remember::remember_across_reuse;
use crate::runtime::{
    CompositionData, CompositionId, Compositions, try_with_runtime, with_runtime,
};
use crate::slot_table::{GroupKind, NONE, SlotTable, SlotValue};

/// The key of the group of `remember_composition_context`, as Compose's `referenceKey`.
const CONTEXT_KEY: u64 = key_of("compose_runtime::rememberCompositionContext");

/// A point compositions compose under, as `CompositionContext`: a [`Recomposer`]
/// ([`as_context`](Recomposer::as_context)) or a place in a composition
/// ([`remember_composition_context`]). A `Copy` handle: the place outlives its group as what
/// compositions made under it say, which then read no locals through it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CompositionContext {
    /// `None`: the thread's default.
    pub(crate) recomposer: Option<Recomposer>,
    pub(crate) place: Option<ContextId>,
}

impl CompositionContext {
    pub(crate) fn of_recomposer(recomposer: Recomposer) -> CompositionContext {
        CompositionContext { recomposer: Some(recomposer), place: None }
    }

    /// Schedules `action` for the end of the current or next frame, as
    /// `scheduleFrameEndCallback`.
    pub fn schedule_frame_end_callback(
        &self,
        action: impl FnOnce() + 'static,
    ) -> CancellationHandle {
        self.recomposer
            .unwrap_or_else(Recomposer::current_thread)
            .schedule_frame_end_callback(action)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ContextId {
    index: u32,
    generation: u32,
}

struct ContextEntry {
    generation: u32,
    composition: CompositionId,
    /// The group of the `remember_composition_context` call.
    group: u32,
    /// The composite key hash there, as last composed: what a composition under it starts from,
    /// also when it's gone (Compose's `compositeKeyHashCode`).
    key_hash: u64,
    /// Its group's values were dropped: its place is no more, for good.
    released: bool,
    /// Compositions made under it, still there.
    children: u32,
}

/// The contexts of a thread's compositions: an arena as they are.
pub(crate) struct Contexts {
    entries: Vec<ContextEntry>,
    free: Vec<u32>,
    base: u32,
}

impl Contexts {
    pub(crate) const fn new() -> Contexts {
        Contexts { entries: Vec::new(), free: Vec::new(), base: 1 }
    }

    fn alloc(&mut self, composition: CompositionId, group: u32) -> ContextId {
        let (released, children) = (false, 0);
        if let Some(index) = self.free.pop() {
            let entry = &mut self.entries[index as usize];
            entry.composition = composition;
            entry.group = group;
            entry.key_hash = 0;
            entry.released = released;
            entry.children = children;
            ContextId { index, generation: entry.generation }
        } else {
            let index = u32::try_from(self.entries.len()).expect("too many contexts");
            let generation = self.base;
            self.entries.push(ContextEntry {
                generation,
                composition,
                group,
                key_hash: 0,
                released,
                children,
            });
            ContextId { index, generation }
        }
    }

    /// The composite key hash at `id`, as last composed.
    pub(crate) fn key_hash(&self, id: ContextId) -> u64 {
        self.get(id).map_or(0, |entry| entry.key_hash)
    }

    fn get(&self, id: ContextId) -> Option<&ContextEntry> {
        self.entries.get(id.index as usize).filter(|entry| entry.generation == id.generation)
    }

    fn get_mut(&mut self, id: ContextId) -> Option<&mut ContextEntry> {
        self.entries.get_mut(id.index as usize).filter(|entry| entry.generation == id.generation)
    }

    /// Frees it once nothing can ask: released, and no composition under it.
    fn free_if_unused(&mut self, id: ContextId) {
        let Some(entry) = self.get_mut(id) else { return };
        if entry.released && entry.children == 0 {
            entry.generation = entry.generation.wrapping_add(1);
            self.free.push(id.index);
        }
    }

    pub(crate) fn release(&mut self, id: ContextId) {
        if let Some(entry) = self.get_mut(id) {
            entry.released = true;
        }
        self.free_if_unused(id);
    }

    pub(crate) fn live_count(&self) -> usize {
        self.entries.len() - self.free.len()
    }

    /// A generation above every one this arena gave out, for the arena that replaces it.
    pub(crate) fn next_base(&self) -> u32 {
        crate::runtime::next_base(self.base, self.entries.iter().map(|entry| entry.generation))
    }

    pub(crate) fn set_base(&mut self, base: u32) {
        self.base = base;
    }
}

/// The value in a context's group: dropped with what its group forgets.
struct ContextHolder {
    id: ContextId,
}

impl Drop for ContextHolder {
    fn drop(&mut self) {
        try_with_runtime(|runtime| runtime.compositions.contexts.release(self.id));
    }
}

/// Whether `group` of `table` is still the group of context `id`: emptied with its removal, as
/// the arena gives the index to another.
fn holds_context(table: &SlotTable, group: u32, id: ContextId) -> bool {
    table.groups.get(group as usize).is_some_and(|entry| {
        entry.kind != GroupKind::Free
            && entry.key == CONTEXT_KEY
            && entry.slots.first().is_some_and(|slot| match &slot.value {
                SlotValue::KeptOnReuse(value) => {
                    value.downcast_ref::<Rc<ContextHolder>>().is_some_and(|holder| holder.id == id)
                }
                _ => false,
            })
    })
}

impl Compositions {
    /// The composition and group `id` is in, while it's there: where what's read across it
    /// continues.
    pub(crate) fn context_place(&self, id: ContextId) -> Option<(&CompositionData, u32)> {
        let entry = self.contexts.get(id)?;
        let data = self.get(entry.composition)?;
        holds_context(&data.table, entry.group, id).then_some((data, entry.group))
    }

    /// The composition and group `id` is in, as [`context_place`](Self::context_place), and which
    /// composition it is.
    pub(crate) fn context_at(
        &self,
        id: ContextId,
    ) -> Option<(CompositionId, &CompositionData, u32)> {
        let (data, group) = self.context_place(id)?;
        Some((self.contexts.get(id)?.composition, data, group))
    }

    /// What moves when what `id` reads above it does in a way no state tells: a static value, or the
    /// locals provided (`Group::provider_version`), or which is the nearest provider. `None` once
    /// it's gone.
    pub(crate) fn provider_epoch(&self, id: ContextId) -> Option<u64> {
        let (mut data, mut place) = self.context_place(id)?;
        loop {
            let provider = data.table.provider_at_or_above(place);
            if provider != NONE {
                let version = data.table[provider].provider_version();
                return Some((u64::from(provider) << 32) | u64::from(version));
            }
            let Some(above) = data.parent.and_then(|above| self.context_place(above)) else {
                return Some(u64::MAX);
            };
            (data, place) = above;
        }
    }

    /// `id` under `parent`: its recomposer, and its place if `parent` has one that's there. A
    /// stale place leaves it a composition of its own.
    pub(crate) fn register(&mut self, id: CompositionId, parent: CompositionContext) {
        let place = parent.place.and_then(|place| {
            let entry = self.contexts.get_mut(place)?;
            entry.children += 1;
            let composition = entry.composition;
            match self.get_mut(composition) {
                Some(data) => data.children.push(id),
                None => {
                    self.remove_child(place, id);
                    return None;
                }
            }
            Some(place)
        });
        if let Some(data) = self.get_mut(id) {
            data.recomposer = parent.recomposer;
            data.parent = place;
        }
    }

    /// `id`, made under `place`, is gone.
    pub(crate) fn remove_child(&mut self, place: ContextId, id: CompositionId) {
        let Some(entry) = self.contexts.get_mut(place) else { return };
        entry.children -= 1;
        let composition = entry.composition;
        if let Some(data) = self.get_mut(composition) {
            data.children.retain(|child| *child != id);
        }
        self.contexts.free_if_unused(place);
    }

    /// Whether what holds its context was removed in the pass under way: it's going, by its
    /// owner, and isn't recomposed, as Compose's `compositionsRemoved`. If its owner keeps it, it
    /// goes on composing after that.
    pub(crate) fn is_going(&self, data: &CompositionData) -> bool {
        let Some(place) = data.parent else { return false };
        let Some(entry) = self.contexts.get(place) else { return false };
        !entry.released && self.context_place(place).is_none()
    }

    /// Whether a composition under `data`'s has something for a frame to compose.
    pub(crate) fn has_work_below(&self, data: &CompositionData) -> bool {
        data.children.iter().any(|child| {
            self.get(*child).is_some_and(|child| {
                !self.is_going(child) && (child.has_own_work() || self.has_work_below(child))
            })
        })
    }

    /// Who asks for a frame for it: its own requester, or the nearest above it, a child being
    /// recomposed with its root.
    pub(crate) fn frame_requester_of(&self, data: &CompositionData) -> Option<Rc<dyn Fn()>> {
        let mut data = data;
        loop {
            if let Some(requester) = &data.frame_requester {
                return Some(requester.clone());
            }
            let entry = self.contexts.get(data.parent?)?;
            data = self.get(entry.composition)?;
        }
    }

    /// Whether its frames are its own: nothing above it composes.
    pub(crate) fn is_tree_root(&self, data: &CompositionData) -> bool {
        let entry = data.parent.and_then(|place| self.contexts.get(place));
        entry.is_none_or(|entry| self.get(entry.composition).is_none_or(|parent| parent.closed))
    }
}

/// A context at this place of the composition, for compositions to compose under, as
/// `rememberCompositionContext()`: the same one each time, also for content reused around it.
///
/// # Panics
///
/// Outside composition.
pub fn remember_composition_context() -> CompositionContext {
    let group = ReplaceGroup::start(CONTEXT_KEY, "compose_runtime::rememberCompositionContext");
    let holder = remember_across_reuse(|| {
        Rc::new(ContextHolder {
            id: with_runtime(|runtime| {
                let composition = runtime.active.expect("composing").composition;
                let place = runtime.frames.last().expect("composing").group;
                runtime.compositions.contexts.alloc(composition, place)
            }),
        })
    });
    let recomposer = with_runtime(|runtime| {
        let composition = runtime.check_composing();
        let place = runtime.frames.last().expect("composing").group;
        let compositions = &mut runtime.compositions;
        let data = compositions.data(composition);
        let key_hash = crate::saveable::composite_key_hash_at(compositions, data, place);
        let recomposer = data.recomposer;
        if let Some(entry) = compositions.contexts.get_mut(holder.id) {
            entry.key_hash = key_hash;
        }
        recomposer
    });
    drop(group);
    CompositionContext { recomposer, place: Some(holder.id) }
}

/// A composition under `parent` emitting into the tree of `parent`'s composition, below `root`, a
/// node of that tree: what a `SubcomposeLayout`'s slot is. Its passes use the same applier, as the
/// root composition's do. While measuring borrows that applier, one composes through
/// [`with_lent_applier`].
///
/// # Panics
///
/// If `parent` isn't a place in a composition that's there.
pub fn subcomposition(parent: CompositionContext, root: NodeRef) -> Composition {
    Composition::sharing_applier(parent, root)
}

/// [`subcomposition`], reusable, as `ReusableComposition(applier, parent)`.
pub fn reusable_subcomposition(parent: CompositionContext, root: NodeRef) -> ReusableComposition {
    ReusableComposition::of(subcomposition(parent, root), false)
}

/// [`subcomposition`], pausable, as `PausableComposition(applier, parent)`.
pub fn pausable_subcomposition(parent: CompositionContext, root: NodeRef) -> PausableComposition {
    PausableComposition::of(subcomposition(parent, root))
}

/// The applier a composition's parent composes with, for [`subcomposition`].
pub(crate) fn parent_applier(parent: CompositionContext) -> Rc<RefCell<dyn ErasedApplier>> {
    let applier = parent.place.and_then(|place| {
        with_runtime(|runtime| {
            let compositions = &runtime.compositions;
            let entry = compositions.contexts.get(place)?;
            Some(compositions.get(entry.composition)?.applier.clone())
        })
    });
    applier.unwrap_or_else(|| misuse(ComposeError::NoParentApplier))
}

/// An applier that can stand in for itself while it's lent: a cheap empty value.
pub trait LendableApplier: Applier {
    /// The empty value that's in the holder's hands while the applier is lent.
    fn placeholder() -> Self;
}

/// A shared applier lent: its compositions' passes use `cell`.
pub(crate) struct Lend {
    shared: Rc<RefCell<dyn ErasedApplier>>,
    cell: Rc<RefCell<dyn ErasedApplier>>,
}

/// The applier a composition with `applier` uses now: the same, but while it's lent.
pub(crate) fn resolve_applier(
    lent: &[Lend],
    applier: &Rc<RefCell<dyn ErasedApplier>>,
) -> Rc<RefCell<dyn ErasedApplier>> {
    let lend = lent.iter().rev().find(|lend| Rc::ptr_eq(&lend.shared, applier));
    lend.map_or_else(|| applier.clone(), |lend| lend.cell.clone())
}

/// Lends `applier`, which the caller holds borrowed from `composition`'s
/// ([`with_applier`](Composition::with_applier)), to the passes `f` runs of compositions sharing
/// it: what a measurement composing a slot does, the tree being in its hands (D1304). The applier
/// moves into a cell they use, and comes back as `f` returns or unwinds; meanwhile `applier` is
/// [`placeholder`](LendableApplier::placeholder).
///
/// # Panics
///
/// If `composition`'s applier isn't borrowed, so `applier` can't be it.
pub fn with_lent_applier<A: LendableApplier, R>(
    composition: &Composition,
    applier: &mut A,
    f: impl FnOnce() -> R,
) -> R {
    let (shared, cell) = with_runtime(|runtime| {
        let data = runtime.compositions.get_mut(composition.id).expect("a live composition");
        (data.applier.clone(), data.lending_cell.take())
    });
    if shared.try_borrow_mut().is_ok() {
        misuse(ComposeError::ApplierNotHeld)
    }
    let cell = cell.unwrap_or_else(|| {
        let cell: Rc<RefCell<dyn ErasedApplier>> =
            Rc::new(RefCell::new(crate::applier::SharedApplier::new(A::placeholder())));
        cell
    });
    swap_with_cell(&cell, applier);
    with_runtime(|runtime| runtime.lent.push(Lend { shared, cell: cell.clone() }));

    struct Lent<'a, A: LendableApplier> {
        composition: CompositionId,
        applier: &'a mut A,
        cell: Rc<RefCell<dyn ErasedApplier>>,
    }
    impl<A: LendableApplier> Drop for Lent<'_, A> {
        fn drop(&mut self) {
            try_with_runtime(|runtime| {
                runtime.lent.pop();
                if let Some(data) = runtime.compositions.get_mut(self.composition) {
                    data.lending_cell = Some(self.cell.clone());
                }
            });
            swap_with_cell(&self.cell, self.applier);
        }
    }
    let _lent = Lent { composition: composition.id, applier, cell };
    f()
}

fn swap_with_cell<A: Applier>(cell: &Rc<RefCell<dyn ErasedApplier>>, applier: &mut A) {
    let mut held = cell.try_borrow_mut().unwrap_or_else(|_| misuse(ComposeError::ApplierBusy));
    let held = held
        .as_any_mut()
        .downcast_mut::<A>()
        .unwrap_or_else(|| misuse(ComposeError::WrongApplierType));
    std::mem::swap(held, applier);
}
