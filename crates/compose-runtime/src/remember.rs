//! `remember`, `key` and remember observers: ordinary Rust on top of the composer, as in Compose.

use std::any::{Any, TypeId, type_name};
use std::cell::Cell;
use std::hash::Hash;
use std::ops::Deref;
use std::rc::Rc;

use crate::compose_arg::ComposeArg;
use crate::composer::{SlotPosition, current};
use crate::error::{ComposeError, misuse};
use crate::hash::{hash_key, key_of};
use crate::runtime::with_runtime;
use crate::slot_table::SlotValue;
use crate::state::{MutableState, Owned, OwnedState};

/// What `remember` keeps in its slot and what it returns, as a function of the remembered type.
///
/// | Remembered `T`               | The slot keeps | The call returns         |
/// |------------------------------|----------------|--------------------------|
/// | `Clone` values (incl. `Rc`)  | the value      | a clone of it            |
/// | `RefCell` or `Cell`          | refused: a clone would lose every write; `Rc<RefCell<_>>` |  |
/// | `OwnedState<V>`              | the owner      | its handle `MutableState<V>` |
/// | `Owned<H>` (a state holder)  | the owner      | its handle `H`           |
pub trait Remember: Sized + 'static {
    type Stored: 'static;
    type Output;
    fn into_stored(self) -> Self::Stored;
    fn output(stored: &Self::Stored) -> Self::Output;
}

impl<T: Clone + 'static> Remember for T {
    type Stored = T;
    type Output = T;

    fn into_stored(self) -> T {
        self
    }

    fn output(stored: &T) -> T {
        stored.clone()
    }
}

impl<V: 'static> Remember for OwnedState<V> {
    type Stored = OwnedState<V>;
    type Output = MutableState<V>;

    fn into_stored(self) -> OwnedState<V> {
        self
    }

    fn output(stored: &OwnedState<V>) -> MutableState<V> {
        stored.handle()
    }
}

impl<H: Copy + 'static> Remember for Owned<H> {
    type Stored = Owned<H>;
    type Output = H;

    fn into_stored(self) -> Owned<H> {
        self
    }

    fn output(stored: &Owned<H>) -> H {
        stored.handle()
    }
}

fn slot_next(type_id: TypeId) -> (SlotPosition, Option<Rc<dyn Any>>) {
    with_runtime(|runtime| runtime.slot_next(type_id, false))
}

fn slot_set(position: SlotPosition, value: SlotValue, type_name: &'static str) {
    if let Err((value, error)) =
        with_runtime(|runtime| runtime.slot_set(position, value, type_name))
    {
        // The user's value drops outside the runtime's borrow, then the misuse panics.
        drop(value);
        misuse(error)
    }
}

fn downcast<T: 'static>(value: &Rc<dyn Any>) -> &T {
    value.downcast_ref::<T>().expect("slot type checked by TypeId")
}

/// The value `calc` computed the first time this call ran at this position, as Compose's
/// `remember { … }`.
///
/// `remember(|| mutable_state_of(0))` returns a `Copy` [`MutableState`] handle, and the slot owns
/// the state: leaving the group frees it, after the group's nodes are released.
pub fn remember<T: Remember>(calc: impl FnOnce() -> T) -> T::Output {
    // A shim, instantiated per call site (`calc` is a closure type of its own): the work is in
    // `remembered` and `remember_new`, instantiated per remembered type, and in the runtime's
    // non-generic slot functions.
    match remembered::<T>(true) {
        Ok(output) => output,
        Err(position) => remember_new(position, calc()),
    }
}

