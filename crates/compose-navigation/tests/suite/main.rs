//! The compose-navigation integration tests in one binary, each file a module, so they build
//! and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod navigation;
mod review;
