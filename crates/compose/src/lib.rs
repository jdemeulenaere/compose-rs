//! compose-rs: Jetpack Compose's model in Rust.
//!
//! This facade re-exports the framework's crates as modules mirroring Compose's packages:
//! `compose::runtime` is `androidx.compose.runtime`, `compose::ui` is `androidx.compose.ui`
//! (with `graphics`, `unit` and `text`), and, with the `test` feature, `compose::test` is
//! `androidx.compose.ui.test`.

/// `androidx.compose.runtime`: composition, `remember`, state, effects, and the `#[composable]`
/// transform.
pub mod runtime {
    pub use compose_runtime::*;
}

/// `androidx.compose.ui`: layout nodes, `Modifier`, `Layout` and `MeasurePolicy`, drawing,
/// input, focus, semantics, and the host the platforms wrap.
pub mod ui {
    pub use compose_ui::*;
}

/// `androidx.compose.ui.test`: the test rule, finders, actions, assertions and event scripts.
#[cfg(feature = "test")]
pub mod test {
    pub use compose_test::*;
}

/// What `#[composable]` generated code calls. Not a public API.
#[doc(hidden)]
pub mod __private {
    pub use compose_runtime::__private::*;
}
