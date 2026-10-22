//! `SaveableStateHolder`, as Compose's `androidx.compose.runtime.saveable.SaveableStateHolder`:
//! the saveable state of content that leaves the composition, kept by key and given back when
//! content with that key comes back (a navigation back stack's screens, a lazy list's items).
//!
//! Each provider's content registers its values with a [`SaveableStateRegistry`] of its own;
//! when the content leaves, the holder keeps what that registry saves, and gives it to the
//! registry of the content that next comes with the key. Values in a provider are found again
//! by their place within it (its group stops [`current_composite_key_hash`]'s walk), so a
//! provider moved elsewhere (from one animated container's slot to another's) finds them too.
//!
//! [`current_composite_key_hash`]: crate::current_composite_key_hash

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

use crate::hash::{hash_key, key_of};
use crate::remember::{RememberObserver, remember, remember_observer};
use crate::saveable::{
    LOCAL_SAVEABLE_STATE_REGISTRY, SaveableStateRegistry, SavedState, SavedValue, Saver,
    remember_saveable_with, saver,
};
use crate::state::{ObjectHandle, Owned};

/// The group key of [`SaveableStateHolder::SaveableStateProvider`]'s groups, whose data key is
/// the hash of the provider's key. `current_composite_key_hash` stops there (D290).
pub(crate) const PROVIDER_GROUP: u64 = key_of("compose_runtime::SaveableStateProvider");

/// The group of a provider's [`ProviderEntry`], after its content's.
const ENTRY_GROUP: u64 = key_of("compose_runtime::SaveableStateProvider::entry");

/// Content composed in a provider: its registry, and whether to keep what it saves when it
/// leaves.
struct ActiveProvider {
    registry: SaveableStateRegistry,
    should_save: Cell<bool>,
}

#[derive(Default)]
struct HolderInner {
    /// What content that left the composition saved, by key.
    saved_states: RefCell<HashMap<u64, SavedState>>,
    /// The content in the composition, by key.
    active: RefCell<HashMap<u64, Rc<ActiveProvider>>>,
}

impl HolderInner {
    /// The saved state of every key: of content that left, and of content in the composition.
    fn save_all(&self) -> HashMap<u64, SavedState> {
        let mut states = self.saved_states.borrow().clone();
        // The registries save without the borrow: their providers run user code.
        let active: Vec<(u64, Rc<ActiveProvider>)> =
            self.active.borrow().iter().map(|(key, active)| (*key, active.clone())).collect();
        for (key, active) in active {
            if !active.should_save.get() {
                continue;
            }
            let state = active.registry.perform_save();
            if !state.is_empty() {
                states.insert(key, state);
            }
        }
        states
    }
}

/// Keeps the saveable state of content that leaves the composition, by key, and gives it back
/// when content with that key comes back, as Compose's `SaveableStateHolder`: each screen of a
/// navigation back stack, each item of a lazy list. What's kept is what `remember_saveable`
/// remembered there (a scroll position, a counter); `remember`ed values go with the content.
/// [`remember_saveable_state_holder`] saves the holder, with what it keeps, across the host's
/// recreation.
///
/// A `Copy` handle (D240): [`new`](Self::new) returns its owner.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SaveableStateHolder {
    object: ObjectHandle<HolderInner>,
}

impl crate::ComposeArg for SaveableStateHolder {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl SaveableStateHolder {
    #[track_caller]
    pub fn new() -> Owned<SaveableStateHolder> {
        SaveableStateHolder::with_states(HashMap::new())
    }

    #[track_caller]
    fn with_states(states: HashMap<u64, SavedState>) -> Owned<SaveableStateHolder> {
        let inner = HolderInner { saved_states: RefCell::new(states), ..HolderInner::default() };
        Owned::new(inner, |object| SaveableStateHolder { object })
    }

    /// Composes `content` with the saveable state kept under `key`, as Compose's
    /// `SaveableStateProvider`: what its `remember_saveable` calls remember is kept by the
    /// holder when it leaves the composition, and given back when content with `key` comes
    /// back. Content with one key is composed at one place at a time. Like `key(…)`, it's a
    /// keyed group: its content's state follows the key.
    #[allow(non_snake_case)]
    #[track_caller]
    pub fn SaveableStateProvider(&self, key: impl Hash, content: impl FnOnce()) {
        let key = hash_key(&key);
        let composer = crate::composer::current();
        composer.start_movable_group(PROVIDER_GROUP, key);
        let holder = self.object.get();
        // The registry the content's values register with, given back what was kept under
        // this key.
        let registry = remember(|| {
            let restored =
                holder.as_ref().and_then(|holder| holder.saved_states.borrow().get(&key).cloned());
            SaveableStateRegistry::new(restored)
        });
        crate::local::CompositionLocalProvider(
            LOCAL_SAVEABLE_STATE_REGISTRY.provides(Some(registry.clone())),
            content,
        );
        // In a group after the content's: a removed subtree's observers are forgotten in the
        // reverse of its pre-order (a group's slots before its children's), so this one saves
        // the content's values before they unregister themselves.
        composer.start_replace_group(ENTRY_GROUP);
        remember_observer(|| ProviderEntry {
            holder,
            key,
            active: Rc::new(ActiveProvider { registry, should_save: Cell::new(true) }),
        });
        composer.end_group();
        composer.end_group();
    }

