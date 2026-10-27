//! Snapshots (docs/parity/design/snapshots.md): state objects' records and identities.

use compose_runtime::platform::state_records;
use compose_runtime::snapshots::StateObjectId;
use compose_runtime::*;

/// A state keeps one record, and each write that changes it makes a new version; an
/// equivalent write writes nothing.
#[test]
fn a_write_makes_a_new_version_of_the_one_record() {
    let state = mutable_state_of(1);
    let (records, first) = state_records(state.handle());
    assert_eq!(records, 1);
    state.set(2);
    let (records, second) = state_records(state.handle());
    assert_eq!(records, 1);
    assert_ne!(second, first);
    state.set(2);
    assert_eq!(state_records(state.handle()).1, second, "an equivalent write writes nothing");
    state.edit(|value| *value += 1);
    assert_ne!(state_records(state.handle()).1, second);
    assert_eq!(state.get(), 3);
}

/// A state's handles convert into one identity; another state's differs.
#[test]
fn a_states_handles_share_its_identity() {
    let (state, other) = (mutable_state_of(0), mutable_state_of(0));
    let id = StateObjectId::from(state.handle());
    assert_eq!(id, StateObjectId::from(state.as_state()));
    assert_eq!(id, StateObjectId::from(&state));
    assert_eq!(id, state.state_object_id());
    assert_ne!(id, other.state_object_id());
}
