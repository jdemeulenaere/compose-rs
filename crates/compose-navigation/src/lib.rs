//! Navigation. Navigation 3 (`androidx.navigation3`, D1400, D1405), which replaces what follows
//! (D704), and Navigation Compose (`androidx.navigation.compose`): a
//! [`NavController`]
//! holds a back stack of the app's destinations, values of its own type (an enum, typically,
//! rather than string routes), and a [`NavHost`] shows the top one, each entry's saveable state
//! kept while it's on the stack. Back pops the stack. The stack is saved across the host's
//! recreation ([`remember_nav_controller`]). See D290 to D293 in docs/decisions.md.

#![forbid(unsafe_code)]
// Every type reachable from the public API can be named (D260): a type in a private
// module that a public signature uses must be re-exported.
#![deny(unnameable_types)]

extern crate compose_runtime as compose;

mod controller;
mod host;
mod runtime;
mod ui;

pub use controller::{NavBackStackEntry, NavController, NavOptions, remember_nav_controller};
pub use host::NavHost;
pub use runtime::*;
pub use ui::*;

#[doc(hidden)]
pub use compose_animation::{EnterTransition, ExitTransition};
#[doc(hidden)]
pub use compose_runtime::remember_saveable_state_holder;
#[doc(hidden)]
pub use compose_ui::{Alignment, Modifier};
