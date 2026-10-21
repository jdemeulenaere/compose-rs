//! The compose-animation integration tests in one binary, each file a module, so
//! they build and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod animation;
mod content;
mod crossfade;
mod review_animation;
mod review_round10;
mod review_round8;
mod review_round9;
mod visibility;