/// Like [`remember`], but content reused around it (`ReusableContent` with another key, a lazy
/// list's item reused for another) finds the value it remembered, where `remember` computes a
/// new one: as a value of Compose's `ReusableRememberObserver` (`rememberCompositionContext`'s, a
/// `SubcomposeLayoutState` kept on its reused node). For state a component keeps for the content
/// it composes, which reused content reuses too: a lazy list keeps its items' slots so (D430).
pub fn remember_across_reuse<T: Remember>(calc: impl FnOnce() -> T) -> T::Output {
    let type_id = TypeId::of::<T::Stored>();
    let (position, existing) = with_runtime(|runtime| runtime.slot_next(type_id, true));
    if let Some(existing) = existing {
        return T::output(downcast::<T::Stored>(&existing));
    }
    let stored = Rc::new(calc().into_stored());
    let output = T::output(&stored);
    slot_set(position, SlotValue::KeptOnReuse(stored), type_name::<T::Stored>());
    output
}

/// Like [`remember`], recomputed whenever `keys` is not [`same`](ComposeArg::same) as last time,
/// as Compose's `remember(key1, …) { … }`. The replaced value is forgotten at the end of the pass.
pub fn remember_keyed<K: ComposeArg, T: Remember>(keys: K, calc: impl FnOnce() -> T) -> T::Output {
    let keys_same = remember_keys(keys);
    match remembered::<T>(keys_same) {
        Ok(output) => output,
        Err(position) => remember_new(position, calc()),
    }
}

/// Reserves the next slot: what it holds, if `reuse` and it holds a `T`, or its position.
fn remembered<T: Remember>(reuse: bool) -> Result<T::Output, SlotPosition> {
    let (position, existing) = slot_next(TypeId::of::<T::Stored>());
    if reuse && let Some(existing) = existing {
        return Ok(T::output(downcast::<T::Stored>(&existing)));
    }
    Err(position)
}

/// A `RefCell` or a `Cell` remembered as itself would come back a clone each time, and every write
/// would be lost. Stable Rust can't leave them out of the blanket `Remember`.
fn refuse_a_bare_cell(type_name: &'static str) {
    if type_name.starts_with("core::cell::RefCell<") || type_name.starts_with("core::cell::Cell<") {
        misuse(ComposeError::RememberedCell(type_name));
    }
}

/// Stores a new value in the slot [`remembered`] reserved.
fn remember_new<T: Remember>(position: SlotPosition, value: T) -> T::Output {
    refuse_a_bare_cell(type_name::<T>());
    let stored = Rc::new(value.into_stored());
    let output = T::output(&stored);
    slot_set(position, SlotValue::Plain(stored), type_name::<T::Stored>());
    output
}

/// Reserves the next slot for `keys`, keeps them unless they are `same` as the kept ones, and
/// says whether they were.
pub(crate) fn remember_keys<K: ComposeArg>(keys: K) -> bool {
    let (position, kept) = slot_next(TypeId::of::<K>());
    if let Some(kept) = kept
        && downcast::<K>(&kept).same(&keys)
    {
        return true;
    }
    slot_set(position, SlotValue::Plain(Rc::new(keys)), type_name::<K>());
    false
}

/// Reserves the next slot for `value`, as Compose's `rememberedValue() != value` followed by
/// `updateRememberedValue(value)`: keeps a clone of `value` unless the slot holds a `same` one,
/// and says whether it changed.
pub(crate) fn update_remembered<V: ComposeArg + Clone>(value: &V) -> bool {
    let (position, kept) = slot_next(TypeId::of::<V>());
    if let Some(kept) = kept
        && downcast::<V>(&kept).same(value)
    {
        return false;
    }
    slot_set(position, SlotValue::Plain(Rc::new(value.clone())), type_name::<V>());
    true
}

/// Callbacks for a remembered value entering and leaving the composition, as Compose's
/// `RememberObserver`. Each value gets exactly one of `on_forgotten` (after `on_remembered`) or
/// `on_abandoned` (if its pass never completed).
pub trait RememberObserver: 'static {
    /// The value entered the composition: called at the end of the pass that remembered it.
    fn on_remembered(&self) {}
    /// The value left the composition: called at the end of the pass that removed it.
    fn on_forgotten(&self) {}
    /// The value was remembered by a pass that never completed.
    fn on_abandoned(&self) {}
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ObserverState {
    New,
    Remembered,
    Done,
}

