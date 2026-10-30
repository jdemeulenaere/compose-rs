//! Snapshots, as Compose's `androidx.compose.runtime.snapshots` (docs/parity/design/snapshots.md):
//! state objects whose values are kept in records, one per snapshot that wrote them.
//!
//! Each thread has its own snapshot world, as its own state (D14, D750).

mod global;
mod list;
mod map;
mod observer;
mod records;
mod set;

pub use global::{ChangeSet, ObserverHandle, Snapshot, StateObserverFn, Turn};
pub(crate) use global::{
    Snapshots, advance_around_pass, notify_read, send_unsent_outside_turns, wrote, wrote_unwinding,
};
pub(crate) use global::{
    begin_calculation, calculation_depth, calculation_reads, end_calculation, next_record,
    valid_mark,
};
pub use list::{
    OwnedStateList, SnapshotStateList, StateListIterator, StateSubList, mutable_state_list_from,
    mutable_state_list_of, snapshot_state_list,
};
pub use map::{
    OwnedStateMap, SnapshotStateMap, StateMapContents, mutable_state_map_from, mutable_state_map_of,
};
pub use observer::SnapshotStateObserver;
pub(crate) use records::{Records, SnapshotId, StateObjectCell};
pub use set::{
    OrderedSet, OwnedStateSet, SnapshotStateSet, mutable_state_set_from, mutable_state_set_of,
};

use std::fmt;

use crate::state::{MutableState, State, StateId};

/// The identity of a state object (a [`MutableState`], a [`State`], a derived state, a state
/// collection): what apply observers are told changed and what read and write observers are
/// told was read or written, as the `Any` Compose's observers receive. `Copy`, comparable and
/// hashable; a state's handles all convert into the same id.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateObjectId(pub(crate) StateId);

impl fmt::Debug for StateObjectId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "StateObjectId({:?})", self.0)
    }
}

impl<T: 'static> From<MutableState<T>> for StateObjectId {
    fn from(state: MutableState<T>) -> StateObjectId {
        state.state_object_id()
    }
}

impl<T: 'static> From<State<T>> for StateObjectId {
    fn from(state: State<T>) -> StateObjectId {
        state.state_object_id()
    }
}

impl<T: 'static> From<&crate::state::OwnedState<T>> for StateObjectId {
    fn from(state: &crate::state::OwnedState<T>) -> StateObjectId {
        state.state_object_id()
    }
}
