//! Lifecycles, as AndroidX's `androidx.lifecycle` (`lifecycle-common`, `lifecycle-runtime`,
//! `lifecycle-runtime-compose`, `lifecycle-runtime-testing`), D704, D837.
//!
//! A [`Lifecycle`] moves through [`LifecycleState`]s and tells its observers of each
//! [`LifecycleEvent`]; a [`LifecycleRegistry`] is what moves it. Hosts provide theirs in
//! composition ([`LOCAL_LIFECYCLE_OWNER`]), and the effects here act on its events.
//!
//! Rust-shaped: a lifecycle's owner is the [`Lifecycle`] itself, a cheap handle. What Kotlin passes
//! as a `LifecycleOwner` is a `Lifecycle` here, and [`LifecycleOwner`] says how to get one.

extern crate compose_runtime as compose;

mod composition;
mod coroutines;
mod lifecycle;
mod testing;

pub use composition::{
    LOCAL_LIFECYCLE_OWNER, LifecycleEventEffect, LifecyclePauseOrDisposeEffectResult,
    LifecycleResumeEffect, LifecycleResumePauseEffectScope, LifecycleStartEffect,
    LifecycleStartStopEffectScope, LifecycleStopOrDisposeEffectResult, drop_unless_resumed,
    drop_unless_started, remember_lifecycle_owner,
};
pub use coroutines::LifecycleDestroyed;
pub use lifecycle::{
    DefaultLifecycleObserver, DefaultLifecycleObserverAdapter, Lifecycle, LifecycleEvent,
    LifecycleObserver, LifecycleOwner, LifecycleRegistry, LifecycleState,
};
pub use testing::TestLifecycleOwner;
