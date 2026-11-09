//! The compose-lifecycle integration tests in one binary, each file a module, so they build and
//! link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod collect_as_state_with_lifecycle;
mod drop_unless;
mod effects;
mod flow_with_lifecycle;
mod observers;
mod owner;
mod registry;
mod repeat_on_lifecycle;
mod view_model;
mod view_model_compose;
mod with_state;
