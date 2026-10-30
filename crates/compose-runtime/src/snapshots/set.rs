//! `SnapshotStateSet`, as Compose's (docs/parity/design/snapshots.md §3.11): a state object whose
//! record holds an `Rc` of an insertion-ordered set, as `persistentSetOf` keeps insertion order,
//! copied on write only while something else shares it.

use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;

use super::StateObjectId;
use super::map::FixedHasher;
use crate::compose_arg::ComposeArg;
use crate::remember::Remember;
use crate::state::{MutableState, NeverEqualPolicy, OwnedState, mutable_state_of_with};

/// A set iterating in insertion order: entries in order, with tombstones compacted past half, and
/// an index by value.
#[derive(Clone)]
pub struct OrderedSet<T> {
    entries: Vec<Option<T>>,
    index: HashMap<T, usize, FixedHasher>,
}

impl<T> Default for OrderedSet<T> {
    fn default() -> Self {
        OrderedSet { entries: Vec::new(), index: HashMap::default() }
    }
}

impl<T: Clone + Hash + Eq> OrderedSet<T> {
    pub fn contains(&self, value: &T) -> bool {
        self.index.contains_key(value)
    }

    /// Adds `value` at the end, if it isn't in; returns whether it was added.
    fn insert(&mut self, value: T) -> bool {
        if self.index.contains_key(&value) {
            return false;
        }
        self.index.insert(value.clone(), self.entries.len());
        self.entries.push(Some(value));
        true
    }

    fn remove(&mut self, value: &T) -> bool {
        let Some(at) = self.index.remove(value) else { return false };
        self.entries[at] = None;
        if self.entries.len() > 8 && self.index.len() * 2 < self.entries.len() {
            self.entries.retain(Option::is_some);
            for (at, entry) in self.entries.iter().flatten().enumerate() {
                if let Some(slot) = self.index.get_mut(entry) {
                    *slot = at;
                }
            }
        }
        true
    }
}

impl<T> OrderedSet<T> {
    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// The values, in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.entries.iter().filter_map(Option::as_ref)
    }
}

pub(crate) struct SetRecord<T> {
    set: Rc<OrderedSet<T>>,
}

/// A `Copy` handle to a set state, as Compose's `SnapshotStateSet`. Reads subscribe to the whole
/// set; [`OwnedStateSet`] owns it.
pub struct SnapshotStateSet<T: 'static> {
    state: MutableState<SetRecord<T>>,
}

impl<T: 'static> Clone for SnapshotStateSet<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for SnapshotStateSet<T> {}

impl<T: 'static> PartialEq for SnapshotStateSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state
    }
}

impl<T: 'static> Eq for SnapshotStateSet<T> {}

impl<T: 'static> ComposeArg for SnapshotStateSet<T> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: 'static> From<SnapshotStateSet<T>> for StateObjectId {
    fn from(set: SnapshotStateSet<T>) -> StateObjectId {
        set.state_object_id()
    }
}

/// Owns a set state: dropping it retires the state. Derefs to its handle.
pub struct OwnedStateSet<T: 'static> {
    _owner: OwnedState<SetRecord<T>>,
    handle: SnapshotStateSet<T>,
}

impl<T: 'static> std::ops::Deref for OwnedStateSet<T> {
    type Target = SnapshotStateSet<T>;
    fn deref(&self) -> &SnapshotStateSet<T> {
        &self.handle
    }
}

impl<T: 'static> OwnedStateSet<T> {
    /// The `Copy` handle.
    pub fn handle(&self) -> SnapshotStateSet<T> {
        self.handle
    }
}

impl<T: 'static> Remember for OwnedStateSet<T> {
    type Stored = OwnedStateSet<T>;
    type Output = SnapshotStateSet<T>;

    fn into_stored(self) -> OwnedStateSet<T> {
        self
    }

    fn output(stored: &OwnedStateSet<T>) -> SnapshotStateSet<T> {
        stored.handle
    }
}

/// A new empty set state, as Compose's `mutableStateSetOf()`.
#[track_caller]
pub fn mutable_state_set_of<T: Clone + Hash + Eq + 'static>() -> OwnedStateSet<T> {
    mutable_state_set_from(Vec::new())
}

/// A new set state holding `items`, as Compose's `mutableStateSetOf(...)`.
#[track_caller]
pub fn mutable_state_set_from<T: Clone + Hash + Eq + 'static>(
    items: impl IntoIterator<Item = T>,
) -> OwnedStateSet<T> {
    let mut set = OrderedSet::default();
    for item in items {
        set.insert(item);
    }
    let record = SetRecord { set: Rc::new(set) };
    let owner = mutable_state_of_with(record, NeverEqualPolicy);
    let handle = SnapshotStateSet { state: owner.handle() };
    OwnedStateSet { _owner: owner, handle }
}

