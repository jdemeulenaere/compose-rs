//! `SnapshotStateList`, as Compose's (docs/parity/design/snapshots.md §3.11): a state object whose
//! record holds an `Rc` of a `Vec`, copied on write only while something else shares it (a
//! [`to_list`](SnapshotStateList::to_list) result, later an open snapshot). Compose's persistent
//! vector pays on every operation for concurrent readers, which this thread-bound one has none of.
//!
//! A read reads the whole record, so any read subscribes to the whole list; an operation leaving
//! the list as it was writes nothing.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use crate::compose_arg::ComposeArg;
use crate::immutable_list::ImmutableList;
use crate::remember::Remember;
use crate::state::{MutableState, NeverEqualPolicy, OwnedState, mutable_state_of_with};

use super::StateObjectId;

/// A list's record, as Compose's `StateListStateRecord`.
pub(crate) struct ListRecord<T> {
    list: Rc<Vec<T>>,
    /// Structural changes: what sublists and iterators check they're still of the same list.
    structural: u32,
}

impl<T> Clone for ListRecord<T> {
    fn clone(&self) -> Self {
        ListRecord { list: self.list.clone(), structural: self.structural }
    }
}

/// A `Copy` handle to a list state, as Compose's `SnapshotStateList`. Reads subscribe to the whole
/// list; [`OwnedStateList`] owns it.
pub struct SnapshotStateList<T: 'static> {
    state: MutableState<ListRecord<T>>,
}

impl<T: 'static> Clone for SnapshotStateList<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> Copy for SnapshotStateList<T> {}

impl<T: 'static> PartialEq for SnapshotStateList<T> {
    fn eq(&self, other: &Self) -> bool {
        self.state == other.state
    }
}

impl<T: 'static> Eq for SnapshotStateList<T> {}

impl<T: 'static> std::hash::Hash for SnapshotStateList<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.state.hash(state);
    }
}

impl<T: 'static> ComposeArg for SnapshotStateList<T> {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}

impl<T: 'static> From<SnapshotStateList<T>> for StateObjectId {
    fn from(list: SnapshotStateList<T>) -> StateObjectId {
        list.state_object_id()
    }
}

/// Owns a list state: dropping it retires the state, as an [`OwnedState`]. Derefs to its handle.
pub struct OwnedStateList<T: 'static> {
    owner: OwnedState<ListRecord<T>>,
    handle: SnapshotStateList<T>,
}

impl<T: 'static> std::ops::Deref for OwnedStateList<T> {
    type Target = SnapshotStateList<T>;
    fn deref(&self) -> &SnapshotStateList<T> {
        &self.handle
    }
}

impl<T: 'static> OwnedStateList<T> {
    /// The `Copy` handle.
    pub fn handle(&self) -> SnapshotStateList<T> {
        self.handle
    }

    /// Whether the state is still live.
    pub fn is_alive(&self) -> bool {
        self.owner.is_alive()
    }
}

impl<T: 'static> Remember for OwnedStateList<T> {
    type Stored = OwnedStateList<T>;
    type Output = SnapshotStateList<T>;

    fn into_stored(self) -> OwnedStateList<T> {
        self
    }

    fn output(stored: &OwnedStateList<T>) -> SnapshotStateList<T> {
        stored.handle
    }
}

/// A new empty list state, as Compose's `mutableStateListOf()`.
#[track_caller]
pub fn mutable_state_list_of<T: 'static>() -> OwnedStateList<T> {
    mutable_state_list_from(Vec::new())
}

/// A new list state holding `items`, as Compose's `mutableStateListOf(...)` and
/// `toMutableStateList()`.
#[track_caller]
pub fn mutable_state_list_from<T: 'static>(
    items: impl IntoIterator<Item = T>,
) -> OwnedStateList<T> {
    let record = ListRecord { list: Rc::new(items.into_iter().collect()), structural: 0 };
    let owner = mutable_state_of_with(record, NeverEqualPolicy);
    let handle = SnapshotStateList { state: owner.handle() };
    OwnedStateList { owner, handle }
}

