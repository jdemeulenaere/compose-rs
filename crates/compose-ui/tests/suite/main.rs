//! The compose-ui integration tests in one binary, each file a module, so
//! they build and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod mutable_paths;
mod retained_drawing;
mod review_d;
mod review_d2;
mod review_d270;
mod review_d520;
mod review_d_compile_semantics_closure;
mod review_d_incremental;
mod review_d_panic_layout_node;
mod review_d_panic_measure;
mod review_d_panic_poisoned;
#[path = "../review_d_support/mod.rs"]
mod review_d_support;
mod review_groups;
mod review_retained;
#[path = "../support/mod.rs"]
mod support;
mod ui;
