//! `SnapshotStateMap`, as Compose's (docs/parity/design/snapshots.md §3.11): a state object whose
//! record holds an `Rc` of a `HashMap`, copied on write only while something else shares it. Its
//! hasher is fixed, so a map built the same way iterates the same way.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{BuildHasherDefault, Hash};
use std::rc::Rc;

use crate::compose_arg::ComposeArg;
use crate::remember::Remember;
use crate::state::{MutableState, NeverEqualPolicy, OwnedState, mutable_state_of_with};

use super::StateObjectId;

/// The hasher of the state collections: no random seed.
pub(crate) type FixedHasher = BuildHasherDefault<DefaultHasher>;

/// The map a [`SnapshotStateMap`] holds.
pub type StateMapContents<K, V> = HashMap<K, V, FixedHasher>;

pub(crate) struct MapRecord<K, V> {
    map: Rc<StateMapContents<K, V>>,
}

/// A `Copy` handle to a map state, as Compose's `SnapshotStateMap`. Reads subscribe to the whole
/// map; [`OwnedStateMap`] owns it.
pub struct SnapshotStateMap<K: 'static, V: 'static> {
    state: MutableState<MapRecord<K, V>>,
}

impl<K: 'static, V: 'static> Clone for SnapshotStateMap<K, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K: 'static, V: 'static> Copy for SnapshotStateMap<K, V> {}

impl<K: 'static, V: 'static> PartialEq for SnapshotStateMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state
    }
}

impl<K: 'static, V: 'static> Eq for SnapshotStateMap<K, V> {}

impl<K: 'static, V: 'static> Hash for SnapshotStateMap<K, V> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.state.hash(state);
    }
}

impl<K: 'static, V: 'static> ComposeArg for SnapshotStateMap<K, V> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<K: 'static, V: 'static> From<SnapshotStateMap<K, V>> for StateObjectId {
    fn from(map: SnapshotStateMap<K, V>) -> StateObjectId {
        map.state_object_id()
    }
}

/// Owns a map state: dropping it retires the state. Derefs to its handle.
pub struct OwnedStateMap<K: 'static, V: 'static> {
    owner: OwnedState<MapRecord<K, V>>,
    handle: SnapshotStateMap<K, V>,
}

impl<K: 'static, V: 'static> std::ops::Deref for OwnedStateMap<K, V> {
    type Target = SnapshotStateMap<K, V>;
    fn deref(&self) -> &SnapshotStateMap<K, V> {
        &self.handle
    }
}

impl<K: 'static, V: 'static> OwnedStateMap<K, V> {
    /// The `Copy` handle.
    pub fn handle(&self) -> SnapshotStateMap<K, V> {
        self.handle
    }

    /// Whether the state is still live.
    pub fn is_alive(&self) -> bool {
        self.owner.is_alive()
    }
}

impl<K: 'static, V: 'static> Remember for OwnedStateMap<K, V> {
    type Stored = OwnedStateMap<K, V>;
    type Output = SnapshotStateMap<K, V>;

    fn into_stored(self) -> OwnedStateMap<K, V> {
        self
    }

    fn output(stored: &OwnedStateMap<K, V>) -> SnapshotStateMap<K, V> {
        stored.handle
    }
}

/// A new empty map state, as Compose's `mutableStateMapOf()`.
#[track_caller]
pub fn mutable_state_map_of<K: Hash + Eq + 'static, V: 'static>() -> OwnedStateMap<K, V> {
    mutable_state_map_from(Vec::new())
}

/// A new map state holding `entries`, as Compose's `mutableStateMapOf(...)` and
/// `toMutableStateMap()`.
#[track_caller]
pub fn mutable_state_map_from<K: Hash + Eq + 'static, V: 'static>(
    entries: impl IntoIterator<Item = (K, V)>,
) -> OwnedStateMap<K, V> {
    let record = MapRecord { map: Rc::new(entries.into_iter().collect()) };
    let owner = mutable_state_of_with(record, NeverEqualPolicy);
    let handle = SnapshotStateMap { state: owner.handle() };
    OwnedStateMap { owner, handle }
}

impl<K: 'static, V: 'static> SnapshotStateMap<K, V> {
    /// The state's identity, as observers are told it.
    pub fn state_object_id(&self) -> StateObjectId {
        self.state.state_object_id()
    }

    /// Runs `f` with the map. Subscribes to the whole map.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&StateMapContents<K, V>) -> R) -> R {
        self.state.with(|record| f(&record.map))
    }

    #[track_caller]
    pub fn len(&self) -> usize {
        self.with(HashMap::len)
    }

    #[track_caller]
    pub fn is_empty(&self) -> bool {
        self.with(HashMap::is_empty)
    }

    /// The map as it is now: the map itself, in O(1), which a later write copies from.
    #[track_caller]
    pub fn to_map(&self) -> Rc<StateMapContents<K, V>> {
        self.state.with(|record| record.map.clone())
    }

    fn current<R>(&self, f: impl FnOnce(&StateMapContents<K, V>) -> R) -> R {
        self.state.with_current(|record| f(&record.map))
    }

    #[track_caller]
    fn write<R>(&self, f: impl FnOnce(&mut StateMapContents<K, V>) -> R) -> R
    where
        K: Clone,
        V: Clone,
    {
        self.state.edit(|record| f(Rc::make_mut(&mut record.map)))
    }

    /// Removes everything, as `clear`. Empty, it writes nothing, as Compose's `newMap !== map`.
    #[track_caller]
    pub fn clear(&self)
    where
        K: Clone,
        V: Clone,
    {
        if !self.current(HashMap::is_empty) {
            self.write(HashMap::clear);
        }
    }
}

