//! Navigation events, as AndroidX's `androidx.navigationevent` and `navigationevent-compose`: an
//! input (the platform's back gesture, a test's calls) sends back and forward gestures, started,
//! with progress, then completed or cancelled, to a [`NavigationEventDispatcher`], whose
//! highest-priority enabled [`NavigationEventHandler`] takes each. What Navigation 3's
//! `NavigationBackHandler` and `PredictiveBackHandler` are built on.

#![forbid(unsafe_code)]
#![deny(unnameable_types)]

extern crate compose_runtime as compose;

mod composition;
mod dispatcher;
mod event;
mod handler;
mod input;
mod processor;
mod testing;

pub use composition::{
    LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER, NavigationBackHandler, NavigationEventHandler,
    NavigationEventState, NavigationForwardHandler, remember_navigation_event_dispatcher_owner,
    remember_navigation_event_state,
};
pub use dispatcher::{
    NavigationEventDispatcher, OnBackCompletedFallback, OnForwardCompletedFallback,
};
pub use event::{
    NavigationEvent, NavigationEventHistory, NavigationEventInfo, NavigationEventTransitionState,
};
pub use handler::{NavigationEventCallbacks, NavigationEventHandler};
pub use input::{DirectNavigationEventInput, NavigationEventInput, NavigationEventInputCallbacks};
pub use testing::{TestNavigationEventDispatcherOwner, TestNavigationEventHandler};

/// The local owner, or a panic, for the composables' defaults.
#[doc(hidden)]
pub fn local_navigation_event_dispatcher_owner() -> NavigationEventDispatcher {
    LOCAL_NAVIGATION_EVENT_DISPATCHER_OWNER.current().expect(
        "No NavigationEventDispatcherOwner provided in LocalNavigationEventDispatcherOwner. If you \
         intended to create a root dispatcher, explicitly pass null as the parent.",
    )
}