    /// Forgets what's kept under `key`, as Compose's `removeState`: content with that key in
    /// the composition keeps its state while it stays, and leaves nothing behind.
    #[track_caller]
    pub fn remove_state(&self, key: impl Hash) {
        let key = hash_key(&key);
        let Some(holder) = self.object.get() else { return };
        match holder.active.borrow().get(&key) {
            Some(active) => active.should_save.set(false),
            None => {
                holder.saved_states.borrow_mut().remove(&key);
            }
        }
    }

    /// Saves what the holder keeps, with the state of its content in the composition, as
    /// Compose's `SaveableStateHolderImpl.Saver`: a list of each key and its state.
    pub fn saver() -> impl Saver<Owned<SaveableStateHolder>> {
        saver(
            |holder: &Owned<SaveableStateHolder>| {
                let states = holder.object.try_get()?.save_all();
                let mut keys: Vec<&u64> = states.keys().collect();
                keys.sort();
                Some(SavedValue::List(
                    keys.into_iter()
                        .map(|key| {
                            SavedValue::List(vec![
                                SavedValue::Int(*key as i64),
                                saved_state_to_value(&states[key]),
                            ])
                        })
                        .collect(),
                ))
            },
            |saved| {
                let SavedValue::List(entries) = saved else { return None };
                let mut states = HashMap::new();
                for entry in entries {
                    let SavedValue::List(pair) = entry else { return None };
                    let [SavedValue::Int(key), state] = pair.as_slice() else { return None };
                    states.insert(*key as u64, saved_state_from_value(state)?);
                }
                Some(SaveableStateHolder::with_states(states))
            },
        )
    }
}

/// A [`SaveableStateHolder`] kept in the composition and saved across the host's recreation
/// with what it keeps, as Compose's `rememberSaveableStateHolder()`.
#[track_caller]
pub fn remember_saveable_state_holder() -> SaveableStateHolder {
    remember_saveable_with(SaveableStateHolder::saver(), SaveableStateHolder::new)
}

/// A provider's content in the composition: known to its holder while there, and saved into
/// it when it leaves.
struct ProviderEntry {
    holder: Option<Rc<HolderInner>>,
    key: u64,
    active: Rc<ActiveProvider>,
}

impl RememberObserver for ProviderEntry {
    fn on_remembered(&self) {
        let Some(holder) = &self.holder else { return };
        // What was kept is now the registry's: saved again when the content leaves.
        holder.saved_states.borrow_mut().remove(&self.key);
        let previous = holder.active.borrow_mut().insert(self.key, self.active.clone());
        if previous.is_some() {
            log::warn!(
                "SaveableStateProvider: the key {:#x} is composed at two places at once; the \
                 state of the last one composed is kept",
                self.key
            );
        }
    }

    fn on_forgotten(&self) {
        let Some(holder) = &self.holder else { return };
        {
            let mut active = holder.active.borrow_mut();
            if active.get(&self.key).is_some_and(|other| Rc::ptr_eq(other, &self.active)) {
                active.remove(&self.key);
            }
        }
        if self.active.should_save.get() {
            let state = self.active.registry.perform_save();
            if !state.is_empty() {
                holder.saved_states.borrow_mut().insert(self.key, state);
            }
        }
    }
}

/// A saved state as a saved value: a list of each key and its values, sorted by key.
fn saved_state_to_value(state: &SavedState) -> SavedValue {
    let mut keys: Vec<&u64> = state.values.keys().collect();
    keys.sort();
    SavedValue::List(
        keys.into_iter()
            .map(|key| {
                SavedValue::List(vec![
                    SavedValue::Int(*key as i64),
                    SavedValue::List(state.values[key].clone()),
                ])
            })
            .collect(),
    )
}

fn saved_state_from_value(value: &SavedValue) -> Option<SavedState> {
    let SavedValue::List(entries) = value else { return None };
    let mut values = HashMap::new();
    for entry in entries {
        let SavedValue::List(pair) = entry else { return None };
        let [SavedValue::Int(key), SavedValue::List(list)] = pair.as_slice() else { return None };
        values.insert(*key as u64, list.clone());
    }
    Some(SavedState { values })
}