/// A remembered observer and where it is in its lifecycle.
pub struct ObserverBox<T> {
    state: Cell<ObserverState>,
    value: T,
}

pub(crate) trait ErasedObserver: Any {
    fn as_any(&self) -> &dyn Any;
    fn into_any(self: Rc<Self>) -> Rc<dyn Any>;
    fn dispatch_remembered(&self);
    fn dispatch_forgotten(&self);
}

impl<T: RememberObserver> ErasedObserver for ObserverBox<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn into_any(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }

    fn dispatch_remembered(&self) {
        if self.state.get() == ObserverState::New {
            self.state.set(ObserverState::Remembered);
            self.value.on_remembered();
        }
    }

    fn dispatch_forgotten(&self) {
        match self.state.replace(ObserverState::Done) {
            ObserverState::New => self.value.on_abandoned(),
            ObserverState::Remembered => self.value.on_forgotten(),
            ObserverState::Done => {}
        }
    }
}

/// Dispatches `on_forgotten` (or `on_abandoned`) to the observers among removed values, in the
/// reverse of the order they were remembered.
pub(crate) fn dispatch_forgotten(values: &[SlotValue]) {
    for value in values.iter().rev() {
        if let SlotValue::Observer(observer) = value {
            observer.dispatch_forgotten();
        }
    }
}

/// A shared handle to a remembered observer.
pub struct Observed<T: 'static>(Rc<ObserverBox<T>>);

impl<T: 'static> Clone for Observed<T> {
    fn clone(&self) -> Self {
        Observed(self.0.clone())
    }
}

impl<T: 'static> Deref for Observed<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0.value
    }
}

/// Remembers a [`RememberObserver`], which then gets Compose's lifecycle callbacks.
pub fn remember_observer<T: RememberObserver>(calc: impl FnOnce() -> T) -> Observed<T> {
    match observed::<T>(true) {
        Ok(observed) => observed,
        Err(position) => observe_new(position, calc()),
    }
}

/// Like [`remember_observer`], recreated whenever `keys` changes: the old observer is forgotten.
pub fn remember_observer_keyed<K: ComposeArg, T: RememberObserver>(
    keys: K,
    calc: impl FnOnce() -> T,
) -> Observed<T> {
    let keys_same = remember_keys(keys);
    match observed::<T>(keys_same) {
        Ok(observed) => observed,
        Err(position) => observe_new(position, calc()),
    }
}

/// Reserves the next slot: the observer it holds, if `reuse` and it holds a `T`, or its position.
fn observed<T: RememberObserver>(reuse: bool) -> Result<Observed<T>, SlotPosition> {
    let (position, existing) = slot_next(TypeId::of::<ObserverBox<T>>());
    if reuse && let Some(existing) = existing {
        return Ok(Observed(existing.downcast::<ObserverBox<T>>().expect("slot type checked")));
    }
    Err(position)
}

/// Stores a new observer in the slot [`observed`] reserved.
fn observe_new<T: RememberObserver>(position: SlotPosition, value: T) -> Observed<T> {
    let observer = Rc::new(ObserverBox { state: Cell::new(ObserverState::New), value });
    slot_set(position, SlotValue::Observer(observer.clone()), type_name::<T>());
    Observed(observer)
}

/// The group key of every `key(…)` group; its data key is the hash of the key value.
const KEY_GROUP: u64 = key_of("compose_runtime::key");

/// Runs `content` in a movable group identified by `key`, as Compose's `key(…) { … }`: in a
/// loop, an item's state follows its key when items are inserted, removed or reordered.
pub fn key<K: ComposeArg + Hash, R>(key: K, content: impl FnOnce() -> R) -> R {
    let composer = current();
    composer.start_movable_group(KEY_GROUP, hash_key(&key));
    let result = content();
    composer.end_group();
    result
}
