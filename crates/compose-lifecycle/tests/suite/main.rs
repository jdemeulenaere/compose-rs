//! The compose-lifecycle integration tests in one binary, each file a module, so they build and
//! link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod drop_unless;
mod effects;
mod observers;
mod owner;
mod registry;
mod repeat_on_lifecycle;
mod with_state;
