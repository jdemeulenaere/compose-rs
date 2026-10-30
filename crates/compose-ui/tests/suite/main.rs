//! The compose-ui integration tests in one binary, each file a module, so
//! they build and link once (docs/08-fast-builds.md).

extern crate compose_runtime as compose;

mod annotated_string;
mod annotated_string_builder;
mod coordinates_in_passes;
mod delegating_nodes;
mod dp_offset;
mod focus_tests;
mod font_model;
mod font_resolver;
mod intrinsics;
mod layout_aware_nodes;
mod layout_coordinates;
mod modifier_locals;
mod modifier_nodes;
mod mutable_paths;
mod node_invalidation;
mod node_locals;
mod node_traversal;
mod observer_nodes;
mod offset;
mod only_unsafe_module;
mod paragraph;
mod paragraph_style;
mod parent_data;
mod placeholder;
mod placement_pass;
mod pointer_event;
mod retained_drawing;
mod retained_values;
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
mod rulers;
mod single_drawing_extent;
mod span_style;
#[path = "../support/mod.rs"]
mod support;
mod text_measurer;
mod text_range;
mod text_savers;
mod text_style;
mod text_style_attributes;
mod text_style_values;
mod text_unit;
mod ui;
mod unplaced_aware;
mod velocity_data;
mod velocity_tracker;
mod word_boundary;
