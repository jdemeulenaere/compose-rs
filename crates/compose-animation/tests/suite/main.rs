//! The compose-animation integration tests in one binary, each file a module, so
//! they build and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod animated_visibility;
mod animation;
mod animation_objects;
mod content;
mod content_size;
mod converters;
mod crossfade;
mod deferred_target;
mod easing;
mod enter_exit_config;
mod infinite;
mod interruption_aware;
mod physics;
mod review_animation;
mod review_retained;
mod review_round10;
mod review_round11;
mod review_round12;
mod review_round14;
mod review_round8;
mod review_round9;
mod single_value;
mod spring_estimation;
mod transition;
mod visibility;
