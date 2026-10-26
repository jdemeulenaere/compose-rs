//! compose-test's own tests in one binary, each file a module, so they build and link once
//! (docs/08-fast-builds.md). The harness's APIs are tested as Compose's ui-test tests them, each
//! test citing the one it adapts.

extern crate compose_runtime as compose;

#[path = "../../../compose-ui/tests/support/mod.rs"]
mod support;

mod actions;
mod bounds;
mod capture;
mod clock;
mod displayed;
mod input;
mod review_d;
mod rule;
mod selectors;
