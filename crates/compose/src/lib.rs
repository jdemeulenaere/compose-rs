//! compose-rs: Jetpack Compose's model in Rust.
//!
//! This facade re-exports the framework's crates as modules mirroring Compose's packages:
//! `compose::runtime` is `androidx.compose.runtime`, `compose::ui` is `androidx.compose.ui`
//! (with `graphics`, `unit` and `text`), `compose::foundation` (with `layout`) and
//! `compose::material3` and `compose::animation` are their Compose namesakes, and, with the `test` feature, `compose::test` is
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

/// `androidx.compose.foundation`: `BasicText`, `BasicTextField`, shapes, and the foundation
/// modifiers (methods of `Modifier`); `foundation::layout` has `Box`, `Column`, `Row`, `Spacer`
/// and `Arrangement`.
pub mod foundation {
    pub use compose_foundation::*;
}

/// `androidx.compose.animation` and `animation.core`: `Animatable`, `tween`, `spring`, easings,
/// `animate_*_as_state`, infinite transitions.
pub mod animation {
    pub use compose_animation::*;
}

/// `androidx.compose.material3`: `MaterialTheme`, `Text`, `Surface`, the buttons, `Checkbox`,
/// `TextField`.
pub mod material3 {
    pub use compose_material3::*;
}

/// `androidx.compose.ui.test`: the test rule, finders, actions, assertions and event scripts.
#[cfg(feature = "test")]
pub mod test {
    pub use compose_test::*;
}

/// Desktop windows: `run_app` (the `desktop` feature).
#[cfg(feature = "desktop")]
pub mod desktop {
    pub use compose_desktop::*;
}

/// What `#[composable]` generated code calls. Not a public API.
#[doc(hidden)]
pub mod __private {
    pub use compose_runtime::__private::*;
}
