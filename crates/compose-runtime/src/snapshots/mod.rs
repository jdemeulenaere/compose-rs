//! Snapshots, as Compose's `androidx.compose.runtime.snapshots` (docs/parity/design/snapshots.md):
//! state objects whose values are kept in records, one per snapshot that wrote them.
//!
//! Each thread has its own snapshot world, as its own state (D14, D750).

mod records;

pub(crate) use records::{Records, StateObjectCell};

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
