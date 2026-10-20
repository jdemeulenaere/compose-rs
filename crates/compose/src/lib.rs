//! compose-rs: Jetpack Compose's model in Rust.
//!
//! This facade re-exports the framework's crates as modules mirroring Compose's packages:
//! `compose::runtime` is `androidx.compose.runtime`.

/// `androidx.compose.runtime`: composition, `remember`, state, effects.
pub mod runtime {
    pub use compose_runtime::*;
}
