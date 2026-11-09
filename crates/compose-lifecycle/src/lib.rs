//! Lifecycles, as AndroidX's `androidx.lifecycle` (`lifecycle-common`, `lifecycle-runtime`,
//! `lifecycle-runtime-compose`, `lifecycle-runtime-testing`), and ViewModels, as its
//! `lifecycle-viewmodel` and `lifecycle-viewmodel-compose`, D704, D837, D838, D2150.
//!
//! A [`Lifecycle`] moves through [`LifecycleState`]s and tells its observers of each
//! [`LifecycleEvent`]; a [`LifecycleRegistry`] is what moves it. Hosts provide theirs in
//! composition ([`LOCAL_LIFECYCLE_OWNER`]), and the effects and flow adapters
//! ([`Lifecycle::event_flow`], [`flow_with_lifecycle`], [`collect_as_state_with_lifecycle`]) act
//! on its events and state.
//!
//! Rust-shaped: a lifecycle's owner is the [`Lifecycle`] itself, a cheap handle. What Kotlin passes
//! as a `LifecycleOwner` is a `Lifecycle` here, and [`LifecycleOwner`] says how to get one.

#![forbid(unsafe_code)]
#![deny(unnameable_types)]

extern crate compose_runtime as compose;

mod composition;
mod coroutines;
mod flow;
mod lifecycle;
mod testing;
mod viewmodel;
mod viewmodel_compose;

pub use composition::{
    LOCAL_LIFECYCLE_OWNER, LifecycleEventEffect, LifecyclePauseOrDisposeEffectResult,
    LifecycleResumeEffect, LifecycleResumePauseEffectScope, LifecycleStartEffect,
    LifecycleStartStopEffectScope, LifecycleStopOrDisposeEffectResult, drop_unless_resumed,
    drop_unless_started, remember_lifecycle_owner,
};
pub use coroutines::LifecycleDestroyed;
pub use flow::{
    EventFlow, Flow, FlowWithLifecycle, FlowWithLifecycleExt, StateFlowLifecycleExt,
    collect_as_state_with_lifecycle, collect_as_state_with_lifecycle_with, flow_with_lifecycle,
    flow_with_lifecycle_with,
};
pub use lifecycle::{
    DefaultLifecycleObserver, DefaultLifecycleObserverAdapter, Lifecycle, LifecycleEvent,
    LifecycleObserver, LifecycleOwner, LifecycleRegistry, LifecycleState,
};
pub use testing::TestLifecycleOwner;
pub use viewmodel::{
    AutoCloseable, CreationExtras, CreationExtrasKey, DefaultViewModelProviderFactory,
    InitializerViewModelFactory, ModelClass, VIEW_MODEL_KEY, ViewModel, ViewModelBase,
    ViewModelProvider, ViewModelProviderFactory, ViewModelStore, ViewModelStoreOwner,
    view_model_factory,
};
pub use viewmodel_compose::{
    LOCAL_VIEW_MODEL_STORE_OWNER, ReferenceToken, ViewModelArgs, ViewModelStoreOwnerRef,
    ViewModelStoreProvider, remember_view_model_store_owner, remember_view_model_store_owner_in,
    remember_view_model_store_provider, view_model, view_model_args, view_model_owned_with,
    view_model_with,
};

/// The local store's owner, or a panic, for defaults of the composables' parameters.
#[doc(hidden)]
pub fn local_view_model_store_owner() -> ViewModelStoreOwnerRef {
    LOCAL_VIEW_MODEL_STORE_OWNER
        .current()
        .expect("CompositionLocal LocalViewModelStoreOwner not present")
}