/// A new list state of `size` elements made by `init`, as Compose's `SnapshotStateList(size, init)`.
#[track_caller]
pub fn snapshot_state_list<T: 'static>(
    size: usize,
    init: impl FnMut(usize) -> T,
) -> OwnedStateList<T> {
    mutable_state_list_from((0..size).map(init))
}

impl<T: 'static> SnapshotStateList<T> {
    /// The state's identity, as observers are told it.
    pub fn state_object_id(&self) -> StateObjectId {
        self.state.state_object_id()
    }

    /// Runs `f` with the list. Subscribes to the whole list.
    #[track_caller]
    pub fn with<R>(&self, f: impl FnOnce(&Vec<T>) -> R) -> R {
        self.state.with(|record| f(&record.list))
    }

    /// The list's length.
    #[track_caller]
    pub fn len(&self) -> usize {
        self.with(Vec::len)
    }

    #[track_caller]
    pub fn is_empty(&self) -> bool {
        self.with(Vec::is_empty)
    }

    /// The list as it is now: the list itself, in O(1), which a later write copies from.
    #[track_caller]
    pub fn to_list(&self) -> ImmutableList<T> {
        self.state.with(|record| ImmutableList::from_shared(record.list.clone()))
    }

    /// Read without subscribing, as a write reads.
    fn current<R>(&self, f: impl FnOnce(&Vec<T>) -> R) -> R {
        self.state.with_current(|record| f(&record.list))
    }

    fn structural(&self) -> u32 {
        self.state.with_current(|record| record.structural)
    }

    /// A sublist of `range`, live: a view of this list, as `subList`. Panics if the range is out
    /// of bounds; a structural change of the list other than through it makes it panic after.
    #[track_caller]
    pub fn sub_list(&self, range: std::ops::Range<usize>) -> StateSubList<T> {
        let len = self.len();
        assert!(
            range.start <= range.end && range.end <= len,
            "sub_list {range:?} of a list of {len} elements"
        );
        StateSubList {
            list: *self,
            offset: range.start,
            size: Cell::new(range.len()),
            structural: Cell::new(self.structural()),
        }
    }

    /// A cursor at `index`, as `listIterator(index)`.
    #[track_caller]
    pub fn list_iterator(&self, index: usize) -> StateListIterator<T> {
        let len = self.len();
        assert!(index <= len, "list_iterator at {index} of a list of {len} elements");
        let snapshot = Some(self.to_list());
        let cursor = Cursor::List { list: *self, snapshot, structural: self.structural() };
        StateListIterator { cursor, index, last: None }
    }

    /// Writes the list; `structural` if its length or order changes.
    #[track_caller]
    fn write<R>(&self, structural: bool, f: impl FnOnce(&mut Vec<T>) -> R) -> R
    where
        T: Clone,
    {
        self.state.edit(|record| {
            if structural {
                record.structural = record.structural.wrapping_add(1);
            }
            f(Rc::make_mut(&mut record.list))
        })
    }

    /// Changes the list by `f`, whatever it does: always a write. The other methods write only
    /// if they change it.
    #[track_caller]
    pub fn edit<R>(&self, f: impl FnOnce(&mut Vec<T>) -> R) -> R
    where
        T: Clone,
    {
        self.write(true, f)
    }

    /// Adds `value` at the end, as `add`.
    #[track_caller]
    pub fn push(&self, value: T)
    where
        T: Clone,
    {
        self.write(true, |list| list.push(value));
    }

    /// Adds `value` at `index`, as `add(index, value)`. Panics if it's out of bounds.
    #[track_caller]
    pub fn insert(&self, index: usize, value: T)
    where
        T: Clone,
    {
        let len = self.current(Vec::len);
        assert!(index <= len, "insert at {index} in a list of {len} elements");
        self.write(true, |list| list.insert(index, value));
    }

    /// Adds `items` at the end, as `addAll`. Returns whether there were any.
    #[track_caller]
    pub fn add_all(&self, items: impl IntoIterator<Item = T>) -> bool
    where
        T: Clone,
    {
        let at = self.current(Vec::len);
        self.insert_all(at, items)
    }

    /// Adds `items` at `index`, as `addAll(index, items)`. Returns whether there were any.
    #[track_caller]
    pub fn insert_all(&self, index: usize, items: impl IntoIterator<Item = T>) -> bool
    where
        T: Clone,
    {
        let items: Vec<T> = items.into_iter().collect();
        let len = self.current(Vec::len);
        assert!(index <= len, "insert_all at {index} in a list of {len} elements");
        if items.is_empty() {
            return false;
        }
        self.write(true, |list| {
            list.splice(index..index, items);
        });
        true
    }

    /// Removes and returns the element at `index`, as `removeAt`. Panics if it's out of bounds.
    #[track_caller]
    pub fn remove(&self, index: usize) -> T
    where
        T: Clone,
    {
        let len = self.current(Vec::len);
        assert!(index < len, "remove at {index} in a list of {len} elements");
        self.write(true, |list| list.remove(index))
    }

    /// Removes the elements of `range`, as `removeRange`. An empty range writes nothing.
    #[track_caller]
    pub fn remove_range(&self, range: std::ops::Range<usize>)
    where
        T: Clone,
    {
        let len = self.current(Vec::len);
        assert!(
            range.start <= range.end && range.end <= len,
            "remove_range {range:?} in a list of {len} elements"
        );
        if !range.is_empty() {
            self.write(true, |list| {
                list.drain(range);
            });
        }
    }

    /// Replaces the element at `index`, returning the old one, as `set`. An equal value writes
    /// nothing, as Compose's `newList == oldList`, here for the one element. Panics if it's out of
    /// bounds.
    #[track_caller]
    pub fn set(&self, index: usize, value: T) -> T
    where
        T: Clone + PartialEq,
    {
        let same = self.current(|list| {
            let len = list.len();
            let current =
                list.get(index).unwrap_or_else(|| panic!("set at {index} in a list of {len}"));
            (*current == value).then(|| current.clone())
        });
        match same {
            Some(current) => current,
            None => self.write(false, |list| std::mem::replace(&mut list[index], value)),
        }
    }

    /// Like [`set`](Self::set), for values that can't be compared: always a write.
    #[track_caller]
    pub fn set_always(&self, index: usize, value: T) -> T
    where
        T: Clone,
    {
        let len = self.current(Vec::len);
        assert!(index < len, "set at {index} in a list of {len} elements");
        self.write(false, |list| std::mem::replace(&mut list[index], value))
    }

    /// Keeps the elements `keep` accepts, as `retainAll` with a predicate. Returns whether any
    /// went: none going writes nothing.
    #[track_caller]
    pub fn retain(&self, mut keep: impl FnMut(&T) -> bool) -> bool
    where
        T: Clone,
    {
        let kept: Vec<bool> = self.current(|list| list.iter().map(&mut keep).collect());
        if kept.iter().all(|&kept| kept) {
            return false;
        }
        let mut flags = kept.iter();
        self.write(true, |list| list.retain(|_| *flags.next().expect("a flag per element")));
        true
    }

    /// Removes everything, as `clear`: always a write.
    #[track_caller]
    pub fn clear(&self)
    where
        T: Clone,
    {
        self.write(true, Vec::clear);
    }

    /// Keeps the first `len` elements. Fewer already: writes nothing.
    #[track_caller]
    pub fn truncate(&self, len: usize)
    where
        T: Clone,
    {
        if self.current(Vec::len) > len {
            self.write(true, |list| list.truncate(len));
        }
    }

    /// Swaps two elements.
    #[track_caller]
    pub fn swap(&self, a: usize, b: usize)
    where
        T: Clone,
    {
        if a != b {
            self.write(false, |list| list.swap(a, b));
        }
    }

    /// Reverses the list in place, as `reverse`.
    #[track_caller]
    pub fn reverse(&self)
    where
        T: Clone,
    {
        self.write(false, |list| list.reverse());
    }

    /// Sorts the list in place, as `sortWith`.
    #[track_caller]
    pub fn sort_by(&self, compare: impl FnMut(&T, &T) -> std::cmp::Ordering)
    where
        T: Clone,
    {
        self.write(false, |list| list.sort_by(compare));
    }

    /// The element at `index`, as `list[index]`, `None` out of bounds.
    #[track_caller]
    pub fn get(&self, index: usize) -> Option<T>
    where
        T: Clone,
    {
        self.with(|list| list.get(index).cloned())
    }

    /// The elements as they are now, for `for` loops: a read of the list, once.
    #[track_caller]
    pub fn iter_cloned(&self) -> std::vec::IntoIter<T>
    where
        T: Clone,
    {
        self.with(|list| list.clone()).into_iter()
    }

    #[track_caller]
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.with(|list| list.contains(value))
    }

    /// Whether every one of `values` is in the list, as `containsAll`.
    #[track_caller]
    pub fn contains_all(&self, values: &[T]) -> bool
    where
        T: PartialEq,
    {
        self.with(|list| values.iter().all(|value| list.contains(value)))
    }

    /// The first index of `value`, as `indexOf`.
    #[track_caller]
    pub fn index_of(&self, value: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.with(|list| list.iter().position(|item| item == value))
    }

    /// The last index of `value`, as `lastIndexOf`.
    #[track_caller]
    pub fn last_index_of(&self, value: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.with(|list| list.iter().rposition(|item| item == value))
    }

    /// Removes the first element equal to `value`, as `remove(value)`. Returns whether there was
    /// one: none writes nothing.
    #[track_caller]
    pub fn remove_item(&self, value: &T) -> bool
    where
        T: Clone + PartialEq,
    {
        match self.current(|list| list.iter().position(|item| item == value)) {
            Some(index) => {
                self.write(true, |list| list.remove(index));
                true
            }
            None => false,
        }
    }

    /// Removes every element equal to one of `values`, as `removeAll`. Returns whether any went.
    #[track_caller]
    pub fn remove_all(&self, values: &[T]) -> bool
    where
        T: Clone + PartialEq,
    {
        self.retain(|item| !values.contains(item))
    }

    /// Keeps only the elements equal to one of `values`, as `retainAll`. Returns whether any went.
    #[track_caller]
    pub fn retain_all(&self, values: &[T]) -> bool
    where
        T: Clone + PartialEq,
    {
        self.retain(|item| values.contains(item))
    }
}

