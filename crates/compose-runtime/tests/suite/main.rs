//! The compose-runtime integration tests in one binary, each file a module, so
//! they build and link once (docs/08-fast-builds.md).

// `#[composable]` names the runtime `::compose`, the facade's name (docs/03).
extern crate compose_runtime as compose;

mod composition;
mod composition_and_derived_state;
mod composition_local;
mod composition_reusing;
mod compound_hash_key;
mod derive_saveable;
mod derived_snapshot_state;
mod effects;
mod flows;
mod frame_clocks;
mod holders;
mod jvm_composition;
mod mock;
mod model;
mod pausable;
mod produce_state;
mod recomposer;
mod restart;
mod restoration;
mod reusable;
mod review2;
mod review3;
mod review4;
mod review5;
mod review6;
mod review7;
mod saveable_state_registry;
mod scenarios;
mod side_effects;
mod snapshot_observation;
mod snapshots;
mod state_list;
mod state_map;
mod state_set;
