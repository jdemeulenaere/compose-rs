//! The samples of the developer guide (`docs/guide/`), one module per section. The guide quotes
//! the regions between `// ANCHOR: name` and `// ANCHOR_END: name` lines;
//! `tests/suite/snippets.rs` checks that every quote matches its region (`UPDATE_GUIDE=1 cargo
//! test -p guide snippets` rewrites the quotes from the sources), and the other tests run the
//! samples.

#![allow(non_snake_case)]

pub mod accessibility;
pub mod android;
pub mod animation;
pub mod components;
pub mod composables;
pub mod drawing;
pub mod effects;
pub mod gestures;
pub mod getting_started;
pub mod images;
pub mod interop;
pub mod layouts;
pub mod navigation;
pub mod performance;
pub mod state;
pub mod text;
pub mod theming;