impl<K: Clone + Hash + Eq + 'static, V: Clone + 'static> SnapshotStateMap<K, V> {
    #[track_caller]
    pub fn get(&self, key: &K) -> Option<V> {
        self.with(|map| map.get(key).cloned())
    }

    #[track_caller]
    pub fn contains_key(&self, key: &K) -> bool {
        self.with(|map| map.contains_key(key))
    }

    /// Whether `value` is one of the values, as `containsValue`.
    #[track_caller]
    pub fn contains_value(&self, value: &V) -> bool
    where
        V: PartialEq,
    {
        self.with(|map| map.values().any(|other| other == value))
    }

    /// Writes `value` at `key`, as `put`, returning the old one. An equal value writes nothing,
    /// as Compose's `newMap == oldMap`, here for the one entry.
    #[track_caller]
    pub fn insert(&self, key: K, value: V) -> Option<V>
    where
        V: PartialEq,
    {
        let same = self.current(|map| map.get(&key).filter(|old| **old == value).cloned());
        if same.is_some() {
            return same;
        }
        self.write(|map| map.insert(key, value))
    }

    /// Like [`insert`](Self::insert), for values that can't be compared: always a write.
    #[track_caller]
    pub fn insert_always(&self, key: K, value: V) -> Option<V> {
        self.write(|map| map.insert(key, value))
    }

    /// Writes each of `entries`, as `putAll`. Only equal values, or none, write nothing.
    #[track_caller]
    pub fn insert_all(&self, entries: impl IntoIterator<Item = (K, V)>)
    where
        V: PartialEq,
    {
        let entries: Vec<(K, V)> = entries.into_iter().collect();
        let changed = self.current(|map| {
            entries.iter().any(|(key, value)| map.get(key).is_none_or(|old| old != value))
        });
        if changed {
            self.write(|map| map.extend(entries));
        }
    }

    /// Removes `key`, as `remove`, returning its value. Absent: writes nothing.
    #[track_caller]
    pub fn remove(&self, key: &K) -> Option<V> {
        if self.current(|map| map.contains_key(key)) {
            self.write(|map| map.remove(key))
        } else {
            None
        }
    }

    /// Replaces `key`'s value with `f(old)`, `None` for none; `f` giving `None` removes it. An
    /// equal value writes nothing.
    #[track_caller]
    pub fn update(&self, key: K, f: impl FnOnce(Option<&V>) -> Option<V>)
    where
        V: PartialEq,
    {
        let value = self.current(|map| f(map.get(&key)));
        match value {
            Some(value) => {
                self.insert(key, value);
            }
            None => {
                self.remove(&key);
            }
        }
    }

    /// Keeps the entries `keep` accepts, as `entries.retainAll` and its siblings on the keys and
    /// the values. Returns whether any went: none going writes nothing.
    #[track_caller]
    pub fn retain(&self, mut keep: impl FnMut(&K, &V) -> bool) -> bool {
        let gone: Vec<K> = self.current(|map| {
            map.iter()
                .filter(|(key, value)| !keep(key, value))
                .map(|(key, _)| key.clone())
                .collect()
        });
        if gone.is_empty() {
            return false;
        }
        self.write(|map| {
            for key in &gone {
                map.remove(key);
            }
        });
        true
    }

    /// The keys as they are now, as `keys`.
    #[track_caller]
    pub fn keys(&self) -> Vec<K> {
        self.with(|map| map.keys().cloned().collect())
    }

    /// The values as they are now, as `values`.
    #[track_caller]
    pub fn values(&self) -> Vec<V> {
        self.with(|map| map.values().cloned().collect())
    }

    /// The entries as they are now, as `entries`.
    #[track_caller]
    pub fn entries(&self) -> Vec<(K, V)> {
        self.with(|map| map.iter().map(|(key, value)| (key.clone(), value.clone())).collect())
    }
}

impl<K: 'static, V: 'static> fmt::Debug for SnapshotStateMap<K, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SnapshotStateMap({:?})", self.state_object_id())
    }
}

/// `SnapshotStateMap(value={k=v, …})@…`, as Compose's `toString`: read without subscribing.
impl<K: fmt::Debug + 'static, V: fmt::Debug + 'static> fmt::Display for SnapshotStateMap<K, V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = self.state_object_id();
        self.current(|map| {
            write!(formatter, "SnapshotStateMap(value={{")?;
            for (index, (key, value)) in map.iter().enumerate() {
                let separator = if index == 0 { "" } else { ", " };
                write!(formatter, "{separator}{key:?}={value:?}")?;
            }
            write!(formatter, "}})@{id:?}")
        })
    }
}
