//! The compose-navigation integration tests in one binary, each file a module, so they build
//! and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod nav_animated;
mod nav_back_stack;
mod nav_card_stack;
mod nav_decorated_entries;
mod nav_display;
mod nav_entry;
mod nav_lifecycle;
mod nav_predictive_back;
mod nav_results;
mod nav_scene;
mod nav_stack_state;
mod nav_support;
mod nav_view_model;
mod nav_z_index;
