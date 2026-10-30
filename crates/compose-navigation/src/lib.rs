//! Navigation, as Navigation 3 (`androidx.navigation3`): the back stack is state the app owns, of
//! keys of the app's own type (an enum, typically), [`NavEntry`]s are what they show, and
//! [`NavDisplay`] shows them as scenes, animating between them, each entry's saveable state kept
//! while it's on the stack. Back pops the stack. See D1400 and D1405 in docs/decisions.md.

#![forbid(unsafe_code)]
// Every type reachable from the public API can be named (D260): a type in a private
// module that a public signature uses must be re-exported.
#![deny(unnameable_types)]

extern crate compose_runtime as compose;

mod runtime;
mod ui;

pub use compose_runtime::__private::{
    DeepLinkArguments, DeepLinkDecoderError, DeepLinkElement, DeepLinkKey, DeepLinkPrimitive,
    DeepLinkSerializer,
};
pub use runtime::*;
pub use ui::*;

#[doc(hidden)]
pub use compose_lifecycle::local_view_model_store_owner;
#[doc(hidden)]
pub use compose_runtime::remember_saveable_state_holder;
#[doc(hidden)]
pub use compose_ui::{Alignment, Modifier};