impl<T: 'static> SnapshotStateSet<T> {
    /// The state's identity, as observers are told it.
    pub fn state_object_id(&self) -> StateObjectId {
        self.state.state_object_id()
    }

    /// Runs `f` with the set. Subscribes to the whole set.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&OrderedSet<T>) -> R) -> R {
        self.state.with(|record| f(&record.set))
    }

    #[track_caller]
    pub fn len(&self) -> usize {
        self.with(OrderedSet::len)
    }

    #[track_caller]
    pub fn is_empty(&self) -> bool {
        self.with(OrderedSet::is_empty)
    }

    /// The set as it is now: the set itself, in O(1), which a later write copies from.
    #[track_caller]
    pub fn to_set(&self) -> Rc<OrderedSet<T>> {
        self.state.with(|record| record.set.clone())
    }

    fn current<R>(&self, f: impl FnOnce(&OrderedSet<T>) -> R) -> R {
        self.state.with_current(|record| f(&record.set))
    }
}

impl<T: Clone + Hash + Eq + 'static> SnapshotStateSet<T> {
    #[track_caller]
    fn write<R>(&self, f: impl FnOnce(&mut OrderedSet<T>) -> R) -> R {
        self.state.edit(|record| f(Rc::make_mut(&mut record.set)))
    }

    #[track_caller]
    pub fn contains(&self, value: &T) -> bool {
        self.with(|set| set.contains(value))
    }

    /// Whether every one of `values` is in the set, as `containsAll`.
    #[track_caller]
    pub fn contains_all(&self, values: &[T]) -> bool {
        self.with(|set| values.iter().all(|value| set.contains(value)))
    }

    /// The values as they are now, in insertion order.
    #[track_caller]
    pub fn iter_cloned(&self) -> std::vec::IntoIter<T> {
        self.with(|set| set.iter().cloned().collect::<Vec<T>>()).into_iter()
    }

    /// Adds `value`, as `add`. Returns whether it wasn't in: in, it writes nothing.
    #[track_caller]
    pub fn insert(&self, value: T) -> bool {
        if self.current(|set| set.contains(&value)) {
            return false;
        }
        self.write(|set| set.insert(value))
    }

    /// Adds `items`, as `addAll`. Returns whether any wasn't in.
    #[track_caller]
    pub fn add_all(&self, items: impl IntoIterator<Item = T>) -> bool {
        let new: Vec<T> =
            items.into_iter().filter(|item| !self.current(|set| set.contains(item))).collect();
        if new.is_empty() {
            return false;
        }
        self.write(|set| {
            for item in new {
                set.insert(item);
            }
        });
        true
    }

    /// Removes `value`, as `remove`. Returns whether it was in: absent, it writes nothing.
    #[track_caller]
    pub fn remove(&self, value: &T) -> bool {
        if !self.current(|set| set.contains(value)) {
            return false;
        }
        self.write(|set| set.remove(value))
    }

    /// Keeps the values `keep` accepts. Returns whether any went: none going writes nothing.
    #[track_caller]
    pub fn retain(&self, mut keep: impl FnMut(&T) -> bool) -> bool {
        let gone: Vec<T> =
            self.current(|set| set.iter().filter(|value| !keep(value)).cloned().collect());
        if gone.is_empty() {
            return false;
        }
        self.write(|set| {
            for value in &gone {
                set.remove(value);
            }
        });
        true
    }

    /// Removes those of `values` that are in, as `removeAll`. Returns whether any went.
    #[track_caller]
    pub fn remove_all(&self, values: &[T]) -> bool {
        self.retain(|value| !values.contains(value))
    }

    /// Keeps only those of `values` that are in, as `retainAll`. Returns whether any went.
    #[track_caller]
    pub fn retain_all(&self, values: &[T]) -> bool {
        self.retain(|value| values.contains(value))
    }

    /// Removes everything, as `clear`: always a write.
    #[track_caller]
    pub fn clear(&self) {
        self.write(|set| *set = OrderedSet::default());
    }
}

impl<T: 'static> fmt::Debug for SnapshotStateSet<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SnapshotStateSet({:?})", self.state_object_id())
    }
}

/// `SnapshotStateSet(value=[…])@…`, as Compose's `toString`: read without subscribing.
impl<T: fmt::Debug + 'static> fmt::Display for SnapshotStateSet<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = self.state_object_id();
        self.current(|set| {
            write!(formatter, "SnapshotStateSet(value=")?;
            formatter.debug_list().entries(set.iter()).finish()?;
            write!(formatter, ")@{id:?}")
        })
    }
}