impl<T: 'static> fmt::Debug for SnapshotStateList<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SnapshotStateList({:?})", self.state_object_id())
    }
}

/// `SnapshotStateList(value=[…])@…`, as Compose's `toString`: read without subscribing.
impl<T: fmt::Debug + 'static> fmt::Display for SnapshotStateList<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let id = self.state_object_id();
        self.current(|list| write!(formatter, "SnapshotStateList(value={list:?})@{id:?}"))
    }
}

/// A live view of a range of a [`SnapshotStateList`], as Compose's `subList`. A structural change
/// of the list other than through this view makes it panic, as Compose's
/// `ConcurrentModificationException`.
pub struct StateSubList<T: 'static> {
    list: SnapshotStateList<T>,
    offset: usize,
    size: Cell<usize>,
    structural: Cell<u32>,
}

impl<T: 'static> StateSubList<T> {
    #[track_caller]
    fn check(&self) {
        assert!(
            self.structural.get() == self.list.structural(),
            "compose-runtime: a sublist or iterator of a SnapshotStateList used after the list \
             changed structurally (ConcurrentModification)"
        );
    }

    /// A structural change made through this view.
    fn changed(&self, size: usize) {
        self.size.set(size);
        self.structural.set(self.list.structural());
    }

    #[track_caller]
    fn with<R>(&self, f: impl FnOnce(&[T]) -> R) -> R {
        self.check();
        let (offset, size) = (self.offset, self.size.get());
        self.list.with(|list| f(&list[offset..offset + size]))
    }

    /// Its size, as it knows it: no read of the list, as Compose's.
    pub fn len(&self) -> usize {
        self.size.get()
    }

    pub fn is_empty(&self) -> bool {
        self.size.get() == 0
    }

    #[track_caller]
    pub fn get(&self, index: usize) -> Option<T>
    where
        T: Clone,
    {
        self.with(|list| list.get(index).cloned())
    }

    /// The view's elements as they are now.
    #[track_caller]
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.with(<[T]>::to_vec)
    }

    #[track_caller]
    pub fn contains(&self, value: &T) -> bool
    where
        T: PartialEq,
    {
        self.with(|list| list.contains(value))
    }

    #[track_caller]
    pub fn contains_all(&self, values: &[T]) -> bool
    where
        T: PartialEq,
    {
        self.with(|list| values.iter().all(|value| list.contains(value)))
    }

    #[track_caller]
    pub fn index_of(&self, value: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.with(|list| list.iter().position(|item| item == value))
    }

    #[track_caller]
    pub fn last_index_of(&self, value: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.with(|list| list.iter().rposition(|item| item == value))
    }

    /// Adds `value` at the end of the view.
    #[track_caller]
    pub fn push(&self, value: T)
    where
        T: Clone,
    {
        self.insert(self.len(), value);
    }

    #[track_caller]
    pub fn insert(&self, index: usize, value: T)
    where
        T: Clone,
    {
        self.check();
        let size = self.len();
        assert!(index <= size, "insert at {index} in a sublist of {size} elements");
        self.list.insert(self.offset + index, value);
        self.changed(size + 1);
    }

    #[track_caller]
    pub fn add_all(&self, items: impl IntoIterator<Item = T>) -> bool
    where
        T: Clone,
    {
        self.insert_all(self.len(), items)
    }

    #[track_caller]
    pub fn insert_all(&self, index: usize, items: impl IntoIterator<Item = T>) -> bool
    where
        T: Clone,
    {
        self.check();
        let size = self.len();
        assert!(index <= size, "insert_all at {index} in a sublist of {size} elements");
        let items: Vec<T> = items.into_iter().collect();
        let added = items.len();
        let changed = self.list.insert_all(self.offset + index, items);
        self.changed(size + added);
        changed
    }

    #[track_caller]
    pub fn remove(&self, index: usize) -> T
    where
        T: Clone,
    {
        self.check();
        let size = self.len();
        assert!(index < size, "remove at {index} in a sublist of {size} elements");
        let value = self.list.remove(self.offset + index);
        self.changed(size - 1);
        value
    }

    /// Removes the first element equal to `value`. Returns whether there was one.
    #[track_caller]
    pub fn remove_item(&self, value: &T) -> bool
    where
        T: Clone + PartialEq,
    {
        match self.index_of(value) {
            Some(index) => {
                self.remove(index);
                true
            }
            None => false,
        }
    }

    /// Keeps the view's elements `keep` accepts; the rest of the list is left alone. Returns
    /// whether any went.
    #[track_caller]
    pub fn retain(&self, mut keep: impl FnMut(&T) -> bool) -> bool
    where
        T: Clone,
    {
        self.check();
        let (offset, size) = (self.offset, self.len());
        let kept: Vec<bool> =
            self.list.current(|list| list[offset..offset + size].iter().map(&mut keep).collect());
        let left = kept.iter().filter(|&&kept| kept).count();
        if left == kept.len() {
            return false;
        }
        let end = offset + kept.len();
        let mut index = 0;
        self.list.edit(|list| {
            list.retain(|_| {
                let at = index;
                index += 1;
                !(offset..end).contains(&at) || kept[at - offset]
            });
        });
        self.changed(left);
        true
    }

    /// Removes one occurrence per element of `values`, as Compose's `SubList.removeAll` (unlike
    /// the list's, which removes them all). Returns whether any went.
    #[track_caller]
    pub fn remove_all(&self, values: &[T]) -> bool
    where
        T: Clone + PartialEq,
    {
        let mut removed = false;
        for value in values {
            removed |= self.remove_item(value);
        }
        removed
    }

    #[track_caller]
    pub fn retain_all(&self, values: &[T]) -> bool
    where
        T: Clone + PartialEq,
    {
        self.retain(|item| values.contains(item))
    }

    /// Replaces the element at `index`: an equal value writes nothing.
    #[track_caller]
    pub fn set(&self, index: usize, value: T) -> T
    where
        T: Clone + PartialEq,
    {
        self.check();
        let size = self.len();
        assert!(index < size, "set at {index} in a sublist of {size} elements");
        let old = self.list.set(self.offset + index, value);
        self.structural.set(self.list.structural());
        old
    }

    /// Like [`set`](Self::set), for values that can't be compared: always a write.
    #[track_caller]
    pub fn set_always(&self, index: usize, value: T) -> T
    where
        T: Clone,
    {
        self.check();
        let size = self.len();
        assert!(index < size, "set at {index} in a sublist of {size} elements");
        let old = self.list.set_always(self.offset + index, value);
        self.structural.set(self.list.structural());
        old
    }

    /// Removes the view's elements from the list. Empty, it does nothing, not even validating.
    #[track_caller]
    pub fn clear(&self)
    where
        T: Clone,
    {
        let size = self.len();
        if size == 0 {
            return;
        }
        self.check();
        self.list.remove_range(self.offset..self.offset + size);
        self.changed(0);
    }

    /// A view of `range` of this view: of the same list, as `subList`.
    #[track_caller]
    pub fn sub_list(&self, range: std::ops::Range<usize>) -> StateSubList<T> {
        self.check();
        let size = self.len();
        assert!(
            range.start <= range.end && range.end <= size,
            "sub_list {range:?} of a sublist of {size} elements"
        );
        StateSubList {
            list: self.list,
            offset: self.offset + range.start,
            size: Cell::new(range.len()),
            structural: Cell::new(self.structural.get()),
        }
    }

    /// A cursor at `index` of the view, as `listIterator(index)`: it only reads, as Compose's.
    #[track_caller]
    pub fn list_iterator(&self, index: usize) -> StateListIterator<T> {
        let size = self.len();
        assert!(index <= size, "list_iterator at {index} of a sublist of {size} elements");
        let cursor = Cursor::Sub(self.sub_list(0..size));
        StateListIterator { cursor, index, last: None }
    }
}

/// A cursor over a list or a sublist, as Compose's `listIterator`: moves both ways, and, over a
/// whole list, sets, adds and removes where it is. A structural change other than through it makes
/// it panic.
pub struct StateListIterator<T: 'static> {
    cursor: Cursor<T>,
    index: usize,
    /// What the last `next` or `previous` returned, which `set` and `remove` act on.
    last: Option<usize>,
}

enum Cursor<T: 'static> {
    /// A whole list's: over what `to_list` gave, as Kotlin's `stateList.toList()`, dropped before
    /// a write so that the write doesn't copy.
    List { list: SnapshotStateList<T>, snapshot: Option<ImmutableList<T>>, structural: u32 },
    /// A sublist's, which only reads.
    Sub(StateSubList<T>),
}

impl<T: 'static> Cursor<T> {
    /// Its element at `index`, `None` past the end.
    #[track_caller]
    fn element(&self, index: usize) -> Option<T>
    where
        T: Clone,
    {
        match self {
            Cursor::List { list, snapshot, structural } => {
                assert!(
                    *structural == list.structural(),
                    "compose-runtime: an iterator of a SnapshotStateList used after the list \
                     changed structurally (ConcurrentModification)"
                );
                snapshot.as_ref().and_then(|snapshot| snapshot.get(index).cloned())
            }
            Cursor::Sub(list) => list.get(index),
        }
    }

    fn len(&self) -> usize {
        match self {
            Cursor::List { snapshot, .. } => snapshot.as_ref().map_or(0, |snapshot| snapshot.len()),
            Cursor::Sub(list) => list.len(),
        }
    }

    /// Runs a write of the whole list (a sublist's panics): the snapshot dropped before it, so it
    /// doesn't copy, and taken again after.
    #[track_caller]
    fn write<R>(&mut self, f: impl FnOnce(SnapshotStateList<T>) -> R) -> R {
        let Cursor::List { list, snapshot, structural } = self else {
            panic!("Cannot modify a state list through an iterator");
        };
        assert!(
            *structural == list.structural(),
            "compose-runtime: an iterator of a SnapshotStateList used after the list changed \
             structurally (ConcurrentModification)"
        );
        *snapshot = None;
        let result = f(*list);
        *snapshot = Some(list.to_list());
        *structural = list.structural();
        result
    }
}

impl<T: Clone + 'static> Iterator for StateListIterator<T> {
    type Item = T;

    #[track_caller]
    fn next(&mut self) -> Option<T> {
        let value = self.cursor.element(self.index)?;
        self.last = Some(self.index);
        self.index += 1;
        Some(value)
    }
}

impl<T: 'static> StateListIterator<T> {
    /// Whether there's a next element, as it knows it: no read of the list.
    pub fn has_next(&self) -> bool {
        self.index < self.cursor.len()
    }

    pub fn has_previous(&self) -> bool {
        self.index > 0
    }

    pub fn next_index(&self) -> usize {
        self.index
    }

    /// The index `previous` would give; `None` at the start.
    pub fn previous_index(&self) -> Option<usize> {
        self.index.checked_sub(1)
    }

    /// The element before the cursor, moving back over it, as `previous()`.
    #[track_caller]
    pub fn previous(&mut self) -> Option<T>
    where
        T: Clone,
    {
        let index = self.index.checked_sub(1)?;
        let value = self.cursor.element(index)?;
        self.index = index;
        self.last = Some(index);
        Some(value)
    }

    /// Replaces what the last `next` or `previous` returned, as `set`. Panics if nothing was, or
    /// `add` or `remove` came since. An equal value writes nothing.
    #[track_caller]
    pub fn set(&mut self, value: T)
    where
        T: Clone + PartialEq,
    {
        let last = self.last;
        self.cursor.write(|list| {
            list.set(last.expect("set before next or previous"), value);
        });
    }

    /// Like [`set`](Self::set), for values that can't be compared: always a write.
    #[track_caller]
    pub fn set_always(&mut self, value: T)
    where
        T: Clone,
    {
        let last = self.last;
        self.cursor.write(|list| {
            list.set_always(last.expect("set before next or previous"), value);
        });
    }

    /// Adds `value` before the cursor, moving past it, as `add`.
    #[track_caller]
    pub fn add(&mut self, value: T)
    where
        T: Clone,
    {
        let index = self.index;
        self.cursor.write(|list| list.insert(index, value));
        self.index += 1;
        self.last = None;
    }

    /// Removes what the last `next` or `previous` returned, as `remove`. Panics if nothing was,
    /// or `add` or `remove` came since.
    #[track_caller]
    pub fn remove(&mut self)
    where
        T: Clone,
    {
        let last = self.last;
        let index = self.cursor.write(|list| {
            let index = last.expect("remove before next or previous");
            list.remove(index);
            index
        });
        self.last = None;
        self.index = index;
    }
}
